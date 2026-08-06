#[path = "nik_lpa/app.rs"]
mod app;
#[path = "nik_lpa/controller.rs"]
mod controller;
#[path = "nik_lpa/logic.rs"]
mod logic;
#[path = "nik_lpa/model.rs"]
mod model;
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
            .with_inner_size([1_300.0, 860.0])
            .with_min_inner_size([980.0, 680.0]),
        ..Default::default()
    };

    eframe::run_native(
        "NIK LPA",
        options,
        Box::new(|creation_context| {
            theme::configure(&creation_context.egui_ctx, false);
            Ok(Box::new(NikLpaApp::default()))
        }),
    )
}
