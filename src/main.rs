#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

rust_i18n::i18n!("locales", fallback = "en");

mod app;
mod config;
mod hooks;
mod i18n;
mod theme;
mod tray;
mod triggers;
mod ui;
mod win;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([340.0, 280.0])
        .with_min_inner_size([340.0, 280.0])
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
        .with_title("ROrganizer");

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "ROrganizer",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
