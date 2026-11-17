pub mod enumerate;
pub mod focus;
pub mod keyboard_layout;
pub mod process;
pub mod watcher;

pub use enumerate::{DetectedWindow, DEFAULT_TITLE_REGEX};
pub use focus::focus_window;
pub use watcher::{spawn_watcher, WindowsSnapshot};
