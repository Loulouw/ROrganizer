//! Character overlay: a small always-on-top bar with one head per running
//! client, the one in front highlighted. Clicking a head brings that client
//! forward; the grip moves the bar, its corner resizes it, a right-click
//! opens a menu.
//!
//! This module is OS-agnostic: the state shared with the overlay windows,
//! the geometry, the palette, the gestures, the initials rule, the
//! visibility rule and the menu model. Two renderers draw from it: egui
//! viewports (`ui::overlay`, when `platform::OVERLAY_IS_EGUI`) or the
//! backend's native windows (`platform::overlay`).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Pos2, Rect, Vec2, ViewportId};
use rust_i18n::t;
use serde::{Deserialize, Serialize};

use crate::platform::{overlay as native, OverlayWindow, WindowId};
use crate::theme::{self, Theme};
use crate::triggers::SlotKey;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    #[default]
    Horizontal,
    Vertical,
}

/// What each slot shows. Clients without a known class (Dofus Retro, a
/// custom title regex without `class`) show their initials either way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Display {
    #[default]
    ClassHead,
    Initials,
}

/// When the bar shows while ROrganizer is active.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    /// Only while Dofus (or ROrganizer itself) is in front.
    #[default]
    WithDofus,
    Always,
}

/// Side of one head, in points.
pub const HEAD_MIN: f32 = 24.0;
pub const HEAD_MAX: f32 = 96.0;
pub const HEAD_DEFAULT: f32 = 40.0;

/// Window titles: how `platform::overlay::attach` finds the windows egui
/// created for the viewports.
pub const OVERLAY_TITLE: &str = "ROrganizer overlay";
pub const TIP_TITLE: &str = "ROrganizer overlay tooltip";
pub const MENU_TITLE: &str = "ROrganizer overlay menu";

/// How long the bar stays up once neither Dofus nor ROrganizer is in front.
/// Alt-Tab, or a click on the taskbar, briefly puts another window in front:
/// without the delay the bar would blink on every switch.
pub const HIDE_DELAY: Duration = Duration::from_millis(150);

/// Hover time before a head shows its tooltip.
pub const TIP_DELAY: Duration = Duration::from_millis(350);

/// How long a clicked head stays highlighted while its client comes to
/// the front. The foreground switch isn't instant: reading it back right
/// away would flash the previous head.
pub const FOCUS_GRACE: Duration = Duration::from_millis(800);

pub fn overlay_viewport() -> ViewportId {
    ViewportId::from_hash_of("rorganizer-overlay")
}

pub fn tip_viewport() -> ViewportId {
    ViewportId::from_hash_of("rorganizer-overlay-tooltip")
}

pub fn menu_viewport() -> ViewportId {
    ViewportId::from_hash_of("rorganizer-overlay-menu")
}

/// Called by the platform watcher whenever another window comes to the
/// front: the bar repaints its highlight right away, the root re-evaluates
/// whether the bar should show.
pub fn foreground_changed(ctx: &egui::Context) {
    ctx.request_repaint_of(ViewportId::ROOT);
    ctx.request_repaint_of(overlay_viewport());
}

// ---------------------------------------------------------------------------
// State shared between App (root viewport) and the overlay viewports.

pub struct Slot {
    pub key: SlotKey,
    pub window: WindowId,
    pub icon: Option<egui::TextureHandle>,
    pub initials: String,
    pub class: Option<String>,
    pub binding: Option<String>,
}

/// Something the overlay viewport did that App must persist or act on.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Top-left corner, in `platform::overlay` screen coordinates.
    Moved((i32, i32)),
    Resized(f32),
    Menu(MenuAction),
    /// A head was clicked and its client brought forward.
    Focused(SlotKey),
}

/// Pointer gesture in progress, in `platform::overlay` screen coordinates.
#[derive(Clone, Copy, Debug)]
pub enum Gesture {
    Move { cursor: (i32, i32), origin: (i32, i32) },
    Resize { cursor: (i32, i32), head: f32 },
    /// Pressed on a head; a click if released on the same one.
    Press(usize),
}

#[derive(Default)]
pub struct State {
    // Written by App.
    pub slots: Vec<Slot>,
    pub layout: Layout,
    pub display: Display,
    pub head: f32,
    pub theme: Option<Theme>,
    pub visibility: Visibility,
    /// Where the bar was left last time; used once, when the window appears.
    pub saved_pos: Option<(i32, i32)>,

    // Owned by the viewports.
    pub window: Option<OverlayWindow>,
    pub tip_window: Option<OverlayWindow>,
    pub menu_window: Option<OverlayWindow>,
    /// Client in front, kept while ROrganizer itself is (menu, main window).
    pub front: Option<SlotKey>,
    /// Client just clicked, still on its way to the front.
    pub pending_front: Option<(WindowId, Instant)>,
    pub gesture: Option<Gesture>,
    pub hover: Option<(usize, Instant)>,
    pub tip: Option<Tip>,
    pub menu: Option<Menu>,
    /// Window size last sent, in points, and the scale it was fitted at.
    pub sent_size: Option<(Vec2, f32)>,

    // Read and cleared by App.
    pub events: Vec<Event>,
}

/// Tooltip content and where it goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Tip {
    pub name: String,
    pub detail: Option<String>,
    pub binding: Option<String>,
    pub slot: usize,
}

/// The right-click menu while it is open.
#[derive(Clone, Debug)]
pub struct Menu {
    pub items: Vec<(MenuEntry, Option<MenuAction>)>,
    /// Window in front when it opened, given the foreground back on a pick.
    pub previous: Option<WindowId>,
    pub opened: Instant,
    /// `platform::overlay::activate` was called on its first frame.
    pub activated: bool,
    /// The menu window got the focus: losing it now means a click elsewhere.
    pub focused: bool,
}

pub type Shared = Arc<Mutex<State>>;

pub fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, State> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

impl State {
    pub fn geometry(&self) -> Geometry {
        Geometry {
            layout: self.layout,
            head: self.head,
            count: self.slots.len().max(1),
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme.unwrap_or(Theme::Dark)
    }

    /// The head drawn lit: the one under a press lights up right away (the
    /// switch happens on release), otherwise the client in front.
    pub fn lit(&self) -> Option<&SlotKey> {
        match self.gesture {
            Some(Gesture::Press(i)) => self.slots.get(i).map(|s| &s.key),
            _ => self.front.as_ref(),
        }
    }
}

// ---------------------------------------------------------------------------
// Palette, in points, shared by both renderers.

/// Distance between the bar and its tooltip.
pub const TIP_GAP: f32 = 6.0;
pub const TIP_PAD: Vec2 = Vec2::new(8.0, 5.0);
/// Padding of the shortcut pill in the tooltip.
pub const PILL_PAD: Vec2 = Vec2::new(6.0, 2.0);
/// Opacity of the heads that aren't lit.
pub const DIM_ALPHA: u8 = 115;
pub const BAR_RADIUS: u8 = 10;
pub const TIP_RADIUS: u8 = 6;
pub const PILL_RADIUS: u8 = 4;

/// `(fill, border)` of the bar and the tooltip.
pub fn bar_colors(theme: Theme) -> (Color32, Color32) {
    let [r, g, b, _] = theme::window_bg(theme).to_array();
    let border = match theme {
        Theme::Dark => Color32::from_white_alpha(36),
        Theme::Light => Color32::from_black_alpha(46),
    };
    (Color32::from_rgba_unmultiplied(r, g, b, 235), border)
}

/// Outline of the lit head, check mark of the menu.
pub fn active_color(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => theme::ACTIVE_DARK,
        Theme::Light => theme::ACTIVE_LIGHT,
    }
}

/// Disc behind the initials.
pub fn initials_disc(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => Color32::from_rgb(0x3A, 0x3A, 0x33),
        Theme::Light => Color32::from_rgb(0xD9, 0xD6, 0xCE),
    }
}

/// Corner radius of a head's highlight.
pub fn head_radius(head: f32) -> u8 {
    (head * 0.18).round().clamp(4.0, 12.0) as u8
}

// ---------------------------------------------------------------------------
// Geometry, in points, origin at the bar's top-left corner.

const PAD: f32 = 4.0;
const GAP: f32 = 3.0;
const GRIP: f32 = 12.0;
const RESIZE: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Resize,
    Grip,
    Slot(usize),
    Bar,
}

#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub layout: Layout,
    pub head: f32,
    pub count: usize,
}

impl Geometry {
    pub fn size(&self) -> Vec2 {
        let n = self.count as f32;
        let along = PAD + GRIP + GAP + n * self.head + (n - 1.0).max(0.0) * GAP + PAD;
        let across = PAD + self.head + PAD;
        self.oriented(along, across)
    }

    fn oriented(&self, along: f32, across: f32) -> Vec2 {
        match self.layout {
            Layout::Horizontal => Vec2::new(along, across),
            Layout::Vertical => Vec2::new(across, along),
        }
    }

    fn rect(&self, along: f32, across: f32, len_along: f32, len_across: f32) -> Rect {
        let min = self.oriented(along, across);
        Rect::from_min_size(Pos2::new(min.x, min.y), self.oriented(len_along, len_across))
    }

    pub fn grip(&self) -> Rect {
        self.rect(PAD, PAD, GRIP, self.head)
    }

    pub fn slot(&self, i: usize) -> Rect {
        let along = PAD + GRIP + GAP + i as f32 * (self.head + GAP);
        self.rect(along, PAD, self.head, self.head)
    }

    /// Bottom-right corner, whatever the layout.
    pub fn resize_handle(&self) -> Rect {
        let size = self.size();
        Rect::from_min_max(Pos2::new(size.x - RESIZE, size.y - RESIZE), size.to_pos2())
    }

    pub fn hit(&self, p: Pos2) -> Hit {
        if self.resize_handle().contains(p) {
            return Hit::Resize;
        }
        if self.grip().contains(p) {
            return Hit::Grip;
        }
        (0..self.count)
            .find(|&i| self.slot(i).contains(p))
            .map_or(Hit::Bar, Hit::Slot)
    }

    /// Centres of the grip's six dots: two rows of three along the bar.
    pub fn grip_dots(&self) -> impl Iterator<Item = Pos2> {
        let (layout, center) = (self.layout, self.grip().center());
        (-1..=1).flat_map(move |k| {
            [-1.0, 1.0].map(|side| {
                let (dx, dy) = match layout {
                    Layout::Horizontal => (side * 2.5, k as f32 * 5.0),
                    Layout::Vertical => (k as f32 * 5.0, side * 2.5),
                };
                center + Vec2::new(dx, dy)
            })
        })
    }

    /// The two strokes marking the resize corner.
    pub fn resize_marks(&self) -> [[Pos2; 2]; 2] {
        let corner = self.resize_handle().max - Vec2::splat(3.0);
        [4.0, 7.5].map(|len| [corner - Vec2::new(len, 0.0), corner - Vec2::new(0.0, len)])
    }
}

/// Head size while the corner is dragged by `delta` points: the corner
/// follows whichever axis moved the most, the long axis counting once per
/// head.
pub fn resized_head(layout: Layout, count: usize, head: f32, delta: Vec2) -> f32 {
    let (along, across) = match layout {
        Layout::Horizontal => (delta.x, delta.y),
        Layout::Vertical => (delta.y, delta.x),
    };
    let along = along / count.max(1) as f32;
    let change = if along.abs() > across.abs() { along } else { across };
    (head + change).round().clamp(HEAD_MIN, HEAD_MAX)
}

/// Screen rectangle as `(left, top, right, bottom)`.
pub type ScreenRect = (i32, i32, i32, i32);

/// `pos` moved so a `size` rectangle there lies entirely inside `work`
/// (pinned top-left when it can't fit).
pub fn fit_into(work: ScreenRect, pos: (i32, i32), size: (i32, i32)) -> (i32, i32) {
    let (l, t, r, b) = work;
    let x = pos.0.clamp(l, (r - size.0).max(l));
    let y = pos.1.clamp(t, (b - size.1).max(t));
    (x, y)
}

/// Where the bar first appears: top centre of the screen, clear of its edge.
pub fn default_position(work: ScreenRect, size: (i32, i32)) -> (i32, i32) {
    let (l, t, r, _) = work;
    fit_into(work, (l + (r - l - size.0) / 2, t + 8), size)
}

/// Moves `window` to `saved`, kept whole on that screen, or to the default
/// spot of the main screen when there is none or its screen is gone.
/// `size` is the size the window is about to take (egui applies a resize at
/// the end of the frame). Returns where it went.
pub fn place(window: OverlayWindow, saved: Option<(i32, i32)>, size: (i32, i32)) -> (i32, i32) {
    let on_saved_screen = saved.and_then(|p| {
        let work = native::work_area_of((p.0, p.1, p.0 + size.0, p.1 + size.1))?;
        Some(fit_into(work, p, size))
    });
    let pos = on_saved_screen
        .or_else(|| native::main_work_area().map(|work| default_position(work, size)))
        .unwrap_or_default();
    native::move_to(window, pos);
    pos
}

/// Pulls `window` back whole onto its screen at `size`. Some(new corner)
/// if it had to move.
pub fn keep_on_screen(window: OverlayWindow, size: (i32, i32)) -> Option<(i32, i32)> {
    let (l, t, _, _) = native::rect(window)?;
    let fitted = fit_into(native::work_area(window)?, (l, t), size);
    (fitted != (l, t)).then(|| {
        native::move_to(window, fitted);
        fitted
    })
}

/// `v` points in whole screen units at `ppp` units per point.
pub fn physical(v: Vec2, ppp: f32) -> (i32, i32) {
    ((v.x * ppp).round() as i32, (v.y * ppp).round() as i32)
}

// ---------------------------------------------------------------------------
// Behaviour shared by both renderers. `ppp`: screen units per point
// (`platform::overlay` coordinates).

/// Follows the client in front, kept while ROrganizer itself is (menu,
/// window). A head just clicked stays highlighted until its client is
/// actually in front, or `FOCUS_GRACE` runs out: the switch takes a few
/// frames, and reading the foreground meanwhile would flash the previous
/// head. Some(delay) while waiting: look again then.
pub fn update_front(st: &mut State) -> Option<Duration> {
    let front = crate::platform::foreground_window();
    if let Some((target, at)) = st.pending_front {
        let waited = at.elapsed();
        if front != Some(target) && waited < FOCUS_GRACE {
            return Some(FOCUS_GRACE - waited);
        }
        st.pending_front = None;
    }
    if let Some(front) = front
        && let Some(slot) = st.slots.iter().find(|s| s.window == front)
    {
        st.front = Some(slot.key.clone());
    }
    None
}

/// Primary button pressed on `hit`.
pub fn start_gesture(st: &mut State, window: OverlayWindow, hit: Option<Hit>) {
    let cursor = native::cursor_position();
    st.gesture = match hit {
        Some(Hit::Grip) => native::rect(window).map(|(l, t, _, _)| Gesture::Move {
            cursor,
            origin: (l, t),
        }),
        Some(Hit::Resize) => Some(Gesture::Resize { cursor, head: st.head }),
        Some(Hit::Slot(i)) => Some(Gesture::Press(i)),
        _ => None,
    };
}

/// Button still down: moves the window, or sets the head size it should
/// take (the renderer resizes it).
pub fn follow_gesture(st: &mut State, window: OverlayWindow, ppp: f32) {
    let (x, y) = native::cursor_position();
    match st.gesture {
        Some(Gesture::Move { cursor, origin }) => {
            native::move_to(window, (origin.0 + x - cursor.0, origin.1 + y - cursor.1));
        }
        Some(Gesture::Resize { cursor, head }) => {
            let delta = Vec2::new((x - cursor.0) as f32, (y - cursor.1) as f32) / ppp;
            st.head = resized_head(st.layout, st.slots.len(), head, delta);
        }
        _ => {}
    }
}

/// Button released over `hit`: records the move or the size, or brings
/// the clicked client forward.
pub fn end_gesture(st: &mut State, window: OverlayWindow, hit: Option<Hit>, ppp: f32) {
    let size = physical(st.geometry().size(), ppp);
    match st.gesture.take() {
        Some(Gesture::Move { .. }) => {
            let pos = keep_on_screen(window, size)
                .or_else(|| native::rect(window).map(|(l, t, _, _)| (l, t)));
            if let Some(pos) = pos {
                st.events.push(Event::Moved(pos));
            }
        }
        Some(Gesture::Resize { .. }) => {
            st.events.push(Event::Resized(st.head));
        }
        Some(Gesture::Press(i)) if hit == Some(Hit::Slot(i)) => {
            if let Some(slot) = st.slots.get(i) {
                crate::platform::focus_window(slot.window);
                st.front = Some(slot.key.clone());
                st.pending_front = Some((slot.window, Instant::now()));
                st.events.push(Event::Focused(slot.key.clone()));
            }
        }
        _ => {}
    }
}

/// The head the pointer rests on, outside any gesture.
pub fn hovered_slot(st: &State, hit: Option<Hit>) -> Option<usize> {
    match hit {
        Some(Hit::Slot(i)) if st.gesture.is_none() && i < st.slots.len() => Some(i),
        _ => None,
    }
}

/// Tracks the hovered head and returns the tooltip it should show now.
/// Some(delay) while the hover is younger than `TIP_DELAY`: look again then.
pub fn wanted_tip(st: &mut State, hovered: Option<usize>) -> (Option<Tip>, Option<Duration>) {
    match (hovered, st.hover) {
        (Some(i), Some((j, _))) if i == j => {}
        (Some(i), _) => st.hover = Some((i, Instant::now())),
        (None, _) => st.hover = None,
    }
    let Some((i, since)) = st.hover else { return (None, None) };
    let waited = since.elapsed();
    if waited < TIP_DELAY {
        return (None, Some(TIP_DELAY - waited));
    }
    let tip = st.slots.get(i).map(|slot| Tip {
        name: slot.key.clone(),
        detail: slot.class.clone(),
        binding: slot.binding.clone(),
        slot: i,
    });
    (tip, None)
}

/// Top-left corner of a `size` tooltip next to head `slot` of the bar at
/// `bar`: below a horizontal bar (above when the screen ends), right of a
/// vertical one (left when the screen ends), kept inside `work`.
pub fn tip_position(
    bar: ScreenRect,
    work: ScreenRect,
    g: &Geometry,
    slot: usize,
    size: (i32, i32),
    ppp: f32,
) -> (i32, i32) {
    let (bl, bt, br, bb) = bar;
    let (w, h) = size;
    let gap = (TIP_GAP * ppp).round() as i32;
    let center = g.slot(slot).center();
    let cx = bl + (center.x * ppp).round() as i32;
    let cy = bt + (center.y * ppp).round() as i32;
    let pos = match g.layout {
        Layout::Horizontal => {
            let y = if bb + gap + h <= work.3 { bb + gap } else { bt - gap - h };
            (cx - w / 2, y)
        }
        Layout::Vertical => {
            let x = if br + gap + w <= work.2 { br + gap } else { bl - gap - w };
            (x, cy - h / 2)
        }
    };
    fit_into(work, pos, (w, h))
}

/// Applies a menu pick and queues it for App. Returns the window to give
/// the foreground back to: the one in front when the menu opened, except
/// for "Open ROrganizer", which brings the main window in front instead.
pub fn pick(st: &mut State, action: MenuAction, previous: Option<WindowId>) -> Option<WindowId> {
    match action {
        MenuAction::Layout(l) => st.layout = l,
        MenuAction::Display(d) => st.display = d,
        MenuAction::Visibility(v) => st.visibility = v,
        MenuAction::OpenMain | MenuAction::Disable => {}
    }
    st.events.push(Event::Menu(action));
    (action != MenuAction::OpenMain).then_some(previous).flatten()
}

/// Hands the foreground back to `window` once the menu is gone, unless it
/// is ROrganizer's own.
pub fn give_back(window: Option<WindowId>) {
    if let Some(window) = window.filter(|&w| !crate::platform::is_own_window(w)) {
        crate::platform::focus_window(window);
    }
}

// ---------------------------------------------------------------------------
// Visibility.

/// Who is in front, as far as the bar is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Front {
    Dofus,
    /// ROrganizer itself: its main window, the bar, its menu.
    Own,
    Other,
}

/// Decides whether the bar shows, with the hide delay of `HIDE_DELAY`.
#[derive(Default)]
pub struct ShowGate {
    hide_at: Option<Instant>,
}

impl ShowGate {
    /// `eligible`: enabled, ROrganizer active, at least one client. Returns
    /// whether the bar should show, and when to ask again if a hide is
    /// pending.
    pub fn decide(
        &mut self,
        now: Instant,
        eligible: bool,
        front: Front,
        shown: bool,
    ) -> (bool, Option<Duration>) {
        if !eligible {
            self.hide_at = None;
            return (false, None);
        }
        if front != Front::Other {
            self.hide_at = None;
            return (true, None);
        }
        if !shown {
            return (false, None);
        }
        let at = *self.hide_at.get_or_insert(now + HIDE_DELAY);
        if now >= at {
            self.hide_at = None;
            (false, None)
        } else {
            (true, Some(at - now))
        }
    }
}

// ---------------------------------------------------------------------------
// Context menu.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    /// Greyed-out label heading a group.
    Header(String),
    Item { label: String, checked: bool },
    Separator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Layout(Layout),
    Display(Display),
    Visibility(Visibility),
    OpenMain,
    Disable,
}

pub fn menu(
    layout: Layout,
    display: Display,
    visibility: Visibility,
) -> Vec<(MenuEntry, Option<MenuAction>)> {
    use MenuAction as A;
    let header = |label: std::borrow::Cow<'_, str>| (MenuEntry::Header(label.into_owned()), None);
    let item = |label: std::borrow::Cow<'_, str>, checked: bool, action: MenuAction| {
        (MenuEntry::Item { label: label.into_owned(), checked }, Some(action))
    };
    vec![
        header(t!("overlay.layout")),
        item(t!("overlay.horizontal"), layout == Layout::Horizontal, A::Layout(Layout::Horizontal)),
        item(t!("overlay.vertical"), layout == Layout::Vertical, A::Layout(Layout::Vertical)),
        (MenuEntry::Separator, None),
        header(t!("overlay.display")),
        item(t!("overlay.class_head"), display == Display::ClassHead, A::Display(Display::ClassHead)),
        item(t!("overlay.initials"), display == Display::Initials, A::Display(Display::Initials)),
        (MenuEntry::Separator, None),
        header(t!("overlay.visibility")),
        item(
            t!("overlay.with_dofus"),
            visibility == Visibility::WithDofus,
            A::Visibility(Visibility::WithDofus),
        ),
        item(t!("overlay.always"), visibility == Visibility::Always, A::Visibility(Visibility::Always)),
        (MenuEntry::Separator, None),
        item(t!("overlay.open_main"), false, A::OpenMain),
        item(t!("overlay.disable"), false, A::Disable),
    ]
}

// ---------------------------------------------------------------------------
// Initials.

/// Short label for every name in `names`, unique as far as possible.
///
/// - The first two letters (`Kiwi` → `Ki`).
/// - Names sharing those (`Dowoop`, `Dowoup`, `Dosac`): first letter + the
///   letter where a name parts from the others. The group splits at the
///   first position where it differs (`Dosac` → `Ds`), and each part that
///   still holds several names splits further (`Do`, `Du`, `Da`, `Di`).
/// - Still taken, or a name that ends before it parts (`Dowo` next to
///   `Dowoop`): the first three-letter combination still free.
///
/// Only letters and digits count (`Lou-Cra` → `Lo`). `names` should be
/// every character ROrganizer knows, running or not, in a stable order:
/// a label then doesn't change when another client starts.
pub fn initials<'a>(names: &[&'a str]) -> HashMap<&'a str, String> {
    let chars: Vec<Vec<char>> = names.iter().map(|n| significant_chars(n)).collect();

    let mut labels: Vec<Option<String>> = vec![None; names.len()];
    let mut used: HashSet<String> = HashSet::new();

    // Groups sharing their first two letters, in order of first appearance.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, cs) in chars.iter().enumerate() {
        let k = fold_key(cs, &[0, 1]);
        match groups.iter_mut().find(|g| fold_key(&chars[g[0]], &[0, 1]) == k) {
            Some(g) => g.push(i),
            None => groups.push(vec![i]),
        }
    }

    for group in groups.iter().filter(|g| g.len() == 1) {
        let i = group[0];
        used.insert(fold_key(&chars[i], &[0, 1]));
        labels[i] = Some(display(&chars[i], &[0, 1]));
    }

    // Where each name parts from the rest of its group.
    let mut parting: Vec<(usize, Option<usize>)> = Vec::new();
    for group in groups.iter().filter(|g| g.len() > 1) {
        split(&chars, group, 1, &mut parting);
    }
    let mut rest: Vec<usize> = Vec::new();
    for (i, at) in parting {
        let candidate = at.map(|d| fold_key(&chars[i], &[0, d]));
        match (at, candidate) {
            (Some(d), Some(k)) if !used.contains(&k) => {
                used.insert(k);
                labels[i] = Some(display(&chars[i], &[0, d]));
            }
            _ => rest.push(i),
        }
    }

    // Anything left: the first three-letter combination still free, the
    // shortest names first since they have the fewest to pick from.
    rest.sort_unstable_by_key(|&i| (chars[i].len(), i));
    for i in rest {
        let cs = &chars[i];
        let free = (1..cs.len())
            .flat_map(|a| (a + 1..cs.len()).map(move |b| [0, a, b]))
            .find(|idx| !used.contains(&fold_key(cs, idx)));
        let idx = free.unwrap_or([0, 1, 2]);
        used.insert(fold_key(cs, &idx));
        labels[i] = Some(display(cs, &idx));
    }

    names
        .iter()
        .zip(labels)
        .map(|(n, l)| (*n, l.unwrap_or_default()))
        .collect()
}

/// Splits `group` at the first position from `from` where it differs and
/// records, for each name alone in its part, that position; parts with
/// several names split again further on. A name that ends there, or a
/// group that never differs, gets `None`.
fn split(chars: &[Vec<char>], group: &[usize], from: usize, out: &mut Vec<(usize, Option<usize>)>) {
    let Some(d) = first_difference(chars, group, from) else {
        out.extend(group.iter().map(|&i| (i, None)));
        return;
    };
    let mut parts: Vec<(Option<char>, Vec<usize>)> = Vec::new();
    for &i in group {
        let c = chars[i].get(d).map(|&c| fold(c));
        match parts.iter_mut().find(|(k, _)| *k == c) {
            Some((_, v)) => v.push(i),
            None => parts.push((c, vec![i])),
        }
    }
    for (c, part) in parts {
        match (c, part.len()) {
            (None, _) => out.extend(part.iter().map(|&i| (i, None))),
            (Some(_), 1) => out.push((part[0], Some(d))),
            (Some(_), _) => split(chars, &part, d + 1, out),
        }
    }
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn fold_key(cs: &[char], idx: &[usize]) -> String {
    idx.iter().filter_map(|&i| cs.get(i)).map(|&c| fold(c)).collect()
}

fn significant_chars(name: &str) -> Vec<char> {
    let alnum: Vec<char> = name.chars().filter(|c| c.is_alphanumeric()).collect();
    if alnum.is_empty() {
        name.chars().filter(|c| !c.is_whitespace()).collect()
    } else {
        alnum
    }
}

/// First index from `from` where the members of `group` don't all have
/// the same letter (a name that ended counts as different).
fn first_difference(chars: &[Vec<char>], group: &[usize], from: usize) -> Option<usize> {
    let longest = group.iter().map(|&i| chars[i].len()).max()?;
    let fold = |c: &char| c.to_lowercase().next();
    (from..longest).find(|&p| {
        let first = chars[group[0]].get(p).and_then(fold);
        group.iter().any(|&i| chars[i].get(p).and_then(fold) != first)
    })
}

/// Upper-case first letter, lower-case rest.
fn display(cs: &[char], idx: &[usize]) -> String {
    let mut out = String::new();
    for (n, c) in idx.iter().filter_map(|&i| cs.get(i)).enumerate() {
        if n == 0 {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
    }
    if out.is_empty() {
        out.push('?');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(names: &[&str]) -> Vec<String> {
        let map = initials(names);
        names.iter().map(|n| map[n].clone()).collect()
    }

    #[test]
    fn distinct_names_take_two_letters() {
        assert_eq!(labels(&["Kiwi", "Lyra", "Bolt", "Mira"]), ["Ki", "Ly", "Bo", "Mi"]);
    }

    #[test]
    fn shared_prefix_uses_the_letter_where_each_name_parts() {
        assert_eq!(
            labels(&["Dowoop", "Dowoup", "Dowoap", "Dowoip"]),
            ["Do", "Du", "Da", "Di"]
        );
        // Dosac parts from the others at their third letter, the four
        // Dowo… only at their fifth.
        assert_eq!(
            labels(&["Dowoop", "Dowoup", "Dowoap", "Dowoip", "Dosac"]),
            ["Do", "Du", "Da", "Di", "Ds"]
        );
    }

    #[test]
    fn parts_that_share_a_letter_split_further() {
        assert_eq!(labels(&["Dowoap", "Dowxap", "Dowoip"]), ["Da", "Dx", "Di"]);
    }

    #[test]
    fn name_that_is_a_prefix_of_another_gets_three_letters() {
        let l = labels(&["Dowo", "Dowoop"]);
        assert_eq!(l[1], "Do");
        assert_eq!(l[0].chars().count(), 3);
        assert_ne!(l[0], l[1]);
    }

    #[test]
    fn label_does_not_take_one_already_used_elsewhere() {
        // "Du" is Duke's: Dowoup has to find another one.
        let l = labels(&["Duke", "Dowoop", "Dowoup"]);
        assert_eq!(l[0], "Du");
        assert_eq!(l[1], "Do");
        assert_ne!(l[2], "Du");
        assert_ne!(l[2], "Do");
    }

    #[test]
    fn short_names_pick_their_three_letters_first() {
        // Dow has a single three-letter label; Dowoup, also left over, must
        // not take it.
        let l = labels(&["Duke", "Dowoop", "Dowoup", "Dow"]);
        assert_eq!(l[3], "Dow");
        let unique: HashSet<&String> = l.iter().collect();
        assert_eq!(unique.len(), l.len(), "{l:?}");
    }

    #[test]
    fn labels_only_depend_on_the_known_names() {
        // Every known character is passed, running or not: a client starting
        // changes nothing.
        let all = ["Dowoap", "Dowoip", "Kiwi"];
        assert_eq!(initials(&all)["Dowoap"], "Da");
        assert_eq!(initials(&all)["Kiwi"], "Ki");
    }

    #[test]
    fn punctuation_case_and_short_names() {
        assert_eq!(labels(&["lou-cra"]), ["Lo"]);
        assert_eq!(labels(&["ÉLISE"]), ["Él"]);
        assert_eq!(labels(&["X"]), ["X"]);
        assert_eq!(labels(&["--"]), ["--"]);
    }

    #[test]
    fn case_insensitive_collision() {
        let l = labels(&["dowoop", "DOWOUP"]);
        assert_eq!(l, ["Do", "Du"]);
    }

    #[test]
    fn geometry_hits_grip_slots_and_corner() {
        let g = Geometry { layout: Layout::Horizontal, head: 40.0, count: 3 };
        let size = g.size();
        assert_eq!(size, Vec2::new(4.0 + 12.0 + 3.0 + 3.0 * 40.0 + 2.0 * 3.0 + 4.0, 48.0));
        assert_eq!(g.hit(g.grip().center()), Hit::Grip);
        assert_eq!(g.hit(g.slot(2).center()), Hit::Slot(2));
        assert_eq!(g.hit(size.to_pos2() - Vec2::splat(2.0)), Hit::Resize);
        assert_eq!(g.hit(Pos2::new(1.0, 1.0)), Hit::Bar);

        let v = Geometry { layout: Layout::Vertical, ..g };
        assert_eq!(v.size(), Vec2::new(size.y, size.x));
        assert_eq!(v.hit(v.slot(1).center()), Hit::Slot(1));
        assert!(v.slot(0).min.y > v.grip().max.y);
    }

    #[test]
    fn resize_follows_the_dominant_axis_and_clamps() {
        let h = Layout::Horizontal;
        // 4 heads: 40 px along the bar is 10 px per head.
        assert_eq!(resized_head(h, 4, 40.0, Vec2::new(40.0, 2.0)), 50.0);
        assert_eq!(resized_head(h, 4, 40.0, Vec2::new(4.0, 12.0)), 52.0);
        assert_eq!(resized_head(Layout::Vertical, 4, 40.0, Vec2::new(12.0, 4.0)), 52.0);
        assert_eq!(resized_head(h, 4, 40.0, Vec2::new(0.0, -100.0)), HEAD_MIN);
        assert_eq!(resized_head(h, 1, 90.0, Vec2::new(50.0, 0.0)), HEAD_MAX);
    }

    #[test]
    fn fit_keeps_the_whole_bar_on_screen() {
        let work = (-1920, 0, 0, 1040);
        assert_eq!(fit_into(work, (-1500, 300), (200, 48)), (-1500, 300));
        assert_eq!(fit_into(work, (-100, 1020), (200, 48)), (-200, 992));
        assert_eq!(fit_into(work, (-3000, -50), (200, 48)), (-1920, 0));
        // Wider than the screen: pinned to its left edge.
        assert_eq!(fit_into(work, (-500, 10), (2500, 48)), (-1920, 10));
        assert_eq!(default_position((0, 0, 1920, 1040), (200, 48)), (860, 8));
    }

    #[test]
    fn bar_hides_late_when_another_window_comes_forward() {
        let mut gate = ShowGate::default();
        let t0 = Instant::now();
        assert_eq!(gate.decide(t0, true, Front::Dofus, false), (true, None));
        let (show, again) = gate.decide(t0, true, Front::Other, true);
        assert!(show);
        assert_eq!(again, Some(HIDE_DELAY));
        // Back on Dofus before the delay: no hide.
        assert_eq!(gate.decide(t0 + HIDE_DELAY / 2, true, Front::Dofus, true), (true, None));
        gate.decide(t0, true, Front::Other, true);
        assert_eq!(gate.decide(t0 + HIDE_DELAY, true, Front::Other, true), (false, None));
        // Already hidden: stays hidden.
        assert_eq!(gate.decide(t0, true, Front::Other, false), (false, None));
        assert_eq!(gate.decide(t0, true, Front::Own, false), (true, None));
        assert_eq!(gate.decide(t0, false, Front::Dofus, true), (false, None));
    }

    #[test]
    fn tooltip_goes_below_or_above_a_horizontal_bar() {
        let g = Geometry { layout: Layout::Horizontal, head: 40.0, count: 3 };
        let (w, h) = physical(g.size(), 1.0);
        let bar = (100, 100, 100 + w, 100 + h);
        // Head 0 is centred at x = 39: the tooltip is centred under it.
        let work = (0, 0, 1000, 800);
        assert_eq!(tip_position(bar, work, &g, 0, (80, 20), 1.0), (99, 100 + h + 6));
        // No room below: above.
        let short = (0, 0, 1000, 100 + h + 20);
        assert_eq!(tip_position(bar, short, &g, 0, (80, 20), 1.0), (99, 100 - 6 - 20));
        // At the screen's left edge, pulled back inside.
        let left = (0, 100, w, 100 + h);
        assert_eq!(tip_position(left, work, &g, 0, (80, 20), 1.0).0, 0);
    }

    #[test]
    fn tooltip_goes_right_or_left_of_a_vertical_bar() {
        let g = Geometry { layout: Layout::Vertical, head: 40.0, count: 3 };
        let (w, h) = physical(g.size(), 1.0);
        let bar = (100, 100, 100 + w, 100 + h);
        let work = (0, 0, 1000, 800);
        assert_eq!(tip_position(bar, work, &g, 0, (80, 20), 1.0), (100 + w + 6, 100 + 39 - 10));
        let narrow = (0, 0, 100 + w + 40, 800);
        assert_eq!(tip_position(bar, narrow, &g, 0, (80, 20), 1.0).0, 100 - 6 - 80);
    }

    #[test]
    fn tooltip_scales_with_the_screen() {
        // Windows at 200 %: geometry in points, screen in pixels.
        let g = Geometry { layout: Layout::Horizontal, head: 40.0, count: 3 };
        let (w, h) = physical(g.size(), 2.0);
        let bar = (0, 0, w, h);
        let work = (0, 0, 2000, 1000);
        assert_eq!(tip_position(bar, work, &g, 0, (80, 20), 2.0), (78 - 40, h + 12));
    }

    fn state_with_slot() -> State {
        let mut st = State::default();
        st.slots.push(Slot {
            key: "Kiwi".into(),
            window: WindowId::test(1),
            icon: None,
            initials: "Ki".into(),
            class: Some("Cra".into()),
            binding: Some("F1".into()),
        });
        st
    }

    #[test]
    fn tooltip_waits_for_the_hover_delay() {
        let mut st = state_with_slot();
        let (tip, again) = wanted_tip(&mut st, Some(0));
        assert_eq!(tip, None);
        assert!(again.is_some_and(|d| d <= TIP_DELAY));

        st.hover = Some((0, Instant::now() - TIP_DELAY));
        let (tip, again) = wanted_tip(&mut st, Some(0));
        assert_eq!(again, None);
        let tip = tip.expect("delay elapsed");
        assert_eq!((tip.name.as_str(), tip.detail.as_deref()), ("Kiwi", Some("Cra")));
        assert_eq!(tip.binding.as_deref(), Some("F1"));

        // Leaving the head drops the hover: the delay starts over.
        assert_eq!(wanted_tip(&mut st, None), (None, None));
        assert!(st.hover.is_none());
    }

    #[test]
    fn menu_pick_applies_and_gives_the_foreground_back() {
        let mut st = State::default();
        let dofus = Some(WindowId::test(7));
        let back = pick(&mut st, MenuAction::Layout(Layout::Vertical), dofus);
        assert_eq!(st.layout, Layout::Vertical);
        assert_eq!(back, dofus);
        assert_eq!(pick(&mut st, MenuAction::Display(Display::Initials), dofus), dofus);
        assert_eq!(st.display, Display::Initials);
        assert_eq!(pick(&mut st, MenuAction::Disable, dofus), dofus);
        // "Open ROrganizer" brings the main window instead.
        assert_eq!(pick(&mut st, MenuAction::OpenMain, dofus), None);
        assert_eq!(
            st.events,
            [
                Event::Menu(MenuAction::Layout(Layout::Vertical)),
                Event::Menu(MenuAction::Display(Display::Initials)),
                Event::Menu(MenuAction::Disable),
                Event::Menu(MenuAction::OpenMain),
            ]
        );
    }
}
