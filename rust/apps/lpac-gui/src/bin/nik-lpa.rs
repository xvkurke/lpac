#[path = "nik_lpa/controller.rs"]
mod controller;
#[path = "nik_lpa/model.rs"]
mod model;

use std::{
    collections::VecDeque,
    env,
    path::PathBuf,
    time::Duration,
};

use chrono::Utc;
use controller::{WorkerCommand, WorkerController, WorkerEvent};
use eframe::egui;
use lpac_backend::{LpacRun, PcscReader};
use lpac_core::{
    ActivationCode,
    relay::{RelayPacket, RelayStage},
    secure_relay::{DeviceIdentity, PeerIdentity},
};
use lpac_relay_backend::RelayAgentRole;
use model::{AppMode, Page, RelaySession, SessionSide, eid_hash};
use serde_json::{Value, json};
use uuid::Uuid;
use zeroize::Zeroize;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1_300.0, 860.0])
            .with_min_inner_size([980.0, 680.0]),
        ..Default::default()
    };
    eframe::run_native(
        "NIK LPA",
        options,
        Box::new(|creation_context| {
            configure_theme(&creation_context.egui_ctx, false);
            Ok(Box::new(NikLpaApp::default()))
        }),
    )
}

fn configure_theme(context: &egui::Context, dark: bool) {
    if dark {
        context.set_theme(egui::Theme::Dark);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(10, 15, 25);
        visuals.window_fill = egui::Color32::from_rgb(15, 23, 42);
        visuals.extreme_bg_color = egui::Color32::from_rgb(8, 12, 20);
        visuals.selection.bg_fill = egui::Color32::from_rgb(37, 99, 235);
        context.set_visuals_of(egui::Theme::Dark, visuals);
    } else {
        context.set_theme(egui::Theme::Light);
        let mut visuals = egui::Visuals::light();
        visuals.override_text_color = Some(egui::Color32::from_rgb(30, 41, 59));
        visuals.panel_fill = egui::Color32::from_rgb(243, 246, 250);
        visuals.window_fill = egui::Color32::WHITE;
        visuals.extreme_bg_color = egui::Color32::WHITE;
        visuals.faint_bg_color = egui::Color32::from_rgb(241, 245, 249);
        visuals.selection.bg_fill = egui::Color32::from_rgb(37, 99, 235);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(226, 232, 240);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(219, 234, 254);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(37, 99, 235);
        context.set_visuals_of(egui::Theme::Light, visuals);
    }

    let mut style = (*context.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 9.0);
    style.spacing.button_padding = egui::vec2(15.0, 9.0);
    style.spacing.interact_size.y = 38.0;
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
    context.set_style(style);
}

fn bundled_lpac_path() -> String {
    let name = if cfg!(windows) { "lpac.exe" } else { "lpac" };
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join(name)))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
        .display()
        .to_string()
}

#[derive(Debug, Clone, Copy)]
enum PendingRelayAction {
    CardInit,
    CardAuthenticateServer,
    CardPrepareDownload,
    CardInstallBpp,
    ServerInitiateAuthentication,
    ServerAuthenticateClient,
    ServerGetBpp,
}

struct PendingRelay {
    token: Uuid,
    action: PendingRelayAction,
}

struct NikLpaApp {
    worker: WorkerController,
    mode: AppMode,
    page: Page,
    lpac_path: String,
    readers: Vec<PcscReader>,
    selected_reader: u32,
    identity: DeviceIdentity,
    peer_code_input: String,
    peer: Option<PeerIdentity>,
    relay_running: bool,
    relay_role: Option<RelayAgentRole>,
    session: Option<RelaySession>,
    pending_relay: Option<PendingRelay>,
    busy: bool,
    start_card_after_chip_info: bool,
    verify_after_relay_stop: Option<String>,
    card_eid: Option<String>,
    activation_code: String,
    confirmation_code: String,
    local_activation_code: String,
    local_confirmation_code: String,
    chip_info_output: String,
    profiles_output: String,
    log: VecDeque<String>,
    dark_mode: bool,
}

impl Default for NikLpaApp {
    fn default() -> Self {
        Self {
            worker: WorkerController::spawn(),
            mode: AppMode::Welcome,
            page: Page::Dashboard,
            lpac_path: bundled_lpac_path(),
            readers: Vec::new(),
            selected_reader: 0,
            identity: DeviceIdentity::generate(),
            peer_code_input: String::new(),
            peer: None,
            relay_running: false,
            relay_role: None,
            session: None,
            pending_relay: None,
            busy: false,
            start_card_after_chip_info: false,
            verify_after_relay_stop: None,
            card_eid: None,
            activation_code: String::new(),
            confirmation_code: String::new(),
            local_activation_code: String::new(),
            local_confirmation_code: String::new(),
            chip_info_output: String::new(),
            profiles_output: String::new(),
            log: VecDeque::new(),
            dark_mode: false,
        }
    }
}

impl Drop for NikLpaApp {
    fn drop(&mut self) {
        self.activation_code.zeroize();
        self.confirmation_code.zeroize();
        self.local_activation_code.zeroize();
        self.local_confirmation_code.zeroize();
        self.peer_code_input.zeroize();
    }
}

impl NikLpaApp {
    fn add_log(&mut self, message: impl Into<String>) {
        if self.log.len() >= 300 {
            self.log.pop_front();
        }
        self.log
            .push_back(format!("{}  {}", Utc::now().format("%H:%M:%S"), message.into()));
    }

    fn send(&mut self, command: WorkerCommand, label: &str) {
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

    fn poll_worker(&mut self, context: &egui::Context) {
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
            WorkerEvent::Readers(result) => match result {
                Ok(readers) => {
                    if let Some(first) = readers.first()
                        && !readers.iter().any(|reader| reader.index == self.selected_reader)
                    {
                        self.selected_reader = first.index;
                    }
                    self.add_log(format!("Знайдено PC/SC-рідерів: {}", readers.len()));
                    self.readers = readers;
                }
                Err(error) => self.add_log(format!("ПОМИЛКА пошуку рідерів: {error}")),
            },
            WorkerEvent::ChipInfo(result) => match result {
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
                        if self.card_eid.is_none() {
                            self.add_log("ПОМИЛКА: відповідь eUICC не містить EID");
                        } else {
                            self.start_relay(RelayAgentRole::Card);
                        }
                    }
                }
                Err(error) => {
                    self.start_card_after_chip_info = false;
                    self.add_log(format!("ПОМИЛКА читання eUICC: {error}"));
                }
            },
            WorkerEvent::Profiles(result) => match result {
                Ok(run) => {
                    self.profiles_output = run.pretty_log();
                    self.verify_installed_profile(&run);
                    self.add_log("Список профілів оновлено");
                }
                Err(error) => self.add_log(format!("ПОМИЛКА читання профілів: {error}")),
            },
            WorkerEvent::LocalDownload(result) => match result {
                Ok(run) => {
                    self.profiles_output = run.pretty_log();
                    self.add_log("Профіль локально встановлено та перевірено");
                }
                Err(error) => self.add_log(format!("ПОМИЛКА локального встановлення: {error}")),
            },
            WorkerEvent::RelayStarted(result) => match result {
                Ok(role) => {
                    self.relay_running = true;
                    self.relay_role = Some(role);
                    self.add_log(match role {
                        RelayAgentRole::Card => "Card Agent запущено; PC/SC-сеанс утримується відкритим",
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
            },
            WorkerEvent::RelayResponse {
                token,
                operation,
                result,
            } => {
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
                    (Some(_), Err(error)) => {
                        self.add_log(format!("ПОМИЛКА {operation}: {error}"));
                    }
                    (None, _) => self.add_log("Отримано відповідь невідомої relay-операції"),
                }
            }
            WorkerEvent::RelayStopped => {
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
        }
    }

    fn apply_relay_response(&mut self, action: PendingRelayAction, payload: Value) -> Result<(), String> {
        let peer = self.peer.ok_or_else(|| "Peer не спарений".to_owned())?;
        match action {
            PendingRelayAction::CardInit => {
                let eid = self.card_eid.as_deref().ok_or_else(|| "EID не прочитаний".to_owned())?;
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
                let install_payload = json!({
                    "status": "installed",
                    "iccid": expected_iccid,
                    "sequenceNumber": payload.get("sequenceNumber").cloned().unwrap_or(Value::Null),
                    "notification": "pending"
                });
                self.create_and_encrypt_outgoing(RelayStage::InstallResult, None, install_payload)?;
                if let Some(session) = self.session.as_mut() {
                    session.status = "Профіль записано; виконується незалежна перевірка Profile List".into();
                }
                self.verify_after_relay_stop = Some(expected_iccid);
                self.send(WorkerCommand::StopRelay, "Закривається PC/SC relay-сеанс");
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
                    .ok_or_else(|| "Некоректна відповідь server.initiateAuthentication".to_owned())?;
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
                        .ok_or_else(|| "Некоректна відповідь server.authenticateClient".to_owned())?
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

    fn start_card_session(&mut self) {
        if self.peer.is_none() {
            self.add_log("ПОМИЛКА: спочатку імпортуйте pairing code Server Agent");
            return;
        }
        self.session = None;
        self.start_card_after_chip_info = true;
        self.send(
            WorkerCommand::ChipInfo {
                executable: PathBuf::from(&self.lpac_path),
                reader_index: self.selected_reader,
            },
            "Читається EID перед створенням Card Agent сесії",
        );
    }

    fn start_server_session(&mut self) {
        if self.peer.is_none() {
            self.add_log("ПОМИЛКА: спочатку імпортуйте pairing code Card Agent");
            return;
        }
        if let Err(error) = ActivationCode::parse(self.activation_code.clone()) {
            self.add_log(format!("ПОМИЛКА activation code: {error}"));
            return;
        }
        self.session = None;
        self.start_relay(RelayAgentRole::Server);
        self.page = Page::Transfer;
    }

    fn import_incoming(&mut self) {
        let Some(peer) = self.peer else {
            self.add_log("ПОМИЛКА: peer не спарений");
            return;
        };
        let encoded_size = self
            .session
            .as_ref()
            .map(|session| session.incoming_text.trim().len())
            .unwrap_or(0);

        if self.mode == AppMode::ServerAgent && self.session.is_none() {
            let incoming_text = self
                .pending_server_incoming_text()
                .unwrap_or_default();
            let packet: RelayPacket = match self.identity.decrypt(&peer, incoming_text.trim()) {
                Ok(packet) => packet,
                Err(error) => {
                    self.add_log(format!("ПОМИЛКА розшифрування INIT_REQUEST: {error}"));
                    return;
                }
            };
            if packet.stage != RelayStage::InitRequest {
                self.add_log(format!(
                    "ПОМИЛКА: Server Agent очікує INIT_REQUEST, отримано {}",
                    packet.stage.code()
                ));
                return;
            }
            match RelaySession::new_server(packet) {
                Ok(mut session) => {
                    session.timeline.push(model::TimelineEntry {
                        stage: RelayStage::InitRequest,
                        timestamp: Utc::now(),
                        packet_size: incoming_text.trim().len(),
                        note: "Отримано, розшифровано та перевірено".into(),
                    });
                    self.session = Some(session);
                    self.process_server_init_request();
                }
                Err(error) => self.add_log(format!("ПОМИЛКА INIT_REQUEST: {error}")),
            }
            return;
        }

        let packet = {
            let session = match self.session.as_ref() {
                Some(session) => session,
                None => {
                    self.add_log("ПОМИЛКА: сесія ще не створена");
                    return;
                }
            };
            match session.decode_incoming(&self.identity, &peer) {
                Ok(packet) => packet,
                Err(error) => {
                    self.add_log(format!("ПОМИЛКА розшифрування пакета: {error}"));
                    return;
                }
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

    fn pending_server_incoming_text(&self) -> Option<&str> {
        self.session
            .as_ref()
            .map(|session| session.incoming_text.as_str())
            .or_else(|| {
                self.log
                    .front()
                    .and(None)
            })
    }

    fn process_server_init_request(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let Some(packet) = session.last_packet() else {
            return;
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
                "euiccChallenge": packet.payload.get("euiccChallenge").cloned().unwrap_or(Value::Null),
                "euiccInfo1": packet.payload.get("euiccInfo1").cloned().unwrap_or(Value::Null)
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
            (AppMode::CardAgent, RelayStage::ServerAuth) => {
                self.relay_request(
                    "card.authenticateServer",
                    packet.payload,
                    PendingRelayAction::CardAuthenticateServer,
                );
            }
            (AppMode::CardAgent, RelayStage::DownloadPrepare) => {
                self.relay_request(
                    "card.prepareDownload",
                    packet.payload,
                    PendingRelayAction::CardPrepareDownload,
                );
            }
            (AppMode::CardAgent, RelayStage::BoundProfilePackage) => {
                self.relay_request(
                    "card.installBpp",
                    packet.payload,
                    PendingRelayAction::CardInstallBpp,
                );
            }
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
                        "authenticateServerResponse": packet.payload.get("authenticateServerResponse").cloned().unwrap_or(Value::Null)
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
                        "prepareDownloadResponse": packet.payload.get("prepareDownloadResponse").cloned().unwrap_or(Value::Null)
                    }),
                    PendingRelayAction::ServerGetBpp,
                );
            }
            (AppMode::ServerAgent, RelayStage::InstallResult) => {
                if let Some(session) = self.session.as_mut() {
                    session.completed = true;
                    session.status = "Card Agent підтвердив встановлення профілю".into();
                }
                self.add_log("Relay download завершено: отримано INSTALL_RESULT");
            }
            _ => self.add_log(format!("Неочікувана стадія для поточного режиму: {}", stage.code())),
        }
    }

    fn verify_installed_profile(&mut self, run: &LpacRun) {
        let Some(expected) = self.verify_after_relay_stop.take() else {
            return;
        };
        let found = run
            .result
            .data
            .as_array()
            .is_some_and(|profiles| {
                profiles.iter().any(|profile| {
                    profile
                        .get("iccid")
                        .and_then(Value::as_str)
                        .is_some_and(|iccid| iccid == expected)
                })
            });
        if let Some(session) = self.session.as_mut() {
            if found {
                session.status = format!("ICCID {expected} підтверджено свіжим Profile List");
                session.completed = true;
                self.add_log(format!("ICCID {expected} успішно перевірено"));
            } else {
                session.status = format!("ПОМИЛКА: ICCID {expected} відсутній у Profile List");
                session.completed = false;
                self.add_log(format!("ПОМИЛКА: ICCID {expected} не знайдено після встановлення"));
            }
        }
    }

    fn reset_mode(&mut self, mode: AppMode) {
        if self.relay_running {
            let _ = self.worker.send(WorkerCommand::StopRelay);
        }
        self.mode = mode;
        self.page = Page::Dashboard;
        self.session = None;
        self.pending_relay = None;
        self.relay_running = false;
        self.relay_role = None;
    }

    fn render_welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);
            ui.heading("NIK LPA");
            ui.label("Керування eUICC та ручний staged RSP без Інтернету на Card Agent");
            ui.add_space(28.0);
        });

        ui.columns(3, |columns| {
            role_card(
                &mut columns[0],
                "Локальний LPA",
                "Рідер та Інтернет на одному ПК. Звичайне встановлення й керування профілями.",
                || self.reset_mode(AppMode::Local),
            );
            role_card(
                &mut columns[1],
                "Card Agent",
                "Працює з eUICC та PC/SC. Мережеві ES9+ запити не виконує.",
                || self.reset_mode(AppMode::CardAgent),
            );
            role_card(
                &mut columns[2],
                "Server Agent",
                "Працює з activation code та SM-DP+. Рідер не потрібен.",
                || self.reset_mode(AppMode::ServerAgent),
            );
        });
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(if self.dark_mode {
                egui::Color32::from_rgb(15, 23, 42)
            } else {
                egui::Color32::WHITE
            })
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.set_min_width(205.0);
                ui.heading("NIK LPA");
                ui.small(self.mode.title());
                ui.separator();
                for page in Page::ALL {
                    let allowed = match (self.mode, page) {
                        (AppMode::ServerAgent, Page::Profiles | Page::Install) => false,
                        _ => true,
                    };
                    if ui
                        .add_enabled(
                            allowed,
                            egui::Button::new(page.label()).selected(self.page == page),
                        )
                        .clicked()
                    {
                        self.page = page;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if ui.button("Змінити режим").clicked() {
                        self.reset_mode(AppMode::Welcome);
                    }
                    ui.label(if self.relay_running {
                        "● Relay Agent активний"
                    } else {
                        "○ Relay Agent зупинений"
                    });
                });
            });
    }

    fn render_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(self.page.label());
                ui.label(self.mode.description());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(if self.dark_mode { "Світла" } else { "Темна" })
                    .clicked()
                {
                    self.dark_mode = !self.dark_mode;
                    configure_theme(ui.ctx(), self.dark_mode);
                }
                if self.busy {
                    ui.spinner();
                    ui.label("Виконується операція");
                }
            });
        });
        ui.add_space(10.0);
    }

    fn render_dashboard(&mut self, ui: &mut egui::Ui) {
        ui.columns(3, |columns| {
            metric_card(&mut columns[0], "Режим", self.mode.title());
            metric_card(
                &mut columns[1],
                "Pairing",
                if self.peer.is_some() { "Підключено" } else { "Не налаштовано" },
            );
            metric_card(
                &mut columns[2],
                "Сесія",
                self.session
                    .as_ref()
                    .map(|session| session.status.as_str())
                    .unwrap_or("Не створена"),
            );
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.heading("Швидкий старт");
            match self.mode {
                AppMode::CardAgent => {
                    ui.label("1. Виберіть рідер. 2. Спарте Server Agent. 3. Створіть INIT_REQUEST.");
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Перейти до ручної передачі"))
                        .clicked()
                    {
                        self.page = Page::Transfer;
                    }
                }
                AppMode::ServerAgent => {
                    ui.label("1. Спарте Card Agent. 2. Введіть activation code. 3. Запустіть Server Agent.");
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Перейти до ручної передачі"))
                        .clicked()
                    {
                        self.page = Page::Transfer;
                    }
                }
                AppMode::Local => {
                    ui.label("Підключіть рідер, оновіть список і виконайте звичайне встановлення профілю.");
                    if ui.button("Перейти до встановлення").clicked() {
                        self.page = Page::Install;
                    }
                }
                AppMode::Welcome => {}
            }
        });
    }

    fn render_profiles(&mut self, ui: &mut egui::Ui) {
        self.render_reader_selector(ui);
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Профілі eUICC");
                if ui
                    .add_enabled(!self.busy && !self.relay_running, egui::Button::new("Оновити"))
                    .clicked()
                {
                    self.send(
                        WorkerCommand::Profiles {
                            executable: PathBuf::from(&self.lpac_path),
                            reader_index: self.selected_reader,
                        },
                        "Читається Profile List",
                    );
                }
            });
            ui.add(
                egui::TextEdit::multiline(&mut self.profiles_output)
                    .desired_rows(20)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace)
                    .interactive(false),
            );
        });
    }

    fn render_install(&mut self, ui: &mut egui::Ui) {
        self.render_reader_selector(ui);
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.heading("Локальне встановлення");
            ui.label("Activation code");
            ui.add(
                egui::TextEdit::multiline(&mut self.local_activation_code)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            ui.label("Confirmation code — необов'язково");
            ui.add(
                egui::TextEdit::singleline(&mut self.local_confirmation_code)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            if ui
                .add_enabled(
                    !self.busy && !self.relay_running,
                    egui::Button::new("Встановити та перевірити ICCID"),
                )
                .clicked()
            {
                self.send(
                    WorkerCommand::LocalDownload {
                        executable: PathBuf::from(&self.lpac_path),
                        reader_index: self.selected_reader,
                        activation_code: self.local_activation_code.clone(),
                        confirmation_code: (!self.local_confirmation_code.is_empty())
                            .then(|| self.local_confirmation_code.clone()),
                    },
                    "Запущено локальне встановлення",
                );
            }
        });
    }

    fn render_transfer(&mut self, ui: &mut egui::Ui) {
        self.render_pairing(ui);
        ui.add_space(10.0);
        match self.mode {
            AppMode::CardAgent => self.render_card_transfer(ui),
            AppMode::ServerAgent => self.render_server_transfer(ui),
            AppMode::Local => {
                card(ui, |ui| {
                    ui.heading("Relay не потрібен у локальному режимі");
                    ui.label("Оберіть Card Agent або Server Agent через кнопку «Змінити режим».");
                });
            }
            AppMode::Welcome => {}
        }
    }

    fn render_pairing(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.heading("Захищене pairing");
            ui.label("Мій pairing code");
            let mut own_code = self.identity.pairing_code();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut own_code)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .interactive(false),
                );
                if ui.button("Копіювати").clicked() {
                    ui.ctx().copy_text(own_code);
                }
            });
            ui.label("Pairing code іншої сторони");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.peer_code_input)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
                if ui.button("Спарити").clicked() {
                    match PeerIdentity::from_pairing_code(&self.peer_code_input) {
                        Ok(peer) => {
                            self.peer = Some(peer);
                            self.add_log("Peer pairing code прийнято");
                        }
                        Err(error) => self.add_log(format!("ПОМИЛКА pairing: {error}")),
                    }
                }
            });
            ui.small("NIKRSP2 пакети приймаються лише від спареного X25519 peer.");
        });
    }

    fn render_card_transfer(&mut self, ui: &mut egui::Ui) {
        self.render_reader_selector(ui);
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Card Agent сесія");
                if ui
                    .add_enabled(
                        !self.busy && !self.relay_running && self.peer.is_some(),
                        egui::Button::new("Нова сесія з eUICC"),
                    )
                    .clicked()
                {
                    self.start_card_session();
                }
                if ui
                    .add_enabled(self.relay_running, egui::Button::new("Зупинити"))
                    .clicked()
                {
                    self.send(WorkerCommand::StopRelay, "Зупиняється Card Agent");
                }
            });
            ui.label("Card Agent не виконує HTTP-запитів. Він лише працює з PC/SC та імпортованими пакетами.");
        });
        self.render_session_exchange(ui);
    }

    fn render_server_transfer(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.heading("Server Agent конфігурація");
            ui.label("Activation code");
            ui.add(
                egui::TextEdit::multiline(&mut self.activation_code)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            ui.label("Confirmation code — необов'язково");
            ui.add(
                egui::TextEdit::singleline(&mut self.confirmation_code)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.busy && !self.relay_running && self.peer.is_some(),
                        egui::Button::new("Запустити Server Agent"),
                    )
                    .clicked()
                {
                    self.start_server_session();
                }
                if ui
                    .add_enabled(self.relay_running, egui::Button::new("Зупинити"))
                    .clicked()
                {
                    self.send(WorkerCommand::StopRelay, "Зупиняється Server Agent");
                }
            });
        });
        self.render_session_exchange(ui);
    }

    fn render_session_exchange(&mut self, ui: &mut egui::Ui) {
        self.render_timeline(ui);
        ui.add_space(10.0);

        if self.mode == AppMode::ServerAgent && self.session.is_none() && self.relay_running {
            card(ui, |ui| {
                ui.heading("Вхідний INIT_REQUEST");
                ui.label("Вставте NIKRSP2 строку, створену Card Agent.");
                static mut SERVER_BOOTSTRAP: Option<String> = None;
                let bootstrap = unsafe { SERVER_BOOTSTRAP.get_or_insert_with(String::new) };
                ui.add(
                    egui::TextEdit::multiline(bootstrap)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
                if ui.add_enabled(!self.busy, egui::Button::new("Прийняти INIT_REQUEST")).clicked() {
                    let text = std::mem::take(bootstrap);
                    let peer = match self.peer {
                        Some(peer) => peer,
                        None => {
                            self.add_log("ПОМИЛКА: peer не спарений");
                            return;
                        }
                    };
                    match self.identity.decrypt::<RelayPacket>(&peer, text.trim()) {
                        Ok(packet) if packet.stage == RelayStage::InitRequest => {
                            match RelaySession::new_server(packet) {
                                Ok(mut session) => {
                                    session.timeline.push(model::TimelineEntry {
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
                        Ok(packet) => self.add_log(format!(
                            "ПОМИЛКА: очікувався INIT_REQUEST, отримано {}",
                            packet.stage.code()
                        )),
                        Err(error) => self.add_log(format!("ПОМИЛКА розшифрування: {error}")),
                    }
                }
            });
            return;
        }

        let Some(session) = self.session.as_mut() else {
            card(ui, |ui| {
                ui.heading("Сесію ще не створено");
                ui.label(match self.mode {
                    AppMode::CardAgent => "Натисніть «Нова сесія з eUICC».",
                    AppMode::ServerAgent => "Запустіть Server Agent і вставте INIT_REQUEST.",
                    _ => "",
                });
            });
            return;
        };

        card(ui, |ui| {
            ui.heading("Вихідний пакет");
            ui.label("Скопіюйте всю строку та перенесіть її в інше вікно NIK LPA.");
            ui.add(
                egui::TextEdit::multiline(&mut session.outgoing_text)
                    .desired_rows(8)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace)
                    .interactive(false),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!session.outgoing_text.is_empty(), egui::Button::new("Копіювати строку"))
                    .clicked()
                {
                    ui.ctx().copy_text(session.outgoing_text.clone());
                }
                ui.label(format!("{} байт", session.outgoing_text.len()));
            });
        });

        ui.add_space(10.0);
        card(ui, |ui| {
            ui.heading("Вхідний пакет");
            ui.label(
                session
                    .expected_incoming()
                    .map(|stage| format!("Очікується {}", stage.code()))
                    .unwrap_or_else(|| "Новий пакет не очікується".into()),
            );
            ui.add(
                egui::TextEdit::multiline(&mut session.incoming_text)
                    .desired_rows(8)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            if ui
                .add_enabled(
                    !self.busy && session.expected_incoming().is_some(),
                    egui::Button::new("Розшифрувати, перевірити та виконати"),
                )
                .clicked()
            {
                self.import_incoming();
            }
        });
    }

    fn render_timeline(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.heading("Послідовність RSP");
            let packets = self
                .session
                .as_ref()
                .map(|session| &session.packets)
                .cloned()
                .unwrap_or_default();
            egui::Grid::new("rsp_timeline")
                .num_columns(4)
                .spacing([12.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Стан");
                    ui.strong("Пакет");
                    ui.strong("Напрямок");
                    ui.strong("Дія");
                    ui.end_row();
                    for stage in RelayStage::ALL {
                        let completed = packets.iter().any(|packet| packet.stage == stage);
                        ui.label(if completed { "✓" } else { "○" });
                        ui.label(stage.code());
                        ui.monospace(stage.direction().arrow());
                        ui.label(match self.mode {
                            AppMode::CardAgent => stage.offline_action_uk(),
                            AppMode::ServerAgent => stage.online_action_uk(),
                            _ => stage.label_uk(),
                        });
                        ui.end_row();
                    }
                });
            if let Some(session) = self.session.as_ref() {
                ui.separator();
                ui.label(format!("Job ID: {}", session.job_id));
                ui.label(format!("Статус: {}", session.status));
                ui.label(format!("Локальний deadline: {} UTC", session.deadline.format("%Y-%m-%d %H:%M:%S")));
                ui.small("SM-DP+ може завершити свою транзакцію раніше за локальний anti-replay deadline.");
            }
        });
    }

    fn render_sessions(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.heading("Поточна сесія");
            if let Some(session) = self.session.as_ref() {
                ui.label(format!("Job ID: {}", session.job_id));
                ui.label(format!("Сторона: {:?}", session.side));
                ui.label(format!("Пакетів у ланцюжку: {}", session.packets.len()));
                ui.label(format!("Статус: {}", session.status));
                ui.label(format!("Завершено: {}", if session.completed { "так" } else { "ні" }));
            } else {
                ui.label("Активної сесії немає.");
            }
        });
    }

    fn render_logs(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Журнал операцій");
                if ui.button("Очистити").clicked() {
                    self.log.clear();
                }
            });
            let mut text = self.log.iter().cloned().collect::<Vec<_>>().join("\n");
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .desired_rows(28)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace)
                    .interactive(false),
            );
        });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.heading("Runtime");
            ui.label("Шлях до lpac");
            ui.add(
                egui::TextEdit::singleline(&mut self.lpac_path)
                    .desired_width(f32::INFINITY),
            );
            ui.checkbox(&mut self.dark_mode, "Темна тема");
            if ui.button("Застосувати тему").clicked() {
                configure_theme(ui.ctx(), self.dark_mode);
            }
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.heading("Безпека");
            ui.label("Пакети: NIKRSP2 / X25519 / HKDF-SHA256 / XChaCha20-Poly1305.");
            ui.label("Activation code не записується в журнал і передається Card Agent лише як matchingId усередині ciphertext.");
            ui.label("Постійне DPAPI-сховище identity та сесій буде ввімкнено перед release-кандидатом.");
        });
    }

    fn render_reader_selector(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.heading("PC/SC підключення");
            ui.horizontal(|ui| {
                if self.readers.is_empty() {
                    ui.label(format!("Рідер #{}", self.selected_reader));
                } else {
                    egui::ComboBox::from_id_salt("production_pcsc_reader")
                        .selected_text(
                            self.readers
                                .iter()
                                .find(|reader| reader.index == self.selected_reader)
                                .map(|reader| format!("{} — {}", reader.index, reader.name))
                                .unwrap_or_else(|| format!("Рідер #{}", self.selected_reader)),
                        )
                        .width(480.0)
                        .show_ui(ui, |ui| {
                            for reader in &self.readers {
                                ui.selectable_value(
                                    &mut self.selected_reader,
                                    reader.index,
                                    format!("{} — {}", reader.index, reader.name),
                                );
                            }
                        });
                }
                if ui
                    .add_enabled(!self.busy && !self.relay_running, egui::Button::new("Оновити рідери"))
                    .clicked()
                {
                    self.send(
                        WorkerCommand::DiscoverReaders {
                            executable: PathBuf::from(&self.lpac_path),
                        },
                        "Шукаються PC/SC-рідери",
                    );
                }
                if ui
                    .add_enabled(!self.busy && !self.relay_running, egui::Button::new("Інформація eUICC"))
                    .clicked()
                {
                    self.send(
                        WorkerCommand::ChipInfo {
                            executable: PathBuf::from(&self.lpac_path),
                            reader_index: self.selected_reader,
                        },
                        "Читається інформація eUICC",
                    );
                }
            });
            if let Some(eid) = self.card_eid.as_deref() {
                let redacted = if eid.len() > 10 {
                    format!("{}…{}", &eid[..6], &eid[eid.len() - 4..])
                } else {
                    eid.to_owned()
                };
                ui.small(format!("EID: {redacted}"));
            }
        });
    }
}

impl eframe::App for NikLpaApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.dark_mode {
            egui::Color32::from_rgb(10, 15, 25).to_normalized_gamma_f32()
        } else {
            egui::Color32::from_rgb(243, 246, 250).to_normalized_gamma_f32()
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.poll_worker(ui.ctx());
        if self.busy {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(ui.style())
                    .fill(if self.dark_mode {
                        egui::Color32::from_rgb(10, 15, 25)
                    } else {
                        egui::Color32::from_rgb(243, 246, 250)
                    })
                    .inner_margin(0.0),
            )
            .show(ui, |ui| {
                if self.mode == AppMode::Welcome {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.set_max_width(1_150.0);
                        ui.add_space(20.0);
                        self.render_welcome(ui);
                    });
                    return;
                }

                ui.horizontal_top(|ui| {
                    self.render_sidebar(ui);
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(720.0);
                            ui.set_max_width(1_050.0);
                            ui.add_space(18.0);
                            self.render_header(ui);
                            match self.page {
                                Page::Dashboard => self.render_dashboard(ui),
                                Page::Profiles => self.render_profiles(ui),
                                Page::Install => self.render_install(ui),
                                Page::Transfer => self.render_transfer(ui),
                                Page::Sessions => self.render_sessions(ui),
                                Page::Logs => self.render_logs(ui),
                                Page::Settings => self.render_settings(ui),
                            }
                            ui.add_space(24.0);
                        });
                });
            });
    }
}

fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().window_fill)
        .corner_radius(10.0)
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
}

fn metric_card(ui: &mut egui::Ui, title: &str, value: &str) {
    card(ui, |ui| {
        ui.small(title);
        ui.strong(value);
    });
}

fn role_card(ui: &mut egui::Ui, title: &str, description: &str, on_click: impl FnOnce()) {
    let mut clicked = false;
    card(ui, |ui| {
        ui.heading(title);
        ui.label(description);
        ui.add_space(12.0);
        clicked = ui.button("Обрати режим").clicked();
    });
    if clicked {
        on_click();
    }
}
