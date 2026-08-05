use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Duration,
};

use eframe::egui;
use lpac_backend::{LegacyLpacBackend, LpacRun};
use lpac_core::{
    ActivationCode, ActivationJob, encode_transfer_key, encrypt_job, generate_transfer_key,
};
use zeroize::{Zeroize, Zeroizing};

type OperationResult = Result<LpacRun, String>;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "NIK LPA",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::<LpaApp>::default())),
    )
}

struct LpaApp {
    lpac_path: String,
    reader_index: String,
    activation_code: String,
    confirmation_code: String,
    transfer_key: [u8; 32],
    transfer_envelope: String,
    output: String,
    operation_rx: Option<Receiver<OperationResult>>,
}

impl Default for LpaApp {
    fn default() -> Self {
        Self {
            lpac_path: if cfg!(windows) {
                "lpac.exe".into()
            } else {
                "lpac".into()
            },
            reader_index: "0".into(),
            activation_code: String::new(),
            confirmation_code: String::new(),
            transfer_key: generate_transfer_key(),
            transfer_envelope: String::new(),
            output: "Ready. Connect a PC/SC reader and insert an eUICC.".into(),
            operation_rx: None,
        }
    }
}

impl Drop for LpaApp {
    fn drop(&mut self) {
        self.activation_code.zeroize();
        self.confirmation_code.zeroize();
        self.transfer_key.zeroize();
    }
}

impl LpaApp {
    fn backend(&self) -> LegacyLpacBackend {
        LegacyLpacBackend::new(&self.lpac_path)
            .with_reader_index(self.reader_index.parse::<u32>().ok())
    }

    fn is_busy(&self) -> bool {
        self.operation_rx.is_some()
    }

    fn start_operation<F>(&mut self, label: &str, operation: F)
    where
        F: FnOnce() -> anyhow::Result<LpacRun> + Send + 'static,
    {
        if self.is_busy() {
            return;
        }

        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = operation().map_err(|error| format!("{error:#}"));
            let _ = sender.send(result);
        });
        self.operation_rx = Some(receiver);
        self.output = format!("{label}…");
    }

    fn poll_operation(&mut self) {
        let received = self.operation_rx.as_ref().map(Receiver::try_recv);
        match received {
            Some(Ok(Ok(run))) => {
                self.output = run.pretty_log();
                self.operation_rx = None;
            }
            Some(Ok(Err(error))) => {
                self.output = format!("ERROR: {error}");
                self.operation_rx = None;
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.output = "ERROR: operation worker disconnected".into();
                self.operation_rx = None;
            }
            Some(Err(TryRecvError::Empty)) | None => {}
        }
    }
}

impl eframe::App for LpaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.poll_operation();
        let busy = self.is_busy();
        if busy {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        ui.heading("NIK LPA — removable eUICC lab");
        ui.label("Rust GUI over the existing, proven lpac/libeuicc engine.");
        ui.separator();

        egui::Grid::new("settings").num_columns(2).show(ui, |ui| {
            ui.label("lpac executable");
            ui.add_enabled(!busy, egui::TextEdit::singleline(&mut self.lpac_path));
            ui.end_row();
            ui.label("PC/SC reader index");
            ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(&mut self.reader_index),
            );
            ui.end_row();
        });

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Read eUICC info"))
                .clicked()
            {
                let backend = self.backend();
                self.start_operation("Reading eUICC information", move || backend.chip_info());
            }
            if ui
                .add_enabled(!busy, egui::Button::new("List profiles"))
                .clicked()
            {
                let backend = self.backend();
                self.start_operation("Reading profiles", move || backend.profiles());
            }
        });

        ui.separator();
        ui.label("Activation code");
        ui.add_enabled(
            !busy,
            egui::TextEdit::multiline(&mut self.activation_code)
                .desired_rows(2)
                .password(true),
        );
        ui.label("Confirmation code (optional)");
        ui.add_enabled(
            !busy,
            egui::TextEdit::singleline(&mut self.confirmation_code).password(true),
        );

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Install profile"))
                .clicked()
            {
                match ActivationCode::parse(self.activation_code.clone()) {
                    Ok(code) => {
                        let backend = self.backend();
                        let confirmation = (!self.confirmation_code.is_empty())
                            .then(|| Zeroizing::new(self.confirmation_code.clone()));
                        self.activation_code.zeroize();
                        self.confirmation_code.zeroize();
                        self.start_operation("Installing profile", move || {
                            backend.download(
                                &code,
                                confirmation.as_ref().map(|value| value.as_str()),
                            )
                        });
                    }
                    Err(error) => self.output = format!("ERROR: {error}"),
                }
            }

            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new("Create encrypted transfer string"),
                )
                .clicked()
            {
                match ActivationCode::parse(self.activation_code.clone()) {
                    Ok(code) => {
                        let job = ActivationJob::new(
                            &code,
                            Some(self.reader_index.clone()),
                            None,
                            (!self.confirmation_code.is_empty())
                                .then(|| self.confirmation_code.clone()),
                        );
                        match encrypt_job(&job, &self.transfer_key) {
                            Ok(value) => self.transfer_envelope = value,
                            Err(error) => self.output = format!("ERROR: {error}"),
                        }
                    }
                    Err(error) => self.output = format!("ERROR: {error}"),
                }
            }
        });

        if !self.transfer_envelope.is_empty() {
            ui.separator();
            ui.label("Encrypted transport payload");
            ui.add(egui::TextEdit::multiline(&mut self.transfer_envelope).desired_rows(5));
            ui.collapsing("Lab transfer key — share separately", |ui| {
                ui.monospace(encode_transfer_key(&self.transfer_key));
            });
        }

        ui.separator();
        ui.label("Operation log (activation secrets are not logged)");
        ui.add(
            egui::TextEdit::multiline(&mut self.output)
                .desired_rows(14)
                .interactive(false),
        );
    }
}
