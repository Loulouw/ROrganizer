//! The Dock shows one button per app: there is no per-window order to
//! change. `HAS_TASKBAR_SORT` is false, so the UI never asks.

use crate::platform::WindowId;

pub fn is_sorting() -> bool {
    false
}

pub fn reorder(_windows: Vec<WindowId>, on_done: impl FnOnce() + Send + 'static) {
    on_done();
}
