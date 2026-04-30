use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use crate::triggers::Trigger;
use crate::win::WindowsSnapshot;

pub struct HookState {
    /// Snapshot of detected windows. Currently unused from hook callbacks (App
    /// pushes resolved bindings instead) — kept for the cycler in phase 8 which
    /// will need to walk the live ordered list from the hook context.
    #[allow(dead_code)]
    pub windows: WindowsSnapshot,
    pub log_tx: Sender<String>,
    /// Current resolved bindings (Trigger → target HWND), pushed by App every
    /// time the user updates a binding or the watcher refreshes the snapshot.
    pub bindings: Vec<(Trigger, isize)>,
}

pub static HOOK_STATE: OnceLock<Mutex<HookState>> = OnceLock::new();
