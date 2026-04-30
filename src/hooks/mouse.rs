use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, MSLLHOOKSTRUCT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use super::state::HOOK_STATE;
use crate::triggers::{MouseBtn, Trigger, WheelDir};

pub unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
    let msg = wparam.0 as u32;

    // Resolve to a Trigger for "down"-style events. Up events are ignored
    // for matching (we fire on down to mirror keyboard behavior).
    let trigger: Option<Trigger> = match msg {
        WM_MBUTTONDOWN => Some(Trigger::Mouse(MouseBtn::Middle)),
        WM_XBUTTONDOWN => {
            let xb = (info.mouseData >> 16) as u16;
            match xb {
                1 => Some(Trigger::Mouse(MouseBtn::X1)),
                2 => Some(Trigger::Mouse(MouseBtn::X2)),
                _ => None,
            }
        }
        WM_MOUSEWHEEL => {
            let delta = (info.mouseData >> 16) as i16;
            if delta > 0 {
                Some(Trigger::Wheel(WheelDir::Up))
            } else if delta < 0 {
                Some(Trigger::Wheel(WheelDir::Down))
            } else {
                None
            }
        }
        _ => None,
    };

    let mut focus_target: Option<isize> = None;
    if let Some(state_mut) = HOOK_STATE.get() {
        if let Ok(state) = state_mut.lock() {
            // Light log of meaningful events
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

            if let Some(t) = trigger {
                if let Some((_, hwnd)) = state.bindings.iter().find(|(b, _)| *b == t) {
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
