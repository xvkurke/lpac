use eframe::egui;

use crate::theme;

pub fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::card_fill(ui.visuals().dark_mode))
        .stroke(egui::Stroke::new(
            1.0,
            theme::border(ui.visuals().dark_mode),
        ))
        .corner_radius(9.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
}

pub fn transfer_card(ui: &mut egui::Ui, outgoing: bool, content: impl FnOnce(&mut egui::Ui)) {
    let stroke = if outgoing {
        theme::ACCENT
    } else {
        theme::WARNING
    };
    egui::Frame::new()
        .fill(theme::card_fill(ui.visuals().dark_mode))
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(9.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
}

pub fn metric_card(ui: &mut egui::Ui, title: &str, value: &str) {
    card(ui, |ui| {
        ui.small(title);
        ui.strong(value);
    });
}

pub fn role_card(ui: &mut egui::Ui, title: &str, description: &str, button_label: &str) -> bool {
    let mut clicked = false;
    card(ui, |ui| {
        ui.strong(title);
        ui.small(description);
        ui.add_space(8.0);
        clicked = ui.button(button_label).clicked();
    });
    clicked
}

pub fn status_badge(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.16))
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.55)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.colored_label(color, text);
        });
}

pub fn timeline_step(ui: &mut egui::Ui, number: usize, label: &str, completed: bool, active: bool) {
    let color = if completed {
        theme::SUCCESS
    } else if active {
        theme::ACCENT
    } else {
        theme::MUTED
    };
    let fill = if active {
        color.gamma_multiply(0.12)
    } else {
        theme::card_fill(ui.visuals().dark_mode)
    };

    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.65)))
        .corner_radius(7.0)
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_min_width(132.0);
            ui.horizontal(|ui| {
                ui.colored_label(color, if completed { "✓" } else { "○" });
                ui.small(format!("{number:02}"));
            });
            ui.monospace(label);
        });
}
