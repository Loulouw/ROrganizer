use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use crate::win::WindowsSnapshot;

pub struct HookState {
    pub windows: WindowsSnapshot,
    pub log_tx: Sender<String>,
}

pub static HOOK_STATE: OnceLock<Mutex<HookState>> = OnceLock::new();
