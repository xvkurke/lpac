use eframe::egui;
use lpac_backend::LegacyLpacBackend;
use lpac_core::{encrypt_job, generate_transfer_key, encode_transfer_key, ActivationCode, ActivationJob};

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
}

impl Default for LpaApp {
    fn default() -> Self {
        Self {
            lpac_path: if cfg!(windows) { "lpac.exe".into() } else { "lpac".into() },
            reader_index: "0".into(),
            activation_code: String::new(),
            confirmation_code: String::new(),
            transfer_key: generate_transfer_key(),
            transfer_envelope: String::new(),
            output: "Ready. Connect a PC/SC reader and insert an eUICC.".into(),
        }
    }
}

impl LpaApp {
    fn backend(&self) -> LegacyLpacBackend {
        LegacyLpacBackend::new(&self.lpac_path)
            .with_reader_index(self.reader_index.parse::<u32>().ok())
    }

    fn show_result(&mut self, result: anyhow::Result<serde_json::Value>) {
        self.output = match result {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
            Err(error) => format!("ERROR: {error:#}"),
        };
    }
}

impl eframe::App for LpaApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("NIK LPA — removable eUICC lab");
            ui.label("Rust GUI over the existing, proven lpac/libeuicc engine.");
            ui.separator();

            egui::Grid::new("settings").num_columns(2).show(ui, |ui| {
                ui.label("lpac executable");
                ui.text_edit_singleline(&mut self.lpac_path);
                ui.end_row();
                ui.label("PC/SC reader index");
                ui.text_edit_singleline(&mut self.reader_index);
                ui.end_row();
            });

            ui.horizontal(|ui| {
                if ui.button("Read eUICC info").clicked() {
                    let result = self.backend().chip_info();
                    self.show_result(result);
                }
                if ui.button("List profiles").clicked() {
                    let result = self.backend().profiles();
                    self.show_result(result);
                }
            });

            ui.separator();
            ui.label("Activation code");
            ui.add(egui::TextEdit::multiline(&mut self.activation_code).desired_rows(2).password(true));
            ui.label("Confirmation code (optional)");
            ui.add(egui::TextEdit::singleline(&mut self.confirmation_code).password(true));

            ui.horizontal(|ui| {
                if ui.button("Install profile").clicked() {
                    match ActivationCode::parse(self.activation_code.clone()) {
                        Ok(code) => {
                            let confirmation = (!self.confirmation_code.is_empty()).then_some(self.confirmation_code.as_str());
                            let result = self.backend().download(&code, confirmation);
                            self.show_result(result);
                        }
                        Err(error) => self.output = format!("ERROR: {error}"),
                    }
                }

                if ui.button("Create encrypted transfer string").clicked() {
                    match ActivationCode::parse(self.activation_code.clone()) {
                        Ok(code) => {
                            let job = ActivationJob::new(
                                &code,
                                Some(self.reader_index.clone()),
                                None,
                                (!self.confirmation_code.is_empty()).then(|| self.confirmation_code.clone()),
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
            ui.add(egui::TextEdit::multiline(&mut self.output).desired_rows(14).interactive(false));
        });
    }
}
