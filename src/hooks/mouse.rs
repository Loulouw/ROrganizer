use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, MSLLHOOKSTRUCT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use super::state::HOOK_STATE;

pub unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
    let msg = wparam.0 as u32;

    if let Some(state_mut) = HOOK_STATE.get() {
        if let Ok(state) = state_mut.lock() {
            let line = match msg {
                WM_LBUTTONDOWN => Some("mouse left  down".to_string()),
                WM_LBUTTONUP => Some("mouse left  up  ".to_string()),
                WM_RBUTTONDOWN => Some("mouse right down".to_string()),
                WM_RBUTTONUP => Some("mouse right up  ".to_string()),
                WM_MBUTTONDOWN => Some("mouse mid   down".to_string()),
                WM_MBUTTONUP => Some("mouse mid   up  ".to_string()),
                WM_XBUTTONDOWN => {
                    let xb = (info.mouseData >> 16) as u16;
                    Some(format!("mouse x{xb}    down"))
                }
                WM_XBUTTONUP => {
                    let xb = (info.mouseData >> 16) as u16;
                    Some(format!("mouse x{xb}    up  "))
                }
                WM_MOUSEWHEEL => {
                    let delta = (info.mouseData >> 16) as i16;
                    Some(format!("mouse wheel delta={delta}"))
                }
                _ => None,
            };
            if let Some(l) = line {
                let _ = state.log_tx.send(l);
            }
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}
