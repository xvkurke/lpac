use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

use crate::CoreError;

pub const PAIRING_PREFIX: &str = "NIKPAIR1:";
pub const SECURE_RELAY_PREFIX: &str = "NIKRSP2:";
const KEY_INFO: &[u8] = b"NIKRSP2/X25519/HKDF-SHA256/XCHACHA20-POLY1305";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerIdentity {
    public_key: [u8; 32],
}

impl PeerIdentity {
    pub fn from_pairing_code(value: &str) -> Result<Self, CoreError> {
        let encoded = value
            .trim()
            .strip_prefix(PAIRING_PREFIX)
            .ok_or(CoreError::InvalidPairingCode)?;
        let public_key: [u8; 32] = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| CoreError::InvalidPairingCode)?
            .try_into()
            .map_err(|_| CoreError::InvalidPairingCode)?;
        Ok(Self { public_key })
    }

    pub fn pairing_code(&self) -> String {
        format!(
            "{PAIRING_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(self.public_key)
        )
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.public_key
    }
}

pub struct DeviceIdentity {
    private_key: [u8; 32],
}

impl DeviceIdentity {
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        Self {
            private_key: secret.to_bytes(),
        }
    }

    pub fn from_private_key(private_key: [u8; 32]) -> Self {
        Self { private_key }
    }

    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.private_key
    }

    pub fn peer_identity(&self) -> PeerIdentity {
        let secret = StaticSecret::from(self.private_key);
        PeerIdentity {
            public_key: PublicKey::from(&secret).to_bytes(),
        }
    }

    pub fn pairing_code(&self) -> String {
        self.peer_identity().pairing_code()
    }

    pub fn encrypt<T: Serialize>(
        &self,
        recipient: &PeerIdentity,
        value: &T,
    ) -> Result<String, CoreError> {
        let sender_public = self.peer_identity().public_key;
        let recipient_public = recipient.public_key;
        let key = derive_key(
            &self.private_key,
            &recipient_public,
            &sender_public,
            &recipient_public,
        )?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        let mut nonce = [0_u8; 24];
        OsRng.fill_bytes(&mut nonce);
        let aad = associated_data(&sender_public, &recipient_public);
        let mut plaintext = serde_json::to_vec(value)?;
        let encrypted = cipher.encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: &aad,
            },
        );
        plaintext.zeroize();
        let ciphertext = encrypted.map_err(|_| CoreError::Crypto)?;
        let envelope = SecureEnvelope {
            version: 2,
            sender_public_key: URL_SAFE_NO_PAD.encode(sender_public),
            recipient_public_key: URL_SAFE_NO_PAD.encode(recipient_public),
            nonce: URL_SAFE_NO_PAD.encode(nonce),
            ciphertext: URL_SAFE_NO_PAD.encode(ciphertext),
        };
        Ok(format!(
            "{SECURE_RELAY_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&envelope)?)
        ))
    }

    pub fn decrypt<T: DeserializeOwned>(
        &self,
        expected_sender: &PeerIdentity,
        value: &str,
    ) -> Result<T, CoreError> {
        let encoded = value
            .trim()
            .strip_prefix(SECURE_RELAY_PREFIX)
            .ok_or(CoreError::InvalidEnvelope)?;
        let envelope: SecureEnvelope = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(encoded)
                .map_err(|_| CoreError::InvalidEnvelope)?,
        )?;
        if envelope.version != 2 {
            return Err(CoreError::InvalidEnvelope);
        }

        let sender_public = decode_array::<32>(&envelope.sender_public_key)?;
        let recipient_public = decode_array::<32>(&envelope.recipient_public_key)?;
        let own_public = self.peer_identity().public_key;
        if sender_public != expected_sender.public_key || recipient_public != own_public {
            return Err(CoreError::RelayPeerMismatch);
        }
        let nonce = decode_array::<24>(&envelope.nonce)?;
        let ciphertext = URL_SAFE_NO_PAD
            .decode(envelope.ciphertext)
            .map_err(|_| CoreError::InvalidEnvelope)?;
        let key = derive_key(
            &self.private_key,
            &sender_public,
            &sender_public,
            &recipient_public,
        )?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        let aad = associated_data(&sender_public, &recipient_public);
        let mut plaintext = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| CoreError::Crypto)?;
        let result = serde_json::from_slice(&plaintext);
        plaintext.zeroize();
        Ok(result?)
    }
}

impl Drop for DeviceIdentity {
    fn drop(&mut self) {
        self.private_key.zeroize();
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecureEnvelope {
    version: u8,
    sender_public_key: String,
    recipient_public_key: String,
    nonce: String,
    ciphertext: String,
}

fn derive_key(
    own_private: &[u8; 32],
    peer_public: &[u8; 32],
    sender_public: &[u8; 32],
    recipient_public: &[u8; 32],
) -> Result<[u8; 32], CoreError> {
    let secret = StaticSecret::from(*own_private);
    let shared = secret.diffie_hellman(&PublicKey::from(*peer_public));
    let mut salt = [0_u8; 64];
    salt[..32].copy_from_slice(sender_public);
    salt[32..].copy_from_slice(recipient_public);
    let hkdf = Hkdf::<Sha256>::new(Some(&salt), shared.as_bytes());
    let mut key = [0_u8; 32];
    hkdf.expand(KEY_INFO, &mut key)
        .map_err(|_| CoreError::Crypto)?;
    Ok(key)
}

fn associated_data(sender_public: &[u8; 32], recipient_public: &[u8; 32]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(SECURE_RELAY_PREFIX.len() + 64);
    aad.extend_from_slice(SECURE_RELAY_PREFIX.as_bytes());
    aad.extend_from_slice(sender_public);
    aad.extend_from_slice(recipient_public);
    aad
}

fn decode_array<const N: usize>(value: &str) -> Result<[u8; N], CoreError> {
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| CoreError::InvalidEnvelope)?
        .try_into()
        .map_err(|_| CoreError::InvalidEnvelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paired_identities_exchange_authenticated_payloads() {
        let card = DeviceIdentity::generate();
        let server = DeviceIdentity::generate();
        let payload = json!({"stage":"SERVER_AUTH","secret":"hidden"});

        let encoded = server.encrypt(&card.peer_identity(), &payload).unwrap();
        let decoded: serde_json::Value = card.decrypt(&server.peer_identity(), &encoded).unwrap();

        assert_eq!(decoded, payload);
        assert!(!encoded.contains("SERVER_AUTH"));
        assert!(!encoded.contains("hidden"));
    }

    #[test]
    fn rejects_unpaired_sender() {
        let card = DeviceIdentity::generate();
        let server = DeviceIdentity::generate();
        let attacker = DeviceIdentity::generate();
        let encoded = attacker
            .encrypt(&card.peer_identity(), &json!({"stage":"INIT_REQUEST"}))
            .unwrap();

        assert!(matches!(
            card.decrypt::<serde_json::Value>(&server.peer_identity(), &encoded),
            Err(CoreError::RelayPeerMismatch)
        ));
    }

    #[test]
    fn pairing_code_round_trip() {
        let identity = DeviceIdentity::generate();
        let parsed = PeerIdentity::from_pairing_code(&identity.pairing_code()).unwrap();
        assert_eq!(parsed, identity.peer_identity());
    }
}
