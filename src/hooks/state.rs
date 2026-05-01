use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};

use crate::triggers::Trigger;

/// What the hook should do when a Trigger matches.
#[derive(Debug, Clone, Copy)]
pub enum BindingAction {
    Focus(isize),
    CycleNext,
    CyclePrev,
}

pub struct HookState {
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
}

pub static HOOK_STATE: OnceLock<Mutex<HookState>> = OnceLock::new();

/// Master switch on the hook callbacks. Read on every keystroke / mouse
/// event, so we keep it as an atomic to short-circuit *before* taking
/// the `HOOK_STATE` mutex when the user has the app inactive.
pub static HOOK_ENABLED: AtomicBool = AtomicBool::new(false);

