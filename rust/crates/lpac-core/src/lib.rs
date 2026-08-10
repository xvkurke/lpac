use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

mod envelope;
pub mod relay;
pub mod secure_relay;

pub const ENVELOPE_PREFIX: &str = "NIKLPA1:";

#[derive(Clone, PartialEq, Eq)]
pub struct ActivationCode {
    raw: Zeroizing<String>,
    pub smdp: String,
    pub matching_id: String,
    pub confirmation_required: bool,
}

impl ActivationCode {
    pub fn parse(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        let raw = Zeroizing::new(value.trim().to_owned());
        let (smdp, matching_id, confirmation_required) = {
            let body = raw.strip_prefix("LPA:").unwrap_or(raw.as_str());
            let parts: Vec<&str> = body.split('$').collect();
            if parts.len() < 3 || parts[0] != "1" || parts[1].is_empty() {
                return Err(CoreError::InvalidActivationCode);
            }
            if !parts[2]
                .chars()
                .all(|character| character.is_ascii_graphic() && character != '$')
            {
                return Err(CoreError::InvalidMatchingId);
            }

            (
                parts[1].to_owned(),
                parts[2].to_owned(),
                parts.get(4).is_some_and(|value| *value == "1"),
            )
        };

        Ok(Self {
            raw,
            smdp,
            matching_id,
            confirmation_required,
        })
    }

    pub fn expose(&self) -> &str {
        &self.raw
    }

    pub fn redacted(&self) -> String {
        format!("LPA:1${}$***", self.smdp)
    }
}

impl fmt::Debug for ActivationCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActivationCode")
            .field("value", &self.redacted())
            .field("confirmation_required", &self.confirmation_required)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ActivationJob {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub reader: Option<String>,
    pub eid: Option<String>,
    pub activation_code: String,
    pub confirmation_code: Option<String>,
}

impl ActivationJob {
    pub fn new(
        code: &ActivationCode,
        reader: Option<String>,
        eid: Option<String>,
        confirmation_code: Option<String>,
    ) -> Self {
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

impl Drop for ActivationJob {
    fn drop(&mut self) {
        self.activation_code.zeroize();
        if let Some(confirmation_code) = self.confirmation_code.as_mut() {
            confirmation_code.zeroize();
        }
    }
}

impl fmt::Debug for ActivationJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActivationJob")
            .field("id", &self.id)
            .field("created_at", &self.created_at)
            .field("reader", &self.reader)
            .field("eid", &self.eid)
            .field("activation_code", &"<redacted>")
            .field(
                "confirmation_code",
                &self.confirmation_code.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

pub fn generate_transfer_key() -> [u8; 32] {
    let mut key = [0_u8; 32];
    OsRng.fill_bytes(&mut key);
    key
}

pub fn encode_transfer_key(key: &[u8; 32]) -> String {
    URL_SAFE_NO_PAD.encode(key)
}

pub fn decode_transfer_key(value: &str) -> Result<[u8; 32], CoreError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| CoreError::InvalidKey)?;
    bytes.try_into().map_err(|_| CoreError::InvalidKey)
}

pub fn encrypt_job(job: &ActivationJob, key: &[u8; 32]) -> Result<String, CoreError> {
    envelope::encrypt(job, key, ENVELOPE_PREFIX)
}

pub fn decrypt_job(value: &str, key: &[u8; 32]) -> Result<ActivationJob, CoreError> {
    envelope::decrypt(value, key, ENVELOPE_PREFIX)
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
    #[error("invalid device pairing code")]
    InvalidPairingCode,
    #[error("relay packet sender or recipient does not match the paired device")]
    RelayPeerMismatch,
    #[error("encryption or authentication failed")]
    Crypto,
    #[error("invalid relay packet: {0}")]
    InvalidRelayPacket(String),
    #[error("relay packet {stage} arrived after its deadline")]
    RelayExpired { stage: &'static str },
    #[error("relay hash chain mismatch at sequence {sequence}")]
    RelayHashMismatch { sequence: u32 },
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

    #[test]
    fn accepts_sysmocom_matching_id_and_trims_paste_whitespace() {
        let code =
            ActivationCode::parse("\r\n1$smdpp.test.rsp.sysmocom.de$TS48v2_SAIP2.1_NoBERTLV\r\n")
                .unwrap();

        assert_eq!(code.smdp, "smdpp.test.rsp.sysmocom.de");
        assert_eq!(code.matching_id, "TS48v2_SAIP2.1_NoBERTLV");
        assert_eq!(
            code.expose(),
            "1$smdpp.test.rsp.sysmocom.de$TS48v2_SAIP2.1_NoBERTLV"
        );
    }

    #[test]
    fn accepts_empty_matching_id_allowed_by_sgp22() {
        let code = ActivationCode::parse("1$rsp.example$$1.2.3.4").unwrap();
        assert!(code.matching_id.is_empty());
    }

    #[test]
    fn rejects_whitespace_inside_matching_id() {
        assert!(matches!(
            ActivationCode::parse("1$rsp.example$MATCH ID"),
            Err(CoreError::InvalidMatchingId)
        ));
    }

    #[test]
    fn debug_output_redacts_activation_secrets() {
        let code = ActivationCode::parse("LPA:1$rsp.example$MATCH-123").unwrap();
        let job = ActivationJob::new(&code, None, None, Some("4321".into()));
        let debug = format!("{job:?}");

        assert!(!debug.contains("rsp.example"));
        assert!(!debug.contains("MATCH-123"));
        assert!(!debug.contains("4321"));
    }

    #[test]
    fn rejects_invalid_nonce_length_without_panicking() {
        let encoded = format!(
            "{}{}",
            ENVELOPE_PREFIX,
            URL_SAFE_NO_PAD.encode(
                br#"{"version":1,"nonce":"AAAAAAAAAAA","ciphertext":"AAAAAAAAAAAAAAAAAAAAAA"}"#
            )
        );

        assert!(matches!(
            decrypt_job(&encoded, &[0_u8; 32]),
            Err(CoreError::InvalidEnvelope)
        ));
    }
}
