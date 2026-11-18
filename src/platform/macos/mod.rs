//! macOS backend. Everything that reads or drives another app goes through
//! the Accessibility API and the event tap, both gated by the Accessibility
//! grant (`permission`).

mod ax;
mod focus;
mod hooks;
mod keys;
mod locale;
mod main_window;
mod permission;
pub mod placement;
mod shell;
pub mod single_instance;
pub mod taskbar;
mod watcher;

pub use focus::{focus_window, foreground_window};
pub use hooks::spawn_hooks;
pub use keys::{key_from_event, key_label};
pub use locale::detect_system_lang;
pub use main_window::{
    close_main_window, main_window_hidden_to_tray, main_window_restored, minimize_main_window,
    register_main_window, show_main_window,
};
pub use permission::{input_permission_granted, request_input_permission};
pub use shell::open_folder;
pub use watcher::{is_dofus_window, spawn_watcher};

/// The Dock has one button per app, not per window: nothing to sort.
pub const HAS_TASKBAR_SORT: bool = false;
pub const TRAY_IS_MENU_BAR: bool = true;

/// Process id of the Dofus client: one client, one game window.
pub(super) type RawWindowId = i32;
/// Address of the `NSView` winit hosts the main window in.
pub(super) type RawMainWindow = usize;
