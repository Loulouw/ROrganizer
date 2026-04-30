use eframe::egui;
use egui::Sense;

use crate::app::App;
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
                            if titlebar_button(ui, "\u{00D7}", theme).clicked() {
                                app.request_close(ctx);
                            }
                            if titlebar_button(ui, "\u{2013}", theme).clicked() {
                                app.request_minimize(ctx);
                            }
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
