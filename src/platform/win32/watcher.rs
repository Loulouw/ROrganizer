//! Event-driven Dofus window watcher.
//!
//! Subscribes to `SetWinEventHook` for window create / destroy / namechange.
//! A burst of events is coalesced into a single rescan after a short
//! debounce so opening one Dofus client doesn't trigger 30 enumerations.
//! Foreground changes are hooked on a thread of their own, which blocks in
//! `GetMessageW`: the overlay highlight follows Alt-Tab without the
//! coalescing delay.

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;

use arc_swap::ArcSwap;
use eframe::egui;
use regex::Regex;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, GetWindowThreadProcessId, PeekMessageW, TranslateMessage,
    EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_NAMECHANGE, EVENT_SYSTEM_FOREGROUND,
    MSG, PM_REMOVE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

use super::enumerate::enumerate_dofus_windows;
use super::process::ExeCache;
use crate::dofus::{DetectedWindow, WindowsSnapshot};
use crate::platform::WindowId;

/// Every Dofus process owning a visible window at the last rescan, logged
/// in or not. Read by the LL hooks, which must not query processes.
static DOFUS_PIDS: LazyLock<ArcSwap<Vec<u32>>> =
    LazyLock::new(|| ArcSwap::from_pointee(Vec::new()));

/// Whether `hwnd` belongs to a running Dofus client, whatever its title
/// (login screen, character selection).
pub fn is_dofus_window(hwnd: WindowId) -> bool {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd.hwnd(), Some(&mut pid)) };
    pid != 0 && DOFUS_PIDS.load().contains(&pid)
}

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
        .spawn({
            let ctx = ctx.clone();
            move || run(snapshot, ctx, regex, refresh_rx)
        })
        .expect("failed to spawn watcher thread");

    let _ = FOREGROUND_CTX.set(ctx);
    std::thread::Builder::new()
        .name("rorg-foreground".into())
        .spawn(watch_foreground)
        .expect("failed to spawn foreground thread");
}

/// Woken by every foreground change, ROrganizer's own windows included.
static FOREGROUND_CTX: OnceLock<egui::Context> = OnceLock::new();

unsafe extern "system" fn foreground_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _dwms_event_time: u32,
) {
    if let Some(ctx) = FOREGROUND_CTX.get() {
        crate::overlay::foreground_changed(ctx);
    }
}

fn watch_foreground() {
    unsafe {
        let _hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            DispatchMessageW(&msg);
        }
    }
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
    const {
        assert!(EVENT_OBJECT_CREATE <= EVENT_OBJECT_DESTROY);
        assert!(EVENT_OBJECT_DESTROY <= EVENT_OBJECT_NAMECHANGE);
    }

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
    let mut dofus_pids = Vec::new();
    enumerate_dofus_windows(regex, buf, &mut dofus_pids, exe_cache, seen_pids);
    exe_cache.retain(|pid, _| seen_pids.contains(pid));
    if **DOFUS_PIDS.load() != dofus_pids {
        DOFUS_PIDS.store(Arc::new(dofus_pids));
    }
    snapshot.store(Arc::new(buf.clone()));
    ctx.request_repaint();
}
