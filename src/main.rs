#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

rust_i18n::i18n!("locales", fallback = "en");

mod app;
mod config;
mod hooks;
mod i18n;
#[cfg(windows)]
mod single_instance;
mod theme;
mod tray;
mod triggers;
mod ui;
mod win;

use std::sync::Arc;

use eframe::egui;

const ICON_PNG: &[u8] = include_bytes!("../resources/icons/icon.png");

fn main() -> eframe::Result<()> {
    #[cfg(windows)]
    {
        match single_instance::acquire_or_signal_existing() {
            single_instance::AcquireResult::SignalledExisting => {
                std::process::exit(0);
            }
            single_instance::AcquireResult::First(guard) => {
                single_instance::store_guard(guard);
            }
        }
    }

    let icon_data = decode_window_icon();

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([340.0, 280.0])
        .with_min_inner_size([340.0, 280.0])
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
        .with_icon(Arc::new(icon_data))
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

fn decode_window_icon() -> egui::IconData {
    let img = image::load_from_memory(ICON_PNG).expect("decode resources/icons/icon.png");
    let resized = img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    egui::IconData {
        rgba: rgba.into_raw(),
        width: w,
        height: h,
    }
}
