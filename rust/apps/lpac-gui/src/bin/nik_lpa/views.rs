use std::path::PathBuf;

use eframe::egui;
use lpac_core::relay::RelayStage;

use crate::{
    app::NikLpaApp,
    controller::WorkerCommand,
    model::{AppMode, Page, decode_debug_packet, pretty_debug_packet},
    theme,
    widgets::{card, metric_card, role_card, status_badge, timeline_step, transfer_card},
};

impl NikLpaApp {
    pub(crate) fn render_root(&mut self, ui: &mut egui::Ui) {
        if self.mode == AppMode::Welcome {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    self.render_welcome(ui);
                });
            return;
        }

        let available_height = ui.available_height();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let sidebar_width = if ui.available_width() < 1_050.0 {
                176.0
            } else {
                196.0
            };

            ui.allocate_ui_with_layout(
                egui::vec2(sidebar_width, available_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.render_sidebar(ui),
            );
            ui.separator();

            let content_size = ui.available_size();
            ui.allocate_ui_with_layout(
                content_size,
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        ui.vertical(|ui| {
                            let width = (ui.available_width() - 14.0).max(320.0);
                            ui.set_width(width);
                            self.render_header(ui);

                            if self.page == Page::Transfer {
                                self.render_timeline_sticky(ui);
                                ui.add_space(8.0);
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.set_min_width(width);
                                        self.render_transfer(ui);
                                        ui.add_space(18.0);
                                    });
                            } else {
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.set_min_width(width);
                                        match self.page {
                                            Page::Dashboard => self.render_dashboard(ui),
                                            Page::Profiles => self.render_profiles(ui),
                                            Page::Install => self.render_install(ui),
                                            Page::Sessions => self.render_sessions(ui),
                                            Page::Logs => self.render_logs(ui),
                                            Page::Settings => self.render_settings(ui),
                                            Page::Transfer => {}
                                        }
                                        ui.add_space(18.0);
                                    });
                            }
                        });
                    });
                },
            );
        });

        self.render_packet_windows(ui.ctx());
    }

    fn render_welcome(&mut self, ui: &mut egui::Ui) {
        ui.add_space(42.0);
        ui.vertical_centered(|ui| {
            ui.heading("NIK LPA");
            ui.small("eUICC / RSP engineering utility");
        });
        ui.add_space(24.0);

        let mut selected = None;
        if ui.available_width() >= 900.0 {
            ui.columns(3, |columns| {
                if role_card(&mut columns[0], "Local LPA", "PC/SC + SM-DP+", "Відкрити") {
                    selected = Some(AppMode::Local);
                }
                if role_card(&mut columns[1], "Card Agent", "PC/SC / eUICC", "Відкрити") {
                    selected = Some(AppMode::CardAgent);
                }
                if role_card(&mut columns[2], "Server Agent", "SM-DP+ / ES9+", "Відкрити") {
                    selected = Some(AppMode::ServerAgent);
                }
            });
        } else {
            for (mode, title, subtitle) in [
                (AppMode::Local, "Local LPA", "PC/SC + SM-DP+"),
                (AppMode::CardAgent, "Card Agent", "PC/SC / eUICC"),
                (AppMode::ServerAgent, "Server Agent", "SM-DP+ / ES9+"),
            ] {
                if role_card(ui, title, subtitle, "Відкрити") {
                    selected = Some(mode);
                }
                ui.add_space(8.0);
            }
        }

        if let Some(mode) = selected {
            self.reset_mode(mode);
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        let mut change_mode = false;
        let sidebar_size = ui.available_size();

        egui::Frame::new()
            .fill(theme::sidebar(self.dark_mode))
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_min_size(sidebar_size - egui::vec2(24.0, 24.0));
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.heading("NIK LPA");
                    ui.small(self.mode.title());
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(5.0);

                    for page in Page::ALL {
                        let allowed = !matches!(
                            (self.mode, page),
                            (AppMode::ServerAgent, Page::Profiles | Page::Install)
                        );
                        if ui
                            .add_enabled(
                                allowed,
                                egui::Button::new(page.label())
                                    .selected(self.page == page)
                                    .min_size(egui::vec2(ui.available_width(), 31.0)),
                            )
                            .clicked()
                        {
                            self.page = page;
                        }
                    }

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        change_mode = ui
                            .add_sized(
                                [ui.available_width(), 31.0],
                                egui::Button::new("Змінити режим"),
                            )
                            .clicked();
                        ui.add_space(5.0);
                        ui.small(if self.relay_running {
                            "● Relay active"
                        } else {
                            "○ Relay stopped"
                        });
                        ui.small("DEBUG / PLAINTEXT");
                    });
                });
            });

        if change_mode {
            self.reset_mode(AppMode::Welcome);
        }
    }

    fn render_header(&mut self, ui: &mut egui::Ui) {
        let mut toggle_theme = false;
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.heading(self.page.label());
                ui.small(self.mode.description());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                toggle_theme = ui
                    .button(if self.dark_mode { "Light" } else { "Dark" })
                    .clicked();
                if self.page == Page::Transfer {
                    status_badge(ui, "PLAINTEXT DEBUG", theme::WARNING);
                }
                if self.busy {
                    ui.spinner();
                }
            });
        });
        if toggle_theme {
            self.dark_mode = !self.dark_mode;
            theme::configure(ui.ctx(), self.dark_mode);
        }
        ui.add_space(8.0);
    }

    fn render_dashboard(&mut self, ui: &mut egui::Ui) {
        if matches!(self.mode, AppMode::CardAgent | AppMode::Local) {
            self.render_reader_selector(ui);
            ui.add_space(8.0);
        }

        if ui.available_width() >= 760.0 {
            ui.columns(3, |columns| {
                metric_card(&mut columns[0], "Режим", self.mode.title());
                metric_card(&mut columns[1], "Transport", "Plaintext debug");
                metric_card(
                    &mut columns[2],
                    "Relay",
                    if self.relay_running {
                        "Active"
                    } else {
                        "Stopped"
                    },
                );
            });
        } else {
            metric_card(ui, "Режим", self.mode.title());
            ui.add_space(6.0);
            metric_card(ui, "Transport", "Plaintext debug");
            ui.add_space(6.0);
            metric_card(
                ui,
                "Relay",
                if self.relay_running {
                    "Active"
                } else {
                    "Stopped"
                },
            );
        }

        ui.add_space(8.0);
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(
                    self.session
                        .as_ref()
                        .map(|session| session.status.as_str())
                        .unwrap_or("Relay session: idle"),
                );
                if matches!(self.mode, AppMode::CardAgent | AppMode::ServerAgent)
                    && ui.button("Відкрити RSP Relay").clicked()
                {
                    self.page = Page::Transfer;
                }
            });
        });
    }

    fn render_profiles(&mut self, ui: &mut egui::Ui) {
        self.render_reader_selector(ui);
        ui.add_space(8.0);

        let mut refresh = false;
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Profile List");
                refresh = ui
                    .add_enabled(
                        !self.busy && !self.relay_running,
                        egui::Button::new("Оновити"),
                    )
                    .clicked();
            });
            ui.add(
                egui::TextEdit::multiline(&mut self.profiles_output)
                    .desired_rows(18)
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
        ui.add_space(8.0);

        let mut install = false;
        card(ui, |ui| {
            ui.strong("Local install");
            ui.label("Activation code");
            ui.add(
                egui::TextEdit::multiline(&mut self.local_activation_code)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            ui.label("Confirmation code");
            ui.add(
                egui::TextEdit::singleline(&mut self.local_confirmation_code)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            install = ui
                .add_enabled(
                    !self.busy && !self.relay_running,
                    egui::Button::new("Встановити"),
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
        match self.mode {
            AppMode::CardAgent => self.render_card_transfer(ui),
            AppMode::ServerAgent => self.render_server_transfer(ui),
            AppMode::Local => card(ui, |ui| {
                ui.strong("Relay unavailable in Local mode");
            }),
            AppMode::Welcome => {}
        }
    }

    fn render_card_transfer(&mut self, ui: &mut egui::Ui) {
        self.render_reader_selector(ui);
        ui.add_space(8.0);

        let mut start = false;
        let mut stop = false;
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Card Agent");
                start = ui
                    .add_enabled(
                        !self.busy && !self.relay_running,
                        egui::Button::new("Нова сесія"),
                    )
                    .clicked();
                stop = ui
                    .add_enabled(self.relay_running, egui::Button::new("Зупинити"))
                    .clicked();
            });
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
            ui.strong("Server Agent");
            ui.label("Activation code");
            ui.add(
                egui::TextEdit::multiline(&mut self.activation_code)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            ui.label("Confirmation code");
            ui.add(
                egui::TextEdit::singleline(&mut self.confirmation_code)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            ui.horizontal_wrapped(|ui| {
                start = ui
                    .add_enabled(
                        !self.busy && !self.relay_running,
                        egui::Button::new("Запустити"),
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
        ui.add_space(8.0);

        if self.mode == AppMode::ServerAgent && self.session.is_none() && self.relay_running {
            let mut accept = false;
            let mut open_full = false;
            transfer_card(ui, false, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong("ОТРИМАТИ ← Card Agent");
                    status_badge(ui, "INIT_REQUEST", theme::WARNING);
                });
                ui.add(
                    egui::TextEdit::multiline(&mut self.server_bootstrap_input)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
                ui.horizontal_wrapped(|ui| {
                    accept = ui
                        .add_enabled(
                            !self.busy && !self.server_bootstrap_input.trim().is_empty(),
                            egui::Button::new("Прийняти INIT_REQUEST"),
                        )
                        .clicked();
                    open_full = ui
                        .add_enabled(
                            !self.server_bootstrap_input.is_empty(),
                            egui::Button::new("Відкрити повністю"),
                        )
                        .clicked();
                    ui.small(format!("{} байт", self.server_bootstrap_input.len()));
                });
            });
            if open_full {
                self.show_bootstrap_packet = true;
            }
            if accept {
                self.accept_server_bootstrap();
            }
            return;
        }

        if self.session.is_none() {
            return;
        }

        let peer_label = self
            .session
            .as_ref()
            .map(|session| session.side.peer_label())
            .unwrap_or("Peer");

        let mut open_outgoing = false;
        if let Some(session) = self.session.as_mut()
            && !session.outgoing_text.is_empty()
        {
            let stage = decode_debug_packet(&session.outgoing_text)
                .ok()
                .map(|packet| packet.stage.code())
                .unwrap_or("PACKET");
            transfer_card(ui, true, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(format!("НАДІСЛАТИ → {peer_label}"));
                    status_badge(ui, stage, theme::ACCENT);
                });
                ui.add(
                    egui::TextEdit::multiline(&mut session.outgoing_text)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .interactive(false),
                );
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Копіювати").clicked() {
                        ui.ctx().copy_text(session.outgoing_text.clone());
                    }
                    open_outgoing = ui.button("Відкрити повністю").clicked();
                    ui.small(format!("{} байт", session.outgoing_text.len()));
                });
            });
        }
        if open_outgoing {
            self.show_outgoing_packet = true;
        }

        ui.add_space(8.0);
        let busy = self.busy;
        let mut import = false;
        let mut open_incoming = false;
        if let Some(session) = self.session.as_mut() {
            let expected = session.expected_incoming();
            transfer_card(ui, false, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(format!("ОТРИМАТИ ← {peer_label}"));
                    if let Some(stage) = expected {
                        status_badge(ui, stage.code(), theme::WARNING);
                    }
                });
                ui.add(
                    egui::TextEdit::multiline(&mut session.incoming_text)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
                ui.horizontal_wrapped(|ui| {
                    import = ui
                        .add_enabled(
                            !busy && expected.is_some() && !session.incoming_text.trim().is_empty(),
                            egui::Button::new("Прийняти та виконати"),
                        )
                        .clicked();
                    open_incoming = ui
                        .add_enabled(
                            !session.incoming_text.is_empty(),
                            egui::Button::new("Відкрити повністю"),
                        )
                        .clicked();
                    ui.small(format!("{} байт", session.incoming_text.len()));
                });
            });
        }
        if open_incoming {
            self.show_incoming_packet = true;
        }
        if import {
            self.import_session_incoming();
        }
    }

    fn render_timeline_sticky(&mut self, ui: &mut egui::Ui) {
        let completed = self
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
        let active = self
            .session
            .as_ref()
            .and_then(|session| session.last_packet())
            .and_then(|packet| packet.stage.next())
            .or((self.session.is_none()).then_some(RelayStage::InitRequest));

        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("RSP FLOW");
                if let Some(session) = self.session.as_ref() {
                    ui.small(session.status.as_str());
                }
            });
            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (index, stage) in RelayStage::ALL.into_iter().enumerate() {
                            timeline_step(
                                ui,
                                index + 1,
                                stage.code(),
                                completed.contains(&stage),
                                active == Some(stage),
                            );
                            if index + 1 < RelayStage::ALL.len() {
                                ui.monospace("→");
                            }
                        }
                    });
                });
        });
    }

    fn render_sessions(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.strong("Current session");
            if let Some(session) = self.session.as_ref() {
                egui::Grid::new("session_summary")
                    .num_columns(2)
                    .spacing([18.0, 6.0])
                    .show(ui, |ui| {
                        ui.small("Job ID");
                        ui.monospace(session.job_id.to_string());
                        ui.end_row();
                        ui.small("Side");
                        ui.label(format!("{:?}", session.side));
                        ui.end_row();
                        ui.small("Packets");
                        ui.label(session.packets.len().to_string());
                        ui.end_row();
                        ui.small("Status");
                        ui.label(&session.status);
                        ui.end_row();
                        ui.small("Completed");
                        ui.label(if session.completed { "yes" } else { "no" });
                        ui.end_row();
                    });
            } else {
                ui.label("Idle");
            }
        });

        if let Some(session) = self.session.as_ref()
            && !session.timeline.is_empty()
        {
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.strong("Audit");
                egui::ScrollArea::horizontal()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Grid::new("rsp_audit")
                            .num_columns(4)
                            .spacing([14.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.strong("UTC");
                                ui.strong("Stage");
                                ui.strong("Size");
                                ui.strong("Result");
                                ui.end_row();
                                for event in &session.timeline {
                                    ui.monospace(event.timestamp.format("%H:%M:%S").to_string());
                                    ui.monospace(event.stage.code());
                                    ui.label(format!("{} B", event.packet_size));
                                    ui.label(&event.note);
                                    ui.end_row();
                                }
                            });
                    });
            });
        }
    }

    fn render_logs(&mut self, ui: &mut egui::Ui) {
        let mut clear = false;
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Runtime log");
                clear = ui.button("Очистити").clicked();
            });
            let mut text = self.log.iter().cloned().collect::<Vec<_>>().join("\n");
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .desired_rows(24)
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
            ui.strong("Runtime");
            ui.label("lpac");
            ui.add(egui::TextEdit::singleline(&mut self.lpac_path).desired_width(f32::INFINITY));
            ui.checkbox(&mut self.dark_mode, "Dark theme");
            apply_theme = ui.button("Застосувати").clicked();
        });
        if apply_theme {
            theme::configure(ui.ctx(), self.dark_mode);
        }

        ui.add_space(8.0);
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Relay transport");
                status_badge(ui, "PLAINTEXT DEBUG", theme::WARNING);
            });
        });
    }

    fn render_reader_selector(&mut self, ui: &mut egui::Ui) {
        let mut discover = false;
        let mut chip_info = false;
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("PC/SC");
                if self.readers.is_empty() {
                    ui.label(format!("Reader #{}", self.selected_reader));
                } else {
                    egui::ComboBox::from_id_salt("production_pcsc_reader")
                        .selected_text(
                            self.readers
                                .iter()
                                .find(|reader| reader.index == self.selected_reader)
                                .map(|reader| format!("{} — {}", reader.index, reader.name))
                                .unwrap_or_else(|| format!("Reader #{}", self.selected_reader)),
                        )
                        .width((ui.available_width() * 0.55).clamp(220.0, 520.0))
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
                        egui::Button::new("Refresh"),
                    )
                    .clicked();
                chip_info = ui
                    .add_enabled(
                        !self.busy && !self.relay_running,
                        egui::Button::new("eUICC info"),
                    )
                    .clicked();
            });

            if let Some(eid) = self.card_eid.as_deref() {
                let redacted = if eid.len() > 10 {
                    format!("{}…{}", &eid[..6], &eid[eid.len() - 4..])
                } else {
                    eid.to_owned()
                };
                ui.monospace(format!("EID {redacted}"));
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

    fn render_packet_windows(&mut self, context: &egui::Context) {
        if self.show_outgoing_packet {
            let raw = self
                .session
                .as_ref()
                .map(|session| session.outgoing_text.clone())
                .unwrap_or_default();
            let mut pretty = pretty_debug_packet(&raw);
            let mut open = self.show_outgoing_packet;
            egui::Window::new("OUTGOING PACKET")
                .open(&mut open)
                .default_size([860.0, 620.0])
                .resizable(true)
                .show(context, |ui| {
                    ui.horizontal(|ui| {
                        status_badge(ui, "PLAINTEXT", theme::WARNING);
                        if ui.button("Copy raw").clicked() {
                            ui.ctx().copy_text(raw.clone());
                        }
                        ui.label(format!("{} байт", raw.len()));
                    });
                    ui.add(
                        egui::TextEdit::multiline(&mut pretty)
                            .desired_rows(32)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace)
                            .interactive(false),
                    );
                });
            self.show_outgoing_packet = open;
        }

        if self.show_incoming_packet {
            let raw = self
                .session
                .as_ref()
                .map(|session| session.incoming_text.clone())
                .unwrap_or_default();
            let mut pretty = pretty_debug_packet(&raw);
            let mut open = self.show_incoming_packet;
            egui::Window::new("INCOMING PACKET")
                .open(&mut open)
                .default_size([860.0, 620.0])
                .resizable(true)
                .show(context, |ui| {
                    ui.horizontal(|ui| {
                        status_badge(ui, "PLAINTEXT", theme::WARNING);
                        ui.label(format!("{} байт", raw.len()));
                    });
                    ui.add(
                        egui::TextEdit::multiline(&mut pretty)
                            .desired_rows(32)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace)
                            .interactive(false),
                    );
                });
            self.show_incoming_packet = open;
        }

        if self.show_bootstrap_packet {
            let raw = self.server_bootstrap_input.clone();
            let mut pretty = pretty_debug_packet(&raw);
            let mut open = self.show_bootstrap_packet;
            egui::Window::new("INIT_REQUEST")
                .open(&mut open)
                .default_size([860.0, 620.0])
                .resizable(true)
                .show(context, |ui| {
                    ui.horizontal(|ui| {
                        status_badge(ui, "PLAINTEXT", theme::WARNING);
                        ui.label(format!("{} байт", raw.len()));
                    });
                    ui.add(
                        egui::TextEdit::multiline(&mut pretty)
                            .desired_rows(32)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace)
                            .interactive(false),
                    );
                });
            self.show_bootstrap_packet = open;
        }
    }
}
