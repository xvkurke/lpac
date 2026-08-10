use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use zeroize::Zeroize;

use crate::CoreError;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope {
    version: u8,
    nonce: String,
    ciphertext: String,
}

pub(crate) fn encrypt<T: Serialize>(
    value: &T,
    key: &[u8; 32],
    prefix: &str,
) -> Result<String, CoreError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce = [0_u8; 24];
    OsRng.fill_bytes(&mut nonce);

    let mut plaintext = serde_json::to_vec(value)?;
    let encryption_result = cipher.encrypt(
        XNonce::from_slice(&nonce),
        Payload {
            msg: plaintext.as_ref(),
            aad: prefix.as_bytes(),
        },
    );
    plaintext.zeroize();
    let ciphertext = encryption_result.map_err(|_| CoreError::Crypto)?;

    let envelope = Envelope {
        version: 1,
        nonce: URL_SAFE_NO_PAD.encode(nonce),
        ciphertext: URL_SAFE_NO_PAD.encode(ciphertext),
    };
    Ok(format!(
        "{}{}",
        prefix,
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&envelope)?)
    ))
}

pub(crate) fn decrypt<T: DeserializeOwned>(
    value: &str,
    key: &[u8; 32],
    prefix: &str,
) -> Result<T, CoreError> {
    let encoded = value
        .strip_prefix(prefix)
        .ok_or(CoreError::InvalidEnvelope)?;
    let envelope: Envelope = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| CoreError::InvalidEnvelope)?,
    )?;
    if envelope.version != 1 {
        return Err(CoreError::InvalidEnvelope);
    }

    let nonce: [u8; 24] = URL_SAFE_NO_PAD
        .decode(envelope.nonce)
        .map_err(|_| CoreError::InvalidEnvelope)?
        .try_into()
        .map_err(|_| CoreError::InvalidEnvelope)?;
    let ciphertext = URL_SAFE_NO_PAD
        .decode(envelope.ciphertext)
        .map_err(|_| CoreError::InvalidEnvelope)?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut plaintext = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: ciphertext.as_ref(),
                aad: prefix.as_bytes(),
            },
        )
        .map_err(|_| CoreError::Crypto)?;
    let result = serde_json::from_slice(&plaintext);
    plaintext.zeroize();
    Ok(result?)
}
