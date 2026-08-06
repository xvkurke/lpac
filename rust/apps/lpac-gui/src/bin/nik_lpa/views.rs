use std::path::PathBuf;

use eframe::egui;
use lpac_core::relay::RelayStage;

use crate::{
    app::NikLpaApp,
    controller::WorkerCommand,
    model::{AppMode, Page},
    theme,
    widgets::{card, metric_card, role_card},
};

impl NikLpaApp {
    pub(crate) fn render_root(&mut self, ui: &mut egui::Ui) {
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
    }

    fn render_welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);
            ui.heading("NIK LPA");
            ui.label("Керування eUICC та ручний staged RSP без Інтернету на Card Agent");
            ui.add_space(28.0);
        });

        let mut selected = None;
        ui.columns(3, |columns| {
            if role_card(
                &mut columns[0],
                "Локальний LPA",
                "Рідер та Інтернет на одному ПК. Звичайне встановлення і керування профілями.",
                "Обрати режим",
            ) {
                selected = Some(AppMode::Local);
            }
            if role_card(
                &mut columns[1],
                "Card Agent",
                "Працює з eUICC та PC/SC. Мережеві ES9+ запити не виконує.",
                "Обрати режим",
            ) {
                selected = Some(AppMode::CardAgent);
            }
            if role_card(
                &mut columns[2],
                "Server Agent",
                "Працює з activation code та SM-DP+. Рідер не потрібен.",
                "Обрати режим",
            ) {
                selected = Some(AppMode::ServerAgent);
            }
        });
        if let Some(mode) = selected {
            self.reset_mode(mode);
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        let mut change_mode = false;
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
                    let allowed = !matches!(
                        (self.mode, page),
                        (AppMode::ServerAgent, Page::Profiles | Page::Install)
                    );
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
                    change_mode = ui.button("Змінити режим").clicked();
                    ui.label(if self.relay_running {
                        "● Relay Agent активний"
                    } else {
                        "○ Relay Agent зупинений"
                    });
                });
            });
        if change_mode {
            self.reset_mode(AppMode::Welcome);
        }
    }

    fn render_header(&mut self, ui: &mut egui::Ui) {
        let mut toggle_theme = false;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(self.page.label());
                ui.label(self.mode.description());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                toggle_theme = ui
                    .button(if self.dark_mode { "Світла" } else { "Темна" })
                    .clicked();
                if self.busy {
                    ui.spinner();
                    ui.label("Виконується операція");
                }
            });
        });
        if toggle_theme {
            self.dark_mode = !self.dark_mode;
            theme::configure(ui.ctx(), self.dark_mode);
        }
        ui.add_space(10.0);
    }

    fn render_dashboard(&mut self, ui: &mut egui::Ui) {
        ui.columns(3, |columns| {
            metric_card(&mut columns[0], "Режим", self.mode.title());
            metric_card(
                &mut columns[1],
                "Pairing",
                if self.peer.is_some() {
                    "Підключено"
                } else {
                    "Не налаштовано"
                },
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
        let mut next_page = None;
        card(ui, |ui| {
            ui.heading("Швидкий старт");
            match self.mode {
                AppMode::CardAgent => {
                    ui.label(
                        "1. Виберіть рідер. 2. Спарте Server Agent. 3. Створіть INIT_REQUEST.",
                    );
                    if ui.button("Перейти до ручної передачі").clicked() {
                        next_page = Some(Page::Transfer);
                    }
                }
                AppMode::ServerAgent => {
                    ui.label(
                        "1. Спарте Card Agent. 2. Введіть activation code. 3. Запустіть Server Agent.",
                    );
                    if ui.button("Перейти до ручної передачі").clicked() {
                        next_page = Some(Page::Transfer);
                    }
                }
                AppMode::Local => {
                    ui.label(
                        "Підключіть рідер, оновіть список і виконайте звичайне встановлення профілю.",
                    );
                    if ui.button("Перейти до встановлення").clicked() {
                        next_page = Some(Page::Install);
                    }
                }
                AppMode::Welcome => {}
            }
        });
        if let Some(page) = next_page {
            self.page = page;
        }
    }

    fn render_profiles(&mut self, ui: &mut egui::Ui) {
        self.render_reader_selector(ui);
        ui.add_space(10.0);

        let mut refresh = false;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Профілі eUICC");
                refresh = ui
                    .add_enabled(
                        !self.busy && !self.relay_running,
                        egui::Button::new("Оновити"),
                    )
                    .clicked();
            });
            ui.add(
                egui::TextEdit::multiline(&mut self.profiles_output)
                    .desired_rows(20)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace)
                    .interactive(false),
            );
        });
        if refresh {
            self.send(
                WorkerCommand::Profiles {
                    executable: PathBuf::from(&self.lpac_path),
                    reader_index: self.selected_reader,
                },
                "Читається Profile List",
            );
        }
    }

    fn render_install(&mut self, ui: &mut egui::Ui) {
        self.render_reader_selector(ui);
        ui.add_space(10.0);

        let mut install = false;
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
            install = ui
                .add_enabled(
                    !self.busy && !self.relay_running,
                    egui::Button::new("Встановити та перевірити ICCID"),
                )
                .clicked();
        });
        if install {
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
    }

    fn render_transfer(&mut self, ui: &mut egui::Ui) {
        self.render_pairing(ui);
        ui.add_space(10.0);
        match self.mode {
            AppMode::CardAgent => self.render_card_transfer(ui),
            AppMode::ServerAgent => self.render_server_transfer(ui),
            AppMode::Local => card(ui, |ui| {
                ui.heading("Relay не потрібен у локальному режимі");
                ui.label("Оберіть Card Agent або Server Agent через кнопку «Змінити режим».");
            }),
            AppMode::Welcome => {}
        }
    }

    fn render_pairing(&mut self, ui: &mut egui::Ui) {
        let mut pair_clicked = false;
        let mut own_code = self.identity.pairing_code();
        card(ui, |ui| {
            ui.heading("Захищене pairing");
            ui.label("Мій pairing code");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut own_code)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .interactive(false),
                );
                if ui.button("Копіювати").clicked() {
                    ui.ctx().copy_text(own_code.clone());
                }
            });
            ui.label("Pairing code іншої сторони");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.peer_code_input)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
                pair_clicked = ui.button("Спарити").clicked();
            });
            ui.small("NIKRSP2 пакети приймаються лише від спареного X25519 peer.");
        });
        if pair_clicked {
            self.pair_peer();
        }
    }

    fn render_card_transfer(&mut self, ui: &mut egui::Ui) {
        self.render_reader_selector(ui);
        ui.add_space(10.0);

        let mut start = false;
        let mut stop = false;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Card Agent сесія");
                start = ui
                    .add_enabled(
                        !self.busy && !self.relay_running && self.peer.is_some(),
                        egui::Button::new("Нова сесія з eUICC"),
                    )
                    .clicked();
                stop = ui
                    .add_enabled(self.relay_running, egui::Button::new("Зупинити"))
                    .clicked();
            });
            ui.label(
                "Card Agent не виконує HTTP-запитів. Він лише працює з PC/SC та імпортованими пакетами.",
            );
        });
        if start {
            self.start_card_session();
        }
        if stop {
            self.stop_relay();
        }
        self.render_session_exchange(ui);
    }

    fn render_server_transfer(&mut self, ui: &mut egui::Ui) {
        let mut start = false;
        let mut stop = false;
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
                start = ui
                    .add_enabled(
                        !self.busy && !self.relay_running && self.peer.is_some(),
                        egui::Button::new("Запустити Server Agent"),
                    )
                    .clicked();
                stop = ui
                    .add_enabled(self.relay_running, egui::Button::new("Зупинити"))
                    .clicked();
            });
        });
        if start {
            self.start_server_session();
        }
        if stop {
            self.stop_relay();
        }
        self.render_session_exchange(ui);
    }

    fn render_session_exchange(&mut self, ui: &mut egui::Ui) {
        self.render_timeline(ui);
        ui.add_space(10.0);

        if self.mode == AppMode::ServerAgent && self.session.is_none() && self.relay_running {
            let mut accept = false;
            card(ui, |ui| {
                ui.heading("Вхідний INIT_REQUEST");
                ui.label("Вставте NIKRSP2 строку, створену Card Agent.");
                ui.add(
                    egui::TextEdit::multiline(&mut self.server_bootstrap_input)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
                accept = ui
                    .add_enabled(
                        !self.busy && !self.server_bootstrap_input.trim().is_empty(),
                        egui::Button::new("Прийняти INIT_REQUEST"),
                    )
                    .clicked();
            });
            if accept {
                self.accept_server_bootstrap();
            }
            return;
        }

        if self.session.is_none() {
            card(ui, |ui| {
                ui.heading("Сесію ще не створено");
                ui.label(match self.mode {
                    AppMode::CardAgent => "Натисніть «Нова сесія з eUICC».",
                    AppMode::ServerAgent => "Запустіть Server Agent і вставте INIT_REQUEST.",
                    _ => "",
                });
            });
            return;
        }

        if let Some(session) = self.session.as_mut() {
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
                        .add_enabled(
                            !session.outgoing_text.is_empty(),
                            egui::Button::new("Копіювати строку"),
                        )
                        .clicked()
                    {
                        ui.ctx().copy_text(session.outgoing_text.clone());
                    }
                    ui.label(format!("{} байт", session.outgoing_text.len()));
                });
            });
        }

        ui.add_space(10.0);
        let busy = self.busy;
        let mut import = false;
        if let Some(session) = self.session.as_mut() {
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
                import = ui
                    .add_enabled(
                        !busy && session.expected_incoming().is_some(),
                        egui::Button::new("Розшифрувати, перевірити та виконати"),
                    )
                    .clicked();
            });
        }
        if import {
            self.import_session_incoming();
        }
    }

    fn render_timeline(&mut self, ui: &mut egui::Ui) {
        let completed_stages = self
            .session
            .as_ref()
            .map(|session| {
                session
                    .packets
                    .iter()
                    .map(|packet| packet.stage)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mode = self.mode;

        card(ui, |ui| {
            ui.heading("Послідовність RSP");
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
                        ui.label(if completed_stages.contains(&stage) {
                            "✓"
                        } else {
                            "○"
                        });
                        ui.label(stage.code());
                        ui.monospace(stage.direction().arrow());
                        ui.label(match mode {
                            AppMode::CardAgent => stage.offline_action_uk(),
                            AppMode::ServerAgent => stage.online_action_uk(),
                            _ => stage.label_uk(),
                        });
                        ui.end_row();
                    }
                });

            if let Some(session) = self.session.as_ref() {
                if !session.timeline.is_empty() {
                    ui.separator();
                    ui.strong("Audit подій");
                    egui::Grid::new("rsp_audit_timeline")
                        .num_columns(4)
                        .spacing([12.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Час UTC");
                            ui.strong("Стадія");
                            ui.strong("Розмір");
                            ui.strong("Результат");
                            ui.end_row();
                            for event in &session.timeline {
                                ui.monospace(event.timestamp.format("%H:%M:%S").to_string());
                                ui.monospace(event.stage.code());
                                ui.label(format!("{} байт", event.packet_size));
                                ui.label(&event.note);
                                ui.end_row();
                            }
                        });
                }

                ui.separator();
                ui.label(format!("Job ID: {}", session.job_id));
                ui.label(format!("Статус: {}", session.status));
                ui.label(format!(
                    "Локальний deadline: {} UTC",
                    session.deadline.format("%Y-%m-%d %H:%M:%S")
                ));
                ui.small(
                    "SM-DP+ може завершити свою транзакцію раніше за локальний anti-replay deadline.",
                );
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
                ui.label(format!(
                    "Завершено: {}",
                    if session.completed { "так" } else { "ні" }
                ));
            } else {
                ui.label("Активної сесії немає.");
            }
        });
    }

    fn render_logs(&mut self, ui: &mut egui::Ui) {
        let mut clear = false;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Журнал операцій");
                clear = ui.button("Очистити").clicked();
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
        if clear {
            self.log.clear();
        }
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        let mut apply_theme = false;
        card(ui, |ui| {
            ui.heading("Runtime");
            ui.label("Шлях до lpac");
            ui.add(egui::TextEdit::singleline(&mut self.lpac_path).desired_width(f32::INFINITY));
            ui.checkbox(&mut self.dark_mode, "Темна тема");
            apply_theme = ui.button("Застосувати тему").clicked();
        });
        if apply_theme {
            theme::configure(ui.ctx(), self.dark_mode);
        }

        ui.add_space(10.0);
        card(ui, |ui| {
            ui.heading("Безпека");
            ui.label("Пакети: NIKRSP2 / X25519 / HKDF-SHA256 / XChaCha20-Poly1305.");
            ui.label(
                "Activation code не записується в журнал; Card Agent отримує лише потрібні дані всередині ciphertext.",
            );
        });
    }

    fn render_reader_selector(&mut self, ui: &mut egui::Ui) {
        let mut discover = false;
        let mut chip_info = false;
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
                discover = ui
                    .add_enabled(
                        !self.busy && !self.relay_running,
                        egui::Button::new("Оновити рідери"),
                    )
                    .clicked();
                chip_info = ui
                    .add_enabled(
                        !self.busy && !self.relay_running,
                        egui::Button::new("Інформація eUICC"),
                    )
                    .clicked();
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

        if discover {
            self.send(
                WorkerCommand::DiscoverReaders {
                    executable: PathBuf::from(&self.lpac_path),
                },
                "Шукаються PC/SC-рідери",
            );
        }
        if chip_info {
            self.send(
                WorkerCommand::ChipInfo {
                    executable: PathBuf::from(&self.lpac_path),
                    reader_index: self.selected_reader,
                },
                "Читається інформація eUICC",
            );
        }
    }
}
