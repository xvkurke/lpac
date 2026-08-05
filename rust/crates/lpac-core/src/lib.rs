use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chacha20poly1305::{aead::{Aead, KeyInit}, XChaCha20Poly1305, XNonce};
use chrono::{DateTime, Utc};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

pub const ENVELOPE_PREFIX: &str = "NIKLPA1:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationCode {
    raw: Zeroizing<String>,
    pub smdp: String,
    pub matching_id: String,
    pub confirmation_required: bool,
}

impl ActivationCode {
    pub fn parse(value: impl Into<String>) -> Result<Self, CoreError> {
        let raw = Zeroizing::new(value.into());
        let body = raw.strip_prefix("LPA:").unwrap_or(&raw);
        let parts: Vec<&str> = body.split('$').collect();
        if parts.len() < 3 || parts[0] != "1" || parts[1].is_empty() || parts[2].is_empty() {
            return Err(CoreError::InvalidActivationCode);
        }
        if !parts[2].chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(CoreError::InvalidMatchingId);
        }
        Ok(Self {
            raw,
            smdp: parts[1].to_owned(),
            matching_id: parts[2].to_owned(),
            confirmation_required: parts.get(4).is_some_and(|v| *v == "1"),
        })
    }

    pub fn expose(&self) -> &str { &self.raw }

    pub fn redacted(&self) -> String {
        format!("LPA:1${}$***", self.smdp)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationJob {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub reader: Option<String>,
    pub eid: Option<String>,
    pub activation_code: String,
    pub confirmation_code: Option<String>,
}

impl ActivationJob {
    pub fn new(code: &ActivationCode, reader: Option<String>, eid: Option<String>, confirmation_code: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            reader,
            eid,
            activation_code: code.expose().to_owned(),
            confirmation_code,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope {
    version: u8,
    nonce: String,
    ciphertext: String,
}

pub fn generate_transfer_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    key
}

pub fn encode_transfer_key(key: &[u8; 32]) -> String { URL_SAFE_NO_PAD.encode(key) }

pub fn decode_transfer_key(value: &str) -> Result<[u8; 32], CoreError> {
    let bytes = URL_SAFE_NO_PAD.decode(value).map_err(|_| CoreError::InvalidKey)?;
    bytes.try_into().map_err(|_| CoreError::InvalidKey)
}

pub fn encrypt_job(job: &ActivationJob, key: &[u8; 32]) -> Result<String, CoreError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let mut plaintext = serde_json::to_vec(job)?;
    let ciphertext = cipher.encrypt(XNonce::from_slice(&nonce), plaintext.as_ref()).map_err(|_| CoreError::Crypto)?;
    plaintext.zeroize();
    let envelope = Envelope { version: 1, nonce: URL_SAFE_NO_PAD.encode(nonce), ciphertext: URL_SAFE_NO_PAD.encode(ciphertext) };
    Ok(format!("{}{}", ENVELOPE_PREFIX, URL_SAFE_NO_PAD.encode(serde_json::to_vec(&envelope)?)))
}

pub fn decrypt_job(value: &str, key: &[u8; 32]) -> Result<ActivationJob, CoreError> {
    let encoded = value.strip_prefix(ENVELOPE_PREFIX).ok_or(CoreError::InvalidEnvelope)?;
    let envelope: Envelope = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded).map_err(|_| CoreError::InvalidEnvelope)?)?;
    if envelope.version != 1 { return Err(CoreError::InvalidEnvelope); }
    let nonce = URL_SAFE_NO_PAD.decode(envelope.nonce).map_err(|_| CoreError::InvalidEnvelope)?;
    let ciphertext = URL_SAFE_NO_PAD.decode(envelope.ciphertext).map_err(|_| CoreError::InvalidEnvelope)?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut plaintext = cipher.decrypt(XNonce::from_slice(&nonce), ciphertext.as_ref()).map_err(|_| CoreError::Crypto)?;
    let result = serde_json::from_slice(&plaintext)?;
    plaintext.zeroize();
    Ok(result)
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid activation code")]
    InvalidActivationCode,
    #[error("invalid matching ID")]
    InvalidMatchingId,
    #[error("invalid transfer envelope")]
    InvalidEnvelope,
    #[error("invalid transfer key")]
    InvalidKey,
    #[error("encryption or authentication failed")]
    Crypto,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_encrypted_job() {
        let code = ActivationCode::parse("LPA:1$rsp.example$MATCH-123").unwrap();
        let job = ActivationJob::new(&code, Some("reader-0".into()), None, None);
        let key = generate_transfer_key();
        let envelope = encrypt_job(&job, &key).unwrap();
        let decoded = decrypt_job(&envelope, &key).unwrap();
        assert_eq!(decoded.activation_code, code.expose());
        assert!(!envelope.contains("rsp.example"));
    }
}
