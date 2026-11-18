use std::sync::{Mutex, OnceLock};

use dispatch2::{run_on_main, DispatchQueue};
use eframe::egui;
use objc2::rc::Retained;
use objc2::runtime::NSObjectProtocol;
use objc2::{msg_send, sel, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSView, NSWindow};
use objc2_foundation::NSPoint;
use raw_window_handle::RawWindowHandle;

use crate::platform::MainWindow;

impl MainWindow {
    pub fn from_raw_handle(handle: RawWindowHandle) -> Option<Self> {
        match handle {
            RawWindowHandle::AppKit(h) => Some(Self(h.ns_view.as_ptr() as usize)),
            _ => None,
        }
    }

    /// AppKit objects may only be touched on the main thread, hence the
    /// marker.
    pub(super) fn ns_window(self, _mtm: MainThreadMarker) -> Option<Retained<NSWindow>> {
        // SAFETY: winit's content view of the main window, which lives until
        // the process exits.
        let view = unsafe { &*(self.0 as *const NSView) };
        view.window()
    }
}

/// Captured during App::new so the tray handlers and the second-launch
/// waiter can drive the window while the eframe loop may be parked.
static MAIN_WINDOW: OnceLock<MainWindow> = OnceLock::new();

pub fn register_main_window(window: MainWindow) {
    let _ = MAIN_WINDOW.set(window);
}

pub fn show_main_window() {
    let Some(&window) = MAIN_WINDOW.get() else { return };
    run_on_main(move |mtm| {
        let Some(ns_window) = window.ns_window(mtm) else { return };
        if ns_window.isMiniaturized() {
            ns_window.deminiaturize(None);
        }
        ns_window.makeKeyAndOrderFront(None);
        activate(&NSApplication::sharedApplication(mtm));
    });
}

pub fn close_main_window() {
    let Some(&window) = MAIN_WINDOW.get() else { return };
    run_on_main(move |mtm| {
        let Some(ns_window) = window.ns_window(mtm) else { return };
        // The window is borderless, so `performClose:` would only beep.
        // winit's delegate turns `windowShouldClose:` into CloseRequested:
        // eframe then shuts down normally and `on_exit` saves the config.
        if let Some(delegate) = ns_window.delegate() {
            let _: bool = unsafe { msg_send![&*delegate, windowShouldClose: &*ns_window] };
        }
    });
}

/// Miniaturizes the window natively instead of through
/// `ViewportCommand::Minimized`: egui-winit never re-reads the minimized
/// state on macOS (deadlock workaround, egui#3494), so after that command a
/// restore from the Dock leaves egui believing the window is still
/// minimized. eframe then stops running `App::ui`: the UI freezes and the
/// tray events are never drained.
///
/// Queued on the main thread rather than run now: the viewport commands of
/// the current frame apply after it returns, and the tray toggle's resync
/// `Visible(true)` would bring the window straight back.
pub fn minimize_main_window(_ctx: &egui::Context) {
    let Some(&window) = MAIN_WINDOW.get() else { return };
    DispatchQueue::main().exec_async(move || {
        let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
        if let Some(ns_window) = window.ns_window(mtm) {
            ns_window.miniaturize(None);
        }
    });
}

/// Top-left corner of the main window when it was hidden in the menu bar,
/// in AppKit screen coordinates.
///
/// The app resizes the window in the very frame that hides it (the active
/// layout is shorter). AppKit keeps the bottom-left corner of a hidden
/// window fixed, so without this the window would creep down by the height
/// difference on every hide.
static HIDDEN_TOP_LEFT: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// Hidden in the menu bar: no Dock icon, out of Cmd-Tab.
pub fn main_window_hidden_to_tray() {
    let window = MAIN_WINDOW.get().copied();
    run_on_main(move |mtm| {
        if let Some(ns_window) = window.and_then(|w| w.ns_window(mtm)) {
            let frame = ns_window.frame();
            let top_left = (frame.origin.x, frame.origin.y + frame.size.height);
            *HIDDEN_TOP_LEFT.lock().unwrap_or_else(|e| e.into_inner()) = Some(top_left);
        }
        NSApplication::sharedApplication(mtm)
            .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    });
}

pub fn main_window_restored() {
    let window = MAIN_WINDOW.get().copied();
    run_on_main(move |mtm| {
        let top_left = HIDDEN_TOP_LEFT.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let (Some((x, y)), Some(ns_window)) = (top_left, window.and_then(|w| w.ns_window(mtm))) {
            ns_window.setFrameTopLeftPoint(NSPoint::new(x, y));
        }
        NSApplication::sharedApplication(mtm)
            .setActivationPolicy(NSApplicationActivationPolicy::Regular);
    });
}

fn activate(app: &NSApplication) {
    // `activate` only exists from macOS 14; `activateIgnoringOtherApps:`,
    // deprecated since, covers the systems before.
    if app.respondsToSelector(sel!(activate)) {
        app.activate();
    } else {
        let _: () = unsafe { msg_send![app, activateIgnoringOtherApps: true] };
    }
}
