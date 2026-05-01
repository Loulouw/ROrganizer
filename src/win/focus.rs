use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
    SetForegroundWindow, ShowWindow, SW_RESTORE,
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

        // Attach the calling thread to BOTH the current foreground window's
        // thread AND the target thread. Sharing the input state with the
        // current foreground bypasses the SetForegroundWindow lock-out
        // (which otherwise blocks any process that didn't recently receive
        // user input), and attaching to the target lets SetFocus / cursor
        // ride along.
        let fg = GetForegroundWindow();
        let fg_tid = if fg.0.is_null() {
            0
        } else {
            GetWindowThreadProcessId(fg, None)
        };
        let attach_fg = fg_tid != 0 && fg_tid != self_tid && fg_tid != target_tid;
        if attach_fg {
            let _ = AttachThreadInput(self_tid, fg_tid, true);
        }
        let attach_target = target_tid != self_tid;
        if attach_target {
            let _ = AttachThreadInput(self_tid, target_tid, true);
        }

        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));

        if attach_target {
            let _ = AttachThreadInput(self_tid, target_tid, false);
        }
        if attach_fg {
            let _ = AttachThreadInput(self_tid, fg_tid, false);
        }
    }
}
