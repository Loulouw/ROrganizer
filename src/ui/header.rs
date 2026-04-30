use eframe::egui;
use egui::{Sense, Stroke};
use rust_i18n::t;

use crate::app::App;
use crate::i18n::Lang;
use crate::theme::{self, Theme};

const HEIGHT: f32 = 36.0;

pub fn draw(ctx: &egui::Context, app: &mut App) {
    let theme = app.theme();
    egui::TopBottomPanel::top("title_bar")
        .exact_height(HEIGHT)
        .frame(
            egui::Frame::none()
                .fill(theme::window_bg(theme))
                .inner_margin(egui::Margin::symmetric(0.0, 0.0)),
        )
        .show_separator_line(false)
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            let drag_response = ui.interact(
                rect,
                egui::Id::new("title_bar_drag"),
                Sense::click_and_drag(),
            );
            if drag_response.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(12.0);
                    draw_status_dot(ui, theme);
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("ROrganizer")
                            .size(13.0)
                            .strong()
                            .color(theme::text_primary(theme)),
                    );

                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.add_space(6.0);
                            let close = titlebar_button(ui, "\u{00D7}", theme)
                                .on_hover_text(t!("header.tooltip_close").to_string());
                            if close.clicked() {
                                app.request_close(ctx);
                            }
                            let mini = titlebar_button(ui, "\u{2013}", theme)
                                .on_hover_text(t!("header.tooltip_minimize").to_string());
                            if mini.clicked() {
                                app.request_minimize(ctx);
                            }
                            ui.add_space(2.0);
                            let theme_resp = theme_button(ui, theme)
                                .on_hover_text(t!("header.tooltip_theme").to_string());
                            if theme_resp.clicked() {
                                app.toggle_theme();
                            }
                            draw_lang_dropdown(ui, app, theme);
                        },
                    );
                });
            });
        });
}

fn draw_status_dot(ui: &mut egui::Ui, theme: Theme) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 4.0, theme::text_tertiary(theme));
}

fn titlebar_button(ui: &mut egui::Ui, glyph: &str, theme: Theme) -> egui::Response {
    let size = egui::vec2(28.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 4.0, theme::hover_bg(theme));
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(13.0),
        theme::text_primary(theme),
    );
    response
}

/// Procedurally drawn sun (light theme active) or moon (dark theme active),
/// avoiding glyph dependency on the default egui font.
fn theme_button(ui: &mut egui::Ui, current: Theme) -> egui::Response {
    let size = egui::vec2(28.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 4.0, theme::hover_bg(current));
    }
    let center = rect.center();
    let stroke = Stroke::new(1.4, theme::text_primary(current));
    match current {
        Theme::Dark => {
            // Crescent moon: filled disk with an offset background-colored disk masking.
            ui.painter()
                .circle_filled(center, 7.0, theme::text_primary(current));
            ui.painter().circle_filled(
                center + egui::vec2(2.8, -2.2),
                6.0,
                theme::window_bg(current),
            );
        }
        Theme::Light => {
            // Sun: outlined disk + 8 short rays.
            ui.painter().circle_stroke(center, 4.5, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let dir = egui::vec2(a.cos(), a.sin());
                let p1 = center + dir * 6.5;
                let p2 = center + dir * 9.0;
                ui.painter().line_segment([p1, p2], stroke);
            }
        }
    }
    response
}

fn draw_lang_dropdown(ui: &mut egui::Ui, app: &mut App, theme: Theme) {
    let current = app.lang();
    let resp = lang_button(ui, current, theme)
        .on_hover_text(rust_i18n::t!("header.tooltip_lang").to_string());

    let popup_id = ui.make_persistent_id("lang_popup");
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    egui::popup::popup_below_widget(
        ui,
        popup_id,
        &resp,
        egui::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(120.0);
            for lang in Lang::ALL {
                let row = ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    ui.add(
                        egui::Image::from_bytes(lang.flag_uri(), lang.flag_bytes())
                            .fit_to_exact_size(egui::vec2(18.0, 13.0)),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(lang.label())
                            .size(13.0)
                            .color(if lang == current {
                                theme::text_primary(theme)
                            } else {
                                theme::text_secondary(theme)
                            }),
                    );
                });
                let row_id = ui.make_persistent_id(("lang_row", lang.code()));
                let click = ui.interact(row.response.rect, row_id, Sense::click());
                if click.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if click.clicked() {
                    app.set_lang(lang);
                }
            }
        },
    );
}

fn lang_button(ui: &mut egui::Ui, current: Lang, theme: Theme) -> egui::Response {
    let size = egui::vec2(34.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 4.0, theme::hover_bg(theme));
    }
    let flag_size = egui::vec2(18.0, 13.0);
    let flag_rect = egui::Rect::from_min_size(
        rect.left_center() + egui::vec2(2.0, -flag_size.y / 2.0),
        flag_size,
    );
    egui::Image::from_bytes(current.flag_uri(), current.flag_bytes())
        .fit_to_exact_size(flag_size)
        .paint_at(ui, flag_rect);
    // Tiny chevron drawn manually — the default egui font lacks U+25BE.
    let cx = rect.right() - 7.0;
    let cy = rect.center().y;
    let stroke = Stroke::new(1.2, theme::text_secondary(theme));
    ui.painter().line_segment(
        [egui::pos2(cx - 3.0, cy - 1.5), egui::pos2(cx, cy + 1.5)],
        stroke,
    );
    ui.painter().line_segment(
        [egui::pos2(cx, cy + 1.5), egui::pos2(cx + 3.0, cy - 1.5)],
        stroke,
    );
    response
}
