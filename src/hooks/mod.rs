mod keyboard;
mod logger;
mod mouse;
mod runner;
mod state;

use std::sync::Mutex;

use crate::triggers::Trigger;
use crate::win::WindowsSnapshot;

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
    };
    state::HOOK_STATE
        .set(Mutex::new(st))
        .ok()
        .expect("hooks already installed");
    runner::spawn();
}

pub fn set_bindings(bindings: Vec<(Trigger, isize)>) {
    if let Some(state_mut) = state::HOOK_STATE.get() {
        if let Ok(mut state) = state_mut.lock() {
            state.bindings = bindings;
        }
    }
}
