use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::TrayIcon;

use crate::hooks::BindingAction;
use crate::i18n::{self, Lang};
use crate::theme::{self, Theme};
use crate::tray::{self, TrayEvent};
use crate::triggers::{BindingTarget, SlotKey, Trigger};
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
    bindings: HashMap<BindingTarget, Trigger>,
    capture_state: Option<BindingTarget>,
    cycle_order: Vec<SlotKey>,
    /// Slot that the cycler cursor is currently on. Updated on manual focus
    /// clicks and on cycle commands.
    cycle_current: Option<SlotKey>,
    last_synced_state: SyncSnapshot,
    last_synced_height: f32,
}

#[derive(Default, PartialEq, Eq)]
struct SyncSnapshot {
    accounts: Vec<(SlotKey, isize)>,
    cycle_hwnds: Vec<isize>,
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
            cycle_order: Vec::new(),
            cycle_current: None,
            last_synced_state: SyncSnapshot::default(),
            last_synced_height: 0.0,
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

    pub fn cycle_order(&self) -> &[SlotKey] {
        &self.cycle_order
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

    pub fn binding_for(&self, target: &BindingTarget) -> Option<Trigger> {
        self.bindings.get(target).copied()
    }

    pub fn binding_for_account(&self, slot: &str) -> Option<Trigger> {
        self.bindings
            .get(&BindingTarget::Account(slot.to_string()))
            .copied()
    }

    pub fn capture_state(&self) -> Option<&BindingTarget> {
        self.capture_state.as_ref()
    }

    pub fn bind_target(&mut self, target: BindingTarget) {
        self.capture_state = Some(target);
    }

    pub fn cancel_capture(&mut self) {
        self.capture_state = None;
    }

    pub fn commit_binding(&mut self, target: BindingTarget, trigger: Trigger) {
        self.bindings.insert(target, trigger);
        self.capture_state = None;
        self.sync_state_to_hooks(true);
    }

    pub fn clear_binding(&mut self, target: &BindingTarget) {
        if self.bindings.remove(target).is_some() {
            self.sync_state_to_hooks(true);
        }
    }

    /// Aligns the cycler cursor with the manually-focused account so a
    /// subsequent cycle_next/prev starts from there.
    pub fn set_cycle_current(&mut self, slot: SlotKey) {
        self.cycle_current = Some(slot);
        // Push only the cursor — bindings & hwnds didn't change.
        self.push_cycle_index_to_hooks();
    }

    /// Replaces the user-defined cycle order. Called by the drag&drop UI.
    /// The hook auto-remaps its cycle_index based on cycle_current_hwnd, so
    /// no explicit index push is needed here.
    pub fn apply_drag_drop(&mut self, new_order: Vec<SlotKey>) {
        self.cycle_order = new_order;
        self.sync_state_to_hooks(true);
    }

    fn push_cycle_index_to_hooks(&self) {
        let accounts: Vec<SlotKey> = {
            let guard = self.windows.lock().expect("snapshot poisoned");
            guard.iter().map(|w| w.slot_key.clone()).collect()
        };
        let live_order: Vec<&SlotKey> = self
            .cycle_order
            .iter()
            .filter(|s| accounts.iter().any(|a| a == *s))
            .collect();
        let idx = self
            .cycle_current
            .as_ref()
            .and_then(|cur| live_order.iter().position(|s| s.as_str() == cur.as_str()))
            .unwrap_or(0);
        crate::hooks::set_cycle_index(idx);
    }

    /// Ensure every detected slot exists in `cycle_order` (newly detected ones
    /// are appended at the end so the user's drag-ordered prefix is preserved).
    fn ensure_cycle_order_covers_snapshot(&mut self) {
        let snap_slots: Vec<SlotKey> = {
            let guard = self.windows.lock().expect("snapshot poisoned");
            guard.iter().map(|w| w.slot_key.clone()).collect()
        };
        let mut changed = false;
        for s in &snap_slots {
            if !self.cycle_order.contains(s) {
                self.cycle_order.push(s.clone());
                changed = true;
            }
        }
        if changed {
            self.sync_state_to_hooks(true);
        }
    }

    /// Resolves bindings + cycle order against the current window snapshot
    /// and pushes them to the hook state. Does NOT touch the hook's
    /// `cycle_index` — that lives independently. Skipped when nothing has
    /// changed unless `force` is true.
    fn sync_state_to_hooks(&mut self, force: bool) {
        let accounts: Vec<(SlotKey, isize)> = {
            let guard = self.windows.lock().expect("snapshot poisoned");
            guard
                .iter()
                .map(|w| (w.slot_key.clone(), w.hwnd))
                .collect()
        };

        let cycle_hwnds: Vec<isize> = self
            .cycle_order
            .iter()
            .filter_map(|slot| accounts.iter().find(|(s, _)| s == slot).map(|(_, h)| *h))
            .collect();

        let new_state = SyncSnapshot {
            accounts: accounts.clone(),
            cycle_hwnds: cycle_hwnds.clone(),
        };
        if !force && new_state == self.last_synced_state {
            return;
        }
        self.last_synced_state = new_state;

        let mut bindings: Vec<(Trigger, BindingAction)> =
            Vec::with_capacity(self.bindings.len());
        for (target, trig) in &self.bindings {
            let action = match target {
                BindingTarget::Account(slot) => accounts
                    .iter()
                    .find(|(s, _)| s == slot)
                    .map(|(_, h)| BindingAction::Focus(*h)),
                BindingTarget::CycleNext => Some(BindingAction::CycleNext),
                BindingTarget::CyclePrev => Some(BindingAction::CyclePrev),
            };
            if let Some(a) = action {
                bindings.push((*trig, a));
            }
        }
        // Account focus actions take precedence over cycle actions on conflict.
        bindings.sort_by_key(|(_, a)| {
            matches!(a, BindingAction::CycleNext | BindingAction::CyclePrev)
        });

        crate::hooks::set_bindings_and_cycle(bindings, cycle_hwnds);
    }

    /// Resize the OS window to fit the current content height. Width stays
    /// fixed at 340. Height clamped to [280, 600].
    fn apply_dynamic_height(&mut self, ctx: &egui::Context) {
        // Layout cost approximations matching `main_view::draw`.
        const TITLE_BAR: f32 = 36.0;
        const PANEL_PAD: f32 = 24.0; // 12 top + 12 bottom
        const SUMMARY: f32 = 22.0; // count label + refresh button row
        const ROW: f32 = 46.0; // frame ~40 + 6 inter-row gap
        const CYCLE_HEAD: f32 = 32.0; // separator + label + spacing
        const CYCLE_BLOCK: f32 = 2.0 * ROW + 4.0; // 2 rows + small slack
        const SUMMARY_GAP: f32 = 10.0;
        const TAIL_PAD: f32 = 4.0;

        let n_accounts = self.windows.lock().map(|w| w.len()).unwrap_or(0) as f32;
        let n_for_height = n_accounts.max(1.0); // reserve ~1 row even when empty
        let desired = (TITLE_BAR
            + PANEL_PAD
            + SUMMARY
            + SUMMARY_GAP
            + n_for_height * ROW
            + CYCLE_HEAD
            + CYCLE_BLOCK
            + TAIL_PAD)
            .clamp(280.0, 600.0);

        if (desired - self.last_synced_height).abs() > 0.5 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(340.0, desired)));
            self.last_synced_height = desired;
        }
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

        self.ensure_cycle_order_covers_snapshot();
        self.sync_state_to_hooks(false);
        self.apply_dynamic_height(ctx);

        ui::header::draw(ctx, self);
        ui::main_view::draw(ctx, self);
        ui::capture::draw(ctx, self);
    }
}
