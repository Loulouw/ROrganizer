//! Windows single-instance lock backed by a named mutex + named event.
//!
//! Pattern :
//! - 1ère instance : `CreateMutexW` réussit. On crée tout de suite l'event
//!   nommé (avant `eframe::run_native`) puis on spawn un thread dédié qui
//!   attend dessus et appelle `on_signal()` à chaque signal.
//! - 2ème instance : `CreateMutexW` retourne `ERROR_ALREADY_EXISTS`,
//!   `OpenEventW` + `SetEvent` réveillent l'instance existante, puis exit(0).
//! - `CreateMutexW` failure (rare, kernel out of resources) : `eprintln!` en
//!   debug + `process::exit(1)`. On ne fake pas un état "First" qui
//!   contournerait silencieusement la protection.
//!
//! Le `SingleInstanceGuard` ferme proprement les deux handles à son `Drop`,
//! mais en pratique il vit jusqu'à la fin du process et le Kernel s'en
//! occuperait de toute façon — c'est juste plus auto-documentant.

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
    mutex: HANDLE,
    event: HANDLE,
}

// HANDLE = *mut c_void. We never share these between threads (only stored
// in a static OnceLock + read-only `event` copy passed to the waiter).
unsafe impl Send for SingleInstanceGuard {}
unsafe impl Sync for SingleInstanceGuard {}

impl SingleInstanceGuard {
    /// Returns a copy of the event handle for the waiter thread. The waiter
    /// must NOT close it — the guard owns the lifetime.
    pub fn event_handle(&self) -> HANDLE {
        self.event
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.event.0.is_null() {
                let _ = CloseHandle(self.event);
            }
            if !self.mutex.0.is_null() {
                let _ = CloseHandle(self.mutex);
            }
        }
    }
}

pub enum AcquireResult {
    /// We are the first instance. Hold the guard for the rest of the
    /// process lifetime — its Drop closes the mutex + event.
    First(SingleInstanceGuard),
    /// Another instance is already running and was successfully signaled.
    SignalledExisting,
}

static GUARD: OnceLock<SingleInstanceGuard> = OnceLock::new();

pub fn store_guard(g: SingleInstanceGuard) {
    let _ = GUARD.set(g);
}

/// Tries to acquire the single-instance lock. On `CreateMutexW` failure
/// (kernel out of resources), this prints an error and exits(1) — we do
/// NOT want to silently boot a 2nd instance bypassing the protection.
pub fn acquire_or_signal_existing() -> AcquireResult {
    unsafe {
        let mutex_name = HSTRING::from(MUTEX_NAME);
        let mutex = match CreateMutexW(None, false, &mutex_name) {
            Ok(h) => h,
            Err(e) => {
                #[cfg(debug_assertions)]
                eprintln!("CreateMutexW failed: {e:?}");
                let _ = e;
                std::process::exit(1);
            }
        };

        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mutex);
            signal_existing();
            return AcquireResult::SignalledExisting;
        }

        // We are first. Create the event NOW (before eframe boots) so a
        // second-instance launch in the next 50–200 ms can wake us via
        // OpenEventW + SetEvent. Auto-reset = each SetEvent wakes once.
        let event_name = HSTRING::from(EVENT_NAME);
        let event = match CreateEventW(None, false, false, &event_name) {
            Ok(h) => h,
            Err(e) => {
                #[cfg(debug_assertions)]
                eprintln!("CreateEventW failed: {e:?}");
                let _ = e;
                let _ = CloseHandle(mutex);
                std::process::exit(1);
            }
        };

        AcquireResult::First(SingleInstanceGuard { mutex, event })
    }
}

unsafe fn signal_existing() {
    let event_name = HSTRING::from(EVENT_NAME);
    if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, &event_name) {
        let _ = SetEvent(event);
        let _ = CloseHandle(event);
    }
}

/// Spawns a thread that waits on the guard's event and invokes `on_signal`
/// each time it fires. The handle is owned by the guard — this thread only
/// reads from it. We pass the handle as `isize` to sidestep `HANDLE: !Send`
/// and Rust 2021's disjoint-capture inference (which would otherwise
/// pick up just the inner `*mut c_void` field of any wrapper struct).
pub fn spawn_waiter<F: Fn() + Send + 'static>(event: HANDLE, on_signal: F) {
    let event_raw: isize = event.0 as isize;
    thread::Builder::new()
        .name("rorg-singleton-waiter".into())
        .spawn(move || loop {
            let h = HANDLE(event_raw as *mut _);
            let wait = unsafe { WaitForSingleObject(h, INFINITE) };
            if wait != WAIT_OBJECT_0 {
                break;
            }
            on_signal();
        })
        .expect("spawn singleton waiter");
}
