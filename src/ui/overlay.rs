//! The character overlay drawn with egui: two deferred viewports, the bar
//! and its tooltip, registered by `App` on every root pass while the
//! overlay is on. Model, geometry and rules are in `crate::overlay`; the
//! window plumbing in `platform::overlay`.
//!
//! The bar runs its own gestures with the global cursor and native moves:
//! `ViewportCommand::StartDrag` hands the move to the OS, which never moves
//! a window that can't be activated.

use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Galley, Pos2, Rect, Stroke, StrokeKind, Vec2,
    ViewportBuilder, ViewportCommand, ViewportId,
};

use crate::overlay::{
    self, Display, Event, Geometry, Gesture, Hit, Layout, MenuEntry, ScreenRect, Shared, State,
    Tip,
};
use crate::platform::{self, overlay as native};
use crate::theme::{self, Theme};

/// Distance between the bar and its tooltip, in points.
const TIP_GAP: f32 = 6.0;
const TIP_PAD: Vec2 = Vec2::new(8.0, 5.0);
/// Opacity of the heads that aren't in front.
const DIM_ALPHA: u8 = 115;

pub fn register(ctx: &egui::Context, shared: &Shared) {
    let size = {
        let st = overlay::lock(shared);
        geometry(&st).size()
    };
    let bar = base_builder(overlay::OVERLAY_TITLE).with_inner_size(size);
    let s = shared.clone();
    ctx.show_viewport_deferred(overlay::overlay_viewport(), bar, move |ui, _| draw_bar(ui, &s));

    let tip = base_builder(overlay::TIP_TITLE).with_inner_size([10.0, 10.0]);
    let s = shared.clone();
    ctx.show_viewport_deferred(overlay::tip_viewport(), tip, move |ui, _| draw_tip(ui, &s));
}

/// Created hidden: `App` shows the bar once it has been placed, the bar
/// shows its tooltip. The visibility here never changes, so egui never
/// re-sends it over those.
fn base_builder(title: &str) -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(title)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_active(false)
        .with_visible(false)
}

fn geometry(st: &State) -> Geometry {
    Geometry {
        layout: st.layout,
        head: st.head,
        count: st.slots.len().max(1),
    }
}

fn physical(v: Vec2, ppp: f32) -> (i32, i32) {
    ((v.x * ppp).round() as i32, (v.y * ppp).round() as i32)
}

fn draw_bar(ui: &mut egui::Ui, shared: &Shared) {
    let ctx = ui.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let mut st = overlay::lock(shared);
    let theme = st.theme.unwrap_or(Theme::Dark);
    let g = geometry(&st);
    let size = g.size();

    if st.window.is_none()
        && let Some(window) = native::attach(overlay::OVERLAY_TITLE)
    {
        let pos = overlay::place(window, st.saved_pos, physical(size, ppp));
        st.window = Some(window);
        st.sent_size = Some(size);
        st.events.push(Event::Moved(pos));
        // App shows the bar now that it sits where it belongs.
        ctx.request_repaint_of(ViewportId::ROOT);
    }
    if st.tip_window.is_none() {
        st.tip_window = native::attach(overlay::TIP_TITLE);
    }
    let Some(window) = st.window else { return };

    if st.sent_size != Some(size) {
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        if let Some(pos) = overlay::keep_on_screen(window, physical(size, ppp)) {
            st.events.push(Event::Moved(pos));
            ctx.request_repaint_of(ViewportId::ROOT);
        }
        st.sent_size = Some(size);
    }

    // The client in front; kept while ROrganizer itself is (menu, window).
    if let Some(front) = platform::foreground_window()
        && let Some(slot) = st.slots.iter().find(|s| s.window == front)
    {
        st.front = Some(slot.key.clone());
    }

    let (hover, pressed, down, released, menu) = ctx.input(|i| {
        let p = &i.pointer;
        (
            p.hover_pos(),
            p.primary_pressed(),
            p.primary_down(),
            p.primary_released(),
            p.secondary_clicked(),
        )
    });
    let hit = hover.map(|p| g.hit(p));
    handle_gesture(&ctx, &mut st, window, hit, pressed, down, released, ppp);

    let hovered_slot = match hit {
        Some(Hit::Slot(i)) if st.gesture.is_none() && i < st.slots.len() => Some(i),
        _ => None,
    };
    ctx.set_cursor_icon(match (st.gesture, hit) {
        (Some(Gesture::Move { .. }), _) => egui::CursorIcon::Grabbing,
        (Some(Gesture::Resize { .. }), _) | (None, Some(Hit::Resize)) => {
            egui::CursorIcon::ResizeNwSe
        }
        (None, Some(Hit::Grip)) => egui::CursorIcon::Grab,
        (_, Some(Hit::Slot(_))) => egui::CursorIcon::PointingHand,
        _ => egui::CursorIcon::Default,
    });

    paint_bar(ui, &st, &g, theme, hovered_slot, hover.is_some() || st.gesture.is_some());
    update_tip(&ctx, &mut st, hovered_slot.filter(|_| !menu), window, &g, theme, ppp);

    if !st.events.is_empty() {
        ctx.request_repaint_of(ViewportId::ROOT);
    }

    if menu {
        let items = overlay::menu(st.layout, st.display);
        // The menu blocks until closed: release the state first.
        drop(st);
        let entries: Vec<MenuEntry> = items.iter().map(|(e, _)| e.clone()).collect();
        let picked = native::show_menu(window, &entries).and_then(|i| items[i].1);
        if let Some(action) = picked {
            let mut st = overlay::lock(shared);
            match action {
                overlay::MenuAction::Layout(l) => st.layout = l,
                overlay::MenuAction::Display(d) => st.display = d,
                _ => {}
            }
            st.events.push(Event::Menu(action));
        }
        ctx.request_repaint_of(ViewportId::ROOT);
        ctx.request_repaint();
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_gesture(
    ctx: &egui::Context,
    st: &mut State,
    window: platform::OverlayWindow,
    hit: Option<Hit>,
    pressed: bool,
    down: bool,
    released: bool,
    ppp: f32,
) {
    if pressed {
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

    if down {
        let (x, y) = native::cursor_position();
        match st.gesture {
            Some(Gesture::Move { cursor, origin }) => {
                native::move_to(window, (origin.0 + x - cursor.0, origin.1 + y - cursor.1));
            }
            Some(Gesture::Resize { cursor, head }) => {
                let delta = Vec2::new((x - cursor.0) as f32, (y - cursor.1) as f32) / ppp;
                st.head = overlay::resized_head(st.layout, st.slots.len(), head, delta);
            }
            _ => {}
        }
    }

    if released || (!down && st.gesture.is_some() && !pressed) {
        let size = physical(geometry(st).size(), ppp);
        match st.gesture.take() {
            Some(Gesture::Move { .. }) => {
                let pos = overlay::keep_on_screen(window, size)
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
                    platform::focus_window(slot.window);
                    st.front = Some(slot.key.clone());
                    st.events.push(Event::Focused(slot.key.clone()));
                }
            }
            _ => {}
        }
        ctx.request_repaint();
    }
}

fn bar_colors(theme: Theme) -> (Color32, Color32) {
    let [r, g, b, _] = theme::window_bg(theme).to_array();
    let border = match theme {
        Theme::Dark => Color32::from_white_alpha(36),
        Theme::Light => Color32::from_black_alpha(46),
    };
    (Color32::from_rgba_unmultiplied(r, g, b, 235), border)
}

fn active_color(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => theme::ACTIVE_DARK,
        Theme::Light => theme::ACTIVE_LIGHT,
    }
}

fn paint_bar(
    ui: &egui::Ui,
    st: &State,
    g: &Geometry,
    theme: Theme,
    hovered: Option<usize>,
    pointer_on_bar: bool,
) {
    let painter = ui.painter();
    let (fill, border) = bar_colors(theme);
    let bar = Rect::from_min_size(Pos2::ZERO, g.size());
    painter.rect(bar, CornerRadius::same(10), fill, Stroke::new(1.0, border), StrokeKind::Inside);

    // Grip: two rows of three dots along the bar.
    let grip = g.grip();
    let dot = theme::text_tertiary(theme);
    for k in -1..=1 {
        for side in [-1.0, 1.0] {
            let (dx, dy) = match g.layout {
                Layout::Horizontal => (side * 2.5, k as f32 * 5.0),
                Layout::Vertical => (k as f32 * 5.0, side * 2.5),
            };
            painter.circle_filled(grip.center() + Vec2::new(dx, dy), 1.4, dot);
        }
    }

    let active = active_color(theme);
    for (i, slot) in st.slots.iter().enumerate() {
        let rect = g.slot(i);
        let on = st.front.as_ref() == Some(&slot.key);
        let radius = CornerRadius::same((g.head * 0.18).round().clamp(4.0, 12.0) as u8);
        if on {
            painter.rect(
                rect,
                radius,
                active.gamma_multiply(0.16),
                Stroke::new(2.0, active),
                StrokeKind::Inside,
            );
        } else if hovered == Some(i) {
            painter.rect_filled(rect, radius, theme::hover_bg(theme).gamma_multiply(1.5));
        }

        let tint = if on || hovered == Some(i) {
            Color32::WHITE
        } else {
            Color32::from_white_alpha(DIM_ALPHA)
        };
        match (&slot.icon, st.display) {
            (Some(icon), Display::ClassHead) => {
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                painter.image(icon.id(), rect.shrink(g.head * 0.07), uv, tint);
            }
            _ => {
                let disc = match theme {
                    Theme::Dark => Color32::from_rgb(0x3A, 0x3A, 0x33),
                    Theme::Light => Color32::from_rgb(0xD9, 0xD6, 0xCE),
                };
                let text = theme::text_primary(theme);
                let (disc, text) = if on || hovered == Some(i) {
                    (disc, text)
                } else {
                    (disc.gamma_multiply(0.6), text.gamma_multiply(0.55))
                };
                painter.circle_filled(rect.center(), g.head * 0.4, disc);
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    &slot.initials,
                    FontId::proportional(g.head * 0.36),
                    text,
                );
            }
        }
    }

    if pointer_on_bar {
        let handle = g.resize_handle();
        let stroke = Stroke::new(1.2, dot);
        let corner = handle.max - Vec2::splat(3.0);
        for len in [4.0, 7.5] {
            painter.line_segment([corner - Vec2::new(len, 0.0), corner - Vec2::new(0.0, len)], stroke);
        }
    }
}

/// Shows, moves or hides the tooltip. Only the bar knows what is hovered;
/// the tooltip viewport only paints `State::tip`.
fn update_tip(
    ctx: &egui::Context,
    st: &mut State,
    hovered: Option<usize>,
    window: platform::OverlayWindow,
    g: &Geometry,
    theme: Theme,
    ppp: f32,
) {
    match (hovered, st.hover) {
        (Some(i), Some((j, _))) if i == j => {}
        (Some(i), _) => st.hover = Some((i, Instant::now())),
        (None, _) => st.hover = None,
    }
    let wanted = st.hover.and_then(|(i, since)| {
        let waited = since.elapsed();
        if waited < overlay::TIP_DELAY {
            ctx.request_repaint_after(overlay::TIP_DELAY - waited);
            return None;
        }
        let slot = st.slots.get(i)?;
        Some(Tip {
            name: slot.key.clone(),
            detail: slot.class.clone(),
            binding: slot.binding.clone(),
            slot: i,
        })
    });
    if wanted == st.tip {
        return;
    }
    let tip_id = overlay::tip_viewport();
    match (&wanted, st.tip_window) {
        (Some(tip), Some(tip_window)) => {
            let size = tip_layout(ctx, tip, theme).size;
            let pos = tip_position(window, g, tip.slot, size, ppp);
            if let Some(pos) = pos {
                native::move_to(tip_window, pos);
            }
            ctx.send_viewport_cmd_to(tip_id, ViewportCommand::InnerSize(size));
            ctx.send_viewport_cmd_to(tip_id, ViewportCommand::Visible(true));
        }
        _ => ctx.send_viewport_cmd_to(tip_id, ViewportCommand::Visible(false)),
    }
    st.tip = wanted;
    ctx.request_repaint_of(tip_id);
}

/// Next to the hovered head: below a horizontal bar (above when the screen
/// ends), right of a vertical one (left when the screen ends).
fn tip_position(
    window: platform::OverlayWindow,
    g: &Geometry,
    slot: usize,
    size: Vec2,
    ppp: f32,
) -> Option<(i32, i32)> {
    let (bl, bt, br, bb) = native::rect(window)?;
    let work: ScreenRect = native::work_area(window)?;
    let (w, h) = physical(size, ppp);
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
    Some(overlay::fit_into(work, pos, (w, h)))
}

struct TipLayout {
    size: Vec2,
    name: Arc<Galley>,
    detail: Option<Arc<Galley>>,
    binding: Option<Arc<Galley>>,
}

const PILL_PAD: Vec2 = Vec2::new(6.0, 2.0);

fn tip_layout(ctx: &egui::Context, tip: &Tip, theme: Theme) -> TipLayout {
    ctx.fonts_mut(|f| {
        let name = f.layout_no_wrap(
            tip.name.clone(),
            FontId::proportional(12.5),
            theme::text_primary(theme),
        );
        let detail = tip.detail.as_ref().map(|d| {
            f.layout_no_wrap(format!("· {d}"), FontId::proportional(12.0), theme::text_secondary(theme))
        });
        let binding = tip.binding.as_ref().map(|b| {
            f.layout_no_wrap(b.clone(), FontId::monospace(11.0), theme::text_secondary(theme))
        });
        let mut width = TIP_PAD.x * 2.0 + name.size().x;
        let mut height = name.size().y;
        if let Some(d) = &detail {
            width += 5.0 + d.size().x;
        }
        if let Some(b) = &binding {
            width += 8.0 + b.size().x + PILL_PAD.x * 2.0;
            height = height.max(b.size().y + PILL_PAD.y * 2.0);
        }
        TipLayout {
            size: Vec2::new(width.ceil(), (height + TIP_PAD.y * 2.0).ceil()),
            name,
            detail,
            binding,
        }
    })
}

fn draw_tip(ui: &mut egui::Ui, shared: &Shared) {
    let ctx = ui.ctx().clone();
    let mut st = overlay::lock(shared);
    if st.tip_window.is_none() {
        st.tip_window = native::attach(overlay::TIP_TITLE);
    }
    let Some(tip) = st.tip.clone() else { return };
    let theme = st.theme.unwrap_or(Theme::Dark);
    drop(st);

    let layout = tip_layout(&ctx, &tip, theme);
    let painter = ui.painter();
    let rect = Rect::from_min_size(Pos2::ZERO, layout.size);
    let (fill, border) = bar_colors(theme);
    painter.rect(rect, CornerRadius::same(6), fill, Stroke::new(1.0, border), StrokeKind::Inside);

    let mid = rect.center().y;
    let mut x = TIP_PAD.x;
    let put = |galley: Arc<Galley>, x: &mut f32| {
        let w = galley.size().x;
        painter.galley(Pos2::new(*x, mid - galley.size().y / 2.0), galley, Color32::PLACEHOLDER);
        *x += w;
    };
    put(layout.name, &mut x);
    if let Some(detail) = layout.detail {
        x += 5.0;
        put(detail, &mut x);
    }
    if let Some(binding) = layout.binding {
        x += 8.0;
        let pill = Rect::from_min_size(
            Pos2::new(x, mid - binding.size().y / 2.0 - PILL_PAD.y),
            binding.size() + PILL_PAD * 2.0,
        );
        painter.rect(
            pill,
            CornerRadius::same(4),
            theme::window_bg(theme),
            Stroke::new(1.0, bar_colors(theme).1),
            StrokeKind::Inside,
        );
        let mut px = x + PILL_PAD.x;
        put(binding, &mut px);
    }
}
