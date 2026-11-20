//! The tooltip: a second panel, click-through, drawn like the egui one
//! (`ui::overlay::draw_tip`). The bar decides what it shows and where.

use std::cell::RefCell;

use eframe::egui::{Pos2, Rect, Vec2};
use objc2::rc::Retained;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSPanel, NSView};
use objc2_foundation::NSRect;

use super::draw::{self, Font, Text};
use crate::overlay::{self, Tip, PILL_PAD, TIP_PAD};
use crate::theme::{self, Theme};

/// The tooltip's lines of text, measured.
pub(super) struct Layout {
    pub(super) size: Vec2,
    name: Text,
    detail: Option<Text>,
    binding: Option<Text>,
    theme: Theme,
}

impl Layout {
    pub(super) fn new(tip: &Tip, theme: Theme) -> Self {
        let name = Text::new(&tip.name, Font::Proportional(12.5), theme::text_primary(theme));
        let detail = tip.detail.as_ref().map(|d| {
            Text::new(&format!("· {d}"), Font::Proportional(12.0), theme::text_secondary(theme))
        });
        let binding = tip
            .binding
            .as_ref()
            .map(|b| Text::new(b, Font::Monospace(11.0), theme::text_secondary(theme)));
        let mut width = TIP_PAD.x * 2.0 + name.width();
        let mut height = name.height();
        if let Some(d) = &detail {
            width += 5.0 + d.width();
        }
        if let Some(b) = &binding {
            width += 8.0 + b.width() + PILL_PAD.x * 2.0;
            height = height.max(b.height() + PILL_PAD.y * 2.0);
        }
        Self {
            size: Vec2::new(width.ceil(), (height + TIP_PAD.y * 2.0).ceil()),
            name,
            detail,
            binding,
            theme,
        }
    }

    fn paint(&self) {
        let rect = Rect::from_min_size(Pos2::ZERO, self.size);
        let (fill, border) = overlay::bar_colors(self.theme);
        draw::rounded_rect(rect, overlay::TIP_RADIUS, fill, Some((1.0, border)));

        let mid = rect.center().y;
        let put = |text: &Text, x: f32| text.draw(Pos2::new(x, mid - text.height() / 2.0));
        let mut x = TIP_PAD.x;
        put(&self.name, x);
        x += self.name.width();
        if let Some(detail) = &self.detail {
            x += 5.0;
            put(detail, x);
            x += detail.width();
        }
        if let Some(binding) = &self.binding {
            x += 8.0;
            let pill = Rect::from_min_size(
                Pos2::new(x, mid - binding.height() / 2.0 - PILL_PAD.y),
                Vec2::new(binding.width(), binding.height()) + PILL_PAD * 2.0,
            );
            draw::rounded_rect(
                pill,
                overlay::PILL_RADIUS,
                theme::window_bg(self.theme),
                Some((1.0, border)),
            );
            put(binding, x + PILL_PAD.x);
        }
    }
}

pub(super) struct TipIvars {
    layout: RefCell<Option<Layout>>,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements; no Drop.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "ROrganizerOverlayTip"]
    #[ivars = TipIvars]
    pub(super) struct TipView;

    impl TipView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            if let Some(layout) = self.ivars().layout.borrow().as_ref() {
                layout.paint();
            }
        }
    }
);

impl TipView {
    pub(super) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TipIvars { layout: RefCell::new(None) });
        unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] }
    }

    pub(super) fn set(&self, layout: Layout) {
        *self.ivars().layout.borrow_mut() = Some(layout);
        self.setNeedsDisplay(true);
    }
}

/// The tooltip panel, click-through.
pub(super) fn panel(mtm: MainThreadMarker) -> (Retained<NSPanel>, Retained<TipView>) {
    let panel = super::new_panel(mtm);
    panel.setIgnoresMouseEvents(true);
    let view = TipView::new(mtm);
    panel.setContentView(Some(&view));
    (panel, view)
}
