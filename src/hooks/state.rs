use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use crate::triggers::Trigger;
use crate::win::WindowsSnapshot;

/// What the hook should do when a Trigger matches.
#[derive(Debug, Clone, Copy)]
pub enum BindingAction {
    Focus(isize),
    CycleNext,
    CyclePrev,
}

pub struct HookState {
    /// Snapshot of detected windows. Currently unused from hook callbacks (App
    /// pushes resolved bindings instead) — kept for future hook-side logic.
    #[allow(dead_code)]
    pub windows: WindowsSnapshot,
    pub log_tx: Sender<String>,
    /// Trigger → action table, sorted so Account focus actions appear before
    /// Cycle ones (priority on conflict).
    pub bindings: Vec<(Trigger, BindingAction)>,
    /// Live HWNDs of the cycle in user-defined order, with dead slots filtered out.
    pub cycle_hwnds: Vec<isize>,
    /// Current position of the cycler cursor inside `cycle_hwnds`.
    pub cycle_index: usize,
    /// Last HWND the cycler / a focus binding actually focused. Tracked here
    /// so we can re-locate it inside `cycle_hwnds` after a reorder, keeping
    /// the user "on the same account" across drags.
    pub cycle_current_hwnd: Option<isize>,
    /// Master switch — when false, the callback returns CallNextHookEx
    /// without ever matching, so triggers don't fire and keys pass through.
    pub enabled: bool,
}

pub static HOOK_STATE: OnceLock<Mutex<HookState>> = OnceLock::new();
