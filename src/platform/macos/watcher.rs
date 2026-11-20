//! Event-driven Dofus window watcher.
//!
//! App launches, terminations and activations come from NSWorkspace
//! notifications (main thread); window creations and title changes inside
//! a Dofus client from an `AXObserver` per client, scheduled on the watcher
//! thread. Every event only raises a flag: bursts are coalesced into one
//! rescan, as on Windows.

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use block2::RcBlock;
use eframe::egui;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSRunningApplication, NSWorkspace, NSWorkspaceDidActivateApplicationNotification,
    NSWorkspaceDidLaunchApplicationNotification, NSWorkspaceDidTerminateApplicationNotification,
};
use objc2_core_foundation::{kCFRunLoopDefaultMode, CFRunLoop, CFRunLoopRunResult, CFString};
use objc2_foundation::{NSNotification, NSNotificationName};
use regex::Regex;

use super::ax::{Element, Observer};
use super::permission::input_permission_granted;
use crate::dofus::{DetectedWindow, WindowsSnapshot};
use crate::platform::WindowId;

const DOFUS_BUNDLE_ID: &str = "com.Ankama.Dofus";
/// Executable names, for a client whose bundle id differs (Retro was not
/// observed on macOS).
const DOFUS_EXES: [&str; 2] = ["Dofus", "Dofus Retro"];

fn is_dofus_app(bundle_id: Option<&str>, exe: Option<&str>) -> bool {
    bundle_id.is_some_and(|id| id.eq_ignore_ascii_case(DOFUS_BUNDLE_ID))
        || exe.is_some_and(|exe| DOFUS_EXES.iter().any(|d| exe.eq_ignore_ascii_case(d)))
}

/// Every Dofus process owning a window at the last rescan, logged in or
/// not. Read by the event tap, which must not query processes.
static DOFUS_PIDS: LazyLock<ArcSwap<Vec<i32>>> =
    LazyLock::new(|| ArcSwap::from_pointee(Vec::new()));

/// Pid of the frontmost app, kept by the activation notification so the
/// event tap reads it without a system call. 0 until known.
static FRONT_PID: AtomicI32 = AtomicI32::new(0);

pub(super) fn frontmost_pid() -> i32 {
    FRONT_PID.load(Ordering::Relaxed)
}

pub fn is_dofus_window(id: WindowId) -> bool {
    DOFUS_PIDS.load().contains(&id.0)
}

/// Coalescing window, as on Windows: long enough to absorb a client's
/// startup burst, short enough that the UI feels instant.
const COALESCE_DELAY: Duration = Duration::from_millis(150);

/// A client just launched may not answer AX queries yet: its observer is
/// retried this long after.
const RETRY_DELAY: Duration = Duration::from_secs(1);

static EVENT_PENDING: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn on_ax_event(
    _observer: *mut c_void,
    _element: *mut c_void,
    _notification: *const CFString,
    _refcon: *mut c_void,
) {
    EVENT_PENDING.store(true, Ordering::Relaxed);
}

/// Must be called on the main thread (from `App::new`): the NSWorkspace
/// observers are registered there.
pub fn spawn_watcher(
    snapshot: WindowsSnapshot,
    ctx: egui::Context,
    regex_src: String,
    refresh_rx: Receiver<()>,
) {
    let regex = Regex::new(&regex_src).unwrap_or_else(|e| panic!("title_regex invalid: {e}"));
    observe_workspace(
        MainThreadMarker::new().expect("spawn_watcher runs on the main thread"),
        ctx.clone(),
    );

    std::thread::Builder::new()
        .name("rorg-watcher".into())
        .spawn(move || run(snapshot, ctx, regex, refresh_rx))
        .expect("failed to spawn watcher thread");
}

fn observe_workspace(_mtm: MainThreadMarker, ctx: egui::Context) {
    let workspace = NSWorkspace::sharedWorkspace();
    if let Some(app) = workspace.frontmostApplication() {
        FRONT_PID.store(app.processIdentifier(), Ordering::Relaxed);
    }
    let center = workspace.notificationCenter();
    let observe = |name: &NSNotificationName, block: RcBlock<dyn Fn(NonNull<NSNotification>)>| {
        // Queue None: the block runs on the posting thread, the main one.
        let token =
            unsafe { center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) };
        // Observed for the whole life of the process.
        std::mem::forget(token);
    };
    let changed = || RcBlock::new(|_| EVENT_PENDING.store(true, Ordering::Relaxed));
    observe(unsafe { NSWorkspaceDidLaunchApplicationNotification }, changed());
    observe(unsafe { NSWorkspaceDidTerminateApplicationNotification }, changed());
    observe(
        unsafe { NSWorkspaceDidActivateApplicationNotification },
        RcBlock::new(move |_| {
            let front = NSWorkspace::sharedWorkspace().frontmostApplication();
            FRONT_PID.store(front.map_or(0, |a| a.processIdentifier()), Ordering::Relaxed);
            crate::overlay::foreground_changed(&ctx);
        }),
    );
}

struct Watcher {
    snapshot: WindowsSnapshot,
    ctx: egui::Context,
    regex: Regex,
    /// One per running Dofus client, keyed by pid.
    observers: HashMap<i32, Observer>,
    retry_at: Option<Instant>,
}

fn run(snapshot: WindowsSnapshot, ctx: egui::Context, regex: Regex, refresh_rx: Receiver<()>) {
    let mut w = Watcher { snapshot, ctx, regex, observers: HashMap::new(), retry_at: None };
    let mut trusted = input_permission_granted();
    w.rescan();

    loop {
        // Delivers the AX observer callbacks. Returns at once while no
        // observer is scheduled (no client running): sleep instead.
        let result = CFRunLoop::run_in_mode(
            unsafe { kCFRunLoopDefaultMode },
            COALESCE_DELAY.as_secs_f64(),
            false,
        );
        if result == CFRunLoopRunResult::Finished {
            std::thread::sleep(COALESCE_DELAY);
        }

        loop {
            match refresh_rx.try_recv() {
                Ok(()) => EVENT_PENDING.store(true, Ordering::Relaxed),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        // The grant changes in System Settings, with no notification.
        let now_trusted = input_permission_granted();
        if now_trusted != trusted {
            trusted = now_trusted;
            EVENT_PENDING.store(true, Ordering::Relaxed);
        }
        if w.retry_at.is_some_and(|t| Instant::now() >= t) {
            EVENT_PENDING.store(true, Ordering::Relaxed);
        }

        if EVENT_PENDING.swap(false, Ordering::Relaxed) {
            w.rescan();
        }
    }
}

impl Watcher {
    fn rescan(&mut self) {
        let mut found = Vec::new();
        let mut dofus_pids = Vec::new();
        let mut running = Vec::new();
        self.retry_at = None;

        for app in NSWorkspace::sharedWorkspace().runningApplications().iter() {
            if !is_dofus(&app) {
                continue;
            }
            let pid = app.processIdentifier();
            let Some(element) = (pid > 0).then(|| Element::application(pid)).flatten() else {
                continue;
            };
            running.push(pid);
            self.observe(pid, &element);

            let windows = element.windows();
            if windows.is_empty() {
                continue;
            }
            dofus_pids.push(pid);
            for window in &windows {
                // Character selection renames the window it already has.
                if let Some(observer) = self.observers.get(&pid) {
                    observer.watch(window, "AXTitleChanged");
                }
            }
            if let Some(title) = element.game_window().and_then(|w| w.string("AXTitle"))
                && let Some(window) = DetectedWindow::from_title(&self.regex, &title, WindowId(pid))
            {
                found.push(window);
            }
        }

        self.observers.retain(|pid, _| running.contains(pid));
        dofus_pids.sort_unstable();
        found.sort_by_key(|w| w.hwnd.0);
        if **DOFUS_PIDS.load() != dofus_pids {
            DOFUS_PIDS.store(Arc::new(dofus_pids));
        }
        self.snapshot.store(Arc::new(found));
        self.ctx.request_repaint();
    }

    /// Registers the client's observer once it accepts it; until then (just
    /// launched, or no Accessibility grant) a rescan is retried.
    fn observe(&mut self, pid: i32, element: &Element) {
        if self.observers.contains_key(&pid) {
            return;
        }
        let observer = Observer::new(pid, on_ax_event)
            .filter(|o| o.watch(element, "AXWindowCreated") && o.watch(element, "AXTitleChanged"));
        match observer {
            Some(observer) => {
                self.observers.insert(pid, observer);
            }
            None => self.retry_at = Some(Instant::now() + RETRY_DELAY),
        }
    }
}

fn is_dofus(app: &NSRunningApplication) -> bool {
    let bundle_id = app.bundleIdentifier().map(|s| s.to_string());
    let exe = app
        .executableURL()
        .and_then(|url| url.lastPathComponent())
        .map(|s| s.to_string());
    is_dofus_app(bundle_id.as_deref(), exe.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_unity_bundle_and_both_executables() {
        assert!(is_dofus_app(Some("com.Ankama.Dofus"), Some("Dofus")));
        assert!(is_dofus_app(Some("com.ankama.dofus"), None));
        assert!(is_dofus_app(None, Some("Dofus Retro")));
        assert!(is_dofus_app(Some("com.ankama.retro"), Some("Dofus Retro")));
    }

    #[test]
    fn rejects_the_launcher_and_other_apps() {
        assert!(!is_dofus_app(Some("com.ankama.zaap"), Some("Ankama Launcher")));
        assert!(!is_dofus_app(Some("com.apple.finder"), Some("Finder")));
        assert!(!is_dofus_app(None, None));
    }
}
