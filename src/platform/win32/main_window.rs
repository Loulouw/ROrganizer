use std::sync::OnceLock;

use raw_window_handle::RawWindowHandle;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    IsIconic, PostMessageW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW, WM_CLOSE,
};

use crate::platform::MainWindow;

impl MainWindow {
    pub fn from_raw_handle(handle: RawWindowHandle) -> Option<Self> {
        match handle {
            RawWindowHandle::Win32(h) => Some(Self(h.hwnd.get())),
            _ => None,
        }
    }

    pub(super) fn hwnd(self) -> HWND {
        HWND(self.0 as *mut _)
    }
}

/// Main window HWND, captured during App::new so the tray handlers can
/// directly drive ShowWindow / WM_CLOSE while the eframe loop is parked
/// (which happens whenever the viewport is set to Visible(false) — at that
/// point Context::request_repaint() does not wake update()).
static MAIN_HWND: OnceLock<MainWindow> = OnceLock::new();

pub fn register_main_window(window: MainWindow) {
    let _ = MAIN_HWND.set(window);
}

pub fn show_main_window() {
    if let Some(&window) = MAIN_HWND.get() {
        let hwnd = window.hwnd();
        unsafe {
            // Minimized to the taskbar (option): SW_SHOW would leave it
            // iconic.
            let cmd = if IsIconic(hwnd).as_bool() { SW_RESTORE } else { SW_SHOW };
            let _ = ShowWindow(hwnd, cmd);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

pub fn close_main_window() {
    if let Some(&window) = MAIN_HWND.get() {
        let hwnd = window.hwnd();
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}
