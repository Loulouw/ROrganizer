pub mod enumerate;
pub mod focus;
pub mod keyboard_layout;
pub mod placement;
pub mod process;
pub mod taskbar;
pub mod watcher;

pub use enumerate::{DetectedWindow, DEFAULT_TITLE_REGEX};
pub use focus::{focus_window, foreground_window};
pub use watcher::{is_dofus_window, spawn_watcher, WindowsSnapshot};
