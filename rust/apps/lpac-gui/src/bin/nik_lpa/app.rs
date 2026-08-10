use std::{collections::VecDeque, env, path::PathBuf, time::Duration};

use eframe::egui;
use lpac_backend::PcscReader;
use lpac_relay_backend::RelayAgentRole;
use serde_json::Value;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::{
    controller::WorkerController,
    model::{AppMode, Page, RelaySession},
    theme,
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum PendingRelayAction {
    CardInit,
    CardAuthenticateServer,
    CardPrepareDownload,
    CardInstallBpp,
    ServerInitiateAuthentication,
    ServerAuthenticateClient,
    ServerGetBpp,
}

pub(crate) struct PendingRelay {
    pub token: Uuid,
    pub action: PendingRelayAction,
}

pub struct NikLpaApp {
    pub(crate) worker: WorkerController,
    pub(crate) mode: AppMode,
    pub(crate) page: Page,
    pub(crate) lpac_path: String,
    pub(crate) readers: Vec<PcscReader>,
    pub(crate) selected_reader: u32,
    pub(crate) server_bootstrap_input: String,
    pub(crate) relay_running: bool,
    pub(crate) relay_role: Option<RelayAgentRole>,
    pub(crate) session: Option<RelaySession>,
    pub(crate) pending_relay: Option<PendingRelay>,
    pub(crate) busy: bool,
    pub(crate) start_card_after_chip_info: bool,
    pub(crate) verify_after_relay_stop: Option<String>,
    pub(crate) pending_install_payload: Option<Value>,
    pub(crate) card_eid: Option<String>,
    pub(crate) activation_code: String,
    pub(crate) confirmation_code: String,
    pub(crate) local_activation_code: String,
    pub(crate) local_confirmation_code: String,
    pub(crate) chip_info_output: String,
    pub(crate) profiles_output: String,
    pub(crate) log: VecDeque<String>,
    pub(crate) dark_mode: bool,
    pub(crate) show_outgoing_packet: bool,
    pub(crate) show_incoming_packet: bool,
    pub(crate) show_bootstrap_packet: bool,
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
            server_bootstrap_input: String::new(),
            relay_running: false,
            relay_role: None,
            session: None,
            pending_relay: None,
            busy: false,
            start_card_after_chip_info: false,
            verify_after_relay_stop: None,
            pending_install_payload: None,
            card_eid: None,
            activation_code: String::new(),
            confirmation_code: String::new(),
            local_activation_code: String::new(),
            local_confirmation_code: String::new(),
            chip_info_output: String::new(),
            profiles_output: String::new(),
            log: VecDeque::new(),
            dark_mode: true,
            show_outgoing_packet: false,
            show_incoming_packet: false,
            show_bootstrap_packet: false,
        }
    }
}

impl Drop for NikLpaApp {
    fn drop(&mut self) {
        self.activation_code.zeroize();
        self.confirmation_code.zeroize();
        self.local_activation_code.zeroize();
        self.local_confirmation_code.zeroize();
        self.server_bootstrap_input.zeroize();
    }
}

impl eframe::App for NikLpaApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::background(self.dark_mode).to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.poll_worker(ui.ctx());
        if self.busy {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(ui.style())
                    .fill(theme::background(self.dark_mode))
                    .inner_margin(0.0),
            )
            .show(ui, |ui| self.render_root(ui));
    }
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
