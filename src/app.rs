use std::sync::mpsc::{channel, Receiver};

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::TrayIcon;

use crate::theme::{self, Theme};
use crate::tray::{self, TrayEvent};
use crate::ui;

pub struct App {
    theme: Theme,
    tray_rx: Receiver<TrayEvent>,
    _tray_icon: TrayIcon,
    visuals_applied: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Ok(handle) = cc.window_handle() {
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                tray::register_main_hwnd(h.hwnd.get());
            }
        }

        let (tx, rx) = channel();
        let tray_icon = tray::install(cc.egui_ctx.clone(), tx)
            .expect("failed to install tray icon");
        Self {
            theme: Theme::Dark,
            tray_rx: rx,
            _tray_icon: tray_icon,
            visuals_applied: false,
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn request_minimize(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    pub fn request_close(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(ev) = self.tray_rx.try_recv() {
            match ev {
                TrayEvent::ShowRequested => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayEvent::QuitRequested => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        if !self.visuals_applied {
            theme::apply(ctx, self.theme);
            self.visuals_applied = true;
        }

        ui::header::draw(ctx, self);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::window_bg(self.theme)))
            .show(ctx, |ui| {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Phase 1 \u{2014} squelette OK")
                            .size(13.0)
                            .color(theme::text_secondary(self.theme)),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("D\u{00E9}tection des fen\u{00EA}tres : phase 2")
                            .size(11.0)
                            .color(theme::text_tertiary(self.theme)),
                    );
                });
            });
    }
}
