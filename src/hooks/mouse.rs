use std::sync::atomic::Ordering;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, MSLLHOOKSTRUCT, WM_MBUTTONDOWN, WM_MOUSEWHEEL, WM_XBUTTONDOWN,
};

use super::keyboard::resolve_action;
use super::state::{HOOK_ENABLED, HOOK_STATE};
use crate::triggers::{MouseBtn, Trigger, WheelDir};

pub unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    if !HOOK_ENABLED.load(Ordering::Relaxed) {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let msg = wparam.0 as u32;
    let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);

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

    let Some(t) = trigger else {
        return CallNextHookEx(None, code, wparam, lparam);
    };

    let focus_target = HOOK_STATE.get().and_then(|m| {
        let mut state = m.lock().ok()?;
        resolve_action(&mut state, t)
    });

    if let Some(hwnd) = focus_target {
        crate::win::focus_window(hwnd);
        return LRESULT(1);
    }

    CallNextHookEx(None, code, wparam, lparam)
}
