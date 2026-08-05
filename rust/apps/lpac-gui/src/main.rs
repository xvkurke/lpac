use std::{
    env,
    path::PathBuf,
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
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1_040.0, 820.0])
            .with_min_inner_size([760.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "NIK LPA",
        options,
        Box::new(|creation_context| {
            configure_ui(&creation_context.egui_ctx);
            Ok(Box::<LpaApp>::default())
        }),
    )
}

fn configure_ui(context: &egui::Context) {
    context.set_theme(egui::Theme::Light);

    let mut visuals = egui::Visuals::light();
    visuals.override_text_color = Some(egui::Color32::from_rgb(30, 41, 59));
    visuals.panel_fill = egui::Color32::from_rgb(245, 247, 250);
    visuals.window_fill = egui::Color32::WHITE;
    visuals.extreme_bg_color = egui::Color32::WHITE;
    visuals.faint_bg_color = egui::Color32::from_rgb(241, 245, 249);
    visuals.selection.bg_fill = egui::Color32::from_rgb(37, 99, 235);
    visuals.selection.stroke.color = egui::Color32::WHITE;
    visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(248, 250, 252);
    visuals.widgets.noninteractive.fg_stroke.color = egui::Color32::from_rgb(51, 65, 85);
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(226, 232, 240);
    visuals.widgets.inactive.fg_stroke.color = egui::Color32::from_rgb(30, 41, 59);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(219, 234, 254);
    visuals.widgets.hovered.fg_stroke.color = egui::Color32::from_rgb(30, 64, 175);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(37, 99, 235);
    visuals.widgets.active.fg_stroke.color = egui::Color32::WHITE;
    context.set_visuals_of(egui::Theme::Light, visuals);

    let mut style = (*context.style_of(egui::Theme::Light)).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.interact_size.y = 34.0;
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(22.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(14.0));
    context.set_style_of(egui::Theme::Light, style);
}

fn bundled_lpac_path() -> String {
    let executable_name = if cfg!(windows) { "lpac.exe" } else { "lpac" };
    let adjacent = env::current_exe().ok().and_then(|path| {
        path.parent()
            .map(|directory| directory.join(executable_name))
    });

    adjacent
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(executable_name))
        .display()
        .to_string()
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
            lpac_path: bundled_lpac_path(),
            reader_index: "0".into(),
            readers: Vec::new(),
            activation_code: String::new(),
            confirmation_code: String::new(),
            transfer_key: generate_transfer_key(),
            transfer_envelope: String::new(),
            receive_key: String::new(),
            receive_envelope: String::new(),
            imported_job: None,
            output: "Готово. Підключіть PC/SC-рідер і вставте eUICC.".into(),
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
            .unwrap_or_else(|| format!("Рідер #{}", self.reader_index))
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
        self.output = "Пошук PC/SC-рідерів…".into();
    }

    fn poll_operation(&mut self) {
        let received = self.operation_rx.as_ref().map(Receiver::try_recv);
        match received {
            Some(Ok(WorkerResult::Run(Ok(run)))) => {
                self.output = run.pretty_log();
                self.operation_rx = None;
            }
            Some(Ok(WorkerResult::Run(Err(error)))) => {
                self.output = format!("ПОМИЛКА: {error}");
                self.operation_rx = None;
            }
            Some(Ok(WorkerResult::Readers(Ok(readers)))) => {
                if readers.is_empty() {
                    self.output = "lpac не знайшов жодного PC/SC-рідера.".into();
                } else {
                    let current = self.reader_index.parse::<u32>().ok();
                    if !readers.iter().any(|reader| Some(reader.index) == current) {
                        self.reader_index = readers[0].index.to_string();
                    }
                    self.output = format!(
                        "Знайдено рідерів: {}. Вибрано #{}.",
                        readers.len(),
                        self.reader_index
                    );
                }
                self.readers = readers;
                self.operation_rx = None;
            }
            Some(Ok(WorkerResult::Readers(Err(error)))) => {
                self.output = format!("ПОМИЛКА: {error}");
                self.operation_rx = None;
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.output = "ПОМИЛКА: робочий потік несподівано завершився.".into();
                self.operation_rx = None;
            }
            Some(Err(TryRecvError::Empty)) | None => {}
        }
    }

    fn import_transfer_job(&mut self) {
        let mut key = match decode_transfer_key(self.receive_key.trim()) {
            Ok(key) => key,
            Err(error) => {
                self.output = format!("ПОМИЛКА: {error}");
                return;
            }
        };
        let decrypted = decrypt_job(self.receive_envelope.trim(), &key);
        key.zeroize();

        match decrypted {
            Ok(job) => match ActivationCode::parse(job.activation_code.clone()) {
                Ok(code) => {
                    self.output = format!(
                        "Завдання {} розшифровано для {}. Виберіть локальний рідер і запустіть встановлення.",
                        job.id,
                        code.redacted()
                    );
                    self.receive_key.zeroize();
                    self.receive_envelope.zeroize();
                    self.imported_job = Some(job);
                }
                Err(error) => {
                    self.output = format!("ПОМИЛКА: розшифроване завдання некоректне: {error}")
                }
            },
            Err(error) => self.output = format!("ПОМИЛКА: {error}"),
        }
    }
}

impl eframe::App for LpaApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::from_rgb(245, 247, 250).to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.poll_operation();
        let busy = self.is_busy();
        if busy {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(ui.style())
                    .fill(egui::Color32::from_rgb(245, 247, 250))
                    .inner_margin(16.0),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(1_000.0);
                        ui.heading("NIK LPA");
                        ui.label(
                            "Керування removable eUICC через перевірений lpac/libeuicc backend",
                        );
                        ui.add_space(6.0);

                        if busy {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.strong(
                                    "Виконується операція. Інші дії тимчасово заблоковані.",
                                );
                            });
                            ui.add_space(6.0);
                        }

                        ui.group(|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.heading("1. Підключення");
                            ui.label("Шлях до bundled lpac.exe");
                            ui.add_enabled(
                                !busy,
                                egui::TextEdit::singleline(&mut self.lpac_path)
                                    .desired_width(f32::INFINITY),
                            );

                            ui.label("PC/SC-рідер");
                            ui.horizontal_wrapped(|ui| {
                                let selected_label = self.selected_reader_label();
                                if self.readers.is_empty() {
                                    ui.add_enabled(
                                        !busy,
                                        egui::TextEdit::singleline(&mut self.reader_index)
                                            .desired_width(90.0),
                                    );
                                } else {
                                    egui::ComboBox::from_id_salt("pcsc_reader")
                                        .selected_text(selected_label)
                                        .width(420.0)
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
                                    .add_enabled(
                                        !busy,
                                        egui::Button::new("Оновити список рідерів"),
                                    )
                                    .clicked()
                                {
                                    self.start_reader_discovery();
                                }
                            });

                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add_enabled(
                                        !busy,
                                        egui::Button::new("Прочитати інформацію eUICC"),
                                    )
                                    .clicked()
                                {
                                    let backend = self.backend();
                                    self.start_run("Читання інформації eUICC", move || {
                                        backend.chip_info()
                                    });
                                }
                                if ui
                                    .add_enabled(!busy, egui::Button::new("Показати профілі"))
                                    .clicked()
                                {
                                    let backend = self.backend();
                                    self.start_run("Читання списку профілів", move || {
                                        backend.profiles()
                                    });
                                }
                            });
                        });

                        ui.add_space(10.0);
                        ui.group(|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.heading("2. Встановлення профілю");
                            ui.label("Activation code");
                            ui.add_enabled(
                                !busy,
                                egui::TextEdit::multiline(&mut self.activation_code)
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY)
                                    .password(true),
                            );
                            ui.label("Confirmation code — необов’язково");
                            ui.add_enabled(
                                !busy,
                                egui::TextEdit::singleline(&mut self.confirmation_code)
                                    .desired_width(f32::INFINITY)
                                    .password(true),
                            );

                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add_enabled(
                                        !busy,
                                        egui::Button::new("Встановити й перевірити"),
                                    )
                                    .clicked()
                                {
                                    match ActivationCode::parse(self.activation_code.clone()) {
                                        Ok(code) => {
                                            let backend = self.backend();
                                            let confirmation = (!self.confirmation_code.is_empty())
                                                .then(|| {
                                                    Zeroizing::new(self.confirmation_code.clone())
                                                });
                                            self.activation_code.zeroize();
                                            self.confirmation_code.zeroize();
                                            self.start_run(
                                                "Встановлення та перевірка профілю",
                                                move || {
                                                    backend.download_and_verify(
                                                        &code,
                                                        confirmation
                                                            .as_ref()
                                                            .map(|value| value.as_str()),
                                                    )
                                                },
                                            );
                                        }
                                        Err(error) => {
                                            self.output = format!("ПОМИЛКА: {error}")
                                        }
                                    }
                                }

                                if ui
                                    .add_enabled(
                                        !busy,
                                        egui::Button::new("Створити зашифровану строку"),
                                    )
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
                                                Err(error) => {
                                                    self.output = format!("ПОМИЛКА: {error}")
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            self.output = format!("ПОМИЛКА: {error}")
                                        }
                                    }
                                }
                            });
                        });

                        if !self.transfer_envelope.is_empty() {
                            ui.add_space(10.0);
                            ui.group(|ui| {
                                ui.set_min_width(ui.available_width());
                                ui.heading("3. Зашифрована передача");
                                ui.label("Передайте payload і ключ різними каналами.");
                                ui.label("NIKLPA1 payload");
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.transfer_envelope)
                                        .desired_rows(6)
                                        .desired_width(f32::INFINITY)
                                        .font(egui::TextStyle::Monospace),
                                );
                                ui.collapsing(
                                    "Показати лабораторний transfer key",
                                    |ui| {
                                        ui.monospace(encode_transfer_key(&self.transfer_key));
                                    },
                                );
                            });
                        }

                        ui.add_space(10.0);
                        ui.group(|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.heading("4. Отримання зашифрованого завдання");
                            ui.label("NIKLPA1 transfer string");
                            ui.add_enabled(
                                !busy,
                                egui::TextEdit::multiline(&mut self.receive_envelope)
                                    .desired_rows(4)
                                    .desired_width(f32::INFINITY)
                                    .password(true),
                            );
                            ui.label("Lab transfer key");
                            ui.add_enabled(
                                !busy,
                                egui::TextEdit::singleline(&mut self.receive_key)
                                    .desired_width(f32::INFINITY)
                                    .password(true),
                            );
                            if ui
                                .add_enabled(
                                    !busy,
                                    egui::Button::new("Розшифрувати завдання"),
                                )
                                .clicked()
                            {
                                self.import_transfer_job();
                            }

                            if let Some(job) = self.imported_job.as_ref() {
                                let redacted = ActivationCode::parse(job.activation_code.clone())
                                    .map(|code| code.redacted())
                                    .unwrap_or_else(|_| "<некоректне>".into());
                                ui.separator();
                                ui.label(format!("Job ID: {}", job.id));
                                ui.label(format!("Activation: {redacted}"));
                                ui.label(format!(
                                    "EID hint: {}",
                                    job.eid.as_deref().unwrap_or("не вказано")
                                ));
                                ui.label(format!(
                                    "Reader hint: {}",
                                    job.reader.as_deref().unwrap_or("не вказано")
                                ));
                            }

                            if ui
                                .add_enabled(
                                    !busy && self.imported_job.is_some(),
                                    egui::Button::new("Встановити імпортоване завдання"),
                                )
                                .clicked()
                                && let Some(job) = self.imported_job.take()
                            {
                                let backend = self.backend();
                                self.start_run(
                                    "Встановлення імпортованого завдання",
                                    move || {
                                        let code =
                                            ActivationCode::parse(job.activation_code.clone())?;
                                        backend.download_and_verify(
                                            &code,
                                            job.confirmation_code.as_deref(),
                                        )
                                    },
                                );
                            }
                        });

                        ui.add_space(10.0);
                        ui.group(|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.heading("Журнал операцій");
                            ui.label(
                                "Activation і confirmation коди не записуються в журнал.",
                            );
                            ui.add(
                                egui::TextEdit::multiline(&mut self.output)
                                    .desired_rows(12)
                                    .desired_width(f32::INFINITY)
                                    .font(egui::TextStyle::Monospace)
                                    .interactive(false),
                            );
                        });
                    });
            });
    }
}
