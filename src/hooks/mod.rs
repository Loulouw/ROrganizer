mod keyboard;
mod logger;
mod mouse;
mod runner;
mod state;

use std::sync::Mutex;

use crate::triggers::Trigger;
use crate::win::WindowsSnapshot;

pub use state::BindingAction;

pub fn install(windows: WindowsSnapshot) {
    let log_tx = logger::spawn();
    let _ = log_tx.send(format!(
        "rorganizer hooks started @ {:?}",
        std::time::SystemTime::now()
    ));
    let st = state::HookState {
        windows,
        log_tx,
        bindings: Vec::new(),
        cycle_hwnds: Vec::new(),
        cycle_index: 0,
        cycle_current_hwnd: None,
        enabled: false, // App flips this true via set_enabled when user clicks Activer
    };
    state::HOOK_STATE
        .set(Mutex::new(st))
        .ok()
        .expect("hooks already installed");
    runner::spawn();
}

/// Push the resolved binding table + cycle hwnds. Does NOT touch `cycle_index`
/// beyond clamping to the new range (the hook's cursor is its own state and
/// must survive incidental snapshot updates from the watcher). For explicit
/// cursor changes (manual click / drag), use [`set_cycle_index`].
pub fn set_bindings_and_cycle(
    bindings: Vec<(Trigger, BindingAction)>,
    cycle_hwnds: Vec<isize>,
) {
    if let Some(state_mut) = state::HOOK_STATE.get() {
        if let Ok(mut state) = state_mut.lock() {
            state.bindings = bindings;
            state.cycle_hwnds = cycle_hwnds;
            // Try to keep the cursor on the same account across reorders.
            if let Some(cur) = state.cycle_current_hwnd {
                if let Some(pos) = state.cycle_hwnds.iter().position(|&h| h == cur) {
                    state.cycle_index = pos;
                    return;
                }
            }
            // Fallback: clamp to the new length.
            let new_len = state.cycle_hwnds.len();
            if new_len == 0 {
                state.cycle_index = 0;
            } else if state.cycle_index >= new_len {
                state.cycle_index = new_len - 1;
            }
        }
    }
}

/// Master switch on the hook callbacks. When `false`, the callbacks return
/// `CallNextHookEx` immediately without matching any trigger.
pub fn set_enabled(enabled: bool) {
    if let Some(state_mut) = state::HOOK_STATE.get() {
        if let Ok(mut state) = state_mut.lock() {
            state.enabled = enabled;
        }
    }
}

/// Override the cycler cursor explicitly. Called by App when the user clicks
/// an account row in the UI.
pub fn set_cycle_index(cycle_index: usize) {
    if let Some(state_mut) = state::HOOK_STATE.get() {
        if let Ok(mut state) = state_mut.lock() {
            let len = state.cycle_hwnds.len();
            state.cycle_index = if len == 0 { 0 } else { cycle_index.min(len - 1) };
            state.cycle_current_hwnd = state.cycle_hwnds.get(state.cycle_index).copied();
        }
    }
}
