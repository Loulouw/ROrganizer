use eframe::egui;
use egui::{Color32, Frame, Margin, Rounding, Sense, Stroke, Vec2};

use crate::app::App;
use crate::theme::{self, Theme};
use crate::win::{self, DetectedWindow};

pub fn draw(ctx: &egui::Context, app: &mut App) {
    let theme = app.theme();
    let snapshot = app.windows_snapshot();
    let windows: Vec<DetectedWindow> = {
        let guard = snapshot.lock().expect("snapshot mutex poisoned");
        guard.clone()
    };

    egui::CentralPanel::default()
        .frame(
            Frame::none()
                .fill(theme::window_bg(theme))
                .inner_margin(Margin::symmetric(12.0, 12.0)),
        )
        .show(ctx, |ui| {
            draw_summary(ui, theme, windows.len(), || app.request_refresh());
            ui.add_space(10.0);

            if windows.is_empty() {
                draw_empty_state(ui, theme);
            } else {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for w in &windows {
                            let resp = draw_row(ui, theme, w);
                            if resp.clicked() {
                                win::focus_window(w.hwnd);
                            }
                            ui.add_space(6.0);
                        }
                    });
            }
        });
}

fn draw_summary<F: FnOnce()>(ui: &mut egui::Ui, theme: Theme, count: usize, on_refresh: F) {
    ui.horizontal(|ui| {
        let label = match count {
            0 => "0 compte lancé".to_string(),
            1 => "1 compte lancé".to_string(),
            n => format!("{n} comptes lancés"),
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
        "\u{27F3}", // ⟳
        egui::FontId::proportional(14.0),
        theme::text_secondary(theme),
    );
    response.on_hover_text("Rafraîchir")
}

fn draw_empty_state(ui: &mut egui::Ui, theme: Theme) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("Aucun compte détecté")
                .size(13.0)
                .color(theme::text_tertiary(theme)),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Lance Dofus pour voir tes personnages ici.")
                .size(11.0)
                .color(theme::text_tertiary(theme)),
        );
    });
}

fn draw_row(ui: &mut egui::Ui, theme: Theme, w: &DetectedWindow) -> egui::Response {
    let id = ui.make_persistent_id(("row", &w.slot_key));
    let prior_hovered = ui
        .ctx()
        .read_response(id)
        .map(|r| r.hovered())
        .unwrap_or(false);

    let (bg_normal, bg_hover, border) = match theme {
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
    };
    let bg = if prior_hovered { bg_hover } else { bg_normal };

    let avail_width = ui.available_width();
    let frame_resp = Frame::none()
        .fill(bg)
        .rounding(Rounding::same(8.0))
        .stroke(Stroke::new(1.0, border))
        .inner_margin(Margin::symmetric(12.0, 11.0))
        .show(ui, |ui| {
            ui.set_min_width(avail_width - 24.0); // - inner_margin*2
            ui.horizontal(|ui| {
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
            });
        });

    let click = ui.interact(frame_resp.response.rect, id, Sense::click());
    if click.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    click
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
