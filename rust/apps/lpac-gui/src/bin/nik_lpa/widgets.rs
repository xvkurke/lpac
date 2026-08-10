use eframe::egui;

use crate::theme;

pub fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::card_fill(ui.visuals().dark_mode))
        .stroke(egui::Stroke::new(
            1.0,
            theme::border(ui.visuals().dark_mode),
        ))
        .corner_radius(12.0)
        .inner_margin(24.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
}

pub fn compact_card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::card_fill(ui.visuals().dark_mode))
        .stroke(egui::Stroke::new(
            1.0,
            theme::border(ui.visuals().dark_mode),
        ))
        .corner_radius(12.0)
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
}

pub fn transfer_card(ui: &mut egui::Ui, outgoing: bool, content: impl FnOnce(&mut egui::Ui)) {
    let accent = if outgoing {
        theme::PRIMARY
    } else {
        theme::WARNING
    };
    egui::Frame::new()
        .fill(theme::card_fill(ui.visuals().dark_mode))
        .stroke(egui::Stroke::new(
            1.0,
            theme::border(ui.visuals().dark_mode),
        ))
        .corner_radius(12.0)
        .inner_margin(0.0)
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().rect_filled(
                egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + 3.0, rect.max.y)),
                2.0,
                accent,
            );
            ui.add_space(0.0);
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: 21,
                    right: 18,
                    top: 18,
                    bottom: 18,
                })
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    content(ui);
                });
        });
}

pub fn metric_card(ui: &mut egui::Ui, title: &str, value: &str) {
    compact_card(ui, |ui| {
        ui.label(
            egui::RichText::new(title)
                .size(12.0)
                .color(theme::muted_text(ui.visuals().dark_mode)),
        );
        ui.add_space(4.0);
        ui.label(egui::RichText::new(value).size(16.0).strong());
    });
}

pub fn role_card(ui: &mut egui::Ui, title: &str, description: &str, button_label: &str) -> bool {
    let mut clicked = false;
    card(ui, |ui| {
        ui.label(egui::RichText::new(title).size(18.0).strong());
        ui.label(
            egui::RichText::new(description)
                .size(14.0)
                .color(theme::muted_text(ui.visuals().dark_mode)),
        );
        ui.add_space(16.0);
        clicked = primary_button(ui, button_label, true).clicked();
    });
    clicked
}

pub fn status_badge(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.42)))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(12.0).strong().color(color));
        });
}

pub fn primary_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            egui::RichText::new(text)
                .color(egui::Color32::WHITE)
                .strong(),
        )
        .fill(theme::PRIMARY)
        .stroke(egui::Stroke::new(1.0, theme::PRIMARY))
        .corner_radius(8.0)
        .min_size(egui::vec2(112.0, 44.0)),
    )
}

pub fn secondary_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(text)
            .fill(theme::card_fill(ui.visuals().dark_mode))
            .stroke(egui::Stroke::new(
                1.0,
                theme::border(ui.visuals().dark_mode),
            ))
            .corner_radius(8.0)
            .min_size(egui::vec2(96.0, 44.0)),
    )
}

pub fn nav_item(
    ui: &mut egui::Ui,
    label: &str,
    compact_label: &str,
    active: bool,
    enabled: bool,
    collapsed: bool,
) -> egui::Response {
    let text = if collapsed { compact_label } else { label };
    let fill = if active {
        theme::nav_selected_fill(ui.visuals().dark_mode)
    } else {
        egui::Color32::TRANSPARENT
    };
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(
            egui::RichText::new(text)
                .size(if collapsed { 12.0 } else { 14.0 })
                .strong(),
        )
        .fill(fill)
        .stroke(egui::Stroke::new(0.0, egui::Color32::TRANSPARENT))
        .corner_radius(8.0)
        .min_size(egui::vec2(ui.available_width(), 40.0)),
    );

    if active {
        let rail = egui::Rect::from_min_max(
            egui::pos2(response.rect.left(), response.rect.top() + 7.0),
            egui::pos2(response.rect.left() + 2.0, response.rect.bottom() - 7.0),
        );
        ui.painter().rect_filled(rail, 1.0, theme::BRAND_RED);
    }

    if collapsed {
        response.on_hover_text(label)
    } else {
        response
    }
}

pub fn timeline_step(
    ui: &mut egui::Ui,
    number: usize,
    label: &str,
    completed: bool,
    active: bool,
    width: f32,
) {
    let state_color = if completed {
        theme::SUCCESS
    } else if active {
        theme::PRIMARY
    } else {
        theme::muted_text(ui.visuals().dark_mode)
    };
    let fill = if active {
        theme::control_fill(ui.visuals().dark_mode)
    } else {
        theme::card_fill(ui.visuals().dark_mode)
    };

    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(
            1.0,
            if active {
                theme::PRIMARY
            } else {
                theme::border(ui.visuals().dark_mode)
            },
        ))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width((width - 20.0).max(96.0));
            ui.set_max_width((width - 20.0).max(96.0));
            ui.horizontal(|ui| {
                ui.colored_label(state_color, if completed { "●" } else { "○" });
                ui.label(
                    egui::RichText::new(format!("{number:02}"))
                        .monospace()
                        .size(11.0)
                        .color(theme::muted_text(ui.visuals().dark_mode)),
                );
                ui.label(egui::RichText::new(label).monospace().size(11.0).strong());
            });
        });
}

pub fn key_value(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.add_sized(
            [112.0, 20.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .size(12.0)
                    .color(theme::muted_text(ui.visuals().dark_mode)),
            ),
        );
        ui.label(egui::RichText::new(value).size(14.0));
    });
}
