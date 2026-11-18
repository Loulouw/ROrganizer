use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN,
    WM_MBUTTONDOWN, WM_MOUSEWHEEL, WM_SYSKEYDOWN, WM_XBUTTONDOWN,
};

use super::focus_window;
use crate::hooks;
use crate::triggers::{MouseBtn, Trigger, WheelDir};

pub fn spawn_hooks() {
    std::thread::Builder::new()
        .name("rorg-hooks".into())
        .spawn(run)
        .expect("hooks thread");
}

fn run() {
    unsafe {
        let _kb = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(kb_proc),
            None,
            0,
        )
        .expect("install kb hook");
        let _mse = SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(mouse_proc),
            None,
            0,
        )
        .expect("install mouse hook");

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn kb_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT { unsafe {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    // Hot-path early-out: lock-free atomic check before touching the mutex.
    if !hooks::is_enabled() {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let msg = wparam.0 as u32;
    if msg != WM_KEYDOWN && msg != WM_SYSKEYDOWN {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let vk = info.vkCode;

    if let Some(target) = hooks::focus_target(Trigger::Key(vk)) {
        focus_window(target);
        return LRESULT(1);
    }

    CallNextHookEx(None, code, wparam, lparam)
}}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT { unsafe {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    if !hooks::is_enabled() {
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

    if let Some(target) = hooks::focus_target(t) {
        focus_window(target);
        return LRESULT(1);
    }

    CallNextHookEx(None, code, wparam, lparam)
}}
