use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::config::{self, StoredConfig};
use crate::hooks::BindingAction;
use crate::i18n::{self, Lang};
use crate::theme::{self, Theme};
use crate::tray::{self, TrayController, TrayEvent};
use crate::triggers::{BindingTarget, SlotKey, Trigger};
use crate::ui;
use crate::win::{self, WindowsSnapshot, DEFAULT_TITLE_REGEX};

const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

pub struct App {
    theme: Theme,
    lang: Lang,
    tray_rx: Receiver<TrayEvent>,
    tray: TrayController,
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
    config_path: Option<PathBuf>,
    last_dirty_at: Option<Instant>,
    is_active: bool,
    last_status_count: usize,
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

        // Load persisted config if any (silently falls back to defaults).
        let config_path = config::default_path();
        let stored = config_path.as_ref().and_then(|p| config::load(p));

        let (theme, lang, bindings, cycle_order) = match stored {
            Some(cfg) => (
                cfg.theme,
                cfg.lang,
                cfg.bindings.into_iter().collect::<HashMap<_, _>>(),
                cfg.cycle_order,
            ),
            None => (
                Theme::Dark,
                i18n::detect_system_lang(),
                HashMap::new(),
                Vec::new(),
            ),
        };
        i18n::set_lang(lang);

        let (tray_tx, tray_rx) = channel();
        let tray = tray::install(cc.egui_ctx.clone(), tray_tx)
            .expect("failed to install tray icon");

        let windows: WindowsSnapshot = Arc::new(Mutex::new(Vec::new()));
        let regex_src = std::env::var("RORGANIZER_TITLE_REGEX")
            .unwrap_or_else(|_| DEFAULT_TITLE_REGEX.to_string());
        let (refresh_tx, refresh_rx) = channel();
        win::spawn_watcher(windows.clone(), cc.egui_ctx.clone(), regex_src, refresh_rx);

        crate::hooks::install(windows.clone());

        Self {
            theme,
            lang,
            tray_rx,
            tray,
            visuals_applied: false,
            windows,
            refresh_tx,
            bindings,
            capture_state: None,
            cycle_order,
            cycle_current: None,
            last_synced_state: SyncSnapshot::default(),
            last_synced_height: 0.0,
            config_path,
            last_dirty_at: None,
            is_active: false,
            last_status_count: usize::MAX, // forces an initial set_status push
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn request_activate(&mut self, ctx: &egui::Context) {
        self.set_active(true);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    pub fn request_deactivate(&mut self) {
        self.set_active(false);
    }

    fn set_active(&mut self, active: bool) {
        if self.is_active == active {
            return;
        }
        self.is_active = active;
        crate::hooks::set_enabled(active);
        self.tray.set_active(active);
        self.update_tray_status(true);
    }

    fn update_tray_status(&mut self, force: bool) {
        let count = self.windows.lock().map(|w| w.len()).unwrap_or(0);
        if !force && count == self.last_status_count {
            return;
        }
        self.last_status_count = count;
        self.tray.set_status(self.is_active, count);
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    pub fn set_lang(&mut self, l: Lang) {
        if self.lang != l {
            self.lang = l;
            i18n::set_lang(l);
            self.mark_dirty();
        }
    }

    pub fn toggle_theme(&mut self) {
        self.theme = self.theme.toggled();
        self.visuals_applied = false;
        self.mark_dirty();
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

    /// Set of triggers assigned to ≥ 2 BindingTargets. Used by the UI to
    /// highlight conflicting rows + render the banner.
    pub fn conflicting_triggers(&self) -> HashSet<Trigger> {
        let mut counts: HashMap<Trigger, u32> = HashMap::new();
        for t in self.bindings.values() {
            *counts.entry(*t).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .filter(|(_, c)| *c > 1)
            .map(|(t, _)| t)
            .collect()
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
        self.mark_dirty();
    }

    pub fn clear_binding(&mut self, target: &BindingTarget) {
        if self.bindings.remove(target).is_some() {
            self.sync_state_to_hooks(true);
            self.mark_dirty();
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
        self.mark_dirty();
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
            self.mark_dirty();
        }
    }

    fn mark_dirty(&mut self) {
        self.last_dirty_at = Some(Instant::now());
    }

    fn flush_if_due(&mut self) {
        let Some(when) = self.last_dirty_at else { return };
        if when.elapsed() < SAVE_DEBOUNCE {
            return;
        }
        self.persist_config();
        self.last_dirty_at = None;
    }

    fn persist_config(&self) {
        let Some(path) = &self.config_path else { return };
        let cfg = StoredConfig {
            version: config::current_version(),
            bindings: self
                .bindings
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            cycle_order: self.cycle_order.clone(),
            lang: self.lang,
            theme: self.theme,
        };
        let _ = config::save_atomic(path, &cfg);
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

        // 1. Account bindings ordered by cycle_order — the topmost slot wins
        //    on a conflict, and the user controls that order via drag&drop.
        for slot in &self.cycle_order {
            if let Some(trig) = self.bindings.get(&BindingTarget::Account(slot.clone())) {
                if let Some(hwnd) = accounts
                    .iter()
                    .find(|(s, _)| s == slot)
                    .map(|(_, h)| *h)
                {
                    bindings.push((*trig, BindingAction::Focus(hwnd)));
                }
            }
        }
        // 2. Cycle bindings come after — Account always wins on conflict.
        if let Some(trig) = self.bindings.get(&BindingTarget::CycleNext) {
            bindings.push((*trig, BindingAction::CycleNext));
        }
        if let Some(trig) = self.bindings.get(&BindingTarget::CyclePrev) {
            bindings.push((*trig, BindingAction::CyclePrev));
        }

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
        // Banner: 6 leading space + 16 frame inner_margin + 14 per line.
        const BANNER_BASE: f32 = 22.0;
        const BANNER_PER_LINE: f32 = 14.0;
        // Activate/deactivate full-width button + hint when inactive.
        const TOGGLE_BUTTON: f32 = 46.0;
        const TOGGLE_HINT: f32 = 22.0;

        let n_accounts = self.windows.lock().map(|w| w.len()).unwrap_or(0) as f32;
        let n_for_height = n_accounts.max(1.0); // reserve ~1 row even when empty
        let n_conflicts = self.conflicting_triggers().len() as f32;
        let banner_h = if n_conflicts > 0.0 {
            BANNER_BASE + n_conflicts * BANNER_PER_LINE
        } else {
            0.0
        };
        let toggle_h = TOGGLE_BUTTON + if self.is_active { 0.0 } else { TOGGLE_HINT };
        let desired = (TITLE_BAR
            + PANEL_PAD
            + SUMMARY
            + SUMMARY_GAP
            + n_for_height * ROW
            + banner_h
            + CYCLE_HEAD
            + CYCLE_BLOCK
            + toggle_h
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
                TrayEvent::ToggleRequested => {
                    let new_active = !self.is_active;
                    self.set_active(new_active);
                    if new_active {
                        // Tray Activer: hide window (the tray handler already
                        // showed it to wake the loop ; we hide it back).
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    } else {
                        // Tray Désactiver: window was shown via Win32 from the
                        // tray callback. Send Visible(true) so eframe's tracked
                        // viewport state matches reality — otherwise a later
                        // Visible(false) gets deduplicated to a no-op.
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                }
            }
        }

        if !self.visuals_applied {
            theme::apply(ctx, self.theme);
            self.visuals_applied = true;
        }

        self.ensure_cycle_order_covers_snapshot();
        self.sync_state_to_hooks(false);
        self.update_tray_status(false);
        self.apply_dynamic_height(ctx);

        ui::header::draw(ctx, self);
        ui::main_view::draw(ctx, self);
        ui::capture::draw(ctx, self);

        self.flush_if_due();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.last_dirty_at.is_some() {
            self.persist_config();
            self.last_dirty_at = None;
        }
    }
}
