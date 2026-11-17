//! Position of the main window across launches, in physical screen pixels.

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONULL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsIconic, IsWindowVisible, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_NOZORDER,
};

/// Height of the strip, from the top edge, that must stay on a screen so
/// the window can still be dragged by its title bar.
const TITLE_STRIP: i32 = 36;

/// Top-left corner of `hwnd`. None while it is minimized or hidden: Windows
/// then reports a parking spot, not where the user left it.
pub fn position(hwnd: isize) -> Option<(i32, i32)> {
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        if IsIconic(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
            return None;
        }
        let mut r = RECT::default();
        GetWindowRect(hwnd, &mut r).ok()?;
        Some((r.left, r.top))
    }
}

/// Moves `hwnd` to `pos`, pulled inside the work area of the screen showing
/// its title bar. Nothing happens when no screen shows it (screen unplugged
/// since the last launch): the window keeps its default, centred spot.
pub fn restore(hwnd: isize, pos: (i32, i32)) {
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return;
        }
        let width = r.right - r.left;
        let strip = RECT {
            left: pos.0,
            top: pos.1,
            right: pos.0 + width,
            bottom: pos.1 + TITLE_STRIP,
        };
        let monitor = MonitorFromRect(&strip, MONITOR_DEFAULTTONULL);
        if monitor.is_invalid() {
            return;
        }
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let (x, y) = clamp_into(info.rcWork, pos, width);
        let _ = SetWindowPos(
            hwnd,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Keeps the full width and the title strip of a `width`-wide window at
/// `pos` inside `work`.
fn clamp_into(work: RECT, pos: (i32, i32), width: i32) -> (i32, i32) {
    let x = pos.0.clamp(work.left, (work.right - width).max(work.left));
    let y = pos.1.clamp(work.top, (work.bottom - TITLE_STRIP).max(work.top));
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: RECT = RECT { left: -1920, top: 0, right: 0, bottom: 1040 };

    #[test]
    fn position_inside_the_work_area_is_kept() {
        assert_eq!(clamp_into(WORK, (-1500, 300), 340), (-1500, 300));
    }

    #[test]
    fn window_straddling_an_edge_is_pulled_back() {
        // Half past the right edge, title bar under the taskbar.
        assert_eq!(clamp_into(WORK, (-170, 1030), 340), (-340, 1004));
        // Above the top edge: the title bar would be unreachable.
        assert_eq!(clamp_into(WORK, (-1500, -20), 340), (-1500, 0));
    }

    #[test]
    fn work_area_narrower_than_the_window_pins_it_left() {
        let narrow = RECT { left: 0, top: 0, right: 200, bottom: 1040 };
        assert_eq!(clamp_into(narrow, (50, 10), 340), (0, 10));
    }
}
