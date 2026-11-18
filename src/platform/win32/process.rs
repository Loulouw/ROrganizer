use std::collections::HashMap;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::PWSTR;

/// Per-pid cache of `exe basename`. `None` means a previous lookup failed
/// (process is gone, access denied, etc.) — kept so we don't retry the same
/// pid every tick.
pub type ExeCache = HashMap<u32, Option<String>>;

/// Cached lookup of the executable basename for `pid` (e.g. `"Dofus.exe"`).
///
/// First lookup hits the OS (~30-100 µs per call). Subsequent ticks read
/// from the in-memory map. The watcher prunes stale entries after each tick.
pub fn exe_filename_cached(pid: u32, cache: &mut ExeCache) -> Option<&str> {
    cache
        .entry(pid)
        .or_insert_with(|| exe_filename_uncached(pid))
        .as_deref()
}

fn exe_filename_uncached(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        result.ok()?;

        let path = String::from_utf16_lossy(&buf[..size as usize]);
        path.rsplit(['\\', '/']).next().map(|s| s.to_string())
    }
}
