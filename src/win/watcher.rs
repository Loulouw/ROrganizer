//! Event-driven Dofus window watcher.
//!
//! Replaces the previous 1500 ms polling loop with a `SetWinEventHook`
//! subscription. Whenever a window is created, destroyed, or has its title
//! renamed, Windows posts an event to our message pump; we coalesce a burst
//! of events into a single rescan after a short debounce so opening one
//! Dofus client doesn't trigger 30 enumerations.

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use eframe::egui;
use regex::Regex;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY,
    EVENT_OBJECT_NAMECHANGE, MSG, PM_REMOVE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

use super::enumerate::{enumerate_dofus_windows, DetectedWindow};
use super::process::ExeCache;

/// Lock-free snapshot of detected Dofus windows. Single writer (the watcher
/// thread), many readers (UI / hook callbacks). Readers call `.load_full()`
/// and get an `Arc<Vec<...>>` that's a stable view for the duration of the
/// borrow — even if the watcher publishes a new snapshot mid-read.
pub type WindowsSnapshot = Arc<ArcSwap<Vec<DetectedWindow>>>;

/// Coalescing window: after a relevant WinEvent fires we wait up to this
/// long for follow-ups (e.g. CREATE → NAMECHANGE during init) before
/// running a single rescan. Long enough to absorb a Dofus startup burst,
/// short enough that the UI feels instant.
const COALESCE_DELAY: Duration = Duration::from_millis(150);

/// Triggers a rescan within `COALESCE_DELAY`. Set from the WinEvent
/// callback (any thread) or from the manual refresh receiver.
static EVENT_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    _hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _dwms_event_time: u32,
) {
    // OBJID_WINDOW = 0; we only care about window-level events. Filtering
    // here avoids waking the rescan path for every toolbar / scrollbar.
    const OBJID_WINDOW: i32 = 0;
    if id_object != OBJID_WINDOW {
        return;
    }
    let _ = event; // CREATE / DESTROY / NAMECHANGE all merit a rescan
    EVENT_PENDING.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn spawn_watcher(
    snapshot: WindowsSnapshot,
    ctx: egui::Context,
    regex_src: String,
    refresh_rx: Receiver<()>,
) {
    let regex = Regex::new(&regex_src)
        .unwrap_or_else(|e| panic!("title_regex invalid: {e}"));

    std::thread::Builder::new()
        .name("rorg-watcher".into())
        .spawn(move || run(snapshot, ctx, regex, refresh_rx))
        .expect("failed to spawn watcher thread");
}

fn run(snapshot: WindowsSnapshot, ctx: egui::Context, regex: Regex, refresh_rx: Receiver<()>) {
    let mut buf: Vec<DetectedWindow> = Vec::with_capacity(8);
    let mut exe_cache: ExeCache = ExeCache::with_capacity(256);
    let mut seen_pids: HashSet<u32> = HashSet::with_capacity(256);

    // Initial scan so the first frame sees an accurate state.
    rescan(
        &snapshot,
        &ctx,
        &regex,
        &mut buf,
        &mut exe_cache,
        &mut seen_pids,
    );

    // Register WinEvent hook from this thread (WINEVENT_OUTOFCONTEXT means
    // callbacks can land on any thread, but the hook itself must outlive
    // this run() — we hold it in `_hook` until the watcher exits).
    let _hook = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_NAMECHANGE,
            None,
            Some(win_event_proc),
            0, // all processes
            0, // all threads
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };

    // Sanity: EVENT_OBJECT_DESTROY (0x8001) is in [CREATE=0x8000,
    // NAMECHANGE=0x800C] so the single SetWinEventHook range covers it.
    debug_assert!(EVENT_OBJECT_CREATE <= EVENT_OBJECT_DESTROY);
    debug_assert!(EVENT_OBJECT_DESTROY <= EVENT_OBJECT_NAMECHANGE);

    loop {
        // Pump the message queue so OUTOFCONTEXT callbacks get delivered.
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Wait briefly — manual refresh OR coalesce delay OR shutdown.
        match refresh_rx.recv_timeout(COALESCE_DELAY) {
            Ok(_) => EVENT_PENDING.store(true, std::sync::atomic::Ordering::Relaxed),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        // Drain any queued manual-refresh signals.
        while refresh_rx.try_recv().is_ok() {}

        if EVENT_PENDING.swap(false, std::sync::atomic::Ordering::Relaxed) {
            rescan(
                &snapshot,
                &ctx,
                &regex,
                &mut buf,
                &mut exe_cache,
                &mut seen_pids,
            );
        }
    }

    unsafe {
        let _ = UnhookWinEvent(_hook);
    }
}

fn rescan(
    snapshot: &WindowsSnapshot,
    ctx: &egui::Context,
    regex: &Regex,
    buf: &mut Vec<DetectedWindow>,
    exe_cache: &mut ExeCache,
    seen_pids: &mut HashSet<u32>,
) {
    enumerate_dofus_windows(regex, buf, exe_cache, seen_pids);
    exe_cache.retain(|pid, _| seen_pids.contains(pid));
    snapshot.store(Arc::new(buf.clone()));
    ctx.request_repaint();
}
