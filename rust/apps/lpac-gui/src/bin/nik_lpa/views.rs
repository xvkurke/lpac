use std::path::PathBuf;

use eframe::egui;
use lpac_core::relay::{RelayPacket, RelayStage};

use crate::{
    app::NikLpaApp,
    controller::WorkerCommand,
    model::{AppMode, Page, decode_debug_packet, pretty_debug_packet},
    theme,
    widgets::{
        card, compact_card, key_value, metric_card, nav_item, primary_button, role_card,
        secondary_button, status_badge, timeline_step, transfer_card,
    },
};

impl NikLpaApp {
    pub(crate) fn render_root(&mut self, ui: &mut egui::Ui) {
        if self.mode == AppMode::Welcome {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| self.render_welcome(ui));
            self.render_packet_windows(ui.ctx());
            return;
        }

        let available_height = ui.available_height();
        let collapsed_sidebar = ui.available_width() < 1_120.0;
        let sidebar_width = if collapsed_sidebar { 60.0 } else { 260.0 };

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(
                egui::vec2(sidebar_width, available_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.render_sidebar(ui, collapsed_sidebar),
            );

            let workspace_size = ui.available_size();
            ui.allocate_ui_with_layout(
                workspace_size,
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    self.render_topbar(ui);

                    if self.page == Page::Transfer {
                        Self::content_rail(ui, |ui| {
                            ui.add_space(16.0);
                            self.render_timeline_sticky(ui);
                            ui.add_space(16.0);
                        });

                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                Self::content_rail(ui, |ui| {
                                    self.render_transfer(ui);
                                    ui.add_space(40.0);
                                });
                            });
                    } else {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                Self::content_rail(ui, |ui| {
                                    ui.add_space(32.0);
                                    match self.page {
                                        Page::Dashboard => self.render_dashboard(ui),
                                        Page::Profiles => self.render_profiles(ui),
                                        Page::Install => self.render_install(ui),
                                        Page::Sessions => self.render_sessions(ui),
                                        Page::Logs => self.render_logs(ui),
                                        Page::Settings => self.render_settings(ui),
                                        Page::Transfer => {}
                                    }
                                    ui.add_space(40.0);
                                });
                            });
                    }
                },
            );
        });

        self.render_packet_windows(ui.ctx());
    }

    fn content_rail(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
        let available = ui.available_width();
        let gutter = if available < 900.0 { 16.0 } else { 24.0 };
        let rail_width = (available - gutter * 2.0).clamp(320.0, 830.0);
        let side_space = ((available - rail_width) / 2.0).max(0.0);

        ui.horizontal(|ui| {
            ui.add_space(side_space);
            ui.allocate_ui_with_layout(
                egui::vec2(rail_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(rail_width);
                    content(ui);
                },
            );
        });
    }

    fn render_welcome(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_width();
        let rail_width = (available - 48.0).clamp(320.0, 830.0);
        let side_space = ((available - rail_width) / 2.0).max(0.0);
        let mut selected = None;

        ui.horizontal(|ui| {
            ui.add_space(side_space);
            ui.vertical(|ui| {
                ui.set_width(rail_width);
                ui.add_space(64.0);
                nik_wordmark(ui, 28.0);
                ui.label(
                    egui::RichText::new("eUICC engineering utility")
                        .size(14.0)
                        .color(theme::muted_text(ui.visuals().dark_mode)),
                );
                ui.add_space(32.0);

                if rail_width >= 760.0 {
                    ui.columns(3, |columns| {
                        if role_card(&mut columns[0], "Local LPA", "PC/SC + SM-DP+", "Відкрити")
                        {
                            selected = Some(AppMode::Local);
                        }
                        if role_card(&mut columns[1], "Card Agent", "PC/SC / eUICC", "Відкрити")
                        {
                            selected = Some(AppMode::CardAgent);
                        }
                        if role_card(&mut columns[2], "Server Agent", "SM-DP+ / ES9+", "Відкрити")
                        {
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
                        ui.add_space(16.0);
                    }
                }
            });
        });

        if let Some(mode) = selected {
            self.reset_mode(mode);
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui, collapsed: bool) {
        let mut change_mode = false;
        let size = ui.available_size();

        egui::Frame::new()
            .fill(theme::sidebar(self.dark_mode))
            .stroke(egui::Stroke::new(1.0, theme::border(self.dark_mode)))
            .inner_margin(if collapsed { 10.0 } else { 12.0 })
            .show(ui, |ui| {
                ui.set_min_size(size - egui::vec2(if collapsed { 20.0 } else { 24.0 }, 24.0));

                if collapsed {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("N")
                                .size(22.0)
                                .strong()
                                .color(theme::BRAND_RED),
                        );
                    });
                } else {
                    nik_wordmark(ui, 20.0);
                    ui.label(
                        egui::RichText::new(self.mode.title())
                            .size(12.0)
                            .color(theme::muted_text(ui.visuals().dark_mode)),
                    );
                }

                ui.add_space(16.0);
                for page in Page::ALL {
                    let allowed = !matches!(
                        (self.mode, page),
                        (AppMode::ServerAgent, Page::Profiles | Page::Install)
                    );
                    if nav_item(
                        ui,
                        page.label(),
                        page_compact_label(page),
                        self.page == page,
                        allowed,
                        collapsed,
                    )
                    .clicked()
                    {
                        self.page = page;
                    }
                    ui.add_space(4.0);
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    change_mode = ui
                        .add_sized(
                            [ui.available_width(), 40.0],
                            egui::Button::new(if collapsed {
                                "M"
                            } else {
                                "Змінити режим"
                            })
                            .corner_radius(8.0),
                        )
                        .on_hover_text("Змінити режим")
                        .clicked();
                    ui.add_space(8.0);

                    if collapsed {
                        ui.vertical_centered(|ui| {
                            ui.colored_label(
                                if self.relay_running {
                                    theme::SUCCESS
                                } else {
                                    theme::muted_text(ui.visuals().dark_mode)
                                },
                                "●",
                            );
                        });
                    } else {
                        ui.label(
                            egui::RichText::new(if self.relay_running {
                                "● Relay active"
                            } else {
                                "○ Relay stopped"
                            })
                            .size(12.0)
                            .color(theme::muted_text(ui.visuals().dark_mode)),
                        );
                        ui.label(
                            egui::RichText::new("PLAINTEXT DEBUG")
                                .size(11.0)
                                .color(theme::WARNING),
                        );
                    }
                });
            });

        if change_mode {
            self.reset_mode(AppMode::Welcome);
        }
    }

    fn render_topbar(&mut self, ui: &mut egui::Ui) {
        let mut toggle_theme = false;

        egui::Frame::new()
            .fill(theme::topbar(self.dark_mode))
            .stroke(egui::Stroke::new(1.0, theme::border(self.dark_mode)))
            .inner_margin(egui::Margin::symmetric(24, 12))
            .show(ui, |ui| {
                ui.set_min_height(44.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(self.page.label()).size(20.0).strong());
                        ui.label(
                            egui::RichText::new(self.mode.title())
                                .size(12.0)
                                .color(theme::muted_text(ui.visuals().dark_mode)),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        toggle_theme = secondary_button(
                            ui,
                            if self.dark_mode {
                                "Світла"
                            } else {
                                "Темна"
                            },
                            true,
                        )
                        .clicked();
                        if self.page == Page::Transfer {
                            status_badge(ui, "PLAINTEXT DEBUG", theme::WARNING);
                        }
                        if self.busy {
                            ui.spinner();
                        }
                    });
                });
            });

        if toggle_theme {
            self.dark_mode = !self.dark_mode;
            theme::configure(ui.ctx(), self.dark_mode);
        }
    }

    fn render_dashboard(&mut self, ui: &mut egui::Ui) {
        if matches!(self.mode, AppMode::CardAgent | AppMode::Local) {
            self.render_reader_selector(ui);
            ui.add_space(24.0);
        }

        if ui.available_width() >= 680.0 {
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
            ui.add_space(12.0);
            metric_card(ui, "Transport", "Plaintext debug");
            ui.add_space(12.0);
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

        ui.add_space(24.0);
        card(ui, |ui| {
            section_title(ui, "Поточний стан");
            ui.add_space(8.0);
            ui.label(
                self.session
                    .as_ref()
                    .map(|session| session.status.as_str())
                    .unwrap_or("Relay session: idle"),
            );
            if matches!(self.mode, AppMode::CardAgent | AppMode::ServerAgent) {
                ui.add_space(16.0);
                if primary_button(ui, "Відкрити RSP Relay", true).clicked() {
                    self.page = Page::Transfer;
                }
            }
        });
    }

    fn render_profiles(&mut self, ui: &mut egui::Ui) {
        self.render_reader_selector(ui);
        ui.add_space(24.0);

        let mut refresh = false;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                section_title(ui, "Profile List");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    refresh = secondary_button(ui, "Оновити", !self.busy && !self.relay_running)
                        .clicked();
                });
            });
            ui.add_space(16.0);

            if self.profiles_output.is_empty() {
                ui.label(
                    egui::RichText::new("Профілі не завантажені")
                        .color(theme::muted_text(ui.visuals().dark_mode)),
                );
            } else {
                ui.add(
                    egui::TextEdit::multiline(&mut self.profiles_output)
                        .desired_rows(16)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .interactive(false),
                );
            }
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
        ui.add_space(24.0);

        let mut install = false;
        card(ui, |ui| {
            section_title(ui, "Встановити профіль");
            ui.add_space(16.0);
            field_label(ui, "Activation code");
            ui.add(
                egui::TextEdit::multiline(&mut self.local_activation_code)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            ui.add_space(12.0);
            field_label(ui, "Confirmation code");
            ui.add(
                egui::TextEdit::singleline(&mut self.local_confirmation_code)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            ui.add_space(20.0);
            install = primary_button(
                ui,
                "Встановити",
                !self.busy && !self.relay_running && !self.local_activation_code.trim().is_empty(),
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
                ui.label("Relay недоступний у Local mode");
            }),
            AppMode::Welcome => {}
        }
    }

    fn render_card_transfer(&mut self, ui: &mut egui::Ui) {
        self.render_reader_selector(ui);
        ui.add_space(24.0);

        let mut start = false;
        let mut stop = false;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                section_title(ui, "Card Agent");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    stop = secondary_button(ui, "Зупинити", self.relay_running).clicked();
                    start = primary_button(ui, "Нова сесія", !self.busy && !self.relay_running)
                        .clicked();
                });
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
            section_title(ui, "Server Agent");
            ui.add_space(16.0);
            field_label(ui, "Activation code");
            ui.add(
                egui::TextEdit::multiline(&mut self.activation_code)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            ui.add_space(12.0);
            field_label(ui, "Confirmation code");
            ui.add(
                egui::TextEdit::singleline(&mut self.confirmation_code)
                    .desired_width(f32::INFINITY)
                    .password(true),
            );
            ui.add_space(20.0);
            ui.horizontal_wrapped(|ui| {
                start = primary_button(
                    ui,
                    "Запустити",
                    !self.busy && !self.relay_running && !self.activation_code.trim().is_empty(),
                )
                .clicked();
                stop = secondary_button(ui, "Зупинити", self.relay_running).clicked();
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
        ui.add_space(24.0);

        if self.mode == AppMode::ServerAgent && self.session.is_none() && self.relay_running {
            let mut accept = false;
            let mut open_full = false;
            transfer_card(ui, false, |ui| {
                transfer_header(ui, "ОТРИМАТИ", "Card Agent", "INIT_REQUEST", false);
                ui.add_space(12.0);
                ui.add(
                    egui::TextEdit::multiline(&mut self.server_bootstrap_input)
                        .hint_text("Вставте NIKRSP-DEBUG1 пакет")
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    accept = primary_button(
                        ui,
                        "Прийняти",
                        !self.busy && !self.server_bootstrap_input.trim().is_empty(),
                    )
                    .clicked();
                    open_full = secondary_button(
                        ui,
                        "Відкрити повністю",
                        !self.server_bootstrap_input.is_empty(),
                    )
                    .clicked();
                    byte_count(ui, self.server_bootstrap_input.len());
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
            compact_card(ui, |ui| {
                ui.label(
                    egui::RichText::new("Сесія не запущена")
                        .color(theme::muted_text(ui.visuals().dark_mode)),
                );
            });
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
            let mut preview = packet_preview(&session.outgoing_text);

            transfer_card(ui, true, |ui| {
                transfer_header(ui, "НАДІСЛАТИ", peer_label, stage, true);
                ui.add_space(12.0);
                ui.add(
                    egui::TextEdit::multiline(&mut preview)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .interactive(false),
                );
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    if primary_button(ui, "Копіювати", true).clicked() {
                        ui.ctx().copy_text(session.outgoing_text.clone());
                    }
                    open_outgoing = secondary_button(ui, "Відкрити повністю", true).clicked();
                    byte_count(ui, session.outgoing_text.len());
                });
            });
        }
        if open_outgoing {
            self.show_outgoing_packet = true;
        }

        ui.add_space(16.0);
        let busy = self.busy;
        let mut import = false;
        let mut open_incoming = false;
        if let Some(session) = self.session.as_mut() {
            let expected = session.expected_incoming();
            transfer_card(ui, false, |ui| {
                transfer_header(
                    ui,
                    "ОТРИМАТИ",
                    peer_label,
                    expected.map(|stage| stage.code()).unwrap_or("—"),
                    false,
                );
                ui.add_space(12.0);
                ui.add(
                    egui::TextEdit::multiline(&mut session.incoming_text)
                        .hint_text("Вставте пакет з іншого Agent")
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    import = primary_button(
                        ui,
                        "Прийняти та виконати",
                        !busy && expected.is_some() && !session.incoming_text.trim().is_empty(),
                    )
                    .clicked();
                    open_incoming = secondary_button(
                        ui,
                        "Відкрити повністю",
                        !session.incoming_text.is_empty(),
                    )
                    .clicked();
                    byte_count(ui, session.incoming_text.len());
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

        compact_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("RSP FLOW").size(14.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{}/8", completed.len().min(8)))
                            .monospace()
                            .size(12.0)
                            .color(theme::muted_text(ui.visuals().dark_mode)),
                    );
                });
            });
            ui.add_space(12.0);

            let columns: usize = if ui.available_width() >= 560.0 { 4 } else { 2 };
            let gap = 8.0;
            let cell_width =
                (ui.available_width() - gap * (columns.saturating_sub(1) as f32)) / columns as f32;

            egui::Grid::new("rsp_flow_grid")
                .num_columns(columns)
                .spacing([gap, 8.0])
                .show(ui, |ui| {
                    for (index, stage) in RelayStage::ALL.into_iter().enumerate() {
                        timeline_step(
                            ui,
                            index + 1,
                            stage.code(),
                            completed.contains(&stage),
                            active == Some(stage),
                            cell_width,
                        );
                        if (index + 1) % columns == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
    }

    fn render_sessions(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            section_title(ui, "Поточна сесія");
            ui.add_space(16.0);
            if let Some(session) = self.session.as_ref() {
                key_value(ui, "Job ID", &session.job_id.to_string());
                key_value(ui, "Side", &format!("{:?}", session.side));
                key_value(ui, "Packets", &session.packets.len().to_string());
                key_value(
                    ui,
                    "Completed",
                    if session.completed { "yes" } else { "no" },
                );
                ui.add_space(12.0);
                ui.label(&session.status);
            } else {
                ui.label(
                    egui::RichText::new("Idle").color(theme::muted_text(ui.visuals().dark_mode)),
                );
            }
        });

        if let Some(session) = self.session.as_ref()
            && !session.timeline.is_empty()
        {
            ui.add_space(24.0);
            card(ui, |ui| {
                section_title(ui, "Audit");
                ui.add_space(16.0);
                for event in &session.timeline {
                    egui::Frame::new()
                        .fill(theme::control_fill(ui.visuals().dark_mode))
                        .corner_radius(8.0)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.monospace(event.timestamp.format("%H:%M:%S").to_string());
                                status_badge(ui, event.stage.code(), theme::PRIMARY);
                                ui.label(format!("{} B", event.packet_size));
                            });
                            ui.label(
                                egui::RichText::new(&event.note)
                                    .size(12.0)
                                    .color(theme::muted_text(ui.visuals().dark_mode)),
                            );
                        });
                    ui.add_space(8.0);
                }
            });
        }
    }

    fn render_logs(&mut self, ui: &mut egui::Ui) {
        let mut clear = false;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                section_title(ui, "Журнал операцій");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    clear = secondary_button(ui, "Очистити", !self.log.is_empty()).clicked();
                });
            });
            ui.add_space(16.0);
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
        card(ui, |ui| {
            section_title(ui, "Runtime");
            ui.add_space(16.0);
            field_label(ui, "lpac executable");
            ui.add(egui::TextEdit::singleline(&mut self.lpac_path).desired_width(f32::INFINITY));
        });

        ui.add_space(24.0);
        card(ui, |ui| {
            section_title(ui, "Вигляд");
            ui.add_space(16.0);
            ui.horizontal_wrapped(|ui| {
                let light = secondary_button(ui, "Світла", self.dark_mode).clicked();
                let dark = secondary_button(ui, "Темна", !self.dark_mode).clicked();
                if light {
                    self.dark_mode = false;
                    theme::configure(ui.ctx(), false);
                }
                if dark {
                    self.dark_mode = true;
                    theme::configure(ui.ctx(), true);
                }
            });
        });

        ui.add_space(24.0);
        compact_card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Relay transport");
                status_badge(ui, "PLAINTEXT DEBUG", theme::WARNING);
            });
        });
    }

    fn render_reader_selector(&mut self, ui: &mut egui::Ui) {
        let mut discover = false;
        let mut chip_info = false;

        card(ui, |ui| {
            ui.horizontal(|ui| {
                section_title(ui, "PC/SC");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    status_badge(
                        ui,
                        if self.card_eid.is_some() {
                            "eUICC READY"
                        } else {
                            "READER"
                        },
                        if self.card_eid.is_some() {
                            theme::SUCCESS
                        } else {
                            theme::muted_text(ui.visuals().dark_mode)
                        },
                    );
                });
            });
            ui.add_space(16.0);
            field_label(ui, "Reader");

            if self.readers.is_empty() {
                let mut reader_text = format!("Reader #{}", self.selected_reader);
                ui.add_enabled(
                    false,
                    egui::TextEdit::singleline(&mut reader_text).desired_width(f32::INFINITY),
                );
            } else {
                egui::ComboBox::from_id_salt("production_pcsc_reader")
                    .selected_text(
                        self.readers
                            .iter()
                            .find(|reader| reader.index == self.selected_reader)
                            .map(|reader| format!("{} — {}", reader.index, reader.name))
                            .unwrap_or_else(|| format!("Reader #{}", self.selected_reader)),
                    )
                    .width(ui.available_width())
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

            ui.add_space(16.0);
            ui.horizontal_wrapped(|ui| {
                discover =
                    secondary_button(ui, "Оновити", !self.busy && !self.relay_running).clicked();
                chip_info =
                    secondary_button(ui, "Інформація eUICC", !self.busy && !self.relay_running)
                        .clicked();
            });

            if let Some(eid) = self.card_eid.as_deref() {
                ui.add_space(12.0);
                let redacted = if eid.len() > 10 {
                    format!("{}…{}", &eid[..6], &eid[eid.len() - 4..])
                } else {
                    eid.to_owned()
                };
                key_value(ui, "EID", &redacted);
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
            egui::Window::new("Вихідний пакет")
                .open(&mut open)
                .default_size([760.0, 560.0])
                .resizable(true)
                .show(context, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        status_badge(ui, "PLAINTEXT", theme::WARNING);
                        if primary_button(ui, "Copy raw", true).clicked() {
                            ui.ctx().copy_text(raw.clone());
                        }
                        byte_count(ui, raw.len());
                    });
                    ui.add_space(12.0);
                    ui.add(
                        egui::TextEdit::multiline(&mut pretty)
                            .desired_rows(28)
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
            egui::Window::new("Вхідний пакет")
                .open(&mut open)
                .default_size([760.0, 560.0])
                .resizable(true)
                .show(context, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        status_badge(ui, "PLAINTEXT", theme::WARNING);
                        byte_count(ui, raw.len());
                    });
                    ui.add_space(12.0);
                    ui.add(
                        egui::TextEdit::multiline(&mut pretty)
                            .desired_rows(28)
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
                .default_size([760.0, 560.0])
                .resizable(true)
                .show(context, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        status_badge(ui, "PLAINTEXT", theme::WARNING);
                        byte_count(ui, raw.len());
                    });
                    ui.add_space(12.0);
                    ui.add(
                        egui::TextEdit::multiline(&mut pretty)
                            .desired_rows(28)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace)
                            .interactive(false),
                    );
                });
            self.show_bootstrap_packet = open;
        }
    }
}

fn nik_wordmark(ui: &mut egui::Ui, size: f32) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("NIK")
                .size(size)
                .strong()
                .color(theme::BRAND_RED),
        );
        ui.label(egui::RichText::new("LPA").size(size).strong());
    });
}

fn page_compact_label(page: Page) -> &'static str {
    match page {
        Page::Dashboard => "OV",
        Page::Profiles => "PR",
        Page::Install => "IN",
        Page::Transfer => "RSP",
        Page::Sessions => "SE",
        Page::Logs => "LG",
        Page::Settings => "ST",
    }
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(16.0).strong());
}

fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(12.0)
            .strong()
            .color(theme::muted_text(ui.visuals().dark_mode)),
    );
    ui.add_space(4.0);
}

fn byte_count(ui: &mut egui::Ui, bytes: usize) {
    ui.label(
        egui::RichText::new(format!("{bytes} B"))
            .size(12.0)
            .color(theme::muted_text(ui.visuals().dark_mode)),
    );
}

fn transfer_header(ui: &mut egui::Ui, action: &str, peer: &str, stage: &str, outgoing: bool) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(if outgoing {
                format!("{action}  →  {peer}")
            } else {
                format!("{action}  ←  {peer}")
            })
            .size(16.0)
            .strong(),
        );
        status_badge(
            ui,
            stage,
            if outgoing {
                theme::PRIMARY
            } else {
                theme::WARNING
            },
        );
    });
}

fn packet_preview(raw: &str) -> String {
    match decode_debug_packet(raw) {
        Ok(packet) => packet_summary(&packet),
        Err(_) => truncate_middle(raw, 300),
    }
}

fn packet_summary(packet: &RelayPacket) -> String {
    let transaction = packet
        .transaction_id
        .as_deref()
        .map(|value| truncate_middle(value, 68))
        .unwrap_or_else(|| "—".into());
    let payload_keys = packet
        .payload
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>().join(", "))
        .unwrap_or_else(|| "value".into());

    format!(
        "stage={}   sequence={}   job={}\ntransaction={}\npayload keys: {}",
        packet.stage.code(),
        packet.sequence,
        packet.job_id,
        transaction,
        payload_keys
    )
}

fn truncate_middle(value: &str, max_chars: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return value.to_owned();
    }
    let left = max_chars * 2 / 3;
    let right = max_chars - left;
    format!(
        "{}…{}",
        chars[..left].iter().collect::<String>(),
        chars[chars.len() - right..].iter().collect::<String>()
    )
}
