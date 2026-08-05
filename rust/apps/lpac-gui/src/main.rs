use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Duration,
};

use eframe::egui;
use lpac_backend::{LegacyLpacBackend, LpacRun, PcscReader};
use lpac_core::{
    ActivationCode, ActivationJob, decode_transfer_key, decrypt_job, encode_transfer_key,
    encrypt_job, generate_transfer_key,
};
use zeroize::{Zeroize, Zeroizing};

enum WorkerResult {
    Run(Result<LpacRun, String>),
    Readers(Result<Vec<PcscReader>, String>),
}

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
    readers: Vec<PcscReader>,
    activation_code: String,
    confirmation_code: String,
    transfer_key: [u8; 32],
    transfer_envelope: String,
    receive_key: String,
    receive_envelope: String,
    imported_job: Option<ActivationJob>,
    output: String,
    operation_rx: Option<Receiver<WorkerResult>>,
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
            readers: Vec::new(),
            activation_code: String::new(),
            confirmation_code: String::new(),
            transfer_key: generate_transfer_key(),
            transfer_envelope: String::new(),
            receive_key: String::new(),
            receive_envelope: String::new(),
            imported_job: None,
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
        self.receive_key.zeroize();
        self.receive_envelope.zeroize();
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

    fn selected_reader_label(&self) -> String {
        let selected = self.reader_index.parse::<u32>().ok();
        self.readers
            .iter()
            .find(|reader| Some(reader.index) == selected)
            .map(|reader| format!("{} — {}", reader.index, reader.name))
            .unwrap_or_else(|| format!("Reader index {}", self.reader_index))
    }

    fn selected_reader_hint(&self) -> Option<String> {
        let selected = self.reader_index.parse::<u32>().ok()?;
        Some(
            self.readers
                .iter()
                .find(|reader| reader.index == selected)
                .map(|reader| format!("{}:{}", reader.index, reader.name))
                .unwrap_or_else(|| selected.to_string()),
        )
    }

    fn start_run<F>(&mut self, label: &str, operation: F)
    where
        F: FnOnce() -> anyhow::Result<LpacRun> + Send + 'static,
    {
        if self.is_busy() {
            return;
        }

        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = operation().map_err(|error| format!("{error:#}"));
            let _ = sender.send(WorkerResult::Run(result));
        });
        self.operation_rx = Some(receiver);
        self.output = format!("{label}…");
    }

    fn start_reader_discovery(&mut self) {
        if self.is_busy() {
            return;
        }

        let backend = LegacyLpacBackend::new(&self.lpac_path);
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = backend.readers().map_err(|error| format!("{error:#}"));
            let _ = sender.send(WorkerResult::Readers(result));
        });
        self.operation_rx = Some(receiver);
        self.output = "Discovering PC/SC readers…".into();
    }

    fn poll_operation(&mut self) {
        let received = self.operation_rx.as_ref().map(Receiver::try_recv);
        match received {
            Some(Ok(WorkerResult::Run(Ok(run)))) => {
                self.output = run.pretty_log();
                self.operation_rx = None;
            }
            Some(Ok(WorkerResult::Run(Err(error)))) => {
                self.output = format!("ERROR: {error}");
                self.operation_rx = None;
            }
            Some(Ok(WorkerResult::Readers(Ok(readers)))) => {
                if readers.is_empty() {
                    self.output = "No PC/SC readers were reported by lpac.".into();
                } else {
                    let current = self.reader_index.parse::<u32>().ok();
                    if !readers.iter().any(|reader| Some(reader.index) == current) {
                        self.reader_index = readers[0].index.to_string();
                    }
                    self.output = format!(
                        "Discovered {} PC/SC reader(s). Selected {}.",
                        readers.len(),
                        self.reader_index
                    );
                }
                self.readers = readers;
                self.operation_rx = None;
            }
            Some(Ok(WorkerResult::Readers(Err(error)))) => {
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

    fn import_transfer_job(&mut self) {
        let mut key = match decode_transfer_key(self.receive_key.trim()) {
            Ok(key) => key,
            Err(error) => {
                self.output = format!("ERROR: {error}");
                return;
            }
        };
        let decrypted = decrypt_job(self.receive_envelope.trim(), &key);
        key.zeroize();

        match decrypted {
            Ok(job) => match ActivationCode::parse(job.activation_code.clone()) {
                Ok(code) => {
                    self.output = format!(
                        "Imported encrypted job {} for {}. Select the local reader and install when ready.",
                        job.id,
                        code.redacted()
                    );
                    self.receive_key.zeroize();
                    self.receive_envelope.zeroize();
                    self.imported_job = Some(job);
                }
                Err(error) => self.output = format!("ERROR: decrypted job is invalid: {error}"),
            },
            Err(error) => self.output = format!("ERROR: {error}"),
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

            ui.label("PC/SC reader");
            ui.horizontal(|ui| {
                let selected_label = self.selected_reader_label();
                if self.readers.is_empty() {
                    ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut self.reader_index).desired_width(70.0),
                    );
                } else {
                    egui::ComboBox::from_id_salt("pcsc_reader")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            for reader in &self.readers {
                                ui.selectable_value(
                                    &mut self.reader_index,
                                    reader.index.to_string(),
                                    format!("{} — {}", reader.index, reader.name),
                                );
                            }
                        });
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("Refresh readers"))
                    .clicked()
                {
                    self.start_reader_discovery();
                }
            });
            ui.end_row();
        });

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Read eUICC info"))
                .clicked()
            {
                let backend = self.backend();
                self.start_run("Reading eUICC information", move || backend.chip_info());
            }
            if ui
                .add_enabled(!busy, egui::Button::new("List profiles"))
                .clicked()
            {
                let backend = self.backend();
                self.start_run("Reading profiles", move || backend.profiles());
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
                        self.start_run("Installing and verifying profile", move || {
                            backend.download_and_verify(
                                &code,
                                confirmation.as_ref().map(|value| value.as_str()),
                            )
                        });
                    }
                    Err(error) => self.output = format!("ERROR: {error}"),
                }
            }

            if ui
                .add_enabled(!busy, egui::Button::new("Create encrypted transfer string"))
                .clicked()
            {
                match ActivationCode::parse(self.activation_code.clone()) {
                    Ok(code) => {
                        let job = ActivationJob::new(
                            &code,
                            self.selected_reader_hint(),
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
        ui.collapsing("Receive encrypted activation job", |ui| {
            ui.label("NIKLPA1 transfer string");
            ui.add_enabled(
                !busy,
                egui::TextEdit::multiline(&mut self.receive_envelope)
                    .desired_rows(4)
                    .password(true),
            );
            ui.label("Lab transfer key");
            ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(&mut self.receive_key).password(true),
            );
            if ui
                .add_enabled(!busy, egui::Button::new("Decrypt job"))
                .clicked()
            {
                self.import_transfer_job();
            }

            if let Some(job) = self.imported_job.as_ref() {
                let redacted = ActivationCode::parse(job.activation_code.clone())
                    .map(|code| code.redacted())
                    .unwrap_or_else(|_| "<invalid>".into());
                ui.label(format!("Job: {}", job.id));
                ui.label(format!("Activation: {redacted}"));
                ui.label(format!(
                    "EID hint: {}",
                    job.eid.as_deref().unwrap_or("not provided")
                ));
                ui.label(format!(
                    "Reader hint: {}",
                    job.reader.as_deref().unwrap_or("not provided")
                ));
            }

            if ui
                .add_enabled(
                    !busy && self.imported_job.is_some(),
                    egui::Button::new("Install imported job"),
                )
                .clicked()
                && let Some(job) = self.imported_job.take()
            {
                let backend = self.backend();
                self.start_run("Installing imported job", move || {
                    let code = ActivationCode::parse(job.activation_code.clone())?;
                    backend.download_and_verify(&code, job.confirmation_code.as_deref())
                });
            }
        });

        ui.separator();
        ui.label("Operation log (activation secrets are not logged)");
        ui.add(
            egui::TextEdit::multiline(&mut self.output)
                .desired_rows(14)
                .interactive(false),
        );
    }
}
