use eframe::egui;

pub fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().window_fill)
        .corner_radius(10.0)
        .inner_margin(16.0)
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

pub fn role_card(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    button_label: &str,
) -> bool {
    let mut clicked = false;
    card(ui, |ui| {
        ui.heading(title);
        ui.label(description);
        ui.add_space(12.0);
        clicked = ui.button(button_label).clicked();
    });
    clicked
}
