#![cfg_attr(windows, windows_subsystem = "windows")]

#[path = "nik_lpa/app.rs"]
mod app;
#[path = "nik_lpa/controller.rs"]
mod controller;
#[path = "nik_lpa/logic.rs"]
mod logic;
#[path = "nik_lpa/model.rs"]
mod model;
#[path = "nik_lpa/roboto_data.rs"]
mod roboto_data;
#[path = "nik_lpa/theme.rs"]
mod theme;
#[path = "nik_lpa/views.rs"]
mod views;
#[path = "nik_lpa/widgets.rs"]
mod widgets;

use app::NikLpaApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1_180.0, 720.0])
            .with_min_inner_size([820.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "NIK LPA",
        options,
        Box::new(|creation_context| {
            theme::install_fonts(&creation_context.egui_ctx);
            theme::configure(&creation_context.egui_ctx, true);
            Ok(Box::new(NikLpaApp::default()))
        }),
    )
}
