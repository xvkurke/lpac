use eframe::egui;

pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(37, 99, 235);
pub const SUCCESS: egui::Color32 = egui::Color32::from_rgb(34, 197, 94);
pub const WARNING: egui::Color32 = egui::Color32::from_rgb(245, 158, 11);
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(100, 116, 139);

pub fn configure(context: &egui::Context, dark: bool) {
    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    context.set_theme(theme);

    if dark {
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(egui::Color32::from_rgb(226, 232, 240));
        visuals.panel_fill = egui::Color32::from_rgb(2, 6, 23);
        visuals.window_fill = egui::Color32::from_rgb(11, 18, 32);
        visuals.extreme_bg_color = egui::Color32::from_rgb(5, 10, 22);
        visuals.faint_bg_color = egui::Color32::from_rgb(15, 23, 42);
        visuals.selection.bg_fill = ACCENT;
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(17, 24, 39);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(30, 41, 59);
        visuals.widgets.active.bg_fill = ACCENT;
        visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 41, 59));
        context.set_visuals_of(theme, visuals);
    } else {
        let mut visuals = egui::Visuals::light();
        visuals.override_text_color = Some(egui::Color32::from_rgb(30, 41, 59));
        visuals.panel_fill = egui::Color32::from_rgb(248, 250, 252);
        visuals.window_fill = egui::Color32::WHITE;
        visuals.extreme_bg_color = egui::Color32::from_rgb(241, 245, 249);
        visuals.faint_bg_color = egui::Color32::from_rgb(241, 245, 249);
        visuals.selection.bg_fill = ACCENT;
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(241, 245, 249);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(219, 234, 254);
        visuals.widgets.active.bg_fill = ACCENT;
        context.set_visuals_of(theme, visuals);
    }

    let mut style = (*context.style_of(theme)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.interact_size.y = 34.0;
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(22.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.5));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    context.set_style_of(theme, style);
}

pub fn background(dark: bool) -> egui::Color32 {
    if dark {
        egui::Color32::from_rgb(2, 6, 23)
    } else {
        egui::Color32::from_rgb(248, 250, 252)
    }
}

pub fn sidebar(dark: bool) -> egui::Color32 {
    if dark {
        egui::Color32::from_rgb(7, 12, 26)
    } else {
        egui::Color32::WHITE
    }
}

pub fn card_fill(dark: bool) -> egui::Color32 {
    if dark {
        egui::Color32::from_rgb(11, 18, 32)
    } else {
        egui::Color32::WHITE
    }
}

pub fn border(dark: bool) -> egui::Color32 {
    if dark {
        egui::Color32::from_rgb(30, 41, 59)
    } else {
        egui::Color32::from_rgb(226, 232, 240)
    }
}
