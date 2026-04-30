mod keyboard;
mod logger;
mod mouse;
mod runner;
mod state;

use std::sync::Mutex;

use crate::win::WindowsSnapshot;

pub fn install(windows: WindowsSnapshot) {
    let log_tx = logger::spawn();
    let _ = log_tx.send(format!(
        "rorganizer hooks started @ {:?}",
        std::time::SystemTime::now()
    ));
    let st = state::HookState { windows, log_tx };
    state::HOOK_STATE
        .set(Mutex::new(st))
        .ok()
        .expect("hooks already installed");
    runner::spawn();
}
