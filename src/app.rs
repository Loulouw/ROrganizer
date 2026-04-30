use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::TrayIcon;

use crate::i18n::{self, Lang};
use crate::theme::{self, Theme};
use crate::tray::{self, TrayEvent};
use crate::triggers::Trigger;
use crate::ui;
use crate::win::{self, WindowsSnapshot, DEFAULT_TITLE_REGEX};

pub type SlotKey = String;

pub struct App {
    theme: Theme,
    lang: Lang,
    tray_rx: Receiver<TrayEvent>,
    _tray_icon: TrayIcon,
    visuals_applied: bool,
    windows: WindowsSnapshot,
    refresh_tx: Sender<()>,
    bindings: HashMap<SlotKey, Trigger>,
    capture_state: Option<SlotKey>,
    /// Last `(slot_key, hwnd)` set pushed to the hooks. Used to skip redundant
    /// re-syncs when nothing relevant in the snapshot has changed.
    last_synced: Vec<(SlotKey, isize)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Ok(handle) = cc.window_handle() {
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                tray::register_main_hwnd(h.hwnd.get());
            }
        }

        egui_extras::install_image_loaders(&cc.egui_ctx);

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

        crate::hooks::install(windows.clone());

        Self {
            theme: Theme::Dark,
            lang,
            tray_rx,
            _tray_icon: tray_icon,
            visuals_applied: false,
            windows,
            refresh_tx,
            bindings: HashMap::new(),
            capture_state: None,
            last_synced: Vec::new(),
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

    pub fn binding_for(&self, slot: &str) -> Option<Trigger> {
        self.bindings.get(slot).copied()
    }

    pub fn capture_state(&self) -> Option<&SlotKey> {
        self.capture_state.as_ref()
    }

    pub fn bind_for(&mut self, slot: SlotKey) {
        self.capture_state = Some(slot);
    }

    pub fn cancel_capture(&mut self) {
        self.capture_state = None;
    }

    pub fn commit_binding(&mut self, slot: SlotKey, trigger: Trigger) {
        self.bindings.insert(slot, trigger);
        self.capture_state = None;
        self.sync_bindings_to_hooks(true);
    }

    /// Removes any binding for `slot` and re-syncs the hook bindings table.
    /// No-op if the slot has no binding.
    pub fn clear_binding(&mut self, slot: &str) {
        if self.bindings.remove(slot).is_some() {
            self.sync_bindings_to_hooks(true);
        }
    }

    /// Resolve `(SlotKey -> Trigger)` against the current snapshot of detected
    /// windows and push the resulting `(Trigger, hwnd)` pairs into the hook state.
    /// `force` skips the "no change" guard — used after explicit binding edits.
    fn sync_bindings_to_hooks(&mut self, force: bool) {
        let snap_pairs: Vec<(SlotKey, isize)> = {
            let guard = self.windows.lock().expect("snapshot poisoned");
            guard
                .iter()
                .map(|w| (w.slot_key.clone(), w.hwnd))
                .collect()
        };
        if !force && snap_pairs == self.last_synced {
            return;
        }

        let mut out: Vec<(Trigger, isize)> = Vec::with_capacity(self.bindings.len());
        for (slot, hwnd) in &snap_pairs {
            if let Some(trig) = self.bindings.get(slot) {
                out.push((*trig, *hwnd));
            }
        }
        crate::hooks::set_bindings(out);
        self.last_synced = snap_pairs;
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

        // Re-sync bindings if the snapshot of windows changed (e.g. a Dofus
        // was relaunched and got a new HWND for the same slot_key).
        self.sync_bindings_to_hooks(false);

        ui::header::draw(ctx, self);
        ui::main_view::draw(ctx, self);
        ui::capture::draw(ctx, self);
    }
}
