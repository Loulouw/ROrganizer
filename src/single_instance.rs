//! Windows single-instance lock backed by a named mutex + named event.
//!
//! Pattern :
//! - 1ère instance : `CreateMutexW` réussit avec un nouveau handle. Un thread
//!   dédié owne `CreateEventW` et appelle `on_signal()` à chaque signal.
//! - 2ème instance : `CreateMutexW` retourne `ERROR_ALREADY_EXISTS`,
//!   `OpenEventW` + `SetEvent` réveillent l'instance existante, puis exit(0).
//!
//! Le mutex est volontairement *jamais* libéré pendant la vie du process —
//! le Kernel s'en occupe à la sortie. On stocke juste son handle dans un
//! `OnceLock` static pour empêcher le `Drop` d'arriver tôt.

#![cfg(windows)]

use std::sync::OnceLock;
use std::thread;

use windows::core::HSTRING;
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE,
    INFINITE,
};

const MUTEX_NAME: &str = "Local\\ROrganizer_v1_mutex";
const EVENT_NAME: &str = "Local\\ROrganizer_v1_show_event";

pub struct SingleInstanceGuard {
    _mutex: HANDLE,
}

// HANDLE is just a pointer; safe to send between threads as long as we don't
// race on it (which we don't — only stored in a static OnceLock).
unsafe impl Send for SingleInstanceGuard {}
unsafe impl Sync for SingleInstanceGuard {}

pub enum AcquireResult {
    First(SingleInstanceGuard),
    SignalledExisting,
}

static GUARD: OnceLock<SingleInstanceGuard> = OnceLock::new();

pub fn store_guard(g: SingleInstanceGuard) {
    let _ = GUARD.set(g);
}

pub fn acquire_or_signal_existing() -> AcquireResult {
    unsafe {
        let name = HSTRING::from(MUTEX_NAME);
        let mutex = match CreateMutexW(None, false, &name) {
            Ok(h) => h,
            Err(_) => return AcquireResult::First(SingleInstanceGuard { _mutex: HANDLE(std::ptr::null_mut()) }),
        };

        if GetLastError() == ERROR_ALREADY_EXISTS {
            // Another instance is running. Best-effort signal it.
            let _ = CloseHandle(mutex);
            signal_existing();
            return AcquireResult::SignalledExisting;
        }

        AcquireResult::First(SingleInstanceGuard { _mutex: mutex })
    }
}

unsafe fn signal_existing() {
    let event_name = HSTRING::from(EVENT_NAME);
    if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, &event_name) {
        let _ = SetEvent(event);
        let _ = CloseHandle(event);
    }
}

/// Spawns a thread that owns a named manual-reset event and invokes
/// `on_signal` each time the event fires (which happens whenever a 2nd
/// instance launches and calls `signal_existing`).
pub fn spawn_waiter<F: Fn() + Send + 'static>(on_signal: F) {
    thread::spawn(move || unsafe {
        let event_name = HSTRING::from(EVENT_NAME);
        // Manual-reset = false (auto-reset) so each SetEvent triggers exactly one wake.
        let event = match CreateEventW(None, false, false, &event_name) {
            Ok(h) => h,
            Err(_) => return,
        };

        loop {
            let wait = WaitForSingleObject(event, INFINITE);
            if wait != WAIT_OBJECT_0 {
                break;
            }
            on_signal();
        }

        let _ = CloseHandle(event);
    });
}
