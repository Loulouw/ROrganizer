use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use super::state::HOOK_STATE;
use crate::triggers::Trigger;

pub unsafe extern "system" fn kb_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let msg = wparam.0 as u32;
    let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let vk = info.vkCode;

    // Lock briefly to: log + lookup matching binding.
    // Lock released before focus_window to avoid holding it during Win32 calls.
    let mut focus_target: Option<isize> = None;
    if let Some(state_mut) = HOOK_STATE.get() {
        if let Ok(state) = state_mut.lock() {
            let _ = state.log_tx.send(format!(
                "kb {} vk=0x{:02X}",
                if is_down { "down" } else { "up  " },
                vk
            ));
            if is_down {
                let trigger = Trigger::Key(vk);
                if let Some((_, hwnd)) = state.bindings.iter().find(|(t, _)| *t == trigger) {
                    focus_target = Some(*hwnd);
                }
            }
        }
    }

    if let Some(hwnd) = focus_target {
        crate::win::focus_window(hwnd);
        return LRESULT(1);
    }

    CallNextHookEx(None, code, wparam, lparam)
}
