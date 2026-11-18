mod resolve;
mod state;

use std::sync::atomic::Ordering;
use std::sync::Mutex;

use crate::platform::{self, WindowId};
use crate::triggers::Trigger;

pub use state::BindingAction;

/// Handle on the global LL-hook state. Hides the underlying
/// `OnceLock<Mutex<HookState>>` + atomic enabled flag behind a typed API,
/// so callers don't reach into module statics.
///
/// The platform hook callbacks get no userdata pointer (Windows invokes
/// them as `extern "system"` callbacks), so they go through the free
/// functions [`is_enabled`] and [`focus_target`], which read the globals.
/// Everything else talks through `HookHandle`.
pub struct HookHandle {
    _private: (),
}

impl HookHandle {
    /// Master switch. Lock-free read on the keystroke / click hot path,
    /// so toggling activation is observed by the next event.
    pub fn set_enabled(&self, enabled: bool) {
        state::HOOK_ENABLED.store(enabled, Ordering::Relaxed);
    }

    /// Push the resolved binding table + cycle hwnds. Does NOT touch
    /// `cycle_index` beyond clamping to the new range — the cursor must
    /// survive incidental snapshot updates from the watcher. For
    /// explicit cursor changes (manual click / drag), use
    /// [`set_cycle_index`].
    pub fn set_bindings_and_cycle(
        &self,
        bindings: Vec<(Trigger, BindingAction)>,
        cycle_hwnds: Vec<WindowId>,
    ) {
        let Some(state_mut) = state::HOOK_STATE.get() else { return };
        let Ok(mut state) = state_mut.lock() else { return };
        state.bindings = bindings;
        state.cycle_index = remap_cycle_index_after_reorder(
            state.cycle_current_hwnd,
            &cycle_hwnds,
            state.cycle_index,
        );
        state.cycle_hwnds = cycle_hwnds;
    }

    /// Override the cycler cursor explicitly. Called by App when the
    /// user clicks an account row in the UI.
    pub fn set_cycle_index(&self, cycle_index: usize) {
        let Some(state_mut) = state::HOOK_STATE.get() else { return };
        let Ok(mut state) = state_mut.lock() else { return };
        let len = state.cycle_hwnds.len();
        state.cycle_index = if len == 0 { 0 } else { cycle_index.min(len - 1) };
        state.cycle_current_hwnd = state.cycle_hwnds.get(state.cycle_index).copied();
    }

    /// Bindings only fire while a Dofus window is in front (user option).
    pub fn set_dofus_only(&self, on: bool) {
        let Some(state_mut) = state::HOOK_STATE.get() else { return };
        let Ok(mut state) = state_mut.lock() else { return };
        state.dofus_only = on;
    }
}

/// Returns the new `cycle_index` after `cycle_hwnds` has been replaced.
///
/// Strategy:
/// 1. If `current_hwnd` (the HWND we last actually focused) still appears
///    in the new list, snap the cursor to its position so the user stays
///    "on the same account" after a drag-and-drop or watcher refresh.
/// 2. Otherwise keep the previous index, clamped into the new range
///    (or 0 when the list is empty).
fn remap_cycle_index_after_reorder(
    current_hwnd: Option<WindowId>,
    new_cycle_hwnds: &[WindowId],
    prev_index: usize,
) -> usize {
    if let Some(cur) = current_hwnd
        && let Some(pos) = new_cycle_hwnds.iter().position(|&h| h == cur)
    {
        return pos;
    }
    let len = new_cycle_hwnds.len();
    if len == 0 {
        0
    } else {
        prev_index.min(len - 1)
    }
}

/// Master switch, read lock-free by the platform callbacks before anything
/// else.
pub fn is_enabled() -> bool {
    state::HOOK_ENABLED.load(Ordering::Relaxed)
}

/// Window to bring forward for `trigger`, or None to let the event through.
/// Called by the platform hook callbacks: locks the state, then resolves
/// the trigger against the bindings and the window in front.
pub fn focus_target(trigger: Trigger) -> Option<WindowId> {
    state::HOOK_STATE.get().and_then(|m| {
        let mut state = m.lock().ok()?;
        resolve::resolve_action(
            &mut state,
            trigger,
            platform::foreground_window,
            platform::is_dofus_window,
        )
    })
}

/// Installs the global LL hooks (keyboard + mouse) and returns the handle
/// the rest of the app uses to drive them. `dofus_only` is the user's
/// "shortcuts only in Dofus" option. Panics if called twice.
pub fn install(dofus_only: bool) -> HookHandle {
    let st = state::HookState {
        bindings: Vec::new(),
        cycle_hwnds: Vec::new(),
        cycle_index: 0,
        cycle_current_hwnd: None,
        dofus_only,
    };
    state::HOOK_STATE
        .set(Mutex::new(st))
        .ok()
        .expect("hooks already installed");
    platform::spawn_hooks();
    HookHandle { _private: () }
}

#[cfg(test)]
mod tests {
    use super::remap_cycle_index_after_reorder;
    use crate::platform::WindowId;

    fn w(n: i16) -> WindowId {
        WindowId::test(n)
    }

    fn ids(ns: &[i16]) -> Vec<WindowId> {
        ns.iter().map(|&n| w(n)).collect()
    }

    #[test]
    fn remap_empty_list_resets_to_zero() {
        // No hwnds → cursor must collapse to 0 even if a previous position
        // existed; otherwise resolve_action would index past the end.
        assert_eq!(remap_cycle_index_after_reorder(None, &[], 5), 0);
        assert_eq!(remap_cycle_index_after_reorder(Some(w(123)), &[], 5), 0);
    }

    #[test]
    fn remap_snaps_to_current_when_present() {
        // The HWND we last focused must drag the cursor with it across
        // reorders so the next CycleNext continues from there.
        let hwnds = ids(&[100, 200, 300, 400]);
        assert_eq!(remap_cycle_index_after_reorder(Some(w(300)), &hwnds, 0), 2);
    }

    #[test]
    fn remap_snaps_even_when_prev_index_out_of_range() {
        let hwnds = ids(&[100, 200]);
        // Was at index 9, list shrank, but current hwnd is still there → snap.
        assert_eq!(remap_cycle_index_after_reorder(Some(w(200)), &hwnds, 9), 1);
    }

    #[test]
    fn remap_keeps_prev_when_current_missing_and_in_range() {
        // current was lost (window closed) but the previous position still
        // makes sense in the new list → keep it.
        let hwnds = ids(&[100, 200, 300]);
        assert_eq!(remap_cycle_index_after_reorder(Some(w(999)), &hwnds, 1), 1);
        assert_eq!(remap_cycle_index_after_reorder(None, &hwnds, 1), 1);
    }

    #[test]
    fn remap_clamps_prev_when_list_shrinks() {
        let hwnds = ids(&[100, 200]);
        // prev_index past new len → clamp to len - 1.
        assert_eq!(remap_cycle_index_after_reorder(Some(w(999)), &hwnds, 5), 1);
        assert_eq!(remap_cycle_index_after_reorder(None, &hwnds, 5), 1);
    }

    #[test]
    fn remap_keeps_zero_on_first_position() {
        let hwnds = ids(&[100, 200, 300]);
        assert_eq!(remap_cycle_index_after_reorder(None, &hwnds, 0), 0);
    }
}
