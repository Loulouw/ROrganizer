//! Bringing a Dofus client forward through the Accessibility API.
//!
//! The AX calls block until the target app answers: up to the whole Dock
//! animation (~600 ms) for a minimized window, and a full Space switch for
//! a fullscreen one. They therefore run on a worker thread, never in the
//! event tap (which would stall every input on the machine) nor on the UI
//! thread. Only the latest request is kept: switching fast while an
//! animation plays must land on the last account asked for, not replay
//! every intermediate one.

use std::sync::{Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::ax::Element;
use super::watcher::frontmost_pid;
use crate::platform::WindowId;

/// Longest a request may take to show in `frontmost_pid`: the Dock
/// restore animation, with margin.
const ACTIVATION_GRACE: Duration = Duration::from_secs(1);

/// Last account asked for and when, until it is in front.
static REQUESTED: Mutex<Option<(i32, Instant)>> = Mutex::new(None);

/// The app in front, or the one on its way there. Activation is
/// asynchronous (~100 ms, a Dock animation for a minimized window): a cycle
/// step arriving meanwhile must continue from where the previous step
/// went, not snap back to the app it is leaving and replay that step.
pub fn foreground_window() -> Option<WindowId> {
    let front = frontmost_pid();
    let mut requested = REQUESTED.lock().unwrap_or_else(|e| e.into_inner());
    match *requested {
        Some((pid, at)) if pid != front && at.elapsed() < ACTIVATION_GRACE => Some(WindowId(pid)),
        _ => {
            *requested = None;
            (front > 0).then_some(WindowId(front))
        }
    }
}

/// Latest account asked for, taken by the worker.
static TARGET: Mutex<Option<i32>> = Mutex::new(None);
static WAKE: Condvar = Condvar::new();

static WORKER: LazyLock<()> = LazyLock::new(|| {
    std::thread::Builder::new()
        .name("rorg-focus".into())
        .spawn(|| loop {
            let pid = {
                let mut target = TARGET.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    if let Some(pid) = target.take() {
                        break pid;
                    }
                    target = WAKE.wait(target).unwrap_or_else(|e| e.into_inner());
                }
            };
            bring_forward(pid);
        })
        .expect("spawn focus thread");
});

/// Queues `id` to be brought forward; returns at once.
pub fn focus_window(id: WindowId) {
    LazyLock::force(&WORKER);
    *REQUESTED.lock().unwrap_or_else(|e| e.into_inner()) = Some((id.0, Instant::now()));
    *TARGET.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.0);
    WAKE.notify_one();
}

/// Activating the app first, then restoring its window, then activating
/// again is the order that works for a window minimized to the Dock: the
/// other way round, the window comes back but its app is not activated
/// (measured on macOS 27). The minimized state is read before anything
/// else: `AXRaise` already starts restoring the window, after which it no
/// longer reads as minimized and the second activation would be skipped.
fn bring_forward(pid: i32) {
    let Some(app) = Element::application(pid) else { return };
    let window = app.game_window();
    let minimized = window.as_ref().is_some_and(|w| w.bool("AXMinimized") == Some(true));
    raise(&app, window.as_ref());
    if let Some(window) = window.as_ref().filter(|_| minimized) {
        window.set_bool("AXMinimized", false);
        raise(&app, Some(window));
    }
}

fn raise(app: &Element, window: Option<&Element>) {
    app.set_bool("AXFrontmost", true);
    if let Some(window) = window {
        window.set_bool("AXMain", true);
        window.perform("AXRaise");
    }
}
