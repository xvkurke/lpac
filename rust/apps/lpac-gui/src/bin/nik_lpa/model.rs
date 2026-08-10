use chrono::{DateTime, TimeDelta, Utc};
use lpac_core::{
    CoreError,
    relay::{RelayPacket, RelayStage},
    secure_relay::{DeviceIdentity, PeerIdentity},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroize;

pub const SESSION_LOCAL_TTL_HOURS: i64 = 72;

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
            Self::Local => "Локальний LPA",
            Self::CardAgent => "Card Agent",
            Self::ServerAgent => "Server Agent",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Welcome => "Оберіть, де працює eUICC і де доступний SM-DP+.",
            Self::Local => "PC/SC та Інтернет доступні на одному комп'ютері.",
            Self::CardAgent => "Цей комп'ютер працює з eUICC без доступу до Інтернету.",
            Self::ServerAgent => "Цей комп'ютер працює з SM-DP+ без доступу до рідера.",
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
            Self::Transfer => "Ручна передача",
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
            status: "Початковий пакет eUICC готовий до передачі".into(),
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
            status: "Початковий пакет eUICC прийнято".into(),
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
            note: "Отримано, розшифровано та перевірено".into(),
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

    pub fn set_outgoing(
        &mut self,
        identity: &DeviceIdentity,
        peer: &PeerIdentity,
        packet: &RelayPacket,
    ) -> Result<(), CoreError> {
        self.outgoing_text = identity.encrypt(peer, packet)?;
        self.timeline.push(TimelineEntry {
            stage: packet.stage,
            timestamp: Utc::now(),
            packet_size: self.outgoing_text.len(),
            note: "Створено зашифрований NIKRSP2 пакет".into(),
        });
        Ok(())
    }

    pub fn decode_incoming(
        &self,
        identity: &DeviceIdentity,
        peer: &PeerIdentity,
    ) -> Result<RelayPacket, CoreError> {
        identity.decrypt(peer, self.incoming_text.trim())
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
