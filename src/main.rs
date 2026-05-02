#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

rust_i18n::i18n!("locales", fallback = "en");

mod app;
mod assets;
mod class_icon;
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

fn main() -> eframe::Result<()> {
    #[cfg(windows)]
    let waiter_event = match single_instance::acquire_or_signal_existing() {
        single_instance::AcquireResult::SignalledExisting => {
            std::process::exit(0);
        }
        single_instance::AcquireResult::First(guard) => {
            let event = guard.event_handle();
            single_instance::store_guard(guard);
            event
        }
    };

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
        Box::new(move |cc| {
            #[cfg(windows)]
            let app = app::App::new(cc, waiter_event);
            #[cfg(not(windows))]
            let app = app::App::new(cc);
            Ok(Box::new(app))
        }),
    )
}

fn decode_window_icon() -> egui::IconData {
    let img = image::load_from_memory(assets::ICON_PNG).expect("decode resources/icons/icon.png");
    let resized = img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    egui::IconData {
        rgba: rgba.into_raw(),
        width: w,
        height: h,
    }
}
