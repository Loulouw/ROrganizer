//! The character overlay is not built on macOS yet: `HAS_OVERLAY` is false,
//! so the settings never offer it and nothing below is ever called. The
//! macOS version will be a native non-activating `NSPanel` (an egui window
//! activates the app on click and can't join a fullscreen Space), driven by
//! the same `crate::overlay::State`.

use crate::overlay::ScreenRect;
use crate::platform::OverlayWindow;

pub fn attach(_title: &str, _activates: bool) -> Option<OverlayWindow> {
    None
}

pub fn work_area_of(_rect: ScreenRect) -> Option<ScreenRect> {
    None
}

pub fn main_work_area() -> Option<ScreenRect> {
    None
}

pub fn rect(_window: OverlayWindow) -> Option<ScreenRect> {
    None
}

pub fn work_area(_window: OverlayWindow) -> Option<ScreenRect> {
    None
}

pub fn move_to(_window: OverlayWindow, _pos: (i32, i32)) {}

pub fn cursor_position() -> (i32, i32) {
    (0, 0)
}

pub fn activate(_window: OverlayWindow) {}
