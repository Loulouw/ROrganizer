//! AppKit drawing primitives for the overlay, fed with the egui palette
//! and geometry of `crate::overlay` so both renderers stay alike. Only
//! called from `drawRect:` and layout code, on the main thread, in a
//! flipped view (y downwards, as egui).

use eframe::egui::{Color32, Pos2, Rect};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AllocAnyThread;
use objc2_app_kit::{
    NSBezierPath, NSColor, NSCompositingOperation, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSFontWeightRegular, NSImage, NSLineCapStyle,
    NSStringDrawing,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSPoint, NSRect, NSSize, NSString,
};

type Attributes = NSDictionary<NSAttributedStringKey, AnyObject>;

fn attributes(font: &NSFont, c: Color32) -> Retained<Attributes> {
    let c = color(c);
    let keys: [&NSAttributedStringKey; 2] =
        unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&AnyObject; 2] = [font, &c];
    NSDictionary::from_slices(&keys, &values)
}

/// `text` in `font` and color `c`, for an AppKit control (menu item).
pub(super) fn attributed(text: &str, font: &NSFont, c: Color32) -> Retained<NSAttributedString> {
    let attributes = attributes(font, c);
    unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(text),
            Some(&attributes),
        )
    }
}

pub(super) fn color(c: Color32) -> Retained<NSColor> {
    let [r, g, b, a] = c.to_srgba_unmultiplied().map(|v| f64::from(v) / 255.0);
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, a)
}

pub(super) fn ns_rect(r: Rect) -> NSRect {
    NSRect::new(
        NSPoint::new(f64::from(r.min.x), f64::from(r.min.y)),
        NSSize::new(f64::from(r.width()), f64::from(r.height())),
    )
}

fn rounded(r: Rect, radius: f32) -> Retained<NSBezierPath> {
    let radius = f64::from(radius.max(0.0));
    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(ns_rect(r), radius, radius)
}

/// egui's `painter.rect(rect, radius, fill, stroke, StrokeKind::Inside)`:
/// the stroke lies entirely inside `r`.
pub(super) fn rounded_rect(r: Rect, radius: u8, fill: Color32, stroke: Option<(f32, Color32)>) {
    let radius = f32::from(radius);
    color(fill).setFill();
    rounded(r, radius).fill();
    if let Some((width, c)) = stroke {
        let path = rounded(r.shrink(width / 2.0), radius - width / 2.0);
        path.setLineWidth(f64::from(width));
        color(c).setStroke();
        path.stroke();
    }
}

pub(super) fn circle(center: Pos2, radius: f32, fill: Color32) {
    let r = Rect::from_center_size(center, eframe::egui::Vec2::splat(radius * 2.0));
    color(fill).setFill();
    NSBezierPath::bezierPathWithOvalInRect(ns_rect(r)).fill();
}

pub(super) fn line(a: Pos2, b: Pos2, width: f32, c: Color32) {
    let path = NSBezierPath::new();
    path.moveToPoint(NSPoint::new(f64::from(a.x), f64::from(a.y)));
    path.lineToPoint(NSPoint::new(f64::from(b.x), f64::from(b.y)));
    path.setLineWidth(f64::from(width));
    path.setLineCapStyle(NSLineCapStyle::Round);
    color(c).setStroke();
    path.stroke();
}

/// `image` scaled into `r` at `opacity`, upright in the flipped view.
pub(super) fn image(image: &NSImage, r: Rect, opacity: f32) {
    unsafe {
        image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
            ns_rect(r),
            NSRect::ZERO,
            NSCompositingOperation::SourceOver,
            f64::from(opacity),
            true,
            None,
        );
    }
}

/// One line of text in one font and color.
pub(super) struct Text {
    string: Retained<NSString>,
    attributes: Retained<Attributes>,
    pub(super) size: NSSize,
}

pub(super) enum Font {
    Proportional(f32),
    Monospace(f32),
}

impl Text {
    pub(super) fn new(text: &str, font: Font, c: Color32) -> Self {
        let font = match font {
            Font::Proportional(size) => NSFont::systemFontOfSize(f64::from(size)),
            Font::Monospace(size) => {
                NSFont::monospacedSystemFontOfSize_weight(f64::from(size), unsafe {
                    NSFontWeightRegular
                })
            }
        };
        let attributes = attributes(&font, c);
        let string = NSString::from_str(text);
        let size = unsafe { string.sizeWithAttributes(Some(&attributes)) };
        Self { string, attributes, size }
    }

    pub(super) fn width(&self) -> f32 {
        self.size.width as f32
    }

    pub(super) fn height(&self) -> f32 {
        self.size.height as f32
    }

    /// Drawn with its top-left corner at `at`.
    pub(super) fn draw(&self, at: Pos2) {
        let at = NSPoint::new(f64::from(at.x), f64::from(at.y));
        unsafe { self.string.drawAtPoint_withAttributes(at, Some(&self.attributes)) };
    }
}
