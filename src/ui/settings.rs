//! Settings modal, opened by the gear button in the header: an Options tab
//! and an About tab in one card.
//!
//! Layout follows the same backdrop + centred card pattern as
//! `capture::draw`. The card is theme-aware (dark/light palettes from
//! `crate::theme`) and stays inside the main viewport — no second OS
//! window, no second tray icon, nothing to clean up on close.

use eframe::egui;
use egui::{Color32, CornerRadius, Frame, Margin, Sense, Stroke, StrokeKind, Vec2};
use rust_i18n::t;

use crate::app::App;
use crate::overlay::{Display, Layout, Visibility};
use crate::platform;
use crate::theme::{self, Theme};

const GITHUB_URL: &str = "https://github.com/Loulouw/ROrganizer";
const ISSUES_URL: &str = "https://github.com/Loulouw/ROrganizer/issues";
const WEBSITE_URL: &str = "https://rorganizer.loulouw-labs.fr";
/// Affiché sans le schéma : c'est un domaine, il se lit dans les trois
/// langues sans traduction, donc pas de clé i18n.
const WEBSITE_LABEL: &str = "rorganizer.loulouw-labs.fr";
const LICENSE_LABEL: &str = "MIT OR Apache-2.0";

/// Card height, sized for the About tab (the taller one). The viewport is
/// kept at least this tall + margin while the modal is open
/// (`App::apply_dynamic_height`), so the close button is never clipped.
pub const CARD_HEIGHT: f32 = 470.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Options,
    About,
}

/// Actions with side effects outside egui, applied once the card is drawn.
#[derive(Default)]
struct Clicks {
    close: bool,
    github: bool,
    bug: bool,
    website: bool,
    config_dir: bool,
}

pub fn draw(ctx: &egui::Context, app: &mut App) {
    if !app.is_settings_open() {
        return;
    }
    let theme = app.theme();

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.close_settings();
        return;
    }

    let screen_rect = ctx.content_rect();
    let backdrop = match theme {
        Theme::Dark => Color32::from_rgba_unmultiplied(0x10, 0x10, 0x0E, 0xCC),
        Theme::Light => Color32::from_rgba_unmultiplied(0xD8, 0xD5, 0xCD, 0xCC),
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
    // on each side.
    let card_size = egui::vec2(320.0, CARD_HEIGHT);
    let card_rect = egui::Rect::from_center_size(screen_rect.center(), card_size);
    let mut clicks = Clicks::default();

    egui::Area::new(egui::Id::unique("settings_overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen_rect.left_top())
        .show(ctx, |ui| {
            // Click on the backdrop (outside the card) closes the modal.
            // Done before painting the card so the card's own clicks win
            // when they overlap.
            let backdrop_resp = ui.interact(
                screen_rect,
                egui::Id::unique("settings_backdrop"),
                Sense::click(),
            );
            ui.painter()
                .rect_filled(screen_rect, CornerRadius::ZERO, backdrop);
            if backdrop_resp.clicked()
                && backdrop_resp
                    .interact_pointer_pos()
                    .is_some_and(|p| !card_rect.contains(p))
            {
                clicks.close = true;
            }

            ui.scope_builder(egui::UiBuilder::new().max_rect(card_rect), |ui| {
                Frame::new()
                    .fill(card_bg)
                    .corner_radius(CornerRadius::same(10))
                    .stroke(Stroke::new(1.0, card_border))
                    .inner_margin(Margin::symmetric(18, 16))
                    .show(ui, |ui| {
                        ui.set_min_size(card_size - egui::vec2(36.0, 32.0));
                        if let Some(tab) = tab_bar(ui, theme, app.settings_tab()) {
                            app.set_settings_tab(tab);
                        }
                        ui.add_space(14.0);
                        match app.settings_tab() {
                            Tab::Options => draw_options(ui, ctx, theme, app),
                            Tab::About => draw_about(ui, theme, app, &mut clicks),
                        }
                        // Footer pinned to the bottom of the card, whatever
                        // the height of the tab above.
                        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(t!("settings.esc_hint"))
                                    .size(9.0)
                                    .color(theme::text_tertiary(theme)),
                            );
                            ui.add_space(2.0);
                            if close_button(ui, theme).clicked() {
                                clicks.close = true;
                            }
                        });
                    });
            });
        });

    if clicks.github {
        let _ = webbrowser::open(GITHUB_URL);
    }
    if clicks.bug {
        let _ = webbrowser::open(ISSUES_URL);
    }
    if clicks.website {
        let _ = webbrowser::open(WEBSITE_URL);
    }
    if clicks.config_dir
        && let Some(dir) = app.config_dir()
    {
        platform::open_folder(dir);
    }
    if clicks.close {
        app.close_settings();
    }
}

/// Returns the tab clicked, if it isn't the current one.
fn tab_bar(ui: &mut egui::Ui, theme: Theme, current: Tab) -> Option<Tab> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (tab, label) in [
            (Tab::Options, t!("settings.tab_options")),
            (Tab::About, t!("settings.tab_about")),
        ] {
            if tab_button(ui, theme, &label, tab == current).clicked() && tab != current {
                picked = Some(tab);
            }
        }
    });
    let border = match theme {
        Theme::Dark => theme::BORDER_STRONG_DARK,
        Theme::Light => theme::BORDER_STRONG_LIGHT,
    };
    let y = ui.cursor().top();
    ui.painter()
        .hline(ui.max_rect().x_range(), y, Stroke::new(1.0, border));
    picked
}

fn tab_button(ui: &mut egui::Ui, theme: Theme, label: &str, selected: bool) -> egui::Response {
    let color = if selected {
        theme::text_primary(theme)
    } else {
        theme::text_secondary(theme)
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(13.0),
        color,
    );
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(galley.size().x + 16.0, 28.0), Sense::click());
    if response.hovered() && !selected {
        ui.painter().rect_filled(rect, 4.0, theme::hover_bg(theme));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
    if selected {
        let accent = match theme {
            Theme::Dark => theme::ACTIVE_DARK,
            Theme::Light => theme::ACTIVE_LIGHT,
        };
        ui.painter().hline(
            rect.x_range().shrink(4.0),
            rect.bottom() - 1.0,
            Stroke::new(2.0, accent),
        );
    }
    response
}

fn draw_options(ui: &mut egui::Ui, ctx: &egui::Context, theme: Theme, app: &mut App) {
    let mut dofus_only = app.shortcuts_only_in_dofus();
    if option_row(
        ui,
        theme,
        &mut dofus_only,
        &t!("settings.dofus_only"),
        &t!("settings.dofus_only_hint"),
    ) {
        app.set_shortcuts_only_in_dofus(dofus_only);
    }
    ui.add_space(12.0);
    let mut to_taskbar = app.minimize_to_taskbar();
    let (to_taskbar_label, to_taskbar_hint) = if platform::TRAY_IS_MENU_BAR {
        (t!("settings.minimize_to_dock"), t!("settings.minimize_to_dock_hint"))
    } else {
        (t!("settings.minimize_to_taskbar"), t!("settings.minimize_to_taskbar_hint"))
    };
    if option_row(ui, theme, &mut to_taskbar, &to_taskbar_label, &to_taskbar_hint) {
        app.set_minimize_to_taskbar(to_taskbar);
    }
    ui.add_space(12.0);
    let mut on_top = app.always_on_top();
    if option_row(
        ui,
        theme,
        &mut on_top,
        &t!("settings.always_on_top"),
        &t!("settings.always_on_top_hint"),
    ) {
        app.set_always_on_top(ctx, on_top);
    }
    if platform::HAS_OVERLAY {
        ui.add_space(12.0);
        draw_overlay_options(ui, theme, app);
    }
}

fn draw_overlay_options(ui: &mut egui::Ui, theme: Theme, app: &mut App) {
    let mut on = app.overlay_enabled();
    if option_row(ui, theme, &mut on, &t!("settings.overlay"), &t!("settings.overlay_hint")) {
        app.set_overlay_enabled(on);
    }
    ui.add_space(4.0);
    ui.add_enabled_ui(on, |ui| {
        let layout = choice_row(
            ui,
            theme,
            &t!("overlay.layout"),
            app.overlay_layout(),
            [
                (Layout::Horizontal, t!("overlay.horizontal")),
                (Layout::Vertical, t!("overlay.vertical")),
            ],
        );
        app.set_overlay_layout(layout);
        let display = choice_row(
            ui,
            theme,
            &t!("overlay.display"),
            app.overlay_display(),
            [
                (Display::ClassHead, t!("overlay.class_head")),
                (Display::Initials, t!("overlay.initials")),
            ],
        );
        app.set_overlay_display(display);
        let visibility = choice_row(
            ui,
            theme,
            &t!("overlay.visibility"),
            app.overlay_visibility(),
            [
                (Visibility::WithDofus, t!("overlay.with_dofus")),
                (Visibility::Always, t!("overlay.always")),
            ],
        );
        app.set_overlay_visibility(visibility);
    });
}

/// Label then radio buttons, indented under the checkbox above. Returns the
/// value picked, `current` if none was clicked.
fn choice_row<T: PartialEq + Copy, const N: usize>(
    ui: &mut egui::Ui,
    theme: Theme,
    label: &str,
    current: T,
    choices: [(T, std::borrow::Cow<'_, str>); N],
) -> T {
    let mut value = current;
    let indent = ui.spacing().icon_width + ui.spacing().icon_spacing;
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.allocate_ui_with_layout(
            egui::vec2(72.0, 18.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(72.0);
                ui.label(
                    egui::RichText::new(label)
                        .size(11.0)
                        .color(theme::text_tertiary(theme)),
                );
            },
        );
        for (choice, text) in choices {
            ui.radio_value(
                &mut value,
                choice,
                egui::RichText::new(text)
                    .size(11.5)
                    .color(theme::text_secondary(theme)),
            );
        }
    });
    value
}

/// Checkbox + its explanation, indented under the label. Returns true when
/// the box was toggled this frame.
fn option_row(ui: &mut egui::Ui, theme: Theme, value: &mut bool, label: &str, hint: &str) -> bool {
    let changed = ui
        .checkbox(
            value,
            egui::RichText::new(label)
                .size(12.5)
                .color(theme::text_primary(theme)),
        )
        .changed();
    let indent = ui.spacing().icon_width + ui.spacing().icon_spacing;
    Frame::new()
        .inner_margin(Margin {
            left: indent as i8,
            ..Margin::ZERO
        })
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(hint)
                    .size(10.5)
                    .color(theme::text_tertiary(theme)),
            );
        });
    changed
}

fn draw_about(ui: &mut egui::Ui, theme: Theme, app: &App, clicks: &mut Clicks) {
    ui.vertical_centered(|ui| {
        // App icon — pre-decoded into a TextureHandle in App::new because
        // egui_extras isn't built with the "image" feature, so PNGs from
        // `bytes://` would render as the red error placeholder.
        if let Some(handle) = app.about_icon() {
            ui.add(egui::Image::new(handle).fit_to_exact_size(egui::vec2(56.0, 56.0)));
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("ROrganizer")
                .size(16.0)
                .strong()
                .color(theme::text_primary(theme)),
        );
        ui.label(
            egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                .size(11.0)
                .color(theme::text_tertiary(theme)),
        );
        // Une ligne de texte plutôt qu'une 4e ligne d'action : l'onglet
        // doit tenir dans CARD_HEIGHT avec les onglets et le pied de carte.
        ui.add_space(6.0);
        let site = ui.add(
            egui::Label::new(
                egui::RichText::new(WEBSITE_LABEL)
                    .size(11.0)
                    .color(match theme {
                        Theme::Dark => theme::ACTIVE_DARK,
                        Theme::Light => theme::ACTIVE_LIGHT,
                    }),
            )
            .sense(Sense::click()),
        );
        if site.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        clicks.website = site.clicked();
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
    clicks.github = github_link(ui, theme).clicked();
    ui.add_space(8.0);
    clicks.bug = action_row(ui, theme, &t!("about.report_bug"), BUG_GLYPH).clicked();
    ui.add_space(8.0);
    clicks.config_dir =
        action_row(ui, theme, &t!("about.open_config_dir"), FOLDER_GLYPH).clicked();
    ui.add_space(12.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(LICENSE_LABEL)
                .size(10.0)
                .color(theme::text_tertiary(theme)),
        );
    });
}

// WARNING SIGN and FILE FOLDER: only the bundled emoji fonts carry them
// (egui feature `monochrome_emoji_fonts`).
const BUG_GLYPH: &str = "\u{26A0}";
const FOLDER_GLYPH: &str = "\u{1F4C1}";

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
            let base = Color32::from_rgb(0xEA, 0xE8, 0xE1);
            let bg = if hovered {
                Color32::from_rgb(0xE0, 0xDD, 0xD5)
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
        t!("settings.close").as_ref(),
        egui::FontId::proportional(12.0),
        txt,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}
