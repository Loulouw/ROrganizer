use eframe::egui;
use egui::{Color32, Frame, Margin, Rounding, Stroke};
use rust_i18n::t;

use crate::app::App;
use crate::theme::{self, Theme};
use crate::triggers::{key_to_vk, BindingTarget, MouseBtn, Trigger, WheelDir};

pub fn draw(ctx: &egui::Context, app: &mut App) {
    let Some(target) = app.capture_state().cloned() else {
        return;
    };
    let theme = app.theme();

    // 1) Esc clears any existing binding for this target, then closes the modal.
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.clear_binding(&target);
        app.cancel_capture();
        return;
    }

    // 2) Try to capture the first valid trigger from this frame's events.
    let captured = ctx.input(|i| extract_first_trigger(&i.events));
    if let Some(trigger) = captured {
        app.commit_binding(target.clone(), trigger);
        return;
    }

    // 3) Render the modal overlay (semi-transparent backdrop + centered card).
    let screen_rect = ctx.screen_rect();
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

    let target_label = match &target {
        BindingTarget::Account(slot) => {
            t!("binding.for_account", name = slot.as_str()).to_string()
        }
        BindingTarget::CycleNext => t!("cycle.next").to_string(),
        BindingTarget::CyclePrev => t!("cycle.prev").to_string(),
    };

    egui::Area::new(egui::Id::new("capture_overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen_rect.left_top())
        .show(ctx, |ui| {
            ui.painter()
                .rect_filled(screen_rect, Rounding::ZERO, backdrop);

            let card_size = egui::vec2(280.0, 140.0);
            let card_rect = egui::Rect::from_center_size(screen_rect.center(), card_size);
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(card_rect), |ui| {
                Frame::none()
                    .fill(card_bg)
                    .rounding(Rounding::same(10.0))
                    .stroke(Stroke::new(1.0, card_border))
                    .inner_margin(Margin::symmetric(18.0, 16.0))
                    .show(ui, |ui| {
                        ui.set_min_size(card_size - egui::vec2(36.0, 32.0));
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new(t!("binding.modal_title").to_uppercase())
                                    .size(11.0)
                                    .color(theme::text_tertiary(theme)),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(target_label)
                                    .size(14.0)
                                    .strong()
                                    .color(theme::text_primary(theme)),
                            );
                            ui.add_space(14.0);
                            let active = match theme {
                                Theme::Dark => theme::ACTIVE_DARK,
                                Theme::Light => theme::ACTIVE_LIGHT,
                            };
                            ui.label(
                                egui::RichText::new("\u{2026}")
                                    .size(20.0)
                                    .color(active),
                            );
                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new(t!("binding.prompt"))
                                    .size(11.0)
                                    .color(theme::text_secondary(theme)),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(t!("binding.cancel_hint"))
                                    .size(10.0)
                                    .color(theme::text_tertiary(theme)),
                            );
                        });
                    });
            });
        });
}

fn extract_first_trigger(events: &[egui::Event]) -> Option<Trigger> {
    for ev in events {
        match ev {
            egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                ..
            } => {
                if matches!(key, egui::Key::Escape) {
                    continue;
                }
                if let Some(vk) = key_to_vk(*key) {
                    return Some(Trigger::Key(vk));
                }
            }
            egui::Event::PointerButton {
                button,
                pressed: true,
                ..
            } => match button {
                egui::PointerButton::Middle => return Some(Trigger::Mouse(MouseBtn::Middle)),
                egui::PointerButton::Extra1 => return Some(Trigger::Mouse(MouseBtn::X1)),
                egui::PointerButton::Extra2 => return Some(Trigger::Mouse(MouseBtn::X2)),
                _ => {}
            },
            egui::Event::MouseWheel { delta, .. } => {
                if delta.y > 0.5 {
                    return Some(Trigger::Wheel(WheelDir::Up));
                }
                if delta.y < -0.5 {
                    return Some(Trigger::Wheel(WheelDir::Down));
                }
            }
            _ => {}
        }
    }
    None
}
