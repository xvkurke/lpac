use std::path::PathBuf;

use chrono::Utc;
use eframe::egui;
use lpac_backend::{LpacRun, PcscReader};
use lpac_core::{
    ActivationCode,
    relay::{RelayPacket, RelayStage},
    secure_relay::PeerIdentity,
};
use lpac_relay_backend::RelayAgentRole;
use serde_json::{Value, json};
use uuid::Uuid;
use zeroize::Zeroize;

use crate::{
    app::{NikLpaApp, PendingRelay, PendingRelayAction},
    controller::{WorkerCommand, WorkerEvent},
    model::{AppMode, Page, RelaySession, TimelineEntry, eid_hash},
};

impl NikLpaApp {
    pub(crate) fn add_log(&mut self, message: impl Into<String>) {
        if self.log.len() >= 300 {
            self.log.pop_front();
        }
        self.log.push_back(format!(
            "{}  {}",
            Utc::now().format("%H:%M:%S"),
            message.into()
        ));
    }

    pub(crate) fn send(&mut self, command: WorkerCommand, label: &str) {
        match self.worker.send(command) {
            Ok(()) => {
                self.busy = true;
                self.add_log(label);
            }
            Err(error) => self.add_log(format!("ПОМИЛКА: {error}")),
        }
    }

    fn relay_request(&mut self, operation: &str, payload: Value, action: PendingRelayAction) {
        let token = Uuid::new_v4();
        self.pending_relay = Some(PendingRelay { token, action });
        self.send(
            WorkerCommand::RelayRequest {
                token,
                operation: operation.to_owned(),
                payload,
            },
            &format!("Виконується {operation}"),
        );
    }

    pub(crate) fn poll_worker(&mut self, context: &egui::Context) {
        loop {
            match self.worker.try_recv() {
                Ok(Some(event)) => {
                    self.busy = false;
                    self.handle_worker_event(event);
                    context.request_repaint();
                }
                Ok(None) => break,
                Err(error) => {
                    self.busy = false;
                    self.add_log(format!("ПОМИЛКА: {error}"));
                    break;
                }
            }
        }
    }

    fn handle_worker_event(&mut self, event: WorkerEvent) {
        match event {
            WorkerEvent::Readers(result) => self.handle_readers(result),
            WorkerEvent::ChipInfo(result) => self.handle_chip_info(result),
            WorkerEvent::Profiles(result) => self.handle_profiles(result),
            WorkerEvent::LocalDownload(result) => self.handle_local_download(result),
            WorkerEvent::RelayStarted(result) => self.handle_relay_started(result),
            WorkerEvent::RelayResponse {
                token,
                operation,
                result,
            } => self.handle_relay_response(token, &operation, result),
            WorkerEvent::RelayStopped => self.handle_relay_stopped(),
        }
    }

    fn handle_readers(&mut self, result: Result<Vec<PcscReader>, String>) {
        match result {
            Ok(readers) => {
                if let Some(first) = readers.first()
                    && !readers
                        .iter()
                        .any(|reader| reader.index == self.selected_reader)
                {
                    self.selected_reader = first.index;
                }
                self.add_log(format!("Знайдено PC/SC-рідерів: {}", readers.len()));
                self.readers = readers;
            }
            Err(error) => self.add_log(format!("ПОМИЛКА пошуку рідерів: {error}")),
        }
    }

    fn handle_chip_info(&mut self, result: Result<LpacRun, String>) {
        match result {
            Ok(run) => {
                self.chip_info_output = run.pretty_log();
                self.card_eid = run
                    .result
                    .data
                    .get("eidValue")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                self.add_log("Інформацію eUICC прочитано");

                if self.start_card_after_chip_info {
                    self.start_card_after_chip_info = false;
                    if self.card_eid.is_some() {
                        self.start_relay(RelayAgentRole::Card);
                    } else {
                        self.add_log("ПОМИЛКА: відповідь eUICC не містить EID");
                    }
                }
            }
            Err(error) => {
                self.start_card_after_chip_info = false;
                self.add_log(format!("ПОМИЛКА читання eUICC: {error}"));
            }
        }
    }

    fn handle_profiles(&mut self, result: Result<LpacRun, String>) {
        match result {
            Ok(run) => {
                self.profiles_output = run.pretty_log();
                self.verify_installed_profile(&run);
                self.add_log("Список профілів оновлено");
            }
            Err(error) => self.add_log(format!("ПОМИЛКА читання профілів: {error}")),
        }
    }

    fn handle_local_download(&mut self, result: Result<LpacRun, String>) {
        match result {
            Ok(run) => {
                self.profiles_output = run.pretty_log();
                self.add_log("Профіль локально встановлено та перевірено");
            }
            Err(error) => self.add_log(format!("ПОМИЛКА локального встановлення: {error}")),
        }
    }

    fn handle_relay_started(&mut self, result: Result<RelayAgentRole, String>) {
        match result {
            Ok(role) => {
                self.relay_running = true;
                self.relay_role = Some(role);
                self.add_log(match role {
                    RelayAgentRole::Card => {
                        "Card Agent запущено; PC/SC-сеанс утримується відкритим"
                    }
                    RelayAgentRole::Server => "Server Agent запущено; WinHTTP готовий до SM-DP+",
                });
                if role == RelayAgentRole::Card {
                    self.relay_request("card.init", json!({}), PendingRelayAction::CardInit);
                }
            }
            Err(error) => {
                self.relay_running = false;
                self.relay_role = None;
                self.add_log(format!("ПОМИЛКА запуску Relay Agent: {error}"));
            }
        }
    }

    fn handle_relay_response(
        &mut self,
        token: Uuid,
        operation: &str,
        result: Result<Value, String>,
    ) {
        let action = self
            .pending_relay
            .take()
            .filter(|pending| pending.token == token)
            .map(|pending| pending.action);

        match (action, result) {
            (Some(action), Ok(payload)) => {
                self.add_log(format!("{operation} завершено"));
                if let Err(error) = self.apply_relay_response(action, payload) {
                    self.add_log(format!("ПОМИЛКА обробки відповіді: {error}"));
                }
            }
            (Some(_), Err(error)) => self.add_log(format!("ПОМИЛКА {operation}: {error}")),
            (None, _) => self.add_log("Отримано відповідь невідомої relay-операції"),
        }
    }

    fn handle_relay_stopped(&mut self) {
        self.relay_running = false;
        self.relay_role = None;
        self.add_log("Relay Agent зупинено");
        if self.verify_after_relay_stop.is_some() {
            self.send(
                WorkerCommand::Profiles {
                    executable: PathBuf::from(&self.lpac_path),
                    reader_index: self.selected_reader,
                },
                "Перевіряється встановлений ICCID через свіжий Profile List",
            );
        }
    }

    fn apply_relay_response(
        &mut self,
        action: PendingRelayAction,
        payload: Value,
    ) -> Result<(), String> {
        let peer = self.peer.ok_or_else(|| "Peer не спарений".to_owned())?;

        match action {
            PendingRelayAction::CardInit => {
                let eid = self
                    .card_eid
                    .as_deref()
                    .ok_or_else(|| "EID не прочитаний".to_owned())?;
                let mut session = RelaySession::new_card(eid_hash(eid), payload)
                    .map_err(|error| error.to_string())?;
                let packet = session
                    .last_packet()
                    .cloned()
                    .ok_or_else(|| "INIT_REQUEST не створений".to_owned())?;
                session
                    .set_outgoing(&self.identity, &peer, &packet)
                    .map_err(|error| error.to_string())?;
                self.session = Some(session);
                self.page = Page::Transfer;
            }
            PendingRelayAction::CardAuthenticateServer => {
                self.create_and_encrypt_outgoing(RelayStage::EuiccAuth, None, payload)?;
            }
            PendingRelayAction::CardPrepareDownload => {
                self.create_and_encrypt_outgoing(RelayStage::EuiccPrepared, None, payload)?;
            }
            PendingRelayAction::CardInstallBpp => {
                let expected_iccid = payload
                    .get("iccid")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "LoadBoundProfilePackage не повернув ICCID".to_owned())?
                    .to_owned();
                self.pending_install_payload = Some(json!({
                    "status": "installed",
                    "iccid": expected_iccid,
                    "sequenceNumber": payload
                        .get("sequenceNumber")
                        .cloned()
                        .unwrap_or(Value::Null),
                    "notification": "pending"
                }));
                self.verify_after_relay_stop = Some(expected_iccid);
                if let Some(session) = self.session.as_mut() {
                    session.status =
                        "BPP завантажено; очікується незалежна перевірка Profile List".into();
                }
                self.send(
                    WorkerCommand::StopRelay,
                    "Закривається PC/SC relay-сеанс перед перевіркою",
                );
            }
            PendingRelayAction::ServerInitiateAuthentication => {
                let transaction_id = payload
                    .get("transactionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "SM-DP+ не повернув transactionId".to_owned())?
                    .to_owned();
                let code = ActivationCode::parse(self.activation_code.clone())
                    .map_err(|error| error.to_string())?;
                let mut outgoing_payload = payload;
                let object = outgoing_payload
                    .as_object_mut()
                    .ok_or_else(|| "Некоректна відповідь InitiateAuthentication".to_owned())?;
                object.insert("matchingId".into(), Value::String(code.matching_id));
                object.insert("serverAddress".into(), Value::String(code.smdp));
                self.create_and_encrypt_outgoing(
                    RelayStage::ServerAuth,
                    Some(transaction_id),
                    outgoing_payload,
                )?;
            }
            PendingRelayAction::ServerAuthenticateClient => {
                let mut outgoing_payload = payload;
                if !self.confirmation_code.is_empty() {
                    outgoing_payload
                        .as_object_mut()
                        .ok_or_else(|| "Некоректна відповідь AuthenticateClient".to_owned())?
                        .insert(
                            "confirmationCode".into(),
                            Value::String(self.confirmation_code.clone()),
                        );
                }
                self.create_and_encrypt_outgoing(
                    RelayStage::DownloadPrepare,
                    None,
                    outgoing_payload,
                )?;
            }
            PendingRelayAction::ServerGetBpp => {
                self.create_and_encrypt_outgoing(RelayStage::BoundProfilePackage, None, payload)?;
            }
        }
        Ok(())
    }

    fn create_and_encrypt_outgoing(
        &mut self,
        stage: RelayStage,
        transaction_id: Option<String>,
        payload: Value,
    ) -> Result<(), String> {
        let peer = self.peer.ok_or_else(|| "Peer не спарений".to_owned())?;
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| "Relay-сесія не створена".to_owned())?;
        let packet = session
            .create_outgoing(stage, transaction_id, payload)
            .map_err(|error| error.to_string())?;
        session
            .set_outgoing(&self.identity, &peer, &packet)
            .map_err(|error| error.to_string())?;
        session.status = format!("{} готовий до передачі", stage.code());
        if stage == RelayStage::InstallResult {
            session.completed = true;
        }
        Ok(())
    }

    fn start_relay(&mut self, role: RelayAgentRole) {
        let reader_index = (role == RelayAgentRole::Card).then_some(self.selected_reader);
        self.send(
            WorkerCommand::StartRelay {
                executable: PathBuf::from(&self.lpac_path),
                role,
                reader_index,
            },
            match role {
                RelayAgentRole::Card => "Запускається Card Agent",
                RelayAgentRole::Server => "Запускається Server Agent",
            },
        );
    }

    pub(crate) fn start_card_session(&mut self) {
        if self.peer.is_none() {
            self.add_log("ПОМИЛКА: спочатку імпортуйте pairing code Server Agent");
            return;
        }
        self.session = None;
        self.pending_install_payload = None;
        self.verify_after_relay_stop = None;
        self.start_card_after_chip_info = true;
        self.send(
            WorkerCommand::ChipInfo {
                executable: PathBuf::from(&self.lpac_path),
                reader_index: self.selected_reader,
            },
            "Читається EID перед створенням Card Agent сесії",
        );
    }

    pub(crate) fn start_server_session(&mut self) {
        if self.peer.is_none() {
            self.add_log("ПОМИЛКА: спочатку імпортуйте pairing code Card Agent");
            return;
        }
        if let Err(error) = ActivationCode::parse(self.activation_code.clone()) {
            self.add_log(format!("ПОМИЛКА activation code: {error}"));
            return;
        }
        self.session = None;
        self.server_bootstrap_input.zeroize();
        self.start_relay(RelayAgentRole::Server);
        self.page = Page::Transfer;
    }

    pub(crate) fn pair_peer(&mut self) {
        match PeerIdentity::from_pairing_code(&self.peer_code_input) {
            Ok(peer) => {
                self.peer = Some(peer);
                self.add_log("Peer pairing code прийнято");
            }
            Err(error) => self.add_log(format!("ПОМИЛКА pairing: {error}")),
        }
    }

    pub(crate) fn accept_server_bootstrap(&mut self) {
        let Some(peer) = self.peer else {
            self.add_log("ПОМИЛКА: peer не спарений");
            return;
        };
        let text = std::mem::take(&mut self.server_bootstrap_input);
        let packet = match self.identity.decrypt::<RelayPacket>(&peer, text.trim()) {
            Ok(packet) => packet,
            Err(error) => {
                self.add_log(format!("ПОМИЛКА розшифрування INIT_REQUEST: {error}"));
                return;
            }
        };
        if packet.stage != RelayStage::InitRequest {
            self.add_log(format!(
                "ПОМИЛКА: очікувався INIT_REQUEST, отримано {}",
                packet.stage.code()
            ));
            return;
        }

        match RelaySession::new_server(packet) {
            Ok(mut session) => {
                session.timeline.push(TimelineEntry {
                    stage: RelayStage::InitRequest,
                    timestamp: Utc::now(),
                    packet_size: text.trim().len(),
                    note: "Отримано, розшифровано та перевірено".into(),
                });
                self.session = Some(session);
                self.process_server_init_request();
            }
            Err(error) => self.add_log(format!("ПОМИЛКА INIT_REQUEST: {error}")),
        }
    }

    pub(crate) fn import_session_incoming(&mut self) {
        let Some(peer) = self.peer else {
            self.add_log("ПОМИЛКА: peer не спарений");
            return;
        };
        let encoded_size = match self.session.as_ref() {
            Some(session) => session.incoming_text.trim().len(),
            None => {
                self.add_log("ПОМИЛКА: сесія ще не створена");
                return;
            }
        };
        let packet = match self
            .session
            .as_ref()
            .expect("session checked above")
            .decode_incoming(&self.identity, &peer)
        {
            Ok(packet) => packet,
            Err(error) => {
                self.add_log(format!("ПОМИЛКА розшифрування пакета: {error}"));
                return;
            }
        };
        let stage = packet.stage;
        if let Some(session) = self.session.as_mut()
            && let Err(error) = session.accept_incoming(packet, encoded_size)
        {
            self.add_log(format!("ПОМИЛКА перевірки ланцюжка: {error}"));
            return;
        }
        self.process_received_stage(stage);
    }

    fn process_server_init_request(&mut self) {
        let packet = match self.session.as_ref().and_then(RelaySession::last_packet) {
            Some(packet) => packet.clone(),
            None => return,
        };
        let code = match ActivationCode::parse(self.activation_code.clone()) {
            Ok(code) => code,
            Err(error) => {
                self.add_log(format!("ПОМИЛКА activation code: {error}"));
                return;
            }
        };
        self.relay_request(
            "server.initiateAuthentication",
            json!({
                "serverAddress": code.smdp,
                "euiccChallenge": packet
                    .payload
                    .get("euiccChallenge")
                    .cloned()
                    .unwrap_or(Value::Null),
                "euiccInfo1": packet
                    .payload
                    .get("euiccInfo1")
                    .cloned()
                    .unwrap_or(Value::Null)
            }),
            PendingRelayAction::ServerInitiateAuthentication,
        );
    }

    fn process_received_stage(&mut self, stage: RelayStage) {
        let packet = match self.session.as_ref().and_then(RelaySession::last_packet) {
            Some(packet) => packet.clone(),
            None => return,
        };
        match (self.mode, stage) {
            (AppMode::CardAgent, RelayStage::ServerAuth) => self.relay_request(
                "card.authenticateServer",
                packet.payload,
                PendingRelayAction::CardAuthenticateServer,
            ),
            (AppMode::CardAgent, RelayStage::DownloadPrepare) => self.relay_request(
                "card.prepareDownload",
                packet.payload,
                PendingRelayAction::CardPrepareDownload,
            ),
            (AppMode::CardAgent, RelayStage::BoundProfilePackage) => self.relay_request(
                "card.installBpp",
                packet.payload,
                PendingRelayAction::CardInstallBpp,
            ),
            (AppMode::ServerAgent, RelayStage::EuiccAuth) => {
                let code = match ActivationCode::parse(self.activation_code.clone()) {
                    Ok(code) => code,
                    Err(error) => {
                        self.add_log(format!("ПОМИЛКА activation code: {error}"));
                        return;
                    }
                };
                self.relay_request(
                    "server.authenticateClient",
                    json!({
                        "serverAddress": code.smdp,
                        "transactionId": packet.transaction_id,
                        "authenticateServerResponse": packet
                            .payload
                            .get("authenticateServerResponse")
                            .cloned()
                            .unwrap_or(Value::Null)
                    }),
                    PendingRelayAction::ServerAuthenticateClient,
                );
            }
            (AppMode::ServerAgent, RelayStage::EuiccPrepared) => {
                let code = match ActivationCode::parse(self.activation_code.clone()) {
                    Ok(code) => code,
                    Err(error) => {
                        self.add_log(format!("ПОМИЛКА activation code: {error}"));
                        return;
                    }
                };
                self.relay_request(
                    "server.getBpp",
                    json!({
                        "serverAddress": code.smdp,
                        "transactionId": packet.transaction_id,
                        "prepareDownloadResponse": packet
                            .payload
                            .get("prepareDownloadResponse")
                            .cloned()
                            .unwrap_or(Value::Null)
                    }),
                    PendingRelayAction::ServerGetBpp,
                );
            }
            (AppMode::ServerAgent, RelayStage::InstallResult) => {
                if let Some(session) = self.session.as_mut() {
                    session.completed = true;
                    session.status = "Card Agent підтвердив встановлення та Profile List".into();
                }
                self.add_log("Relay download завершено: отримано перевірений INSTALL_RESULT");
            }
            _ => self.add_log(format!(
                "Неочікувана стадія для поточного режиму: {}",
                stage.code()
            )),
        }
    }

    fn verify_installed_profile(&mut self, run: &LpacRun) {
        let Some(expected) = self.verify_after_relay_stop.take() else {
            return;
        };
        let found = run.result.data.as_array().is_some_and(|profiles| {
            profiles.iter().any(|profile| {
                profile
                    .get("iccid")
                    .and_then(Value::as_str)
                    .is_some_and(|iccid| iccid == expected)
            })
        });

        if !found {
            self.pending_install_payload = None;
            if let Some(session) = self.session.as_mut() {
                session.status = format!("ПОМИЛКА: ICCID {expected} відсутній у Profile List");
                session.completed = false;
            }
            self.add_log(format!(
                "ПОМИЛКА: ICCID {expected} не знайдено після встановлення"
            ));
            return;
        }

        let payload = match self.pending_install_payload.take() {
            Some(payload) => payload,
            None => {
                self.add_log("ПОМИЛКА: втрачено результат LoadBoundProfilePackage");
                return;
            }
        };
        if let Err(error) =
            self.create_and_encrypt_outgoing(RelayStage::InstallResult, None, payload)
        {
            self.add_log(format!("ПОМИЛКА створення INSTALL_RESULT: {error}"));
            return;
        }
        if let Some(session) = self.session.as_mut() {
            session.status = format!("ICCID {expected} підтверджено свіжим Profile List");
            session.completed = true;
        }
        self.add_log(format!("ICCID {expected} успішно перевірено"));
    }

    pub(crate) fn reset_mode(&mut self, mode: AppMode) {
        if self.relay_running {
            let _ = self.worker.send(WorkerCommand::StopRelay);
        }
        self.mode = mode;
        self.page = Page::Dashboard;
        self.session = None;
        self.pending_relay = None;
        self.pending_install_payload = None;
        self.verify_after_relay_stop = None;
        self.relay_running = false;
        self.relay_role = None;
        self.server_bootstrap_input.zeroize();
    }

    pub(crate) fn stop_relay(&mut self) {
        if self.relay_running {
            self.send(WorkerCommand::StopRelay, "Зупиняється Relay Agent");
        }
    }
}
