use chrono::{DateTime, TimeDelta, Utc};
use lpac_core::{
    CoreError,
    relay::{RelayPacket, RelayStage},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroize;

pub const SESSION_LOCAL_TTL_HOURS: i64 = 72;
pub const DEBUG_PACKET_PREFIX: &str = "NIKRSP-DEBUG1:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Welcome,
    Local,
    CardAgent,
    ServerAgent,
}

impl AppMode {
    pub fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Вибір режиму",
            Self::Local => "Local LPA",
            Self::CardAgent => "Card Agent",
            Self::ServerAgent => "Server Agent",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Welcome => "Оберіть режим роботи",
            Self::Local => "PC/SC + SM-DP+",
            Self::CardAgent => "PC/SC / eUICC",
            Self::ServerAgent => "SM-DP+ / ES9+",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Profiles,
    Install,
    Transfer,
    Sessions,
    Logs,
    Settings,
}

impl Page {
    pub const ALL: [Self; 7] = [
        Self::Dashboard,
        Self::Profiles,
        Self::Install,
        Self::Transfer,
        Self::Sessions,
        Self::Logs,
        Self::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Огляд",
            Self::Profiles => "Профілі",
            Self::Install => "Встановлення",
            Self::Transfer => "RSP Relay",
            Self::Sessions => "Сесії",
            Self::Logs => "Журнал",
            Self::Settings => "Налаштування",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSide {
    Card,
    Server,
}

impl SessionSide {
    pub fn expected_incoming(self, last_stage: Option<RelayStage>) -> Option<RelayStage> {
        match (self, last_stage) {
            (Self::Card, Some(RelayStage::InitRequest)) => Some(RelayStage::ServerAuth),
            (Self::Card, Some(RelayStage::EuiccAuth)) => Some(RelayStage::DownloadPrepare),
            (Self::Card, Some(RelayStage::EuiccPrepared)) => Some(RelayStage::BoundProfilePackage),
            (Self::Card, Some(RelayStage::InstallResult)) => Some(RelayStage::CompleteAck),
            (Self::Server, None) => Some(RelayStage::InitRequest),
            (Self::Server, Some(RelayStage::ServerAuth)) => Some(RelayStage::EuiccAuth),
            (Self::Server, Some(RelayStage::DownloadPrepare)) => Some(RelayStage::EuiccPrepared),
            (Self::Server, Some(RelayStage::BoundProfilePackage)) => {
                Some(RelayStage::InstallResult)
            }
            _ => None,
        }
    }

    pub fn owns_stage(self, stage: RelayStage) -> bool {
        match self {
            Self::Card => matches!(
                stage,
                RelayStage::InitRequest
                    | RelayStage::EuiccAuth
                    | RelayStage::EuiccPrepared
                    | RelayStage::InstallResult
            ),
            Self::Server => matches!(
                stage,
                RelayStage::ServerAuth
                    | RelayStage::DownloadPrepare
                    | RelayStage::BoundProfilePackage
                    | RelayStage::CompleteAck
            ),
        }
    }

    pub fn peer_label(self) -> &'static str {
        match self {
            Self::Card => "Server Agent",
            Self::Server => "Card Agent",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimelineEntry {
    pub stage: RelayStage,
    pub timestamp: DateTime<Utc>,
    pub packet_size: usize,
    pub note: String,
}

pub struct RelaySession {
    pub side: SessionSide,
    pub job_id: Uuid,
    pub target_eid_hash: String,
    pub deadline: DateTime<Utc>,
    pub packets: Vec<RelayPacket>,
    pub timeline: Vec<TimelineEntry>,
    pub incoming_text: String,
    pub outgoing_text: String,
    pub status: String,
    pub completed: bool,
}

impl RelaySession {
    pub fn new_card(target_eid_hash: String, payload: Value) -> Result<Self, CoreError> {
        let now = Utc::now();
        let deadline = now + TimeDelta::hours(SESSION_LOCAL_TTL_HOURS);
        let packet = RelayPacket {
            version: 1,
            job_id: Uuid::new_v4(),
            sequence: RelayStage::InitRequest.sequence(),
            stage: RelayStage::InitRequest,
            direction: RelayStage::InitRequest.direction(),
            target_eid_hash: target_eid_hash.clone(),
            transaction_id: None,
            created_at: now,
            expires_at: deadline,
            previous_message_hash: None,
            payload,
        };
        packet.validate_after(None, now)?;
        Ok(Self {
            side: SessionSide::Card,
            job_id: packet.job_id,
            target_eid_hash,
            deadline,
            packets: vec![packet],
            timeline: Vec::new(),
            incoming_text: String::new(),
            outgoing_text: String::new(),
            status: "INIT_REQUEST готовий".into(),
            completed: false,
        })
    }

    pub fn new_server(initial: RelayPacket) -> Result<Self, CoreError> {
        initial.validate_after(None, Utc::now())?;
        Ok(Self {
            side: SessionSide::Server,
            job_id: initial.job_id,
            target_eid_hash: initial.target_eid_hash.clone(),
            deadline: initial.expires_at,
            packets: vec![initial],
            timeline: Vec::new(),
            incoming_text: String::new(),
            outgoing_text: String::new(),
            status: "INIT_REQUEST прийнято".into(),
            completed: false,
        })
    }

    pub fn last_packet(&self) -> Option<&RelayPacket> {
        self.packets.last()
    }

    pub fn expected_incoming(&self) -> Option<RelayStage> {
        self.side
            .expected_incoming(self.last_packet().map(|packet| packet.stage))
    }

    pub fn accept_incoming(
        &mut self,
        packet: RelayPacket,
        encoded_size: usize,
    ) -> Result<(), CoreError> {
        let expected = self.expected_incoming().ok_or_else(|| {
            CoreError::InvalidRelayPacket("this side is not waiting for another packet".into())
        })?;
        if packet.stage != expected {
            return Err(CoreError::InvalidRelayPacket(format!(
                "expected {}, received {}",
                expected.code(),
                packet.stage.code()
            )));
        }
        packet.validate_after(self.last_packet(), Utc::now())?;
        self.timeline.push(TimelineEntry {
            stage: packet.stage,
            timestamp: Utc::now(),
            packet_size: encoded_size,
            note: "Прийнято та перевірено".into(),
        });
        self.packets.push(packet);
        self.incoming_text.zeroize();
        Ok(())
    }

    pub fn create_outgoing(
        &mut self,
        stage: RelayStage,
        transaction_id: Option<String>,
        payload: Value,
    ) -> Result<RelayPacket, CoreError> {
        if !self.side.owns_stage(stage) {
            return Err(CoreError::InvalidRelayPacket(format!(
                "{} cannot create {}",
                match self.side {
                    SessionSide::Card => "Card Agent",
                    SessionSide::Server => "Server Agent",
                },
                stage.code()
            )));
        }
        let previous = self.last_packet().ok_or_else(|| {
            CoreError::InvalidRelayPacket("relay session has no previous packet".into())
        })?;
        if previous.stage.next() != Some(stage) {
            return Err(CoreError::InvalidRelayPacket(format!(
                "{} cannot follow {}",
                stage.code(),
                previous.stage.code()
            )));
        }
        let transaction_id = if stage == RelayStage::ServerAuth {
            transaction_id
        } else {
            previous.transaction_id.clone()
        };
        let packet = RelayPacket {
            version: 1,
            job_id: self.job_id,
            sequence: stage.sequence(),
            stage,
            direction: stage.direction(),
            target_eid_hash: self.target_eid_hash.clone(),
            transaction_id,
            created_at: Utc::now(),
            expires_at: self.deadline,
            previous_message_hash: Some(previous.digest()?),
            payload,
        };
        packet.validate_after(Some(previous), Utc::now())?;
        self.packets.push(packet.clone());
        Ok(packet)
    }

    pub fn set_outgoing(&mut self, packet: &RelayPacket) -> Result<(), CoreError> {
        let json = serde_json::to_string(packet)?;
        self.outgoing_text = format!("{DEBUG_PACKET_PREFIX}{json}");
        self.timeline.push(TimelineEntry {
            stage: packet.stage,
            timestamp: Utc::now(),
            packet_size: self.outgoing_text.len(),
            note: "Створено plaintext debug packet".into(),
        });
        Ok(())
    }

    pub fn decode_incoming(&self) -> Result<RelayPacket, CoreError> {
        decode_debug_packet(self.incoming_text.trim())
    }
}

pub fn decode_debug_packet(text: &str) -> Result<RelayPacket, CoreError> {
    let json = text.strip_prefix(DEBUG_PACKET_PREFIX).unwrap_or(text);
    Ok(serde_json::from_str(json)?)
}

pub fn pretty_debug_packet(text: &str) -> String {
    match decode_debug_packet(text.trim()) {
        Ok(packet) => serde_json::to_string_pretty(&packet).unwrap_or_else(|_| text.to_owned()),
        Err(_) => text.to_owned(),
    }
}

impl Drop for RelaySession {
    fn drop(&mut self) {
        self.incoming_text.zeroize();
        self.outgoing_text.zeroize();
        for packet in &mut self.packets {
            if let Some(object) = packet.payload.as_object_mut() {
                for value in object.values_mut() {
                    if let Some(secret) = value.as_str() {
                        let mut owned = secret.to_owned();
                        owned.zeroize();
                    }
                }
            }
        }
    }
}

pub fn eid_hash(eid: &str) -> String {
    format!("{:x}", Sha256::digest(eid.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plaintext_debug_packet_round_trips() {
        let mut session = RelaySession::new_card("eid-hash".into(), json!({
            "euiccChallenge": "visible",
            "euiccInfo1": "visible-too"
        }))
        .unwrap();
        let packet = session.last_packet().cloned().unwrap();
        session.set_outgoing(&packet).unwrap();
        assert!(session.outgoing_text.starts_with(DEBUG_PACKET_PREFIX));
        assert!(session.outgoing_text.contains("euiccChallenge"));
        let decoded = decode_debug_packet(&session.outgoing_text).unwrap();
        assert_eq!(decoded, packet);
    }
}
