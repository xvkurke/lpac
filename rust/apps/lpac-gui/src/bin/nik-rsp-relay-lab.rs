use eframe::egui;
use lpac_core::{
    encode_transfer_key, generate_transfer_key,
    relay::{
        RelayDirection, RelayPacket, decrypt_relay_packet, demo_relay_session,
        encrypt_relay_packet, validate_relay_chain,
    },
};
use zeroize::Zeroize;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1_260.0, 900.0])
            .with_min_inner_size([940.0, 680.0]),
        ..Default::default()
    };

    eframe::run_native(
        "NIK RSP Relay Lab",
        options,
        Box::new(|creation_context| {
            configure_ui(&creation_context.egui_ctx);
            Ok(Box::<RelayLabApp>::default())
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
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(226, 232, 240);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(219, 234, 254);
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
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
    context.set_style_of(egui::Theme::Light, style);
}

#[derive(Debug)]
struct RelayRecord {
    packet: RelayPacket,
    ciphertext: String,
    aead_verified: bool,
    validation_error: Option<String>,
}

struct RelayLabApp {
    delay_seconds: i64,
    key: [u8; 32],
    records: Vec<RelayRecord>,
    visible_count: usize,
    selected_index: usize,
    chain_valid: bool,
    status: String,
}

impl Default for RelayLabApp {
    fn default() -> Self {
        let mut app = Self {
            delay_seconds: 60,
            key: generate_transfer_key(),
            records: Vec::new(),
            visible_count: 0,
            selected_index: 0,
            chain_valid: false,
            status: String::new(),
        };
        app.rebuild();
        app
    }
}

impl Drop for RelayLabApp {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

impl RelayLabApp {
    fn rebuild(&mut self) {
        self.key.zeroize();
        self.key = generate_transfer_key();
        self.records.clear();
        self.visible_count = 1;
        self.selected_index = 0;

        let packets = match demo_relay_session(self.delay_seconds) {
            Ok(packets) => packets,
            Err(error) => {
                self.chain_valid = false;
                self.status = format!("Не вдалося створити демонстрацію: {error}");
                return;
            }
        };

        let mut decoded_packets = Vec::with_capacity(packets.len());
        for packet in packets {
            let ciphertext = match encrypt_relay_packet(&packet, &self.key) {
                Ok(ciphertext) => ciphertext,
                Err(error) => {
                    self.chain_valid = false;
                    self.status = format!("Помилка шифрування: {error}");
                    return;
                }
            };
            let decoded = match decrypt_relay_packet(&ciphertext, &self.key) {
                Ok(decoded) => decoded,
                Err(error) => {
                    self.chain_valid = false;
                    self.status = format!("Помилка розшифрування: {error}");
                    return;
                }
            };
            let validation_error = decoded
                .validate_after(decoded_packets.last(), decoded.created_at)
                .err()
                .map(|error| error.to_string());
            let aead_verified = decoded == packet;
            decoded_packets.push(decoded.clone());
            self.records.push(RelayRecord {
                packet: decoded,
                ciphertext,
                aead_verified,
                validation_error,
            });
        }

        match validate_relay_chain(&decoded_packets) {
            Ok(()) => {
                self.chain_valid = true;
                self.status = format!(
                    "Ланцюжок валідний: {} пакетів, пауза {} с, одна RSP-транзакція.",
                    self.records.len(),
                    self.delay_seconds
                );
            }
            Err(error) => {
                self.chain_valid = false;
                self.status = format!("Ланцюжок невалідний: {error}");
            }
        }
    }

    fn reveal_next(&mut self) {
        if self.visible_count < self.records.len() {
            self.visible_count += 1;
            self.selected_index = self.visible_count - 1;
        }
    }

    fn show_header(&self, ui: &mut egui::Ui) {
        ui.heading("NIK RSP Relay Lab");
        ui.label("Наочна модель асинхронного store-and-forward обміну між офлайн eUICC та online SM-DP+ broker.");
        ui.add_space(8.0);

        egui::Frame::group(ui.style())
            .fill(egui::Color32::from_rgb(255, 251, 235))
            .stroke(egui::Stroke::new(
                1.0,
                egui::Color32::from_rgb(217, 119, 6),
            ))
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.strong("Що тут реальне");
                ui.label(
                    "NIKRSP1-пакети реально серіалізуються, шифруються XChaCha20-Poly1305, розшифровуються, перевіряються за sequence, transactionId, TTL і previous-message hash.",
                );
                ui.strong("Що поки демонстраційне");
                ui.label(
                    "Значення ES9+/ES10b payload позначені DEMO. Живі staged-виклики libeuicc ще не підключені; поточний lpac download залишається монолітним.",
                );
            });
    }

    fn show_controls(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Параметри сесії");
            ui.horizontal_wrapped(|ui| {
                ui.label("Пауза між пакетами, секунд:");
                ui.add(
                    egui::DragValue::new(&mut self.delay_seconds)
                        .range(0..=300)
                        .speed(1),
                );
                if ui.button("Перегенерувати сесію").clicked() {
                    self.rebuild();
                }
                if ui
                    .add_enabled(
                        self.visible_count < self.records.len(),
                        egui::Button::new("Наступний пакет"),
                    )
                    .clicked()
                {
                    self.reveal_next();
                }
                if ui.button("Показати весь обмін").clicked() {
                    self.visible_count = self.records.len();
                    self.selected_index = self.records.len().saturating_sub(1);
                }
                if ui.button("Скинути прогрес").clicked() {
                    self.visible_count = 0;
                    self.selected_index = 0;
                }
            });

            let status_color = if self.chain_valid {
                egui::Color32::from_rgb(21, 128, 61)
            } else {
                egui::Color32::from_rgb(185, 28, 28)
            };
            ui.colored_label(status_color, &self.status);
            ui.label(format!(
                "Лабораторний ключ: {}…  |  deadline демонстрації: 10 хвилин",
                &encode_transfer_key(&self.key)[..12]
            ));
        });
    }

    fn show_timeline(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Послідовність обміну");

            egui::Grid::new("relay_timeline")
                .num_columns(3)
                .min_col_width(250.0)
                .spacing([16.0, 12.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Офлайн-пристрій / eUICC");
                    ui.strong("Зашифрований канал NIKRSP1");
                    ui.strong("Online broker / SM-DP+");
                    ui.end_row();

                    for (index, record) in self.records.iter().enumerate() {
                        let visible = index < self.visible_count;
                        let packet = &record.packet;
                        let elapsed = self
                            .records
                            .first()
                            .map(|first| {
                                (packet.created_at - first.packet.created_at).num_seconds()
                            })
                            .unwrap_or_default();

                        ui.add_enabled_ui(visible, |ui| {
                            ui.vertical(|ui| {
                                ui.strong(packet.stage.offline_action_uk());
                                ui.small(if packet.direction == RelayDirection::OfflineToOnline {
                                    "формує пакет"
                                } else {
                                    "приймає та виконує APDU"
                                });
                            });
                        });

                        ui.add_enabled_ui(visible, |ui| {
                            let title = format!(
                                "#{}  {}\n{}  t+{}с",
                                packet.sequence,
                                packet.stage.code(),
                                packet.direction.arrow(),
                                elapsed
                            );
                            if ui
                                .selectable_label(visible && self.selected_index == index, title)
                                .clicked()
                                && visible
                            {
                                self.selected_index = index;
                            }
                            if visible {
                                let result =
                                    if record.validation_error.is_none() && record.aead_verified {
                                        "AEAD ✓  chain ✓"
                                    } else {
                                        "перевірка ✕"
                                    };
                                ui.small(result);
                            } else {
                                ui.small("очікує передачі");
                            }
                        });

                        ui.add_enabled_ui(visible, |ui| {
                            ui.vertical(|ui| {
                                ui.strong(packet.stage.online_action_uk());
                                ui.small(if packet.direction == RelayDirection::OnlineToOffline {
                                    "формує пакет"
                                } else {
                                    "приймає та виконує ES9+"
                                });
                            });
                        });
                        ui.end_row();
                    }
                });
        });
    }

    fn show_selected_packet(&mut self, ui: &mut egui::Ui) {
        if self.records.is_empty() || self.visible_count == 0 {
            ui.group(|ui| {
                ui.set_min_width(ui.available_width());
                ui.heading("Деталі пакета");
                ui.label("Натисніть «Наступний пакет» або «Показати весь обмін».");
            });
            return;
        }

        let index = self
            .selected_index
            .min(self.visible_count.saturating_sub(1))
            .min(self.records.len() - 1);
        let packet = self.records[index].packet.clone();
        let fields = packet.payload_fields().join(", ");
        let previous_hash = packet
            .previous_message_hash
            .as_deref()
            .map(|hash| format!("{}…", &hash[..hash.len().min(18)]))
            .unwrap_or_else(|| "немає — перший пакет".into());
        let validation = self.records[index]
            .validation_error
            .clone()
            .unwrap_or_else(|| "успішно".into());

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading(format!(
                "Пакет #{} — {}",
                packet.sequence,
                packet.stage.label_uk()
            ));
            egui::Grid::new("selected_packet_metadata")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Job ID");
                    ui.monospace(packet.job_id.to_string());
                    ui.end_row();
                    ui.label("Напрямок");
                    ui.label(packet.direction.label_uk());
                    ui.end_row();
                    ui.label("Transaction ID");
                    ui.monospace(packet.transaction_id.as_deref().unwrap_or("ще не виданий"));
                    ui.end_row();
                    ui.label("Створено");
                    ui.monospace(packet.created_at.to_rfc3339());
                    ui.end_row();
                    ui.label("Дійсний до");
                    ui.monospace(packet.expires_at.to_rfc3339());
                    ui.end_row();
                    ui.label("Previous hash");
                    ui.monospace(previous_hash);
                    ui.end_row();
                    ui.label("Payload fields");
                    ui.label(fields);
                    ui.end_row();
                    ui.label("Перевірка");
                    ui.label(validation);
                    ui.end_row();
                    ui.label("Encrypted size");
                    ui.label(format!("{} символів", self.records[index].ciphertext.len()));
                    ui.end_row();
                });

            ui.collapsing(
                "Показати зашифровану NIKRSP1 строку",
                |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.records[index].ciphertext)
                            .desired_rows(7)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace),
                    );
                },
            );
        });
    }
}

impl eframe::App for RelayLabApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::from_rgb(245, 247, 250).to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
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
                        ui.set_max_width(1_220.0);
                        self.show_header(ui);
                        ui.add_space(10.0);
                        self.show_controls(ui);
                        ui.add_space(10.0);
                        self.show_timeline(ui);
                        ui.add_space(10.0);
                        self.show_selected_packet(ui);
                    });
            });
    }
}
