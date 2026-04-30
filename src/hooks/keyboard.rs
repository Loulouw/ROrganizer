use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use super::state::{BindingAction, HOOK_STATE};
use crate::triggers::Trigger;

pub unsafe extern "system" fn kb_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let msg = wparam.0 as u32;
    let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let vk = info.vkCode;

    let mut focus_target: Option<isize> = None;
    if let Some(state_mut) = HOOK_STATE.get() {
        if let Ok(mut state) = state_mut.lock() {
            let _ = state.log_tx.send(format!(
                "kb {} vk=0x{:02X}{}",
                if is_down { "down" } else { "up  " },
                vk,
                if state.enabled { "" } else { " [disabled]" }
            ));
            if state.enabled && is_down {
                let trigger = Trigger::Key(vk);
                focus_target = resolve_action(&mut state, trigger);
            }
        }
    }

    if let Some(hwnd) = focus_target {
        crate::win::focus_window(hwnd);
        return LRESULT(1);
    }

    CallNextHookEx(None, code, wparam, lparam)
}

/// Looks up the trigger against the binding table and returns the HWND to
/// focus, advancing the cycler cursor as needed. Caller still holds the lock.
pub(super) fn resolve_action(
    state: &mut super::state::HookState,
    trigger: Trigger,
) -> Option<isize> {
    let action = state
        .bindings
        .iter()
        .find(|(t, _)| *t == trigger)
        .map(|(_, a)| *a)?;
    let target = match action {
        BindingAction::Focus(h) => {
            // A direct focus binding aligns the cycler on its target so the
            // next CycleNext from there continues the user's expected flow.
            if let Some(pos) = state.cycle_hwnds.iter().position(|&x| x == h) {
                state.cycle_index = pos;
            }
            Some(h)
        }
        BindingAction::CycleNext => {
            let len = state.cycle_hwnds.len();
            if len == 0 {
                return None;
            }
            state.cycle_index = (state.cycle_index + 1) % len;
            Some(state.cycle_hwnds[state.cycle_index])
        }
        BindingAction::CyclePrev => {
            let len = state.cycle_hwnds.len();
            if len == 0 {
                return None;
            }
            state.cycle_index = (state.cycle_index + len - 1) % len;
            Some(state.cycle_hwnds[state.cycle_index])
        }
    };
    if let Some(h) = target {
        state.cycle_current_hwnd = Some(h);
    }
    target
}
