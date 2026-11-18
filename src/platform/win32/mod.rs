mod enumerate;
mod focus;
mod hooks;
mod keyboard_layout;
mod keys;
mod locale;
mod main_window;
pub mod placement;
mod process;
mod shell;
pub mod single_instance;
pub mod taskbar;
mod watcher;

use windows::Win32::Foundation::HWND;

use super::WindowId;

pub use focus::{focus_window, foreground_window};
pub use hooks::spawn_hooks;
pub use keys::{key_from_event, key_label};
pub use locale::detect_system_lang;
pub use main_window::{
    close_main_window, main_window_hidden_to_tray, main_window_restored, minimize_main_window,
    register_main_window, show_main_window,
};
pub use shell::open_folder;
pub use watcher::{is_dofus_window, spawn_watcher};

pub const HAS_TASKBAR_SORT: bool = true;
pub const TRAY_IS_MENU_BAR: bool = false;
pub const WINDOW_ICON_PNG: &[u8] = crate::assets::ICON_PNG;
pub const WINDOW_ICON_SIZE: u32 = 64;

/// Reading other windows and installing LL hooks need no per-app grant.
pub fn input_permission_granted() -> bool {
    true
}

/// Never called: the permission is always granted.
pub fn request_input_permission() {}

/// HWND, as an integer.
pub(super) type RawWindowId = isize;
/// HWND, as an integer.
pub(super) type RawMainWindow = isize;

impl WindowId {
    fn from_hwnd(hwnd: HWND) -> Self {
        Self(hwnd.0 as isize)
    }

    fn hwnd(self) -> HWND {
        HWND(self.0 as *mut _)
    }
}
