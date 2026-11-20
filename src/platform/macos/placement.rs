//! Position of the main window across launches: its top-left corner in
//! points, with the Windows convention of y growing downwards from the top
//! of the primary screen. AppKit counts y upwards from its bottom: every
//! value is converted on the way in and out.

use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_foundation::{NSPoint, NSRect};

use crate::platform::MainWindow;

/// Height of the strip, from the top edge, that must stay on a screen so
/// the window can still be dragged by its title bar.
const TITLE_STRIP: i32 = 36;

/// Screen area in points, y downwards from the top of the primary screen.
/// `overlay` works in the same coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rect {
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) right: i32,
    pub(super) bottom: i32,
}

impl Rect {
    pub(super) fn from_appkit(r: NSRect, primary_height: f64) -> Self {
        let top = primary_height - (r.origin.y + r.size.height);
        Self {
            left: r.origin.x.round() as i32,
            top: top.round() as i32,
            right: (r.origin.x + r.size.width).round() as i32,
            bottom: (top + r.size.height).round() as i32,
        }
    }

    fn overlap(self, other: Rect) -> i64 {
        let w = (self.right.min(other.right) - self.left.max(other.left)).max(0);
        let h = (self.bottom.min(other.bottom) - self.top.max(other.top)).max(0);
        i64::from(w) * i64::from(h)
    }
}

/// Top-left corner of the window. None while it is minimized or hidden, or
/// off the main thread (AppKit).
pub fn position(window: MainWindow) -> Option<(i32, i32)> {
    let mtm = MainThreadMarker::new()?;
    let ns_window = window.ns_window(mtm)?;
    if ns_window.isMiniaturized() || !ns_window.isVisible() {
        return None;
    }
    let frame = Rect::from_appkit(ns_window.frame(), primary_height(mtm)?);
    Some((frame.left, frame.top))
}

/// Moves the window to `pos`, pulled inside the visible area (menu bar and
/// Dock excluded) of the screen showing most of its title bar. Nothing
/// happens when no screen shows it (screen unplugged since the last
/// launch): the window keeps its default, centred spot.
pub fn restore(window: MainWindow, pos: (i32, i32)) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(ns_window) = window.ns_window(mtm) else { return };
    let Some(primary_height) = primary_height(mtm) else { return };
    let width = ns_window.frame().size.width.round() as i32;
    let strip = Rect { left: pos.0, top: pos.1, right: pos.0 + width, bottom: pos.1 + TITLE_STRIP };
    let Some(work) = visible_area_showing(mtm, strip, primary_height) else { return };
    let (x, y) = clamp_into(work, pos, width);
    let top_left = NSPoint::new(f64::from(x), primary_height - f64::from(y));
    ns_window.setFrameTopLeftPoint(top_left);
}

/// Visible area (menu bar and Dock excluded) of the screen showing most of
/// `rect`; None when no screen shows any of it.
pub(super) fn visible_area_showing(
    mtm: MainThreadMarker,
    rect: Rect,
    primary_height: f64,
) -> Option<Rect> {
    NSScreen::screens(mtm)
        .iter()
        .map(|s| (Rect::from_appkit(s.frame(), primary_height), s.visibleFrame()))
        .map(|(frame, visible)| (frame.overlap(rect), visible))
        .filter(|(overlap, _)| *overlap > 0)
        .max_by_key(|(overlap, _)| *overlap)
        .map(|(_, visible)| Rect::from_appkit(visible, primary_height))
}

/// Height of the screen holding the menu bar, origin of both coordinate
/// systems.
pub(super) fn primary_height(mtm: MainThreadMarker) -> Option<f64> {
    NSScreen::screens(mtm).firstObject().map(|s| s.frame().size.height)
}

/// Keeps the full width and the title strip of a `width`-wide window at
/// `pos` inside `work`.
fn clamp_into(work: Rect, pos: (i32, i32), width: i32) -> (i32, i32) {
    let x = pos.0.clamp(work.left, (work.right - width).max(work.left));
    let y = pos.1.clamp(work.top, (work.bottom - TITLE_STRIP).max(work.top));
    (x, y)
}

#[cfg(test)]
mod tests {
    use objc2_foundation::NSSize;

    use super::*;

    /// MacBook screen, 1512x982 points; visible area below the 33 pt menu
    /// bar and above a 70 pt Dock.
    const WORK: Rect = Rect { left: 0, top: 33, right: 1512, bottom: 912 };

    #[test]
    fn appkit_frames_flip_to_y_downwards() {
        let screen = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1512.0, 982.0));
        let window = NSRect::new(NSPoint::new(586.0, 176.0), NSSize::new(340.0, 455.0));
        assert_eq!(Rect::from_appkit(screen, 982.0), Rect { left: 0, top: 0, right: 1512, bottom: 982 });
        assert_eq!(Rect::from_appkit(window, 982.0).top, 351);
        // A second screen above the primary one has a negative top.
        let above = NSRect::new(NSPoint::new(0.0, 982.0), NSSize::new(1920.0, 1080.0));
        assert_eq!(Rect::from_appkit(above, 982.0).top, -1080);
    }

    #[test]
    fn position_inside_the_visible_area_is_kept() {
        assert_eq!(clamp_into(WORK, (586, 351), 340), (586, 351));
    }

    #[test]
    fn title_bar_under_the_menu_bar_or_the_dock_is_pulled_back() {
        assert_eq!(clamp_into(WORK, (586, 10), 340), (586, 33));
        assert_eq!(clamp_into(WORK, (1400, 900), 340), (1172, 876));
    }

    #[test]
    fn the_screen_showing_most_of_the_title_bar_wins() {
        let left = Rect { left: -1920, top: 0, right: 0, bottom: 1080 };
        let strip = Rect { left: -100, top: 200, right: 240, bottom: 236 };
        assert!(WORK.overlap(strip) > left.overlap(strip));
        let gone = Rect { left: 5000, top: 200, right: 5340, bottom: 236 };
        assert_eq!(WORK.overlap(gone), 0);
    }
}
