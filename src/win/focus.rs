use windows::Win32::Foundation::{BOOL, HWND};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetWindowThreadProcessId, IsIconic, SetForegroundWindow, ShowWindow,
    SW_RESTORE,
};

/// Bring the Win32 window owning `hwnd_raw` to the foreground.
///
/// Uses the AttachThreadInput dance because a non-foreground process is otherwise
/// blocked from calling `SetForegroundWindow`. Best-effort: silently does nothing
/// if the HWND is no longer valid. Must be called on a thread whose process is
/// either foreground or recently received user input — invocations from a global
/// LL hook context may silently no-op due to the Windows foreground lock-out.
/// The phase-5 hard-coded F12 trigger is therefore a smoke-test only; real
/// user-bound focus actions will be wired through the UI thread in phase 7.
pub fn focus_window(hwnd_raw: isize) {
    if hwnd_raw == 0 {
        return;
    }
    let hwnd = HWND(hwnd_raw as *mut _);
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }

        let target_tid = GetWindowThreadProcessId(hwnd, None);
        if target_tid == 0 {
            return;
        }
        let self_tid = GetCurrentThreadId();

        let attached = AttachThreadInput(self_tid, target_tid, BOOL(1)).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);
        if attached {
            let _ = AttachThreadInput(self_tid, target_tid, BOOL(0));
        }
    }
}
