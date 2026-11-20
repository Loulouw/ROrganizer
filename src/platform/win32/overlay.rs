//! Windows side of the character overlay: the windows egui creates for the
//! overlay viewports, stripped of their frame, made non-activating (bar,
//! tooltip) and placed in physical pixels.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_DONOTROUND,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MonitorFromRect, MonitorFromWindow, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetCursorPos, GetWindowLongPtrW, GetWindowRect, GetWindowTextW,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE, STYLESTRUCT,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WM_MOUSEACTIVATE,
    WM_NCACTIVATE, WM_NCCALCSIZE, WM_NCDESTROY, WM_NCPAINT, WM_STYLECHANGING, WS_CAPTION,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
    WS_THICKFRAME,
};

use crate::overlay::ScreenRect;
use crate::platform::OverlayWindow;

/// The caption winit keeps on undecorated windows (for Aero Snap): DWM
/// draws its top edge and a ghost close button across a transparent one.
const FRAME: u32 =
    WS_CAPTION.0 | WS_SYSMENU.0 | WS_THICKFRAME.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0;
const SUBCLASS_ID: usize = 0x524F_4F56; // "ROOV"
const MA_NOACTIVATE: isize = 3;

impl OverlayWindow {
    fn hwnd(self) -> HWND {
        HWND(self.0 as *mut _)
    }
}

pub fn attach(title: &str, activates: bool) -> Option<OverlayWindow> {
    let hwnd = find_thread_window(title)?;
    // Out of the taskbar and Alt-Tab; never activated by a click unless
    // `activates`.
    let ex_style = if activates {
        WS_EX_TOOLWINDOW.0
    } else {
        WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0
    };
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | ex_style as isize);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_STYLE, ((style & !FRAME) | WS_POPUP.0) as isize);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        // winit writes the whole styles back on every change of its own
        // flags (visibility, level…), dropping ours: a hidden then shown bar
        // would activate on the next click. Keep them at the source.
        let _ = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, ex_style as usize);
        // Windows 11 draws a 1 px border and rounds the corners of every
        // top-level window; the bar draws its own. Fails harmlessly on 10.
        let none = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            (&raw const none).cast(),
            size_of_val(&none) as u32,
        );
        let corner = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&raw const corner).cast(),
            size_of_val(&corner) as u32,
        );
    }
    Some(OverlayWindow(hwnd.0 as isize))
}

/// `ex_style`: the extended style bits to keep (`attach`).
unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    ex_style: usize,
) -> LRESULT {
    let ex_style = ex_style as u32;
    unsafe {
        match msg {
            WM_STYLECHANGING if wparam.0 as i32 == GWL_EXSTYLE.0 => {
                let change = &mut *(lparam.0 as *mut STYLESTRUCT);
                change.styleNew |= ex_style;
            }
            WM_STYLECHANGING if wparam.0 as i32 == GWL_STYLE.0 => {
                let change = &mut *(lparam.0 as *mut STYLESTRUCT);
                change.styleNew = (change.styleNew & !FRAME) | WS_POPUP.0;
            }
            WM_MOUSEACTIVATE if ex_style & WS_EX_NOACTIVATE.0 != 0 => {
                return LRESULT(MA_NOACTIVATE);
            }
            // Client area = whole window. winit shifts an undecorated
            // window's client area 1 px down for its drop shadow, leaving a
            // line across the top of a transparent one.
            WM_NCCALCSIZE if wparam.0 != 0 => return LRESULT(0),
            // No frame to paint. On activation the default handler still
            // drew a classic caption, with the window title, over the bar.
            WM_NCPAINT => return LRESULT(0),
            WM_NCACTIVATE => return DefSubclassProc(hwnd, msg, wparam, LPARAM(-1)),
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

/// Window of the calling thread whose title is `title`. egui gives no
/// handle on the windows of its secondary viewports; the title is ours and
/// the search never leaves our own UI thread.
fn find_thread_window(title: &str) -> Option<HWND> {
    struct Search {
        title: Vec<u16>,
        found: Option<HWND>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        let mut buf = [0u16; 128];
        let len = unsafe { GetWindowTextW(hwnd, &mut buf) } as usize;
        if buf[..len] == search.title[..] {
            search.found = Some(hwnd);
            return false.into();
        }
        true.into()
    }
    let mut search = Search {
        title: title.encode_utf16().collect(),
        found: None,
    };
    unsafe {
        let _ = EnumThreadWindows(
            GetCurrentThreadId(),
            Some(visit),
            LPARAM(&raw mut search as isize),
        );
    }
    search.found
}

pub fn rect(window: OverlayWindow) -> Option<ScreenRect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(window.hwnd(), &mut r) }.ok()?;
    Some((r.left, r.top, r.right, r.bottom))
}

pub fn move_to(window: OverlayWindow, pos: (i32, i32)) {
    unsafe {
        let _ = SetWindowPos(
            window.hwnd(),
            None,
            pos.0,
            pos.1,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn monitor_work(monitor: HMONITOR) -> Option<ScreenRect> {
    if monitor.is_invalid() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetMonitorInfoW(monitor, &mut info) }.ok().ok()?;
    let w = info.rcWork;
    Some((w.left, w.top, w.right, w.bottom))
}

pub fn work_area(window: OverlayWindow) -> Option<ScreenRect> {
    monitor_work(unsafe { MonitorFromWindow(window.hwnd(), MONITOR_DEFAULTTONEAREST) })
}

pub fn work_area_of(rect: ScreenRect) -> Option<ScreenRect> {
    let r = RECT {
        left: rect.0,
        top: rect.1,
        right: rect.2,
        bottom: rect.3,
    };
    monitor_work(unsafe { MonitorFromRect(&r, MONITOR_DEFAULTTONULL) })
}

pub fn main_work_area() -> Option<ScreenRect> {
    monitor_work(unsafe { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) })
}

pub fn cursor_position() -> (i32, i32) {
    let mut p = POINT::default();
    let _ = unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
}

/// Our process just got the click that opens the menu, so the foreground
/// lock lets it through.
pub fn activate(window: OverlayWindow) {
    unsafe {
        let _ = SetForegroundWindow(window.hwnd());
    }
}
