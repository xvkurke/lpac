use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{CoreError, envelope};

pub const RELAY_PREFIX: &str = "NIKRSP1:";
pub const DEMO_SESSION_TTL_SECONDS: i64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelayStage {
    InitRequest,
    ServerAuth,
    EuiccAuth,
    DownloadPrepare,
    EuiccPrepared,
    BoundProfilePackage,
    InstallResult,
}

impl RelayStage {
    pub const ALL: [Self; 7] = [
        Self::InitRequest,
        Self::ServerAuth,
        Self::EuiccAuth,
        Self::DownloadPrepare,
        Self::EuiccPrepared,
        Self::BoundProfilePackage,
        Self::InstallResult,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::InitRequest => "INIT_REQUEST",
            Self::ServerAuth => "SERVER_AUTH",
            Self::EuiccAuth => "EUICC_AUTH",
            Self::DownloadPrepare => "DOWNLOAD_PREPARE",
            Self::EuiccPrepared => "EUICC_PREPARED",
            Self::BoundProfilePackage => "BOUND_PROFILE_PACKAGE",
            Self::InstallResult => "INSTALL_RESULT",
        }
    }

    pub fn label_uk(self) -> &'static str {
        match self {
            Self::InitRequest => "Початкові дані eUICC",
            Self::ServerAuth => "Автентифікація сервера",
            Self::EuiccAuth => "Автентифікація eUICC",
            Self::DownloadPrepare => "Параметри підготовки профілю",
            Self::EuiccPrepared => "eUICC готова до пакета",
            Self::BoundProfilePackage => "Bound Profile Package",
            Self::InstallResult => "Результат встановлення",
        }
    }

    pub fn offline_action_uk(self) -> &'static str {
        match self {
            Self::InitRequest => "GetEuiccChallenge + GetEuiccInfo1",
            Self::ServerAuth => "AuthenticateServer",
            Self::EuiccAuth => "Повернути euiccSigned1 + сертифікати",
            Self::DownloadPrepare => "PrepareDownload",
            Self::EuiccPrepared => "Повернути euiccSigned2 + euiccOtpk",
            Self::BoundProfilePackage => "LoadBoundProfilePackage",
            Self::InstallResult => "ProfileList та перевірка ICCID",
        }
    }

    pub fn online_action_uk(self) -> &'static str {
        match self {
            Self::InitRequest => "ES9+ InitiateAuthentication",
            Self::ServerAuth => "Повернути serverSigned1 + CERT.DPauth.SIG",
            Self::EuiccAuth => "ES9+ AuthenticateClient",
            Self::DownloadPrepare => "Повернути profileMetadata + smdpSigned2",
            Self::EuiccPrepared => "ES9+ GetBoundProfilePackage",
            Self::BoundProfilePackage => "Повернути зашифрований BPP",
            Self::InstallResult => "Audit та відкладена notification",
        }
    }

    pub fn direction(self) -> RelayDirection {
        match self {
            Self::InitRequest
            | Self::EuiccAuth
            | Self::EuiccPrepared
            | Self::InstallResult => RelayDirection::OfflineToOnline,
            Self::ServerAuth | Self::DownloadPrepare | Self::BoundProfilePackage => {
                RelayDirection::OnlineToOffline
            }
        }
    }

    pub fn sequence(self) -> u32 {
        match self {
            Self::InitRequest => 1,
            Self::ServerAuth => 2,
            Self::EuiccAuth => 3,
            Self::DownloadPrepare => 4,
            Self::EuiccPrepared => 5,
            Self::BoundProfilePackage => 6,
            Self::InstallResult => 7,
        }
    }

    pub fn next(self) -> Option<Self> {
        match self {
            Self::InitRequest => Some(Self::ServerAuth),
            Self::ServerAuth => Some(Self::EuiccAuth),
            Self::EuiccAuth => Some(Self::DownloadPrepare),
            Self::DownloadPrepare => Some(Self::EuiccPrepared),
            Self::EuiccPrepared => Some(Self::BoundProfilePackage),
            Self::BoundProfilePackage => Some(Self::InstallResult),
            Self::InstallResult => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayDirection {
    OfflineToOnline,
    OnlineToOffline,
}

impl RelayDirection {
    pub fn arrow(self) -> &'static str {
        match self {
            Self::OfflineToOnline => "──────────────►",
            Self::OnlineToOffline => "◄──────────────",
        }
    }

    pub fn label_uk(self) -> &'static str {
        match self {
            Self::OfflineToOnline => "офлайн → онлайн",
            Self::OnlineToOffline => "онлайн → офлайн",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayPacket {
    pub version: u8,
    pub job_id: Uuid,
    pub sequence: u32,
    pub stage: RelayStage,
    pub direction: RelayDirection,
    pub target_eid_hash: String,
    pub transaction_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub previous_message_hash: Option<String>,
    pub payload: Value,
}

impl RelayPacket {
    pub fn digest(&self) -> Result<String, CoreError> {
        let serialized = serde_json::to_vec(self)?;
        Ok(URL_SAFE_NO_PAD.encode(Sha256::digest(serialized)))
    }

    pub fn payload_fields(&self) -> Vec<&str> {
        self.payload
            .as_object()
            .map(|object| object.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    pub fn validate_after(
        &self,
        previous: Option<&RelayPacket>,
        received_at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        if self.version != 1 {
            return Err(CoreError::InvalidRelayPacket(
                "unsupported protocol version".into(),
            ));
        }
        if self.sequence != self.stage.sequence() {
            return Err(CoreError::InvalidRelayPacket(format!(
                "stage {} must use sequence {}",
                self.stage.code(),
                self.stage.sequence()
            )));
        }
        if self.direction != self.stage.direction() {
            return Err(CoreError::InvalidRelayPacket(format!(
                "stage {} has an invalid direction",
                self.stage.code()
            )));
        }
        if self.target_eid_hash.is_empty() {
            return Err(CoreError::InvalidRelayPacket(
                "target EID hash is empty".into(),
            ));
        }
        if !self.payload.is_object() {
            return Err(CoreError::InvalidRelayPacket(
                "payload must be a JSON object".into(),
            ));
        }
        if received_at > self.expires_at || self.created_at > self.expires_at {
            return Err(CoreError::RelayExpired {
                stage: self.stage.code(),
            });
        }

        match previous {
            None => {
                if self.stage != RelayStage::InitRequest
                    || self.sequence != 1
                    || self.previous_message_hash.is_some()
                    || self.transaction_id.is_some()
                {
                    return Err(CoreError::InvalidRelayPacket(
                        "the first packet must be INIT_REQUEST without transaction/hash".into(),
                    ));
                }
            }
            Some(previous) => {
                if self.job_id != previous.job_id {
                    return Err(CoreError::InvalidRelayPacket(
                        "job ID changed inside one relay chain".into(),
                    ));
                }
                if self.target_eid_hash != previous.target_eid_hash {
                    return Err(CoreError::InvalidRelayPacket(
                        "target eUICC changed inside one relay chain".into(),
                    ));
                }
                if self.created_at < previous.created_at {
                    return Err(CoreError::InvalidRelayPacket(
                        "packet timestamp moved backwards".into(),
                    ));
                }
                if previous.stage.next() != Some(self.stage) {
                    return Err(CoreError::InvalidRelayPacket(format!(
                        "{} cannot follow {}",
                        self.stage.code(),
                        previous.stage.code()
                    )));
                }
                if self.sequence != previous.sequence + 1 {
                    return Err(CoreError::InvalidRelayPacket(
                        "packet sequence is not continuous".into(),
                    ));
                }
                let expected_hash = previous.digest()?;
                if self.previous_message_hash.as_deref() != Some(expected_hash.as_str()) {
                    return Err(CoreError::RelayHashMismatch {
                        sequence: self.sequence,
                    });
                }

                if self.stage == RelayStage::ServerAuth {
                    if self.transaction_id.as_deref().is_none_or(str::is_empty) {
                        return Err(CoreError::InvalidRelayPacket(
                            "SERVER_AUTH must introduce transactionId".into(),
                        ));
                    }
                } else if self.transaction_id != previous.transaction_id {
                    return Err(CoreError::InvalidRelayPacket(
                        "transactionId changed inside one RSP session".into(),
                    ));
                }
            }
        }

        Ok(())
    }
}

pub fn encrypt_relay_packet(
    packet: &RelayPacket,
    key: &[u8; 32],
) -> Result<String, CoreError> {
    envelope::encrypt(packet, key, RELAY_PREFIX)
}

pub fn decrypt_relay_packet(value: &str, key: &[u8; 32]) -> Result<RelayPacket, CoreError> {
    envelope::decrypt(value, key, RELAY_PREFIX)
}

pub fn validate_relay_chain(packets: &[RelayPacket]) -> Result<(), CoreError> {
    let mut previous = None;
    for packet in packets {
        packet.validate_after(previous, packet.created_at)?;
        previous = Some(packet);
    }
    Ok(())
}

pub fn demo_relay_session(delay_seconds: i64) -> Result<Vec<RelayPacket>, CoreError> {
    let delay_seconds = delay_seconds.max(0);
    let started_at = Utc::now();
    let expires_at = started_at + TimeDelta::seconds(DEMO_SESSION_TTL_SECONDS);
    let job_id = Uuid::new_v4();
    let transaction_id = format!("DEMO-TXN-{}", &job_id.simple().to_string()[..12]);
    let target_eid_hash = hash_text("DEMO-EID-89049032000000000000000000000001");

    let payloads = [
        json!({
            "euiccChallenge": "DEMO_16_BYTE_RANDOM_CHALLENGE",
            "euiccInfo1": "DEMO_ASN1_EUICC_INFO1",
            "smdpAddress": "rsp.example.test",
            "matchingId": "<encrypted-secret>"
        }),
        json!({
            "transactionId": transaction_id,
            "serverSigned1": "DEMO_SERVER_SIGNED1",
            "serverSignature1": "DEMO_SERVER_SIGNATURE1",
            "certDpAuthSig": "DEMO_CERT_DPAUTH_SIG"
        }),
        json!({
            "euiccSigned1": "DEMO_EUICC_SIGNED1",
            "euiccSignature1": "DEMO_EUICC_SIGNATURE1",
            "certEuiccSig": "DEMO_CERT_EUICC_SIG",
            "certEumSig": "DEMO_CERT_EUM_SIG"
        }),
        json!({
            "profileMetadata": "DEMO_PROFILE_METADATA",
            "smdpSigned2": "DEMO_SMDP_SIGNED2",
            "smdpSignature2": "DEMO_SMDP_SIGNATURE2",
            "certDpPbSig": "DEMO_CERT_DPPB_SIG"
        }),
        json!({
            "euiccSigned2": "DEMO_EUICC_SIGNED2",
            "euiccSignature2": "DEMO_EUICC_SIGNATURE2",
            "euiccOtpk": "DEMO_ONE_TIME_PUBLIC_KEY"
        }),
        json!({
            "boundProfilePackage": "DEMO_BOUND_PROFILE_PACKAGE_BYTES",
            "packageSha256": hash_text("DEMO_BOUND_PROFILE_PACKAGE_BYTES")
        }),
        json!({
            "status": "installed",
            "iccid": "8901000000000000001",
            "notification": "pending"
        }),
    ];

    let mut packets: Vec<RelayPacket> = Vec::with_capacity(RelayStage::ALL.len());
    for (index, stage) in RelayStage::ALL.into_iter().enumerate() {
        let previous_message_hash = packets.last().map(RelayPacket::digest).transpose()?;
        packets.push(RelayPacket {
            version: 1,
            job_id,
            sequence: stage.sequence(),
            stage,
            direction: stage.direction(),
            target_eid_hash: target_eid_hash.clone(),
            transaction_id: (stage != RelayStage::InitRequest).then(|| transaction_id.clone()),
            created_at: started_at
                + TimeDelta::seconds(delay_seconds.saturating_mul(index as i64)),
            expires_at,
            previous_message_hash,
            payload: payloads[index].clone(),
        });
    }

    Ok(packets)
}

fn hash_text(value: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate_transfer_key;

    #[test]
    fn one_minute_store_and_forward_chain_is_valid() {
        let packets = demo_relay_session(60).unwrap();
        validate_relay_chain(&packets).unwrap();
        assert_eq!(packets.len(), 7);
        assert_eq!(packets[1].transaction_id, packets[6].transaction_id);
    }

    #[test]
    fn encrypted_packet_round_trip_hides_payload() {
        let packet = demo_relay_session(60).unwrap().remove(0);
        let key = generate_transfer_key();
        let encoded = encrypt_relay_packet(&packet, &key).unwrap();
        let decoded = decrypt_relay_packet(&encoded, &key).unwrap();

        assert_eq!(decoded, packet);
        assert!(!encoded.contains("euiccChallenge"));
        assert!(!encoded.contains("DEMO_16_BYTE_RANDOM_CHALLENGE"));
    }

    #[test]
    fn rejects_out_of_order_packet() {
        let packets = demo_relay_session(60).unwrap();
        let error = packets[2]
            .validate_after(Some(&packets[0]), packets[2].created_at)
            .unwrap_err();
        assert!(error.to_string().contains("cannot follow"));
    }

    #[test]
    fn rejects_broken_hash_chain() {
        let mut packets = demo_relay_session(60).unwrap();
        packets[3].previous_message_hash = Some("wrong".into());
        assert!(matches!(
            validate_relay_chain(&packets),
            Err(CoreError::RelayHashMismatch { sequence: 4 })
        ));
    }

    #[test]
    fn rejects_session_that_outlives_deadline() {
        let packets = demo_relay_session(120).unwrap();
        assert!(matches!(
            validate_relay_chain(&packets),
            Err(CoreError::RelayExpired { .. })
        ));
    }
}
