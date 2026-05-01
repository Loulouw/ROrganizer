use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::config::{self, StoredConfig};
use crate::hooks::{BindingAction, HookHandle};
use crate::i18n::{self, Lang};
use crate::theme::{self, Theme};
use crate::tray::{self, TrayController, TrayEvent};
use crate::triggers::{BindingTarget, SlotKey, Trigger};
use crate::ui;
use crate::win::{self, WindowsSnapshot, DEFAULT_TITLE_REGEX};

const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Triggers assigned to 2+ BindingTargets in the given binding map.
/// Pure helper, factored out for unit tests.
pub(crate) fn compute_conflicts(
    bindings: &HashMap<BindingTarget, Trigger>,
) -> HashSet<Trigger> {
    let mut counts: HashMap<Trigger, u32> = HashMap::new();
    for t in bindings.values() {
        *counts.entry(*t).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .filter(|(_, c)| *c > 1)
        .map(|(t, _)| t)
        .collect()
}

/// Total order on BindingTarget for deterministic JSON serialization.
/// Account names alphabetical first, then CycleNext, then CyclePrev.
fn binding_target_sort_key(b: &BindingTarget) -> (u8, &str) {
    match b {
        BindingTarget::Account(s) => (0, s.as_str()),
        BindingTarget::CycleNext => (1, ""),
        BindingTarget::CyclePrev => (2, ""),
    }
}

/// Drops a user-supplied `title_regex` if it can't be compiled. Returns
/// `None` for invalid input so the caller falls back on the built-in
/// default and the next save persists `null` rather than a broken regex.
/// Pure helper for unit tests.
pub(crate) fn validate_title_regex(opt: Option<String>) -> Option<String> {
    let s = opt?;
    regex::Regex::new(&s).ok().map(|_| s)
}

/// Decode `assets::ICON_PNG` into a Context-owned texture for the About
/// modal. 128×128 is large enough to stay crisp on HiDPI when the modal
/// renders it at 56×56 logical pixels. Returns None if decoding fails —
/// the modal then just skips the icon row.
fn decode_about_icon(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(crate::assets::ICON_PNG).ok()?;
    let resized = img.resize_exact(128, 128, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw());
    Some(ctx.load_texture("about-icon", color_image, egui::TextureOptions::LINEAR))
}

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
    hooks: HookHandle,
    /// Cached result of `compute_conflicts(&bindings)`. Invalidated to None
    /// on every binding mutation; recomputed lazily on the next read.
    conflicts_cache: Option<HashSet<Trigger>>,
    /// User-overridden title regex from config, or `None` to use the
    /// built-in default. We persist whatever we loaded (round-trip stable).
    title_regex: Option<String>,
    /// Whether the "About" modal is currently displayed. Owned by App so
    /// the dynamic height logic can reserve space for the overlay card.
    show_about: bool,
    /// Decoded once at boot so the About modal can render the app icon as
    /// a `TextureHandle`. The PNG loader of `egui_extras` is **not**
    /// compiled in (only `svg` feature is enabled), so an
    /// `Image::from_bytes("bytes://*.png", ...)` would fall back to the
    /// red error placeholder. Kept as Option so a (highly improbable)
    /// decode failure doesn't bring the whole window down.
    about_icon: Option<egui::TextureHandle>,
}

#[derive(Default, PartialEq, Eq)]
struct SyncSnapshot {
    accounts: Vec<(SlotKey, isize)>,
    cycle_hwnds: Vec<isize>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        #[cfg(windows)] waiter_event: windows::Win32::Foundation::HANDLE,
    ) -> Self {
        if let Ok(handle) = cc.window_handle() {
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                tray::register_main_hwnd(h.hwnd.get());
            }
        }

        egui_extras::install_image_loaders(&cc.egui_ctx);
        let about_icon = decode_about_icon(&cc.egui_ctx);

        // Load persisted config if any (silently falls back to defaults).
        let config_path = config::default_path();
        let stored = config_path.as_ref().and_then(|p| config::load(p));

        let (theme, lang, bindings, cycle_order, title_regex) = match stored {
            Some(cfg) => (
                cfg.theme,
                cfg.lang,
                cfg.bindings.into_iter().collect::<HashMap<_, _>>(),
                cfg.cycle_order,
                // Drop a regex that won't compile so the watcher can't panic
                // at startup. The user's broken value is replaced by None in
                // memory; the next save flushes `null` to disk.
                validate_title_regex(cfg.title_regex),
            ),
            None => (
                Theme::Dark,
                i18n::detect_system_lang(),
                HashMap::new(),
                Vec::new(),
                None,
            ),
        };
        i18n::set_lang(lang);

        let (tray_tx, tray_rx) = channel();
        let tray = tray::install(cc.egui_ctx.clone(), tray_tx.clone())
            .expect("failed to install tray icon");

        #[cfg(windows)]
        {
            let waiter_ctx = cc.egui_ctx.clone();
            let waiter_tx = tray_tx;
            crate::single_instance::spawn_waiter(waiter_event, move || {
                tray::show_main_window_external();
                let _ = waiter_tx.send(TrayEvent::ShowRequested);
                waiter_ctx.request_repaint();
            });
        }

        let windows: WindowsSnapshot = Arc::new(ArcSwap::from_pointee(Vec::new()));
        let regex_src = title_regex
            .clone()
            .unwrap_or_else(|| DEFAULT_TITLE_REGEX.to_string());
        let (refresh_tx, refresh_rx) = channel();
        win::spawn_watcher(windows.clone(), cc.egui_ctx.clone(), regex_src, refresh_rx);

        let hooks = crate::hooks::install();

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
            hooks,
            conflicts_cache: None,
            title_regex,
            show_about: false,
            about_icon,
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn is_about_open(&self) -> bool {
        self.show_about
    }

    pub fn open_about(&mut self) {
        self.show_about = true;
    }

    pub fn close_about(&mut self) {
        self.show_about = false;
    }

    /// Parent of the persisted config file, used by the About modal to
    /// open the directory in Explorer. None when no usable config dir
    /// could be detected (extremely rare on Windows).
    pub fn config_dir(&self) -> Option<&std::path::Path> {
        self.config_path.as_ref().and_then(|p| p.parent())
    }

    pub fn about_icon(&self) -> Option<&egui::TextureHandle> {
        self.about_icon.as_ref()
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
        self.hooks.set_enabled(active);
        self.tray.set_active(active);
        self.update_tray_status(true);
    }

    fn update_tray_status(&mut self, force: bool) {
        let count = self.windows.load().len();
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
            // `relocalize` rebuilds the status label from its cached
            // (active, count) so the new locale shows up immediately.
            self.tray.relocalize();
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
    /// highlight conflicting rows + render the banner. Cached — the
    /// expensive part is the HashMap counting, not the small final clone.
    /// Invalidated on every binding mutation.
    pub fn conflicting_triggers(&mut self) -> HashSet<Trigger> {
        if self.conflicts_cache.is_none() {
            self.conflicts_cache = Some(compute_conflicts(&self.bindings));
        }
        self.conflicts_cache.as_ref().unwrap().clone()
    }

    fn invalidate_conflicts_cache(&mut self) {
        self.conflicts_cache = None;
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
        self.invalidate_conflicts_cache();
        self.sync_state_to_hooks(true);
        self.mark_dirty();
    }

    pub fn clear_binding(&mut self, target: &BindingTarget) {
        if self.bindings.remove(target).is_some() {
            self.invalidate_conflicts_cache();
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
        let accounts: Vec<SlotKey> = self
            .windows
            .load()
            .iter()
            .map(|w| w.slot_key.clone())
            .collect();
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
        self.hooks.set_cycle_index(idx);
    }

    /// Ensure every detected slot exists in `cycle_order` (newly detected ones
    /// are appended at the end so the user's drag-ordered prefix is preserved).
    /// Short-circuit when no clone is needed: most frames see no new slots.
    fn ensure_cycle_order_covers_snapshot(&mut self) {
        let snap = self.windows.load();
        // Fast path: every snapshot slot already in cycle_order → nothing
        // to add. Avoids a Vec clone per frame.
        let already_covered = snap
            .iter()
            .all(|w| self.cycle_order.iter().any(|s| s == &w.slot_key));
        if already_covered {
            return;
        }
        let mut changed = false;
        for w in snap.iter() {
            if !self.cycle_order.iter().any(|s| s == &w.slot_key) {
                self.cycle_order.push(w.slot_key.clone());
                changed = true;
            }
        }
        drop(snap);
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
        // Sort bindings deterministically so identical app state yields
        // byte-identical JSON across saves (clean diffs / VCS).
        let mut bindings: Vec<(BindingTarget, Trigger)> = self
            .bindings
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        bindings.sort_by(|a, b| binding_target_sort_key(&a.0).cmp(&binding_target_sort_key(&b.0)));
        let cfg = StoredConfig {
            version: config::current_version(),
            bindings,
            cycle_order: self.cycle_order.clone(),
            lang: self.lang,
            theme: self.theme,
            title_regex: self.title_regex.clone(),
        };
        let _ = config::save_atomic(path, &cfg);
    }

    /// Resolves bindings + cycle order against the current window snapshot
    /// and pushes them to the hook state. Does NOT touch the hook's
    /// `cycle_index` — that lives independently. Skipped when nothing has
    /// changed unless `force` is true.
    fn sync_state_to_hooks(&mut self, force: bool) {
        let accounts: Vec<(SlotKey, isize)> = self
            .windows
            .load()
            .iter()
            .map(|w| (w.slot_key.clone(), w.hwnd))
            .collect();

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

        self.hooks.set_bindings_and_cycle(bindings, cycle_hwnds);
    }

    /// Resize the OS window to fit the current content height. Width stays
    /// fixed at 340. Height clamped to [280, 600].
    ///
    /// We can't rely on `ctx.used_rect()` here: when the viewport is too
    /// small the activate button overflows below the panel, doesn't get
    /// allocated, and never enters `used_rect` — leaving the window
    /// permanently undersized. Constants below are tuned to match the
    /// layouts in `main_view::draw`.
    fn apply_dynamic_height(&mut self, ctx: &egui::Context) {
        const TITLE_BAR: f32 = 36.0;
        const PANEL_PAD: f32 = 24.0; // 12 top + 12 bottom
        const SUMMARY: f32 = 22.0; // count label + refresh button row
        const ROW: f32 = 46.0; // frame ~40 + 6 inter-row gap
        const CYCLE_HEAD: f32 = 32.0; // separator + label + spacing
        const CYCLE_BLOCK: f32 = 2.0 * ROW + 4.0; // 2 rows + small slack
        const SUMMARY_GAP: f32 = 10.0;
        const TAIL_PAD: f32 = 12.0;
        // Banner: 6 leading space + 16 frame inner_margin + 14 per line.
        const BANNER_BASE: f32 = 22.0;
        const BANNER_PER_LINE: f32 = 14.0;
        // Activate/deactivate full-width button + hint when inactive.
        const TOGGLE_BUTTON: f32 = 46.0;
        const TOGGLE_HINT: f32 = 22.0;

        let n_accounts = self.windows.load().len() as f32;
        let n_for_height = n_accounts.max(1.0); // reserve ~1 row even when empty
        let n_conflicts = self.conflicting_triggers().len() as f32;
        let banner_h = if n_conflicts > 0.0 {
            BANNER_BASE + n_conflicts * BANNER_PER_LINE
        } else {
            0.0
        };
        let toggle_h = TOGGLE_BUTTON + if self.is_active { 0.0 } else { TOGGLE_HINT };
        let desired_content = TITLE_BAR
            + PANEL_PAD
            + SUMMARY
            + SUMMARY_GAP
            + n_for_height * ROW
            + banner_h
            + CYCLE_HEAD
            + CYCLE_BLOCK
            + toggle_h
            + TAIL_PAD;
        // The About modal card is 380 px tall + 20 px padding. When it's
        // open we force the viewport at least that tall so the card
        // doesn't get clipped at the bottom — a clipped close button
        // would visually look like the modal is broken.
        const ABOUT_MIN: f32 = 420.0;
        let mut desired = desired_content;
        if self.show_about {
            desired = desired.max(ABOUT_MIN);
        }
        let desired = desired.clamp(280.0, 600.0);

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
                TrayEvent::ToggleRequested => {
                    // The tray callback already called Win32 ShowWindow to
                    // wake the parked eframe loop. eframe's tracked
                    // visibility state can be stale at this point, so we
                    // *always* send Visible(true) first to resync, then
                    // apply the desired final state. Without the resync,
                    // a later Visible(false) gets deduplicated when eframe
                    // already thinks the viewport is hidden.
                    let new_active = !self.is_active;
                    self.set_active(new_active);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    if new_active {
                        // Tray Activer: hide window (running in background).
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    } else {
                        // Tray Désactiver: bring the window forward.
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
        ui::about::draw(ctx, self);

        self.flush_if_due();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.last_dirty_at.is_some() {
            self.persist_config();
            self.last_dirty_at = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triggers::{BindingTarget, MouseBtn, Trigger};

    fn account(name: &str) -> BindingTarget {
        BindingTarget::Account(name.to_string())
    }

    #[test]
    fn no_bindings_have_no_conflicts() {
        let bindings: HashMap<BindingTarget, Trigger> = HashMap::new();
        assert!(compute_conflicts(&bindings).is_empty());
    }

    #[test]
    fn single_binding_has_no_conflict() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        assert!(compute_conflicts(&bindings).is_empty());
    }

    #[test]
    fn distinct_triggers_have_no_conflict() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        bindings.insert(account("B"), Trigger::Key(0x71));
        bindings.insert(BindingTarget::CycleNext, Trigger::Mouse(MouseBtn::X1));
        assert!(compute_conflicts(&bindings).is_empty());
    }

    #[test]
    fn two_targets_same_trigger_yields_one_conflict() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        bindings.insert(account("B"), Trigger::Key(0x70));
        let conflicts = compute_conflicts(&bindings);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts.contains(&Trigger::Key(0x70)));
    }

    #[test]
    fn three_targets_two_distinct_triggers_one_overlap() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        bindings.insert(account("B"), Trigger::Key(0x71));
        bindings.insert(BindingTarget::CycleNext, Trigger::Key(0x70));
        let conflicts = compute_conflicts(&bindings);
        assert_eq!(conflicts, [Trigger::Key(0x70)].into());
    }

    #[test]
    fn three_targets_all_same_trigger_one_conflict_entry() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        bindings.insert(account("B"), Trigger::Key(0x70));
        bindings.insert(BindingTarget::CycleNext, Trigger::Key(0x70));
        let conflicts = compute_conflicts(&bindings);
        // Conflict set is "trigger has more than one assignment", not a count.
        assert_eq!(conflicts.len(), 1);
    }

    #[test]
    fn account_and_cycle_with_same_trigger_conflict() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Mouse(MouseBtn::X1));
        bindings.insert(BindingTarget::CyclePrev, Trigger::Mouse(MouseBtn::X1));
        let conflicts = compute_conflicts(&bindings);
        assert_eq!(conflicts, [Trigger::Mouse(MouseBtn::X1)].into());
    }

    #[test]
    fn validate_title_regex_passes_through_none() {
        assert_eq!(validate_title_regex(None), None);
    }

    #[test]
    fn validate_title_regex_keeps_valid_pattern() {
        let valid = "^Dofus - .+$".to_string();
        assert_eq!(validate_title_regex(Some(valid.clone())), Some(valid));
    }

    #[test]
    fn validate_title_regex_drops_invalid_pattern() {
        // Unbalanced paren — won't compile, must be dropped to None so the
        // watcher falls back on DEFAULT_TITLE_REGEX instead of panicking.
        assert_eq!(validate_title_regex(Some("((".to_string())), None);
    }

    #[test]
    fn validate_title_regex_keeps_default_pattern() {
        // The built-in default must always be considered valid — sanity check
        // against future drift between the constant and the validator.
        assert_eq!(
            validate_title_regex(Some(crate::win::DEFAULT_TITLE_REGEX.to_string())),
            Some(crate::win::DEFAULT_TITLE_REGEX.to_string()),
        );
    }

    #[test]
    fn binding_target_sort_key_orders_account_before_cycle() {
        // Persistence relies on this ordering: byte-stable JSON requires
        // accounts to come first, then CycleNext, then CyclePrev.
        let mut targets = vec![
            BindingTarget::CyclePrev,
            BindingTarget::Account("B".into()),
            BindingTarget::CycleNext,
            BindingTarget::Account("A".into()),
        ];
        targets.sort_by(|a, b| binding_target_sort_key(a).cmp(&binding_target_sort_key(b)));
        assert_eq!(
            targets,
            vec![
                BindingTarget::Account("A".into()),
                BindingTarget::Account("B".into()),
                BindingTarget::CycleNext,
                BindingTarget::CyclePrev,
            ]
        );
    }

    #[test]
    fn binding_target_sort_key_orders_accounts_alphabetically() {
        let mut targets = vec![
            BindingTarget::Account("Charlie".into()),
            BindingTarget::Account("Alpha".into()),
            BindingTarget::Account("Bravo".into()),
        ];
        targets.sort_by(|a, b| binding_target_sort_key(a).cmp(&binding_target_sort_key(b)));
        assert_eq!(
            targets,
            vec![
                BindingTarget::Account("Alpha".into()),
                BindingTarget::Account("Bravo".into()),
                BindingTarget::Account("Charlie".into()),
            ]
        );
    }
}
