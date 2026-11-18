use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use eframe::egui;
use raw_window_handle::HasWindowHandle;

use crate::config::{self, StoredConfig};
use crate::dofus::{WindowsSnapshot, DEFAULT_TITLE_REGEX};
use crate::hooks::{BindingAction, HookHandle};
use crate::i18n::{self, Lang};
use crate::platform::{self, single_instance, MainWindow, WindowId};
use crate::theme::{self, Theme};
use crate::tray::{self, TrayController, TrayEvent};
use crate::triggers::{BindingTarget, SlotKey, Trigger};
use crate::ui;

const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Triggers assigned to 2+ BindingTargets that are actually wired.
///
/// Account bindings whose slot is absent from `live_slots` are skipped:
/// `sync_state_to_hooks` only emits a `Focus` action for slots that have a
/// live HWND, so a binding left over from a client that isn't running can
/// never collide with anything. Cycle targets always count — they're pushed
/// to the hook regardless of how many clients are up.
pub(crate) fn compute_conflicts(
    bindings: &HashMap<BindingTarget, Trigger>,
    live_slots: &[&str],
) -> HashSet<Trigger> {
    let mut counts: HashMap<Trigger, u32> = HashMap::new();
    for (target, t) in bindings {
        if let BindingTarget::Account(slot) = target
            && !live_slots.contains(&slot.as_str())
        {
            continue;
        }
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

/// Decode every class avatar PNG once at boot. 64×64 is enough to stay
/// crisp on HiDPI when the row renders the icon at 20×20 logical px.
/// Failed decodes are silently skipped — the affected row falls back to
/// "no icon" rather than a placeholder, matching the project's fail-soft
/// stance. Key is the canonical lowercase filename stem (cf. `assets`).
fn decode_class_icons(
    ctx: &egui::Context,
) -> HashMap<&'static str, egui::TextureHandle> {
    let mut map = HashMap::with_capacity(crate::assets::CLASS_PNGS.len());
    for (name, bytes) in crate::assets::CLASS_PNGS {
        let Ok(img) = image::load_from_memory(bytes) else { continue };
        let resized = img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let (w, h) = (rgba.width() as usize, rgba.height() as usize);
        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw());
        let handle = ctx.load_texture(
            format!("class-{name}"),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        map.insert(*name, handle);
    }
    map
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
    /// Cached result of `compute_conflicts`. Invalidated to None on every
    /// binding mutation *and* whenever the window snapshot changes (the set
    /// of live slots is an input); recomputed lazily on the next read.
    conflicts_cache: Option<HashSet<Trigger>>,
    /// User-overridden title regex from config, or `None` to use the
    /// built-in default. We persist whatever we loaded (round-trip stable).
    title_regex: Option<String>,
    /// Whether the settings modal is displayed. Owned by App so the dynamic
    /// height logic can reserve space for the overlay card.
    show_settings: bool,
    settings_tab: ui::settings::Tab,
    shortcuts_only_in_dofus: bool,
    minimize_to_taskbar: bool,
    always_on_top: bool,
    /// `platform::input_permission_granted`, read once per frame so the
    /// banner and the window height agree.
    input_permission: bool,
    main_window: Option<MainWindow>,
    /// Last known top-left corner of the main window (`platform::placement`).
    window_pos: Option<(i32, i32)>,
    /// Decoded once at boot. egui_extras is built without the PNG loader
    /// (only `svg`), so `Image::from_bytes("bytes://*.png", ...)` would
    /// render the red error placeholder. None on decode failure.
    about_icon: Option<egui::TextureHandle>,
    /// One texture per Dofus class, keyed by canonical filename stem
    /// (`"iop"`, `"feca"`, …). Decoded once at boot from `assets::CLASS_PNGS`.
    /// Looked up via `class_icon::class_filename(raw_class)`.
    class_icons: HashMap<&'static str, egui::TextureHandle>,
}

#[derive(Default, PartialEq, Eq)]
struct SyncSnapshot {
    accounts: Vec<(SlotKey, WindowId)>,
    cycle_hwnds: Vec<WindowId>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        instance: single_instance::SingleInstanceGuard,
    ) -> Self {
        let main_window = cc
            .window_handle()
            .ok()
            .and_then(|h| MainWindow::from_raw_handle(h.as_raw()));
        if let Some(window) = main_window {
            platform::register_main_window(window);
        }

        egui_extras::install_image_loaders(&cc.egui_ctx);
        let about_icon = decode_about_icon(&cc.egui_ctx);
        let class_icons = decode_class_icons(&cc.egui_ctx);

        // Load persisted config if any (silently falls back to defaults).
        let config_path = config::default_path();
        let stored = config_path.as_ref().and_then(|p| config::load(p));
        let shortcuts_only_in_dofus = stored.as_ref().is_some_and(|c| c.shortcuts_only_in_dofus);
        let minimize_to_taskbar = stored.as_ref().is_some_and(|c| c.minimize_to_taskbar);
        let always_on_top = stored.as_ref().is_none_or(|c| c.always_on_top);
        let window_pos = stored.as_ref().and_then(|c| c.window_pos);
        if always_on_top {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::AlwaysOnTop,
            ));
        }
        if let (Some(window), Some(pos)) = (main_window, window_pos) {
            platform::placement::restore(window, pos);
        }

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
                platform::detect_system_lang(),
                HashMap::new(),
                Vec::new(),
                None,
            ),
        };
        i18n::set_lang(lang);

        let (tray_tx, tray_rx) = channel();
        let tray = tray::install(cc.egui_ctx.clone(), tray_tx.clone())
            .expect("failed to install tray icon");

        let waiter_ctx = cc.egui_ctx.clone();
        let waiter_tx = tray_tx;
        single_instance::spawn_waiter(instance, move || {
            platform::main_window_restored();
            platform::show_main_window();
            let _ = waiter_tx.send(TrayEvent::ShowRequested);
            waiter_ctx.request_repaint();
        });

        let windows: WindowsSnapshot = Arc::new(ArcSwap::from_pointee(Vec::new()));
        let regex_src = title_regex
            .clone()
            .unwrap_or_else(|| DEFAULT_TITLE_REGEX.to_string());
        let (refresh_tx, refresh_rx) = channel();
        platform::spawn_watcher(windows.clone(), cc.egui_ctx.clone(), regex_src, refresh_rx);

        let hooks = crate::hooks::install(shortcuts_only_in_dofus);

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
            show_settings: false,
            settings_tab: ui::settings::Tab::Options,
            shortcuts_only_in_dofus,
            minimize_to_taskbar,
            always_on_top,
            input_permission: platform::input_permission_granted(),
            main_window,
            window_pos,
            about_icon,
            class_icons,
        }
    }

    pub fn class_icon(&self, name: &str) -> Option<&egui::TextureHandle> {
        self.class_icons.get(name)
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn input_permission(&self) -> bool {
        self.input_permission
    }

    /// The system prompt and the settings pane open as ordinary windows: an
    /// always-on-top ROrganizer would cover them, so it steps down until
    /// the permission arrives (see `ui`).
    pub fn request_input_permission(&self, ctx: &egui::Context) {
        if self.always_on_top {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        }
        platform::request_input_permission();
    }

    pub fn is_settings_open(&self) -> bool {
        self.show_settings
    }

    /// Opens on the Options tab: the gear is first a settings button.
    pub fn open_settings(&mut self) {
        self.show_settings = true;
        self.settings_tab = ui::settings::Tab::Options;
    }

    pub fn close_settings(&mut self) {
        self.show_settings = false;
    }

    pub fn settings_tab(&self) -> ui::settings::Tab {
        self.settings_tab
    }

    pub fn set_settings_tab(&mut self, tab: ui::settings::Tab) {
        self.settings_tab = tab;
    }

    pub fn shortcuts_only_in_dofus(&self) -> bool {
        self.shortcuts_only_in_dofus
    }

    pub fn set_shortcuts_only_in_dofus(&mut self, on: bool) {
        if self.shortcuts_only_in_dofus != on {
            self.shortcuts_only_in_dofus = on;
            self.hooks.set_dofus_only(on);
            self.mark_dirty();
        }
    }

    pub fn minimize_to_taskbar(&self) -> bool {
        self.minimize_to_taskbar
    }

    pub fn set_minimize_to_taskbar(&mut self, on: bool) {
        if self.minimize_to_taskbar != on {
            self.minimize_to_taskbar = on;
            self.mark_dirty();
        }
    }

    pub fn always_on_top(&self) -> bool {
        self.always_on_top
    }

    /// Applied right away, no restart needed.
    pub fn set_always_on_top(&mut self, ctx: &egui::Context, on: bool) {
        if self.always_on_top != on {
            self.always_on_top = on;
            let level = if on {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            self.mark_dirty();
        }
    }

    /// Parent of the persisted config file, used by the settings modal to
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
        self.put_away(ctx);
    }

    /// Takes the window off screen the way the user chose: minimized to the
    /// taskbar, or hidden in the system tray.
    fn put_away(&self, ctx: &egui::Context) {
        if self.minimize_to_taskbar {
            platform::minimize_main_window(ctx);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            platform::main_window_hidden_to_tray();
        }
    }

    /// Brings the window back, whichever way it was put away.
    fn bring_back(ctx: &egui::Context) {
        platform::main_window_restored();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
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

    /// Reorders the taskbar buttons of the live accounts to match
    /// `cycle_order`, on a background thread. Repaints once done so the
    /// greyed-out button comes back.
    pub fn sort_taskbar(&self, ctx: &egui::Context) {
        let snap = self.windows.load();
        let hwnds: Vec<WindowId> = self
            .cycle_order
            .iter()
            .filter_map(|slot| snap.iter().find(|w| &w.slot_key == slot).map(|w| w.hwnd))
            .collect();
        let ctx = ctx.clone();
        platform::taskbar::reorder(hwnds, move || ctx.request_repaint());
    }

    /// Records where the user left the window. Skipped while it is minimized
    /// or hidden, where Windows reports a parking spot.
    fn track_window_pos(&mut self) {
        let pos = self.main_window.and_then(platform::placement::position);
        if pos.is_some() && pos != self.window_pos {
            self.window_pos = pos;
            self.mark_dirty();
        }
    }

    pub fn request_minimize(&self, ctx: &egui::Context) {
        self.put_away(ctx);
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

    /// Set of triggers assigned to ≥ 2 wired BindingTargets. Used by the UI
    /// to highlight conflicting rows + render the banner. Cached — the
    /// expensive part is the HashMap counting, not the small final clone.
    /// Invalidated on binding mutations and on snapshot changes.
    pub fn conflicting_triggers(&mut self) -> HashSet<Trigger> {
        if self.conflicts_cache.is_none() {
            let snap = self.windows.load();
            let live: Vec<&str> = snap.iter().map(|w| w.slot_key.as_str()).collect();
            self.conflicts_cache = Some(compute_conflicts(&self.bindings, &live));
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
            shortcuts_only_in_dofus: self.shortcuts_only_in_dofus,
            minimize_to_taskbar: self.minimize_to_taskbar,
            always_on_top: self.always_on_top,
            window_pos: self.window_pos,
        };
        let _ = config::save_atomic(path, &cfg);
    }

    /// Resolves bindings + cycle order against the current window snapshot
    /// and pushes them to the hook state. Does NOT touch the hook's
    /// `cycle_index` — that lives independently. Skipped when nothing has
    /// changed unless `force` is true.
    fn sync_state_to_hooks(&mut self, force: bool) {
        let accounts: Vec<(SlotKey, WindowId)> = self
            .windows
            .load()
            .iter()
            .map(|w| (w.slot_key.clone(), w.hwnd))
            .collect();

        let cycle_hwnds: Vec<WindowId> = self
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
        // The live slot set feeds conflict detection too.
        self.invalidate_conflicts_cache();

        let mut bindings: Vec<(Trigger, BindingAction)> =
            Vec::with_capacity(self.bindings.len());

        // 1. Account bindings ordered by cycle_order — the topmost slot wins
        //    on a conflict, and the user controls that order via drag&drop.
        for slot in &self.cycle_order {
            if let Some(trig) = self.bindings.get(&BindingTarget::Account(slot.clone()))
                && let Some(hwnd) = accounts
                    .iter()
                    .find(|(s, _)| s == slot)
                    .map(|(_, h)| *h)
            {
                bindings.push((*trig, BindingAction::Focus(hwnd)));
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
        const ROW: f32 = 46.0; // frame ~40 (32 icon + 8 v-pad) + 6 inter-row gap
        // `draw_empty_state`: 20 space + 13 pt title + 6 + 11 pt hint. Taller
        // than one ROW: reserving a row clipped the bottom of the window.
        const EMPTY_STATE: f32 = 64.0;
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

        let n_accounts = self.windows.load().len();
        let list_h = if n_accounts == 0 { EMPTY_STATE } else { n_accounts as f32 * ROW };
        let n_conflicts = self.conflicting_triggers().len() as f32;
        let banner_h = if n_conflicts > 0.0 {
            BANNER_BASE + n_conflicts * BANNER_PER_LINE
        } else {
            0.0
        };
        let toggle_h = TOGGLE_BUTTON + if self.is_active { 0.0 } else { TOGGLE_HINT };
        let permission_h = if self.input_permission {
            0.0
        } else {
            ui::main_view::permission_banner_height(ctx, self.theme)
        };
        let desired_content = TITLE_BAR
            + PANEL_PAD
            + SUMMARY
            + SUMMARY_GAP
            + list_h
            + banner_h
            + permission_h
            + CYCLE_HEAD
            + CYCLE_BLOCK
            + toggle_h
            + TAIL_PAD;
        // While the settings card is open, the viewport is kept at least that
        // tall (+ margin) so the card isn't clipped at the bottom — a clipped
        // close button would look like a broken modal.
        let mut desired = desired_content;
        if self.show_settings {
            desired = desired.max(ui::settings::CARD_HEIGHT + 20.0);
        }
        let desired = desired.clamp(280.0, 600.0);

        if (desired - self.last_synced_height).abs() > 0.5 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(340.0, desired)));
            self.last_synced_height = desired;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        while let Ok(ev) = self.tray_rx.try_recv() {
            match ev {
                TrayEvent::ShowRequested => Self::bring_back(ctx),
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
                        // Tray Activer: put the window away (running in background).
                        self.put_away(ctx);
                    } else {
                        // Tray Désactiver: bring the window forward.
                        Self::bring_back(ctx);
                    }
                }
            }
        }

        if !self.visuals_applied {
            theme::apply(ctx, self.theme);
            self.visuals_applied = true;
        }

        let input_permission = platform::input_permission_granted();
        if input_permission && !self.input_permission && self.always_on_top {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::AlwaysOnTop,
            ));
        }
        self.input_permission = input_permission;
        self.ensure_cycle_order_covers_snapshot();
        self.sync_state_to_hooks(false);
        self.update_tray_status(false);
        self.apply_dynamic_height(ctx);
        self.track_window_pos();

        ui::header::draw(ui, self);
        ui::main_view::draw(ui, self);
        ui::capture::draw(ctx, self);
        ui::settings::draw(ctx, self);

        self.flush_if_due();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.track_window_pos();
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
        assert!(compute_conflicts(&bindings, &[]).is_empty());
    }

    #[test]
    fn single_binding_has_no_conflict() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        assert!(compute_conflicts(&bindings, &["A"]).is_empty());
    }

    #[test]
    fn distinct_triggers_have_no_conflict() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        bindings.insert(account("B"), Trigger::Key(0x71));
        bindings.insert(BindingTarget::CycleNext, Trigger::Mouse(MouseBtn::X1));
        assert!(compute_conflicts(&bindings, &["A", "B"]).is_empty());
    }

    #[test]
    fn two_targets_same_trigger_yields_one_conflict() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        bindings.insert(account("B"), Trigger::Key(0x70));
        let conflicts = compute_conflicts(&bindings, &["A", "B"]);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts.contains(&Trigger::Key(0x70)));
    }

    #[test]
    fn three_targets_two_distinct_triggers_one_overlap() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        bindings.insert(account("B"), Trigger::Key(0x71));
        bindings.insert(BindingTarget::CycleNext, Trigger::Key(0x70));
        let conflicts = compute_conflicts(&bindings, &["A", "B"]);
        assert_eq!(conflicts, [Trigger::Key(0x70)].into());
    }

    #[test]
    fn three_targets_all_same_trigger_one_conflict_entry() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        bindings.insert(account("B"), Trigger::Key(0x70));
        bindings.insert(BindingTarget::CycleNext, Trigger::Key(0x70));
        let conflicts = compute_conflicts(&bindings, &["A", "B"]);
        // Conflict set is "trigger has more than one assignment", not a count.
        assert_eq!(conflicts.len(), 1);
    }

    #[test]
    fn account_and_cycle_with_same_trigger_conflict() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Mouse(MouseBtn::X1));
        bindings.insert(BindingTarget::CyclePrev, Trigger::Mouse(MouseBtn::X1));
        let conflicts = compute_conflicts(&bindings, &["A"]);
        assert_eq!(conflicts, [Trigger::Mouse(MouseBtn::X1)].into());
    }

    #[test]
    fn offline_account_does_not_conflict_with_live_one() {
        // The reported bug: reassigning a key held by a client that isn't
        // running must not light up the banner.
        let mut bindings = HashMap::new();
        bindings.insert(account("Live"), Trigger::Key(0x70));
        bindings.insert(account("Offline"), Trigger::Key(0x70));
        assert!(compute_conflicts(&bindings, &["Live"]).is_empty());
    }

    #[test]
    fn two_offline_accounts_do_not_conflict() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        bindings.insert(account("B"), Trigger::Key(0x70));
        assert!(compute_conflicts(&bindings, &[]).is_empty());
    }

    #[test]
    fn offline_account_does_not_conflict_with_cycle() {
        let mut bindings = HashMap::new();
        bindings.insert(account("Offline"), Trigger::Key(0x70));
        bindings.insert(BindingTarget::CycleNext, Trigger::Key(0x70));
        assert!(compute_conflicts(&bindings, &["Other"]).is_empty());
    }

    #[test]
    fn both_cycles_conflict_without_any_live_account() {
        // Cycle bindings are always pushed to the hook, so they collide even
        // when no client is running.
        let mut bindings = HashMap::new();
        bindings.insert(BindingTarget::CycleNext, Trigger::Key(0x70));
        bindings.insert(BindingTarget::CyclePrev, Trigger::Key(0x70));
        assert_eq!(compute_conflicts(&bindings, &[]), [Trigger::Key(0x70)].into());
    }

    #[test]
    fn conflict_reappears_when_the_offline_account_comes_back() {
        let mut bindings = HashMap::new();
        bindings.insert(account("A"), Trigger::Key(0x70));
        bindings.insert(account("B"), Trigger::Key(0x70));
        assert!(compute_conflicts(&bindings, &["A"]).is_empty());
        assert_eq!(
            compute_conflicts(&bindings, &["A", "B"]),
            [Trigger::Key(0x70)].into()
        );
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
            validate_title_regex(Some(DEFAULT_TITLE_REGEX.to_string())),
            Some(DEFAULT_TITLE_REGEX.to_string()),
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
