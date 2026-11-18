use super::state::{BindingAction, HookState};
use crate::platform::WindowId;
use crate::triggers::Trigger;

/// Looks up the trigger against the binding table and returns the window to
/// focus, advancing the cycler cursor as needed. Caller still holds the lock.
///
/// `foreground` is only called once a binding matched, and only when needed:
/// - with `dofus_only`, nothing happens unless the window in front is Dofus:
///   an account of the cycle, or any window `is_dofus` recognises (login
///   screen). `None` lets the caller pass the key on untouched;
/// - for cycle actions, when the window in front is part of the cycle, the
///   cursor snaps to it first. The user may have switched account with the
///   mouse (taskbar, click in game) since our last focus, and Next / Prev
///   must continue from where they actually are.
pub(super) fn resolve_action(
    state: &mut HookState,
    trigger: Trigger,
    foreground: impl FnOnce() -> Option<WindowId>,
    is_dofus: impl FnOnce(WindowId) -> bool,
) -> Option<WindowId> {
    let action = state
        .bindings
        .iter()
        .find(|(t, _)| *t == trigger)
        .map(|(_, a)| *a)?;
    let is_cycle = matches!(action, BindingAction::CycleNext | BindingAction::CyclePrev);
    let fg = if state.dofus_only || is_cycle { foreground() } else { None };
    if state.dofus_only
        && !fg.is_some_and(|h| state.cycle_hwnds.contains(&h) || is_dofus(h))
    {
        return None;
    }
    let target = match action {
        BindingAction::Focus(h) => {
            // A direct focus binding aligns the cycler on its target so the
            // next CycleNext from there continues the user's expected flow.
            if let Some(pos) = state.cycle_hwnds.iter().position(|&x| x == h) {
                state.cycle_index = pos;
            }
            Some(h)
        }
        BindingAction::CycleNext | BindingAction::CyclePrev => {
            let len = state.cycle_hwnds.len();
            if len == 0 {
                return None;
            }
            if let Some(pos) = fg.and_then(|fg| state.cycle_hwnds.iter().position(|&h| h == fg)) {
                state.cycle_index = pos;
            }
            let step = if matches!(action, BindingAction::CycleNext) { 1 } else { len - 1 };
            state.cycle_index = (state.cycle_index + step) % len;
            Some(state.cycle_hwnds[state.cycle_index])
        }
    };
    if let Some(h) = target {
        state.cycle_current_hwnd = Some(h);
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(n: i16) -> WindowId {
        WindowId::test(n)
    }

    fn make_state(
        bindings: Vec<(Trigger, BindingAction)>,
        cycle_hwnds: &[i16],
    ) -> HookState {
        HookState {
            bindings,
            cycle_hwnds: cycle_hwnds.iter().map(|&n| w(n)).collect(),
            cycle_index: 0,
            cycle_current_hwnd: None,
            dofus_only: false,
        }
    }

    /// `resolve_action` with the window in front fixed and no Dofus window
    /// outside the cycle.
    fn resolve(st: &mut HookState, trigger: Trigger, fg: Option<i16>) -> Option<WindowId> {
        resolve_action(st, trigger, move || fg.map(w), |_| false)
    }

    #[test]
    fn empty_bindings_resolves_to_none() {
        let mut st = make_state(Vec::new(), &[100, 200]);
        assert!(resolve(&mut st, Trigger::Key(0x70), None).is_none());
    }

    #[test]
    fn unmatched_trigger_resolves_to_none() {
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::Focus(w(100)))],
            &[100],
        );
        assert!(resolve(&mut st, Trigger::Key(0x71), None).is_none());
    }

    #[test]
    fn focus_aligns_cycle_index_to_target() {
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::Focus(w(300)))],
            &[100, 200, 300, 400],
        );
        st.cycle_index = 0; // start away from 300
        let r = resolve_action(
            &mut st,
            Trigger::Key(0x70),
            || panic!("focus needs no foreground"),
            |_| panic!("focus needs no process lookup"),
        );
        assert_eq!(r, Some(w(300)));
        assert_eq!(st.cycle_index, 2);
        assert_eq!(st.cycle_current_hwnd, Some(w(300)));
    }

    #[test]
    fn focus_does_not_move_cursor_if_target_not_in_cycle() {
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::Focus(w(999)))],
            &[100, 200],
        );
        st.cycle_index = 1;
        resolve(&mut st, Trigger::Key(0x70), None);
        assert_eq!(st.cycle_index, 1, "untouched when target outside cycle");
        assert_eq!(st.cycle_current_hwnd, Some(w(999)));
    }

    #[test]
    fn cycle_next_wraps_last_to_first() {
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::CycleNext)],
            &[100, 200, 300],
        );
        st.cycle_index = 2;
        let r = resolve(&mut st, Trigger::Key(0x70), None);
        assert_eq!(r, Some(w(100)));
        assert_eq!(st.cycle_index, 0);
        assert_eq!(st.cycle_current_hwnd, Some(w(100)));
    }

    #[test]
    fn cycle_next_advances_when_not_at_end() {
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::CycleNext)],
            &[100, 200, 300],
        );
        st.cycle_index = 0;
        let r = resolve(&mut st, Trigger::Key(0x70), None);
        assert_eq!(r, Some(w(200)));
        assert_eq!(st.cycle_index, 1);
    }

    #[test]
    fn cycle_prev_wraps_first_to_last() {
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::CyclePrev)],
            &[100, 200, 300],
        );
        st.cycle_index = 0;
        let r = resolve(&mut st, Trigger::Key(0x70), None);
        assert_eq!(r, Some(w(300)));
        assert_eq!(st.cycle_index, 2);
    }

    #[test]
    fn cycle_prev_decrements_when_not_at_start() {
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::CyclePrev)],
            &[100, 200, 300],
        );
        st.cycle_index = 2;
        let r = resolve(&mut st, Trigger::Key(0x70), None);
        assert_eq!(r, Some(w(200)));
        assert_eq!(st.cycle_index, 1);
    }

    #[test]
    fn cycle_continues_from_account_switched_outside_the_app() {
        // Cursor on 100, then the user clicked 300's taskbar button.
        let mut st = make_state(
            vec![
                (Trigger::Key(0x70), BindingAction::CycleNext),
                (Trigger::Key(0x71), BindingAction::CyclePrev),
            ],
            &[100, 200, 300],
        );
        let r = resolve(&mut st, Trigger::Key(0x70), Some(300));
        assert_eq!(r, Some(w(100)), "Next from 300 wraps to 100");
        assert_eq!(st.cycle_index, 0);
        let r = resolve(&mut st, Trigger::Key(0x71), Some(300));
        assert_eq!(r, Some(w(200)), "Prev from 300 is 200");
        assert_eq!(st.cycle_current_hwnd, Some(w(200)));
    }

    #[test]
    fn cycle_keeps_cursor_when_foreground_is_not_in_cycle() {
        // Browser, ROrganizer itself or an excluded account in front.
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::CycleNext)],
            &[100, 200, 300],
        );
        st.cycle_index = 1;
        let r = resolve(&mut st, Trigger::Key(0x70), Some(999));
        assert_eq!(r, Some(w(300)));
    }

    #[test]
    fn cycle_next_on_empty_list_returns_none() {
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::CycleNext)],
            &[],
        );
        assert!(resolve(&mut st, Trigger::Key(0x70), None).is_none());
        assert_eq!(st.cycle_current_hwnd, None);
    }

    #[test]
    fn cycle_prev_on_empty_list_returns_none() {
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::CyclePrev)],
            &[],
        );
        assert!(resolve(&mut st, Trigger::Key(0x70), None).is_none());
    }

    #[test]
    fn first_matching_binding_wins_on_duplicate_trigger() {
        // App pushes Account focus actions BEFORE Cycle ones, so Account wins.
        let mut st = make_state(
            vec![
                (Trigger::Key(0x70), BindingAction::Focus(w(500))),
                (Trigger::Key(0x70), BindingAction::CycleNext),
            ],
            &[100, 200],
        );
        let r = resolve(&mut st, Trigger::Key(0x70), None);
        assert_eq!(r, Some(w(500)));
    }

    #[test]
    fn dofus_only_lets_the_key_through_outside_dofus() {
        // Browser in front: no focus, and the cycle cursor does not move.
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::CycleNext)],
            &[100, 200, 300],
        );
        st.dofus_only = true;
        assert_eq!(resolve(&mut st, Trigger::Key(0x70), Some(999)), None);
        assert_eq!(resolve(&mut st, Trigger::Key(0x70), None), None);
        assert_eq!(st.cycle_index, 0);
        assert_eq!(st.cycle_current_hwnd, None);
    }

    #[test]
    fn dofus_only_acts_from_an_account_without_process_lookup() {
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::Focus(w(300)))],
            &[100, 200, 300],
        );
        st.dofus_only = true;
        let r = resolve_action(
            &mut st,
            Trigger::Key(0x70),
            || Some(w(100)),
            |_| panic!("an account of the cycle is Dofus"),
        );
        assert_eq!(r, Some(w(300)));
    }

    #[test]
    fn dofus_only_acts_from_a_dofus_window_outside_the_cycle() {
        // Login screen of another client: not an account, still Dofus.
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::Focus(w(300)))],
            &[100, 200, 300],
        );
        st.dofus_only = true;
        let r = resolve_action(&mut st, Trigger::Key(0x70), || Some(w(777)), |h| h == w(777));
        assert_eq!(r, Some(w(300)));
    }

    #[test]
    fn dofus_only_does_not_query_anything_for_unbound_keys() {
        let mut st = make_state(
            vec![(Trigger::Key(0x70), BindingAction::Focus(w(300)))],
            &[300],
        );
        st.dofus_only = true;
        let r = resolve_action(
            &mut st,
            Trigger::Key(0x41),
            || panic!("unbound key: no foreground lookup"),
            |_| panic!("unbound key: no process lookup"),
        );
        assert_eq!(r, None);
    }
}
