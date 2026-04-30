pub mod enumerate;
pub mod process;
pub mod watcher;

pub use enumerate::{DetectedWindow, DEFAULT_TITLE_REGEX};
pub use watcher::{spawn_watcher, WindowsSnapshot};
