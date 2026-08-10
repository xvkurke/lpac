use eframe::egui;

pub const PRIMARY: egui::Color32 = egui::Color32::from_rgb(21, 112, 239); // Blue/600
pub const PRIMARY_HOVER: egui::Color32 = egui::Color32::from_rgb(23, 92, 211); // Blue/700
pub const BRAND_RED: egui::Color32 = egui::Color32::from_rgb(239, 35, 60); // Accent/500
pub const SUCCESS: egui::Color32 = egui::Color32::from_rgb(3, 152, 85); // Success/600
pub const WARNING: egui::Color32 = egui::Color32::from_rgb(220, 104, 3); // Warning/600
pub const ERROR: egui::Color32 = egui::Color32::from_rgb(217, 45, 32); // Error/600
pub const ACCENT: egui::Color32 = PRIMARY;
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(102, 112, 133); // Gray/500

const LIGHT_APP: egui::Color32 = egui::Color32::from_rgb(242, 244, 247); // Gray/100
const LIGHT_SURFACE: egui::Color32 = egui::Color32::WHITE;
const LIGHT_SUBTLE: egui::Color32 = egui::Color32::from_rgb(249, 250, 251); // Gray/50
const LIGHT_BORDER: egui::Color32 = egui::Color32::from_rgb(234, 236, 240); // Gray/200
const LIGHT_BORDER_STRONG: egui::Color32 = egui::Color32::from_rgb(208, 213, 221); // Gray/300
const LIGHT_TEXT: egui::Color32 = egui::Color32::from_rgb(29, 41, 57); // Gray/800
const LIGHT_MUTED: egui::Color32 = egui::Color32::from_rgb(102, 112, 133); // Gray/500

const DARK_APP: egui::Color32 = egui::Color32::from_rgb(14, 27, 50); // Main/900
const DARK_SURFACE: egui::Color32 = egui::Color32::from_rgb(17, 34, 65); // Main/800
const DARK_CONTROL: egui::Color32 = egui::Color32::from_rgb(27, 50, 84); // Main/700
const DARK_BORDER: egui::Color32 = egui::Color32::from_rgb(39, 67, 108); // Main/600
const DARK_TEXT: egui::Color32 = egui::Color32::from_rgb(240, 244, 255); // Main/100
const DARK_MUTED: egui::Color32 = egui::Color32::from_rgb(162, 171, 200); // Main/300

pub fn configure(context: &egui::Context, dark: bool) {
    let selected_theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    context.set_theme(selected_theme);

    if dark {
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(DARK_TEXT);
        visuals.panel_fill = DARK_APP;
        visuals.window_fill = DARK_SURFACE;
        visuals.extreme_bg_color = DARK_CONTROL;
        visuals.faint_bg_color = DARK_CONTROL;
        visuals.selection.bg_fill = PRIMARY;
        visuals.widgets.noninteractive.bg_fill = DARK_SURFACE;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, DARK_BORDER);
        visuals.widgets.inactive.bg_fill = DARK_CONTROL;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, DARK_BORDER);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(43, 73, 115);
        visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(95, 129, 167));
        visuals.widgets.active.bg_fill = PRIMARY;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, PRIMARY_HOVER);
        context.set_visuals_of(selected_theme, visuals);
    } else {
        let mut visuals = egui::Visuals::light();
        visuals.override_text_color = Some(LIGHT_TEXT);
        visuals.panel_fill = LIGHT_APP;
        visuals.window_fill = LIGHT_SURFACE;
        visuals.extreme_bg_color = LIGHT_SUBTLE;
        visuals.faint_bg_color = LIGHT_SUBTLE;
        visuals.selection.bg_fill = PRIMARY;
        visuals.widgets.noninteractive.bg_fill = LIGHT_SURFACE;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, LIGHT_BORDER);
        visuals.widgets.inactive.bg_fill = LIGHT_SURFACE;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, LIGHT_BORDER_STRONG);
        visuals.widgets.hovered.bg_fill = LIGHT_SUBTLE;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, LIGHT_BORDER_STRONG);
        visuals.widgets.active.bg_fill = PRIMARY;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, PRIMARY_HOVER);
        context.set_visuals_of(selected_theme, visuals);
    }

    let mut style = (*context.style_of(selected_theme)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(16.0, 9.0);
    style.spacing.interact_size.y = 44.0;
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
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    context.set_style_of(selected_theme, style);
}

pub fn background(dark: bool) -> egui::Color32 {
    if dark { DARK_APP } else { LIGHT_APP }
}

pub fn sidebar(dark: bool) -> egui::Color32 {
    if dark { DARK_SURFACE } else { LIGHT_SURFACE }
}

pub fn topbar(dark: bool) -> egui::Color32 {
    sidebar(dark)
}

pub fn card_fill(dark: bool) -> egui::Color32 {
    if dark { DARK_SURFACE } else { LIGHT_SURFACE }
}

pub fn control_fill(dark: bool) -> egui::Color32 {
    if dark { DARK_CONTROL } else { LIGHT_SUBTLE }
}

pub fn border(dark: bool) -> egui::Color32 {
    if dark { DARK_BORDER } else { LIGHT_BORDER }
}

pub fn muted_text(dark: bool) -> egui::Color32 {
    if dark { DARK_MUTED } else { LIGHT_MUTED }
}

pub fn nav_selected_fill(dark: bool) -> egui::Color32 {
    if dark {
        DARK_CONTROL
    } else {
        egui::Color32::from_rgb(254, 243, 242) // Accent/50
    }
}
