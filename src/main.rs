#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use skribi::{app::SkribiApp, theme};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(theme::window_icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Skribi",
        options,
        Box::new(|context| Ok(Box::new(SkribiApp::new(context)))),
    )
}
