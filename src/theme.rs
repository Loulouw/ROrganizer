use eframe::egui;
use egui::Color32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn toggled(self) -> Self {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }
}

// Dark palette — only the roles used in phase 1 are wired into Visuals,
// the rest are kept as named constants for the upcoming phases.
pub const BG_WINDOW_DARK: Color32 = Color32::from_rgb(0x1C, 0x1C, 0x1A);
pub const BG_ROW_DARK: Color32 = Color32::from_rgb(0x26, 0x26, 0x1F);
pub const BORDER_SUBTLE_DARK: Color32 = Color32::from_rgba_premultiplied(0x10, 0x10, 0x10, 0x10);
pub const BORDER_STRONG_DARK: Color32 = Color32::from_rgba_premultiplied(0x1A, 0x1A, 0x1A, 0x1A);
pub const TEXT_PRIMARY_DARK: Color32 = Color32::from_rgb(0xE8, 0xE6, 0xDF);
pub const TEXT_SECONDARY_DARK: Color32 = Color32::from_rgb(0xB4, 0xB2, 0xA9);
pub const TEXT_TERTIARY_DARK: Color32 = Color32::from_rgb(0x88, 0x87, 0x80);
pub const ACTIVE_DARK: Color32 = Color32::from_rgb(0x97, 0xC4, 0x59);

// Light palette — same logic.
pub const BG_WINDOW_LIGHT: Color32 = Color32::from_rgb(0xFA, 0xFA, 0xF7);
pub const BG_ROW_LIGHT: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const BORDER_SUBTLE_LIGHT: Color32 = Color32::from_rgba_premultiplied(0x14, 0x14, 0x14, 0x14);
pub const BORDER_STRONG_LIGHT: Color32 = Color32::from_rgba_premultiplied(0x21, 0x21, 0x21, 0x21);
pub const TEXT_PRIMARY_LIGHT: Color32 = Color32::from_rgb(0x2C, 0x2C, 0x2A);
pub const TEXT_SECONDARY_LIGHT: Color32 = Color32::from_rgb(0x5F, 0x5E, 0x5A);
pub const TEXT_TERTIARY_LIGHT: Color32 = Color32::from_rgb(0x88, 0x87, 0x80);
pub const ACTIVE_LIGHT: Color32 = Color32::from_rgb(0x63, 0x99, 0x22);

pub fn apply(ctx: &egui::Context, theme: Theme) {
    let mut visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    match theme {
        Theme::Dark => {
            visuals.window_fill = BG_WINDOW_DARK;
            visuals.panel_fill = BG_WINDOW_DARK;
            visuals.extreme_bg_color = BG_ROW_DARK;
            visuals.override_text_color = Some(TEXT_PRIMARY_DARK);
            visuals.widgets.noninteractive.bg_stroke =
                egui::Stroke::new(1.0, BORDER_SUBTLE_DARK);
            visuals.widgets.inactive.bg_stroke =
                egui::Stroke::new(1.0, BORDER_STRONG_DARK);
        }
        Theme::Light => {
            visuals.window_fill = BG_WINDOW_LIGHT;
            visuals.panel_fill = BG_WINDOW_LIGHT;
            visuals.extreme_bg_color = BG_ROW_LIGHT;
            visuals.override_text_color = Some(TEXT_PRIMARY_LIGHT);
            visuals.widgets.noninteractive.bg_stroke =
                egui::Stroke::new(1.0, BORDER_SUBTLE_LIGHT);
            visuals.widgets.inactive.bg_stroke =
                egui::Stroke::new(1.0, BORDER_STRONG_LIGHT);
        }
    }
    ctx.set_visuals(visuals);
}

pub fn window_bg(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => BG_WINDOW_DARK,
        Theme::Light => BG_WINDOW_LIGHT,
    }
}

pub fn text_primary(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => TEXT_PRIMARY_DARK,
        Theme::Light => TEXT_PRIMARY_LIGHT,
    }
}

pub fn text_secondary(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => TEXT_SECONDARY_DARK,
        Theme::Light => TEXT_SECONDARY_LIGHT,
    }
}

pub fn text_tertiary(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => TEXT_TERTIARY_DARK,
        Theme::Light => TEXT_TERTIARY_LIGHT,
    }
}

#[allow(dead_code)]
pub fn active(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => ACTIVE_DARK,
        Theme::Light => ACTIVE_LIGHT,
    }
}

pub fn hover_bg(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => Color32::from_rgba_premultiplied(0x18, 0x18, 0x18, 0x18),
        Theme::Light => Color32::from_rgba_premultiplied(0x18, 0x18, 0x18, 0x18),
    }
}
