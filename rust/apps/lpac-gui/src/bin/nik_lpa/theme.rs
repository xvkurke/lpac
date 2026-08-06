use eframe::egui;

pub fn configure(context: &egui::Context, dark: bool) {
    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    context.set_theme(theme);

    if dark {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(10, 15, 25);
        visuals.window_fill = egui::Color32::from_rgb(15, 23, 42);
        visuals.extreme_bg_color = egui::Color32::from_rgb(8, 12, 20);
        visuals.selection.bg_fill = egui::Color32::from_rgb(37, 99, 235);
        context.set_visuals_of(theme, visuals);
    } else {
        let mut visuals = egui::Visuals::light();
        visuals.override_text_color = Some(egui::Color32::from_rgb(30, 41, 59));
        visuals.panel_fill = egui::Color32::from_rgb(243, 246, 250);
        visuals.window_fill = egui::Color32::WHITE;
        visuals.extreme_bg_color = egui::Color32::WHITE;
        visuals.faint_bg_color = egui::Color32::from_rgb(241, 245, 249);
        visuals.selection.bg_fill = egui::Color32::from_rgb(37, 99, 235);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(226, 232, 240);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(219, 234, 254);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(37, 99, 235);
        context.set_visuals_of(theme, visuals);
    }

    let mut style = (*context.style_of(theme)).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 9.0);
    style.spacing.button_padding = egui::vec2(15.0, 9.0);
    style.spacing.interact_size.y = 38.0;
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
    context.set_style_of(theme, style);
}

pub fn background(dark: bool) -> egui::Color32 {
    if dark {
        egui::Color32::from_rgb(10, 15, 25)
    } else {
        egui::Color32::from_rgb(243, 246, 250)
    }
}
