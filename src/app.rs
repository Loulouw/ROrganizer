use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::TrayIcon;

use crate::i18n::{self, Lang};
use crate::theme::{self, Theme};
use crate::tray::{self, TrayEvent};
use crate::ui;
use crate::win::{self, WindowsSnapshot, DEFAULT_TITLE_REGEX};

pub struct App {
    theme: Theme,
    lang: Lang,
    tray_rx: Receiver<TrayEvent>,
    _tray_icon: TrayIcon,
    visuals_applied: bool,
    windows: WindowsSnapshot,
    refresh_tx: Sender<()>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Ok(handle) = cc.window_handle() {
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                tray::register_main_hwnd(h.hwnd.get());
            }
        }

        // SVG loader for the language flag icons.
        egui_extras::install_image_loaders(&cc.egui_ctx);

        // Locale before tray::install — tray menu strings are baked at install time
        // and do not re-localize when the user later changes language.
        let lang = i18n::detect_system_lang();
        i18n::set_lang(lang);

        let (tray_tx, tray_rx) = channel();
        let tray_icon = tray::install(cc.egui_ctx.clone(), tray_tx)
            .expect("failed to install tray icon");

        let windows: WindowsSnapshot = Arc::new(Mutex::new(Vec::new()));
        let regex_src = std::env::var("RORGANIZER_TITLE_REGEX")
            .unwrap_or_else(|_| DEFAULT_TITLE_REGEX.to_string());
        let (refresh_tx, refresh_rx) = channel();
        win::spawn_watcher(windows.clone(), cc.egui_ctx.clone(), regex_src, refresh_rx);

        Self {
            theme: Theme::Dark,
            lang,
            tray_rx,
            _tray_icon: tray_icon,
            visuals_applied: false,
            windows,
            refresh_tx,
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    pub fn set_lang(&mut self, l: Lang) {
        if self.lang != l {
            self.lang = l;
            i18n::set_lang(l);
        }
    }

    pub fn toggle_theme(&mut self) {
        self.theme = self.theme.toggled();
        self.visuals_applied = false;
    }

    pub fn windows_snapshot(&self) -> WindowsSnapshot {
        self.windows.clone()
    }

    pub fn request_refresh(&self) {
        let _ = self.refresh_tx.send(());
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
        ui::main_view::draw(ctx, self);
    }
}
