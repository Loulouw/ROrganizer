//! "About" modal overlay. Triggered by the gear button in the header.
//!
//! Layout follows the same backdrop + centred card pattern as
//! `capture::draw`. The card is theme-aware (dark/light palettes from
//! `crate::theme`) and stays inside the main viewport — no second OS
//! window, no second tray icon, nothing to clean up on close.

use std::path::Path;

use eframe::egui;
use egui::{Color32, CornerRadius, Frame, Margin, Sense, Stroke, StrokeKind, Vec2};
use rust_i18n::t;

use crate::app::App;
use crate::theme::{self, Theme};

const GITHUB_URL: &str = "https://github.com/Loulouw/ROrganizer";
const ISSUES_URL: &str = "https://github.com/Loulouw/ROrganizer/issues";
const LICENSE_LABEL: &str = "MIT OR Apache-2.0";

pub fn draw(ctx: &egui::Context, app: &mut App) {
    if !app.is_about_open() {
        return;
    }
    let theme = app.theme();

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.close_about();
        return;
    }

    let screen_rect = ctx.content_rect();
    let backdrop = match theme {
        Theme::Dark => Color32::from_rgba_unmultiplied(0x10, 0x10, 0x0E, 0xCC),
        Theme::Light => Color32::from_rgba_unmultiplied(0xFA, 0xFA, 0xF7, 0xCC),
    };
    let card_bg = match theme {
        Theme::Dark => theme::BG_ROW_DARK,
        Theme::Light => theme::BG_ROW_LIGHT,
    };
    let card_border = match theme {
        Theme::Dark => theme::BORDER_STRONG_DARK,
        Theme::Light => theme::BORDER_STRONG_LIGHT,
    };

    // Width fits inside the 340-wide viewport with a 10 px breathing room
    // on each side. Height is sized to comfortably hold every section.
    let card_size = egui::vec2(320.0, 420.0);
    let card_rect = egui::Rect::from_center_size(screen_rect.center(), card_size);

    let close_clicked = std::cell::Cell::new(false);
    let mut config_dir_clicked = false;
    let mut github_clicked = false;
    let mut bug_clicked = false;

    egui::Area::new(egui::Id::new("about_overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen_rect.left_top())
        .show(ctx, |ui| {
            // Click on the backdrop (outside the card) closes the modal.
            // Done before painting the card so the card's own clicks win
            // when they overlap.
            let backdrop_resp = ui.interact(
                screen_rect,
                egui::Id::new("about_backdrop"),
                Sense::click(),
            );
            ui.painter()
                .rect_filled(screen_rect, CornerRadius::ZERO, backdrop);
            if backdrop_resp.clicked()
                && backdrop_resp
                    .interact_pointer_pos()
                    .map(|p| !card_rect.contains(p))
                    .unwrap_or(false)
            {
                close_clicked.set(true);
            }

            ui.scope_builder(egui::UiBuilder::new().max_rect(card_rect), |ui| {
                Frame::new()
                    .fill(card_bg)
                    .corner_radius(CornerRadius::same(10))
                    .stroke(Stroke::new(1.0, card_border))
                    .inner_margin(Margin::symmetric(18, 16))
                    .show(ui, |ui| {
                        ui.set_min_size(card_size - egui::vec2(36.0, 32.0));
                        ui.vertical_centered(|ui| {
                            // App icon — pre-decoded into a TextureHandle in
                            // App::new because egui_extras isn't built with
                            // the "image" feature, so PNGs from `bytes://`
                            // would render as the red error placeholder.
                            if let Some(handle) = app.about_icon() {
                                ui.add(
                                    egui::Image::new(handle)
                                        .fit_to_exact_size(egui::vec2(56.0, 56.0)),
                                );
                            }
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("ROrganizer")
                                    .size(16.0)
                                    .strong()
                                    .color(theme::text_primary(theme)),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "v{}",
                                    env!("CARGO_PKG_VERSION")
                                ))
                                .size(11.0)
                                .color(theme::text_tertiary(theme)),
                            );
                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new(t!("about.credit"))
                                    .size(11.0)
                                    .color(theme::text_secondary(theme)),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(t!("about.ankama_credit"))
                                    .size(10.0)
                                    .color(theme::text_tertiary(theme)),
                            );
                        });

                        ui.add_space(14.0);
                        if github_link(ui, theme).clicked() {
                            github_clicked = true;
                        }
                        ui.add_space(8.0);
                        if action_row(
                            ui,
                            theme,
                            &t!("about.report_bug"),
                            BUG_GLYPH,
                        )
                        .clicked()
                        {
                            bug_clicked = true;
                        }
                        ui.add_space(8.0);
                        if action_row(
                            ui,
                            theme,
                            &t!("about.open_config_dir"),
                            FOLDER_GLYPH,
                        )
                        .clicked()
                        {
                            config_dir_clicked = true;
                        }

                        ui.add_space(12.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new(LICENSE_LABEL)
                                    .size(10.0)
                                    .color(theme::text_tertiary(theme)),
                            );
                            ui.add_space(8.0);
                            if close_button(ui, theme).clicked() {
                                close_clicked.set(true);
                            }
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(t!("about.esc_hint"))
                                    .size(9.0)
                                    .color(theme::text_tertiary(theme)),
                            );
                        });
                    });
            });
        });

    // Apply the side-effects after the borrow on `app` from `is_about_open`
    // has been released by going out of scope above.
    if github_clicked {
        let _ = webbrowser::open(GITHUB_URL);
    }
    if bug_clicked {
        let _ = webbrowser::open(ISSUES_URL);
    }
    if config_dir_clicked
        && let Some(dir) = app.config_dir()
    {
        open_folder(dir);
    }
    if close_clicked.get() {
        app.close_about();
    }
}

const BUG_GLYPH: &str = "\u{26A0}"; // ⚠
const FOLDER_GLYPH: &str = "\u{1F4C1}"; // 📁 — best-effort, font may fall back

fn github_link(ui: &mut egui::Ui, theme: Theme) -> egui::Response {
    let height = 36.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());

    let (bg, border) = row_palette(theme, response.hovered());
    ui.painter().rect_filled(rect, 8.0, bg);
    ui.painter()
        .rect_stroke(rect, 8.0, Stroke::new(1.0, border), StrokeKind::Inside);

    // Layout: [12 px pad] [logo 18×18] [8 px gap] [label] [auto] [→ 12 px pad]
    let logo_size = egui::vec2(18.0, 18.0);
    let logo_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 12.0, rect.center().y - logo_size.y / 2.0),
        logo_size,
    );
    let tint = if response.hovered() {
        theme::text_primary(theme)
    } else {
        theme::text_secondary(theme)
    };
    egui::Image::from_bytes("bytes://github.svg", crate::assets::GITHUB_SVG)
        .fit_to_exact_size(logo_size)
        .tint(tint)
        .paint_at(ui, logo_rect);

    let label_pos = egui::pos2(logo_rect.right() + 8.0, rect.center().y);
    ui.painter().text(
        label_pos,
        egui::Align2::LEFT_CENTER,
        "Loulouw/ROrganizer",
        egui::FontId::proportional(13.0),
        theme::text_primary(theme),
    );

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Generic "icon + label, full width, click anywhere" row used for the
/// bug / config-dir actions. Keeps the visual rhythm of the github row.
fn action_row(ui: &mut egui::Ui, theme: Theme, label: &str, glyph: &str) -> egui::Response {
    let height = 32.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());

    let (bg, border) = row_palette(theme, response.hovered());
    ui.painter().rect_filled(rect, 8.0, bg);
    ui.painter()
        .rect_stroke(rect, 8.0, Stroke::new(1.0, border), StrokeKind::Inside);

    ui.painter().text(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        glyph,
        egui::FontId::proportional(13.0),
        theme::text_secondary(theme),
    );
    ui.painter().text(
        egui::pos2(rect.left() + 38.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.0),
        theme::text_primary(theme),
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn row_palette(theme: Theme, hovered: bool) -> (Color32, Color32) {
    match theme {
        Theme::Dark => {
            let base = Color32::from_rgb(0x2C, 0x2C, 0x2A);
            let bg = if hovered {
                Color32::from_rgb(0x36, 0x36, 0x32)
            } else {
                base
            };
            (bg, theme::BORDER_SUBTLE_DARK)
        }
        Theme::Light => {
            let base = Color32::from_rgb(0xF1, 0xEF, 0xE8);
            let bg = if hovered {
                Color32::from_rgb(0xE8, 0xE6, 0xDF)
            } else {
                base
            };
            (bg, theme::BORDER_SUBTLE_LIGHT)
        }
    }
}

fn close_button(ui: &mut egui::Ui, theme: Theme) -> egui::Response {
    let size = Vec2::new(96.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (bg, txt) = theme::button_primary(theme);
    let bg = if response.hovered() {
        // Slightly brighten on hover. A direct `mix` would be cleaner but
        // pulling in `ui::main_view::mix` from another module just for
        // this would over-couple the modules.
        match theme {
            Theme::Dark => Color32::from_rgb(0xF0, 0xEE, 0xE7),
            Theme::Light => Color32::from_rgb(0x3A, 0x3A, 0x37),
        }
    } else {
        bg
    };
    ui.painter().rect_filled(rect, 6.0, bg);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        t!("about.close").as_ref(),
        egui::FontId::proportional(12.0),
        txt,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

#[cfg(windows)]
fn open_folder(path: &Path) {
    // `explorer.exe <path>` is the lightest way to open a folder in
    // Explorer without pulling in `Win32_UI_Shell` just for this.
    // Failure is silent — the user can't act on it anyway.
    let _ = std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn();
}

#[cfg(not(windows))]
fn open_folder(_path: &Path) {}
