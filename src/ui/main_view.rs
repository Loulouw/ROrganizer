use std::collections::HashSet;

use eframe::egui;
use egui::{Color32, Frame, Margin, Rounding, Sense, Stroke, Vec2};
use rust_i18n::t;

use crate::app::App;
use crate::theme::{self, Theme};
use crate::triggers::{BindingTarget, SlotKey, Trigger};
use crate::win::{self, DetectedWindow};

pub fn draw(ctx: &egui::Context, app: &mut App) {
    let theme = app.theme();
    let snapshot = app.windows_snapshot();
    let snap_windows: Vec<DetectedWindow> = {
        let guard = snapshot.lock().expect("snapshot mutex poisoned");
        guard.clone()
    };

    // Project cycle_order × snapshot into the visible ordered list.
    // `cycle_order` is the source of truth for ordering — App keeps it in
    // sync with newly detected slots in `ensure_cycle_order_covers_snapshot`.
    let mut ordered: Vec<DetectedWindow> = app
        .cycle_order()
        .iter()
        .filter_map(|slot| snap_windows.iter().find(|w| &w.slot_key == slot).cloned())
        .collect();

    let conflicts = app.conflicting_triggers();

    egui::CentralPanel::default()
        .frame(
            Frame::none()
                .fill(theme::window_bg(theme))
                .inner_margin(Margin::symmetric(12.0, 12.0)),
        )
        .show(ctx, |ui| {
            draw_summary(ui, theme, ordered.len(), || app.request_refresh());
            ui.add_space(10.0);

            if ordered.is_empty() {
                draw_empty_state(ui, theme);
            } else {
                draw_account_list(ui, theme, app, &mut ordered, &conflicts);
            }

            draw_conflict_banner(ui, theme, &conflicts);

            ui.add_space(8.0);
            draw_cycle_section(ui, theme, app, &conflicts);
        });
}

fn draw_summary<F: FnOnce()>(ui: &mut egui::Ui, theme: Theme, count: usize, on_refresh: F) {
    ui.horizontal(|ui| {
        let label = match count {
            0 => t!("main.accounts_zero").to_string(),
            1 => t!("main.accounts_one").to_string(),
            n => t!("main.accounts_many", count = n.to_string()).to_string(),
        };
        ui.label(
            egui::RichText::new(label)
                .size(13.0)
                .color(theme::text_secondary(theme)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if refresh_button(ui, theme).clicked() {
                on_refresh();
            }
        });
    });
}

fn refresh_button(ui: &mut egui::Ui, theme: Theme) -> egui::Response {
    let size = Vec2::new(26.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 4.0, theme::hover_bg(theme));
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "\u{27F3}",
        egui::FontId::proportional(14.0),
        theme::text_secondary(theme),
    );
    response.on_hover_text(t!("header.tooltip_refresh").to_string())
}

fn draw_empty_state(ui: &mut egui::Ui, theme: Theme) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(t!("main.empty_title").to_string())
                .size(13.0)
                .color(theme::text_tertiary(theme)),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(t!("main.empty_hint").to_string())
                .size(11.0)
                .color(theme::text_tertiary(theme)),
        );
    });
}

fn draw_account_list(
    ui: &mut egui::Ui,
    theme: Theme,
    app: &mut App,
    ordered: &mut Vec<DetectedWindow>,
    conflicts: &HashSet<Trigger>,
) {
    let pre_order: Vec<SlotKey> = ordered.iter().map(|w| w.slot_key.clone()).collect();
    let mut row_action: Option<RowAction> = None;

    egui_dnd::dnd(ui, "accounts_dnd").show_vec(
        ordered,
        |ui, w, handle, _state| {
            let in_conflict = app
                .binding_for_account(&w.slot_key)
                .map(|t| conflicts.contains(&t))
                .unwrap_or(false);
            let action = draw_account_row(ui, theme, app, w, handle, in_conflict);
            if action.is_some() {
                row_action = action;
            }
        },
    );

    // egui_dnd reorders `ordered` in place during the drag — push to App
    // whenever it differs from what we projected at the start of the frame.
    let new_order: Vec<SlotKey> = ordered.iter().map(|w| w.slot_key.clone()).collect();
    if new_order != pre_order {
        app.apply_drag_drop(new_order);
    }

    if let Some(action) = row_action {
        match action {
            RowAction::PillClicked(slot) => app.bind_target(BindingTarget::Account(slot)),
            RowAction::RowClicked { slot, hwnd } => {
                win::focus_window(hwnd);
                app.set_cycle_current(slot);
            }
        }
    }
}

#[derive(Debug)]
enum RowAction {
    PillClicked(SlotKey),
    RowClicked { slot: SlotKey, hwnd: isize },
}

fn draw_account_row(
    ui: &mut egui::Ui,
    theme: Theme,
    app: &App,
    w: &DetectedWindow,
    handle: egui_dnd::Handle<'_>,
    in_conflict: bool,
) -> Option<RowAction> {
    let id = ui.make_persistent_id(("row", &w.slot_key));
    let prior_hovered = ui
        .ctx()
        .read_response(id)
        .map(|r| r.hovered())
        .unwrap_or(false);

    let (bg_normal, bg_hover, border) = if in_conflict {
        let (bg, b, _, _) = theme::conflict_palette(theme);
        (bg, mix(bg, Color32::WHITE, 0.06), b)
    } else {
        row_palette(theme)
    };
    let bg = if prior_hovered { bg_hover } else { bg_normal };
    let avail_width = ui.available_width();
    let mut pill_rect: Option<egui::Rect> = None;

    let frame_resp = Frame::none()
        .fill(bg)
        .rounding(Rounding::same(8.0))
        .stroke(Stroke::new(1.0, border))
        .inner_margin(Margin::symmetric(12.0, 11.0))
        .show(ui, |ui| {
            ui.set_min_width(avail_width - 24.0);
            ui.horizontal(|ui| {
                handle.ui(ui, |ui| {
                    draw_drag_handle(ui, theme);
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(&w.slot_key)
                        .size(13.0)
                        .strong()
                        .color(theme::text_primary(theme)),
                );
                if let Some(class) = &w.class {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(class)
                            .size(11.0)
                            .color(theme::text_tertiary(theme)),
                    );
                }
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let r = binding_pill(
                            ui,
                            theme,
                            app.binding_for_account(&w.slot_key),
                            in_conflict,
                        );
                        pill_rect = Some(r.rect);
                    },
                );
            });
        });

    let row_resp = ui.interact(frame_resp.response.rect, id, Sense::click());
    if row_resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if !row_resp.clicked() {
        return None;
    }
    let on_pill = row_resp
        .interact_pointer_pos()
        .zip(pill_rect)
        .map(|(p, r)| r.contains(p))
        .unwrap_or(false);
    if on_pill {
        Some(RowAction::PillClicked(w.slot_key.clone()))
    } else {
        Some(RowAction::RowClicked {
            slot: w.slot_key.clone(),
            hwnd: w.hwnd,
        })
    }
}

fn draw_cycle_section(ui: &mut egui::Ui, theme: Theme, app: &mut App, conflicts: &HashSet<Trigger>) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(t!("cycle.section").to_uppercase())
            .size(11.0)
            .color(theme::text_tertiary(theme)),
    );
    ui.add_space(6.0);

    let next_binding = app.binding_for(&BindingTarget::CycleNext);
    let next_conflict = next_binding.map(|t| conflicts.contains(&t)).unwrap_or(false);
    if draw_cycle_row(ui, theme, &t!("cycle.next"), next_binding, next_conflict) {
        app.bind_target(BindingTarget::CycleNext);
    }
    ui.add_space(6.0);
    let prev_binding = app.binding_for(&BindingTarget::CyclePrev);
    let prev_conflict = prev_binding.map(|t| conflicts.contains(&t)).unwrap_or(false);
    if draw_cycle_row(ui, theme, &t!("cycle.prev"), prev_binding, prev_conflict) {
        app.bind_target(BindingTarget::CyclePrev);
    }
}

/// Returns true if the user clicked the pill — caller opens the capture modal.
fn draw_cycle_row(
    ui: &mut egui::Ui,
    theme: Theme,
    label: &str,
    binding: Option<Trigger>,
    in_conflict: bool,
) -> bool {
    let (bg_normal, bg_hover, border) = if in_conflict {
        let (bg, b, _, _) = theme::conflict_palette(theme);
        (bg, mix(bg, Color32::WHITE, 0.06), b)
    } else {
        row_palette(theme)
    };
    let id = ui.make_persistent_id(("cycle_row", label));
    let prior_hovered = ui
        .ctx()
        .read_response(id)
        .map(|r| r.hovered())
        .unwrap_or(false);
    let bg = if prior_hovered { bg_hover } else { bg_normal };
    let avail_width = ui.available_width();
    let mut pill_rect: Option<egui::Rect> = None;

    let frame_resp = Frame::none()
        .fill(bg)
        .rounding(Rounding::same(8.0))
        .stroke(Stroke::new(1.0, border))
        .inner_margin(Margin::symmetric(12.0, 11.0))
        .show(ui, |ui| {
            ui.set_min_width(avail_width - 24.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(13.0)
                        .strong()
                        .color(theme::text_primary(theme)),
                );
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let r = binding_pill(ui, theme, binding, in_conflict);
                        pill_rect = Some(r.rect);
                    },
                );
            });
        });

    let row_resp = ui.interact(frame_resp.response.rect, id, Sense::click());
    if !row_resp.clicked() {
        return false;
    }
    row_resp
        .interact_pointer_pos()
        .zip(pill_rect)
        .map(|(p, r)| r.contains(p))
        .unwrap_or(false)
}

fn draw_drag_handle(ui: &mut egui::Ui, theme: Theme) {
    // 2 columns × 3 rows of small dots — reliable across fonts.
    let size = Vec2::new(10.0, 16.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let color = theme::text_tertiary(theme);
    let cx0 = rect.left() + 2.5;
    let cx1 = rect.left() + 7.5;
    let cys = [rect.top() + 3.0, rect.center().y, rect.bottom() - 3.0];
    let r = 1.4;
    let painter = ui.painter();
    for cy in cys {
        painter.circle_filled(egui::pos2(cx0, cy), r, color);
        painter.circle_filled(egui::pos2(cx1, cy), r, color);
    }
}

fn row_palette(theme: Theme) -> (Color32, Color32, Color32) {
    match theme {
        Theme::Dark => (
            theme::BG_ROW_DARK,
            mix(theme::BG_ROW_DARK, Color32::WHITE, 0.08),
            theme::BORDER_SUBTLE_DARK,
        ),
        Theme::Light => (
            theme::BG_ROW_LIGHT,
            mix(theme::BG_ROW_LIGHT, Color32::BLACK, 0.05),
            theme::BORDER_SUBTLE_LIGHT,
        ),
    }
}

fn binding_pill(
    ui: &mut egui::Ui,
    theme: Theme,
    binding: Option<Trigger>,
    in_conflict: bool,
) -> egui::Response {
    let label = match binding {
        Some(t) => t.display_label(),
        None => t!("binding.set").to_string(),
    };
    let has_binding = binding.is_some();

    let font = egui::FontId::monospace(11.0);
    let inner = ui.fonts(|f| {
        f.layout_no_wrap(label.clone(), font.clone(), Color32::WHITE)
            .size()
    });
    let pad = Vec2::new(8.0, 4.0);
    let pill_size = Vec2::new(inner.x + pad.x * 2.0, inner.y + pad.y * 2.0)
        .max(Vec2::new(44.0, 20.0));

    let (rect, response) = ui.allocate_exact_size(pill_size, Sense::click());

    let (bg, border, txt) = if in_conflict {
        let (_row_bg, b, pill_bg, pill_text) = theme::conflict_palette(theme);
        let bg = if response.hovered() {
            mix(pill_bg, Color32::WHITE, 0.08)
        } else {
            pill_bg
        };
        (bg, b, pill_text)
    } else {
        pill_colors(theme, has_binding, response.hovered())
    };
    ui.painter().rect_filled(rect, 4.0, bg);
    ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, border));
    ui.painter()
        .text(rect.center(), egui::Align2::CENTER_CENTER, &label, font, txt);

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn draw_conflict_banner(ui: &mut egui::Ui, theme: Theme, conflicts: &HashSet<Trigger>) {
    if conflicts.is_empty() {
        return;
    }
    let (_row_bg, border, pill_bg, pill_text) = theme::conflict_palette(theme);
    ui.add_space(6.0);
    Frame::none()
        .fill(pill_bg)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, border))
        .inner_margin(Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width() - 20.0);
            // Stable order across frames for readability.
            let mut sorted: Vec<&Trigger> = conflicts.iter().collect();
            sorted.sort_by_key(|t| t.display_label());
            for trig in sorted {
                ui.label(
                    egui::RichText::new(t!(
                        "binding.conflict",
                        trigger = trig.display_label()
                    ))
                    .size(11.0)
                    .color(pill_text),
                );
            }
        });
}

fn pill_colors(theme: Theme, has_binding: bool, hovered: bool) -> (Color32, Color32, Color32) {
    let (bg_set, bg_unset, border_set, border_unset, txt_set, txt_unset) = match theme {
        Theme::Dark => (
            Color32::from_rgb(0x2C, 0x2C, 0x2A),
            Color32::TRANSPARENT,
            Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x14),
            Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x22),
            Color32::from_rgb(0xD3, 0xD1, 0xC7),
            theme::TEXT_TERTIARY_DARK,
        ),
        Theme::Light => (
            Color32::from_rgb(0xF1, 0xEF, 0xE8),
            Color32::TRANSPARENT,
            Color32::from_rgba_unmultiplied(0x00, 0x00, 0x00, 0x10),
            Color32::from_rgba_unmultiplied(0x00, 0x00, 0x00, 0x22),
            Color32::from_rgb(0x44, 0x44, 0x41),
            theme::TEXT_TERTIARY_LIGHT,
        ),
    };
    let (bg, border, txt) = if has_binding {
        (bg_set, border_set, txt_set)
    } else {
        (bg_unset, border_unset, txt_unset)
    };
    if hovered {
        let bg_h = mix(bg, Color32::WHITE, 0.06);
        (bg_h, border, txt)
    } else {
        (bg, border, txt)
    }
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 * inv + b.r() as f32 * t).round() as u8,
        (a.g() as f32 * inv + b.g() as f32 * t).round() as u8,
        (a.b() as f32 * inv + b.b() as f32 * t).round() as u8,
        (a.a() as f32 * inv + b.a() as f32 * t).round() as u8,
    )
}
