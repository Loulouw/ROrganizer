//! Reorders the Dofus buttons of the Windows taskbar.
//!
//! Windows has no API to move a taskbar button, but a button re-added with
//! `ITaskbarList::AddTab` lands last: removing and re-adding every window
//! in the wanted order yields that order, grouped icon included.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{ITaskbarList, TaskbarList};

/// Pause after each `DeleteTab` and each `AddTab`. Explorer applies them
/// asynchronously: a Delete immediately followed by its Add can collapse
/// into a no-op and leave the button where it was.
const STEP_DELAY: Duration = Duration::from_millis(100);

/// Set while a reorder runs. Two overlapping passes interleave their calls
/// and produce a wrong order, so a request is dropped instead of queued.
static SORTING: AtomicBool = AtomicBool::new(false);

pub fn is_sorting() -> bool {
    SORTING.load(Ordering::Acquire)
}

/// Re-adds the taskbar button of each window of `hwnds`, in that order, on
/// a short-lived thread with its own COM apartment. Ignored while a
/// previous reorder runs. `on_done` runs once finished, failed or not.
pub fn reorder(hwnds: Vec<isize>, on_done: impl FnOnce() + Send + 'static) {
    if hwnds.len() < 2
        || SORTING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return;
    }
    thread::spawn(move || {
        unsafe { readd_tabs(&hwnds) };
        SORTING.store(false, Ordering::Release);
        on_done();
    });
}

unsafe fn readd_tabs(hwnds: &[isize]) {
    unsafe {
        if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_err() {
            return;
        }
        if let Ok(list) =
            CoCreateInstance::<_, ITaskbarList>(&TaskbarList, None, CLSCTX_INPROC_SERVER)
            && list.HrInit().is_ok()
        {
            for &h in hwnds {
                let hwnd = HWND(h as *mut _);
                let _ = list.DeleteTab(hwnd);
                thread::sleep(STEP_DELAY);
                let _ = list.AddTab(hwnd);
                thread::sleep(STEP_DELAY);
            }
        }
        CoUninitialize();
    }
}
