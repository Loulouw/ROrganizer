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
//! - `TRAY_IS_MENU_BAR: bool`: the tray is the macOS menu bar and the
//!   taskbar is the Dock; the UI labels name them accordingly.
//! - `WINDOW_ICON_PNG: &[u8]`, `WINDOW_ICON_SIZE: u32`: the window icon,
//!   decoded and scaled to that side at startup.
//! - Detection: `spawn_watcher(WindowsSnapshot, egui::Context, String,
//!   Receiver<()>)` publishes the windows matching the title regex, and
//!   calls `crate::overlay::foreground_changed` each time another app or
//!   window comes to the front;
//!   `is_dofus_window(WindowId) -> bool` also accepts a client that is not
//!   logged in yet.
//! - Focus: `focus_window(WindowId)`, `foreground_window() ->
//!   Option<WindowId>`, `is_own_window(WindowId) -> bool` (a window of
//!   ROrganizer itself: main window, overlay, menus).
//! - Global input: `spawn_hooks()` starts the system-wide keyboard and
//!   mouse listeners. They stay inert while `hooks::is_enabled()` is false,
//!   and ask `hooks::focus_target(Trigger)` which window to bring forward.
//! - Keys: `Trigger::Key` holds a platform-native key code.
//!   `key_from_event(egui::Key, Option<egui::Key>) -> Option<u32>` converts
//!   an egui key press into it, `key_label(u32) -> String` displays it.
//! - Main window: `register_main_window(MainWindow)` once at startup, then
//!   `show_main_window()` / `close_main_window()` from the tray handlers and
//!   the second-launch waiter, which run while the eframe loop may be
//!   parked. `placement::{position(MainWindow) -> Option<(i32, i32)>,
//!   restore(MainWindow, (i32, i32))}` persist its position.
//!   `minimize_main_window(&egui::Context)` minimizes it to the taskbar.
//! - Tray presence: `main_window_hidden_to_tray()` is called when the main
//!   window is hidden into the tray, never when it is minimized.
//!   `main_window_restored()` is called before every path that shows it
//!   again, including the `show_main_window()` callers; it can arrive
//!   without a prior hide. Both may run off the main thread (the waiter
//!   has its own), and before the window actually changes: viewport
//!   commands apply at the end of the frame. The app starts restored;
//!   `close_main_window()` needs neither, the app exits. Windows: no-ops,
//!   the taskbar button already follows the window's visibility. macOS:
//!   switch the activation policy, `.accessory` when hidden (no Dock icon,
//!   out of Cmd-Tab), `.regular` when restored.
//! - Input permission: `input_permission_granted() -> bool` is read every
//!   frame, so it must stay cheap. While false, detection, global input and
//!   focus can't work: the main view shows a banner whose button calls
//!   `request_input_permission()`, which opens the system prompt. The
//!   permission can change while the app runs; the backend then requests
//!   a repaint (the watcher's `egui::Context`) so the banner follows.
//!   Windows has no such gate: always true, and the request is never called.
//! - `taskbar::is_sorting() -> bool`, `taskbar::reorder(Vec<WindowId>,
//!   impl FnOnce() + Send + 'static)`.
//! - Single instance: `single_instance::acquire_or_signal_existing() ->
//!   AcquireResult` (`First(SingleInstanceGuard)` or `SignalledExisting`),
//!   then `single_instance::spawn_waiter(SingleInstanceGuard, impl Fn() +
//!   Send + 'static)` calls back each time another launch asks to be shown.
//! - `detect_system_lang() -> Lang`, `open_folder(&Path)`.
//! - Character overlay: `HAS_OVERLAY: bool`; the option and the overlay
//!   viewports exist only when true, but `overlay::*` must exist either way.
//!   [`OverlayWindow`] is a window egui created for an overlay viewport.
//!   Coordinates are screen coordinates (Windows: physical pixels).
//!   - `overlay::attach(&str, bool) -> Option<OverlayWindow>`, called from
//!     an overlay viewport's ui callback: finds the window titled so on the
//!     UI thread, strips its frame and keeps it out of the taskbar; unless
//!     the flag says it activates (the menu), a click never activates it,
//!     for good (winit rewrites the window styles on every change of its
//!     own). A click that activated ROrganizer would blind the global input
//!     hooks until Dofus is back in front.
//!   - `overlay::activate(OverlayWindow)`: brings the menu in front, right
//!     after the click that opened it.
//!   - `overlay::rect(OverlayWindow) -> Option<ScreenRect>`,
//!     `overlay::move_to(OverlayWindow, (i32, i32))` (never activates),
//!     `overlay::cursor_position() -> (i32, i32)`.
//!   - Work areas (screen minus taskbar/Dock): `overlay::work_area(
//!     OverlayWindow)` of its screen, `overlay::work_area_of(ScreenRect)`
//!     of the screen showing that rectangle (None when none does),
//!     `overlay::main_work_area()`; all `-> Option<ScreenRect>`.
//!     `crate::overlay::{place, keep_on_screen}` build on them.
//!   - The right-click menu is egui too (`ui::overlay`): it takes the focus
//!     so that a click elsewhere closes it, then gives the foreground back.

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(any(windows, target_os = "macos")))]
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

/// A window egui created for an overlay viewport (`crate::overlay`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayWindow(RawOverlayWindow);
