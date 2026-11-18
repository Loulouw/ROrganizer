use std::collections::HashSet;

use regex::Regex;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

use super::process::{self, ExeCache};
use crate::dofus::DetectedWindow;
use crate::platform::WindowId;

/// Retro ships as an Electron app whose executable is named with a space.
/// Matching `Dofus.exe` alone silently excludes it, before the title is even
/// read.
const DOFUS_EXES: [&str; 2] = ["Dofus.exe", "Dofus Retro.exe"];

fn is_dofus_exe(name: &str) -> bool {
    DOFUS_EXES.iter().any(|exe| name.eq_ignore_ascii_case(exe))
}

struct EnumCtx<'a> {
    re: &'a Regex,
    out: &'a mut Vec<DetectedWindow>,
    dofus_pids: &'a mut Vec<u32>,
    exe_cache: &'a mut ExeCache,
    seen_pids: &'a mut HashSet<u32>,
}

/// Fills `out` with the windows matching `re`, and `dofus_pids` with every
/// Dofus process owning a visible window, matched or not (login screen,
/// character selection).
pub fn enumerate_dofus_windows(
    re: &Regex,
    out: &mut Vec<DetectedWindow>,
    dofus_pids: &mut Vec<u32>,
    exe_cache: &mut ExeCache,
    seen_pids: &mut HashSet<u32>,
) {
    out.clear();
    dofus_pids.clear();
    seen_pids.clear();
    let mut ctx = EnumCtx {
        re,
        out,
        dofus_pids,
        exe_cache,
        seen_pids,
    };
    unsafe {
        let _ = EnumWindows(
            Some(enum_proc),
            LPARAM(&mut ctx as *mut EnumCtx<'_> as isize),
        );
    }
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL { unsafe {
    let ctx = &mut *(lparam.0 as *mut EnumCtx<'_>);

    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }

    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return BOOL(1);
    }
    ctx.seen_pids.insert(pid);

    let is_dofus = match process::exe_filename_cached(pid, ctx.exe_cache) {
        Some(name) => is_dofus_exe(name),
        None => false,
    };
    if !is_dofus {
        return BOOL(1);
    }
    if !ctx.dofus_pids.contains(&pid) {
        ctx.dofus_pids.push(pid);
    }

    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return BOOL(1);
    }
    let mut buf = vec![0u16; len as usize + 1];
    let written = GetWindowTextW(hwnd, &mut buf);
    if written <= 0 {
        return BOOL(1);
    }
    let title = String::from_utf16_lossy(&buf[..written as usize]);

    let Some(caps) = ctx.re.captures(&title) else {
        return BOOL(1);
    };
    let Some(name) = caps.name("name").map(|m| m.as_str().trim().to_string()) else {
        return BOOL(1);
    };
    let class = caps
        .name("class")
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty());

    ctx.out.push(DetectedWindow {
        hwnd: WindowId::from_hwnd(hwnd),
        slot_key: name,
        class,
    });
    BOOL(1)
}}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_both_dofus_executables_case_insensitively() {
        assert!(is_dofus_exe("Dofus.exe"));
        assert!(is_dofus_exe("dofus.exe"));
        assert!(is_dofus_exe("Dofus Retro.exe"));
        assert!(is_dofus_exe("dofus retro.exe"));
    }

    #[test]
    fn rejects_other_executables() {
        // No space: not the real Retro binary name, so not a client we know.
        assert!(!is_dofus_exe("DofusRetro.exe"));
        assert!(!is_dofus_exe("Dofusss.exe"));
        assert!(!is_dofus_exe("notepad.exe"));
        assert!(!is_dofus_exe(""));
    }
}
