//! Everything that talks to the operating system. The rest of the crate is
//! system-agnostic and reaches the OS only through the items below, which
//! each backend provides as free functions with the same signatures, picked
//! at compile time.
//!
//! Contract:
//!
//! - [`WindowId`]: a Dofus client window. [`MainWindow`]: the ROrganizer
//!   window, built by `MainWindow::from_raw_handle(RawWindowHandle) ->
//!   Option<MainWindow>`.
//! - `HAS_TASKBAR_SORT: bool`: the sort button is shown only when true.
//!   `taskbar::{is_sorting, reorder}` must exist either way.
//! - Detection: `spawn_watcher(WindowsSnapshot, egui::Context, String,
//!   Receiver<()>)` publishes the windows matching the title regex;
//!   `is_dofus_window(WindowId) -> bool` also accepts a client that is not
//!   logged in yet.
//! - Focus: `focus_window(WindowId)`, `foreground_window() ->
//!   Option<WindowId>`.
//! - Global input: `spawn_hooks()` starts the system-wide keyboard and
//!   mouse listeners. They stay inert while `hooks::is_enabled()` is false,
//!   and ask `hooks::focus_target(Trigger)` which window to bring forward.
//! - Keys: `Trigger::Key` holds a platform-native key code.
//!   `key_from_event(egui::Key, Option<egui::Key>) -> Option<u32>` converts
//!   an egui key press into it, `key_label(u32) -> String` displays it.
//! - Main window: `register_main_window(MainWindow)` once at startup, then
//!   `show_main_window()` / `close_main_window()` from the tray handlers,
//!   which run while the eframe loop may be parked.
//!   `placement::{position(MainWindow) -> Option<(i32, i32)>,
//!   restore(MainWindow, (i32, i32))}` persist its position.
//! - `taskbar::is_sorting() -> bool`, `taskbar::reorder(Vec<WindowId>,
//!   impl FnOnce() + Send + 'static)`.
//! - Single instance: `single_instance::acquire_or_signal_existing() ->
//!   AcquireResult` (`First(SingleInstanceGuard)` or `SignalledExisting`),
//!   then `single_instance::spawn_waiter(SingleInstanceGuard, impl Fn() +
//!   Send + 'static)` calls back each time another launch asks to be shown.
//! - `detect_system_lang() -> Lang`, `open_folder(&Path)`.

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::*;

#[cfg(not(windows))]
compile_error!("plateforme non supportée");

/// A Dofus client window. Opaque outside `platform`: the raw value only
/// means something to the backend that produced it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct WindowId(RawWindowId);

#[cfg(test)]
impl WindowId {
    /// Distinct ids for tests that must not depend on a system.
    pub fn test(n: i16) -> Self {
        Self(RawWindowId::from(n))
    }
}

/// The ROrganizer window, as opposed to the Dofus windows it manages.
#[derive(Clone, Copy, Debug)]
pub struct MainWindow(RawMainWindow);
