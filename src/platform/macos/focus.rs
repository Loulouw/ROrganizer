use crate::platform::WindowId;

/// Inert until detection exists: no Dofus window is ever known.
pub fn foreground_window() -> Option<WindowId> {
    None
}

pub fn focus_window(_id: WindowId) {}
