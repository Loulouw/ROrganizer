use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_F12;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use super::state::HOOK_STATE;

pub unsafe extern "system" fn kb_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let msg = wparam.0 as u32;
    let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let vk = info.vkCode;

    // Step 1 (lock briefly): log the event and snapshot the focus target.
    // We deliberately do NOT hold the HOOK_STATE lock across focus_window
    // — Win32 input calls can re-enter the hook on the same thread.
    let mut focus_target: Option<isize> = None;
    if let Some(state_mut) = HOOK_STATE.get() {
        if let Ok(state) = state_mut.lock() {
            let _ = state.log_tx.send(format!(
                "kb {} vk=0x{:02X}",
                if is_down { "down" } else { "up  " },
                vk
            ));
            if is_down && vk == VK_F12.0 as u32 {
                if let Ok(snap) = state.windows.lock() {
                    if let Some(first) = snap.first() {
                        let _ = state.log_tx.send(format!(
                            "F12: focus -> {} (hwnd={})",
                            first.slot_key, first.hwnd
                        ));
                        focus_target = Some(first.hwnd);
                    } else {
                        let _ = state.log_tx.send("F12: no windows".to_string());
                    }
                }
            }
        }
    }

    // Step 2 (no lock held): perform the focus and swallow the F12.
    if let Some(hwnd) = focus_target {
        crate::win::focus_window(hwnd);
        return LRESULT(1);
    }

    CallNextHookEx(None, code, wparam, lparam)
}
