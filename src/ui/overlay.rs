//! The character overlay drawn with egui (`platform::OVERLAY_IS_EGUI`):
//! three deferred viewports, the bar, its tooltip and its right-click menu,
//! registered by `App` on every root pass while the overlay is on. Model,
//! geometry, palette and gestures are in `crate::overlay`; the window
//! plumbing in `platform::overlay`.
//!
//! The bar runs its own gestures with the global cursor and native moves:
//! `ViewportCommand::StartDrag` hands the move to the OS, which never moves
//! a window that can't be activated. The menu is drawn here rather than
//! native so that it follows the app's theme.

use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Galley, Pos2, Rect, Stroke, StrokeKind, Vec2,
    ViewportBuilder, ViewportCommand, ViewportId,
};

use crate::overlay::{
    self, physical, Display, Event, Geometry, Gesture, Hit, Menu, MenuAction, MenuEntry, Shared,
    State, Tip, DIM_ALPHA, PILL_PAD, TIP_PAD,
};
use crate::platform::{self, overlay as native};
use crate::theme::{self, Theme};

pub fn register(ctx: &egui::Context, shared: &Shared) {
    let size = overlay::lock(shared).geometry().size();
    let bar = base_builder(overlay::OVERLAY_TITLE).with_inner_size(size);
    let s = shared.clone();
    ctx.show_viewport_deferred(overlay::overlay_viewport(), bar, move |ui, _| draw_bar(ui, &s));

    let tip = base_builder(overlay::TIP_TITLE).with_inner_size([10.0, 10.0]);
    let s = shared.clone();
    ctx.show_viewport_deferred(overlay::tip_viewport(), tip, move |ui, _| draw_tip(ui, &s));

    let menu = base_builder(overlay::MENU_TITLE).with_inner_size([10.0, 10.0]);
    let s = shared.clone();
    ctx.show_viewport_deferred(overlay::menu_viewport(), menu, move |ui, _| draw_menu(ui, &s));
}

/// Hides the tooltip and the menu: the bar went away under them.
pub fn close_popups(ctx: &egui::Context, st: &mut State) {
    st.tip = None;
    st.hover = None;
    st.menu = None;
    ctx.send_viewport_cmd_to(overlay::tip_viewport(), ViewportCommand::Visible(false));
    ctx.send_viewport_cmd_to(overlay::menu_viewport(), ViewportCommand::Visible(false));
}

/// Created hidden: `App` shows the bar once it has been placed, the bar
/// shows its tooltip and its menu. The visibility here never changes, so
/// egui never re-sends it over those.
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

fn draw_bar(ui: &mut egui::Ui, shared: &Shared) {
    let ctx = ui.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let mut st = overlay::lock(shared);
    let theme = st.theme();
    let g = st.geometry();
    let size = g.size();

    if st.window.is_none()
        && let Some(window) = native::attach(overlay::OVERLAY_TITLE, false)
    {
        let pos = overlay::place(window, st.saved_pos, physical(size, ppp));
        st.window = Some(window);
        st.sent_size = Some((size, ppp));
        st.events.push(Event::Moved(pos));
        // App shows the bar now that it sits where it belongs.
        ctx.request_repaint_of(ViewportId::ROOT);
    }
    if st.tip_window.is_none() {
        st.tip_window = native::attach(overlay::TIP_TITLE, false);
    }
    if st.menu_window.is_none() {
        st.menu_window = native::attach(overlay::MENU_TITLE, true);
    }
    let Some(window) = st.window else { return };

    // A new size, or a new scale: the move by `place` can land the window
    // on a screen of another DPI, which resizes it in pixels.
    if st.sent_size != Some((size, ppp)) {
        if st.sent_size.is_none_or(|(sent, _)| sent != size) {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        }
        if let Some(pos) = overlay::keep_on_screen(window, physical(size, ppp)) {
            st.events.push(Event::Moved(pos));
            ctx.request_repaint_of(ViewportId::ROOT);
        }
        st.sent_size = Some((size, ppp));
    }

    if let Some(again) = overlay::update_front(&mut st) {
        ctx.request_repaint_after(again);
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
    if pressed {
        overlay::start_gesture(&mut st, window, hit);
    }
    if down {
        overlay::follow_gesture(&mut st, window, ppp);
    }
    if released || (!down && st.gesture.is_some() && !pressed) {
        overlay::end_gesture(&mut st, window, hit, ppp);
        ctx.request_repaint();
    }

    let hovered_slot = overlay::hovered_slot(&st, hit);
    ctx.set_cursor_icon(match (st.gesture, hit) {
        (Some(Gesture::Move { .. }), _) => egui::CursorIcon::Grabbing,
        (Some(Gesture::Resize { .. }), _) | (None, Some(Hit::Resize)) => {
            egui::CursorIcon::ResizeNwSe
        }
        (None, Some(Hit::Grip)) => egui::CursorIcon::Grab,
        (_, Some(Hit::Slot(_))) => egui::CursorIcon::PointingHand,
        _ => egui::CursorIcon::Default,
    });

    if menu {
        open_menu(&ctx, &mut st, window, theme, ppp);
    }
    paint_bar(ui, &st, &g, theme, hovered_slot, hover.is_some() || st.gesture.is_some());
    let hovered_slot = hovered_slot.filter(|_| st.menu.is_none());
    update_tip(&ctx, &mut st, hovered_slot, window, &g, theme, ppp);

    if !st.events.is_empty() {
        ctx.request_repaint_of(ViewportId::ROOT);
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
    let (fill, border) = overlay::bar_colors(theme);
    let bar = Rect::from_min_size(Pos2::ZERO, g.size());
    painter.rect(
        bar,
        CornerRadius::same(overlay::BAR_RADIUS),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );

    let dot = theme::text_tertiary(theme);
    for center in g.grip_dots() {
        painter.circle_filled(center, 1.4, dot);
    }

    let active = overlay::active_color(theme);
    let lit = st.lit();
    for (i, slot) in st.slots.iter().enumerate() {
        let rect = g.slot(i);
        let on = lit == Some(&slot.key);
        let radius = CornerRadius::same(overlay::head_radius(g.head));
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
                let disc = overlay::initials_disc(theme);
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
        let stroke = Stroke::new(1.2, dot);
        for segment in g.resize_marks() {
            painter.line_segment(segment, stroke);
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
    let (wanted, again) = overlay::wanted_tip(st, hovered);
    if let Some(again) = again {
        ctx.request_repaint_after(again);
    }
    if wanted == st.tip {
        return;
    }
    let tip_id = overlay::tip_viewport();
    match (&wanted, st.tip_window) {
        (Some(tip), Some(tip_window)) => {
            let size = tip_layout(ctx, tip, theme).size;
            let bar = native::rect(window);
            let work = native::work_area(window);
            if let (Some(bar), Some(work)) = (bar, work) {
                let pos = overlay::tip_position(bar, work, g, tip.slot, physical(size, ppp), ppp);
                native::move_to(tip_window, pos);
            }
            ctx.send_viewport_cmd_to(tip_id, ViewportCommand::InnerSize(size));
            ctx.send_viewport_cmd_to(tip_id, ViewportCommand::Visible(true));
            st.tip = wanted;
        }
        // Not attached yet: `draw_bar` retries on a later frame.
        (Some(_), None) => ctx.request_repaint_after(overlay::TIP_DELAY),
        (None, _) => {
            ctx.send_viewport_cmd_to(tip_id, ViewportCommand::Visible(false));
            st.tip = None;
        }
    }
    ctx.request_repaint_of(tip_id);
}

struct TipLayout {
    size: Vec2,
    name: Arc<Galley>,
    detail: Option<Arc<Galley>>,
    binding: Option<Arc<Galley>>,
}

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
        st.tip_window = native::attach(overlay::TIP_TITLE, false);
    }
    let Some(tip) = st.tip.clone() else { return };
    let theme = st.theme();
    drop(st);

    let layout = tip_layout(&ctx, &tip, theme);
    let painter = ui.painter();
    let rect = Rect::from_min_size(Pos2::ZERO, layout.size);
    let (fill, border) = overlay::bar_colors(theme);
    painter.rect(
        rect,
        CornerRadius::same(overlay::TIP_RADIUS),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );

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
            CornerRadius::same(overlay::PILL_RADIUS),
            theme::window_bg(theme),
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        );
        let mut px = x + PILL_PAD.x;
        put(binding, &mut px);
    }
}

// ---------------------------------------------------------------------------
// Right-click menu.

const MENU_PAD: f32 = 5.0;
const MENU_ROW: f32 = 26.0;
const MENU_HEADER: f32 = 22.0;
const MENU_SEPARATOR: f32 = 9.0;
/// Left of the item labels, after the column of the check dots.
const MENU_LABEL_X: f32 = 26.0;
/// Closes a menu that never got the focus, which only a click elsewhere
/// could otherwise close.
const MENU_FOCUS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

struct MenuRow {
    rect: Rect,
    galley: Option<Arc<Galley>>,
}

fn menu_layout(
    ctx: &egui::Context,
    items: &[(MenuEntry, Option<MenuAction>)],
    theme: Theme,
) -> (Vec2, Vec<MenuRow>) {
    ctx.fonts_mut(|f| {
        let mut y = MENU_PAD;
        let mut widest: f32 = 0.0;
        let mut rows = Vec::with_capacity(items.len());
        for (entry, action) in items {
            let (height, galley) = match entry {
                MenuEntry::Header(label) => {
                    let font = FontId::proportional(10.5);
                    (MENU_HEADER, Some(f.layout_no_wrap(label.clone(), font, theme::text_tertiary(theme))))
                }
                MenuEntry::Item { label, .. } => {
                    let color = if *action == Some(MenuAction::Disable) {
                        theme::conflict_palette(theme).3
                    } else {
                        theme::text_primary(theme)
                    };
                    (MENU_ROW, Some(f.layout_no_wrap(label.clone(), FontId::proportional(12.5), color)))
                }
                MenuEntry::Separator => (MENU_SEPARATOR, None),
            };
            if let Some(g) = &galley {
                widest = widest.max(g.size().x);
            }
            rows.push(MenuRow {
                rect: Rect::from_min_size(Pos2::new(MENU_PAD, y), Vec2::new(0.0, height)),
                galley,
            });
            y += height;
        }
        let width = (MENU_LABEL_X + widest + 18.0 + MENU_PAD * 2.0).max(170.0).ceil();
        for row in &mut rows {
            row.rect.set_width(width - MENU_PAD * 2.0);
        }
        (Vec2::new(width, (y + MENU_PAD).ceil()), rows)
    })
}

/// At the cursor, flipped left or up where the screen ends. The menu takes
/// the focus on its first frame (`draw_menu`).
fn open_menu(
    ctx: &egui::Context,
    st: &mut State,
    bar: platform::OverlayWindow,
    theme: Theme,
    ppp: f32,
) {
    let Some(menu_window) = st.menu_window else { return };
    let items = overlay::menu(st.layout, st.display, st.visibility);
    let (size, _) = menu_layout(ctx, &items, theme);
    let (w, h) = physical(size, ppp);
    let (cx, cy) = native::cursor_position();
    if let Some(work) = native::work_area(bar) {
        let x = if cx + w <= work.2 { cx } else { cx - w };
        let y = if cy + h <= work.3 { cy } else { cy - h };
        native::move_to(menu_window, overlay::fit_into(work, (x, y), (w, h)));
    }
    let id = overlay::menu_viewport();
    ctx.send_viewport_cmd_to(id, ViewportCommand::InnerSize(size));
    ctx.send_viewport_cmd_to(id, ViewportCommand::Visible(true));
    // Opened again over itself: the window to give back is still the one
    // from before the first.
    let previous = st
        .menu
        .as_ref()
        .and_then(|m| m.previous)
        .or_else(platform::foreground_window);
    st.menu = Some(Menu {
        items,
        previous,
        opened: Instant::now(),
        activated: false,
        focused: false,
    });
    ctx.request_repaint_of(id);
}

/// Hides the menu; `give_back` gets the foreground (a pick, Escape), unless
/// a click elsewhere already moved it.
fn close_menu(ctx: &egui::Context, st: &mut State, give_back: Option<platform::WindowId>) {
    st.menu = None;
    ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    overlay::give_back(give_back);
    ctx.request_repaint_of(ViewportId::ROOT);
    ctx.request_repaint_of(overlay::overlay_viewport());
}

fn draw_menu(ui: &mut egui::Ui, shared: &Shared) {
    let ctx = ui.ctx().clone();
    let mut st = overlay::lock(shared);
    if st.menu_window.is_none() {
        st.menu_window = native::attach(overlay::MENU_TITLE, true);
    }
    let (Some(menu), Some(menu_window)) = (st.menu.clone(), st.menu_window) else { return };
    let theme = st.theme();

    // Focused, a click elsewhere takes the focus away and closes it, as a
    // native menu does. ROrganizer is in front meanwhile, like with one.
    if !menu.activated {
        native::activate(menu_window);
    }
    let focused = ctx.input(|i| i.viewport().focused) == Some(true);
    if !focused && (menu.focused || menu.opened.elapsed() >= MENU_FOCUS_TIMEOUT) {
        close_menu(&ctx, &mut st, None);
        return;
    }
    if !focused {
        ctx.request_repaint_after(MENU_FOCUS_TIMEOUT);
    }
    if let Some(m) = st.menu.as_mut() {
        m.activated = true;
        m.focused |= focused;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close_menu(&ctx, &mut st, menu.previous);
        return;
    }

    let (size, rows) = menu_layout(&ctx, &menu.items, theme);
    let painter = ui.painter();
    let (fill, border) = match theme {
        Theme::Dark => (theme::BG_ROW_DARK, theme::BORDER_STRONG_DARK),
        Theme::Light => (theme::BG_ROW_LIGHT, theme::BORDER_STRONG_LIGHT),
    };
    painter.rect(
        Rect::from_min_size(Pos2::ZERO, size),
        CornerRadius::same(8),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );

    let (hover, clicked) = ctx.input(|i| (i.pointer.hover_pos(), i.pointer.primary_clicked()));
    let mut picked = None;
    for ((entry, action), row) in menu.items.iter().zip(rows) {
        let mid = row.rect.center().y;
        match (entry, row.galley) {
            (MenuEntry::Separator, _) => {
                let x = row.rect.x_range();
                painter.hline((x.min + 4.0)..=(x.max - 4.0), mid, Stroke::new(1.0, border));
            }
            (MenuEntry::Header(_), Some(galley)) => {
                let pos = Pos2::new(row.rect.min.x + 10.0, mid - galley.size().y / 2.0);
                painter.galley(pos, galley, Color32::PLACEHOLDER);
            }
            (MenuEntry::Item { checked, .. }, Some(galley)) => {
                let hovered = hover.is_some_and(|p| row.rect.contains(p));
                if hovered {
                    painter.rect_filled(row.rect, CornerRadius::same(5), theme::hover_bg(theme));
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    if clicked {
                        picked = *action;
                    }
                }
                if *checked {
                    let dot = Pos2::new(row.rect.min.x + 13.0, mid);
                    painter.circle_filled(dot, 3.2, overlay::active_color(theme));
                }
                let pos = Pos2::new(row.rect.min.x + MENU_LABEL_X - MENU_PAD, mid - galley.size().y / 2.0);
                painter.galley(pos, galley, Color32::PLACEHOLDER);
            }
            _ => {}
        }
    }

    if let Some(action) = picked {
        let give_back = overlay::pick(&mut st, action, menu.previous);
        close_menu(&ctx, &mut st, give_back);
    }
}
