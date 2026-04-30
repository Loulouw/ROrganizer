use std::collections::HashSet;

use regex::Regex;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

use super::process::{self, ExeCache};

pub const DEFAULT_TITLE_REGEX: &str =
    r"^(?P<name>.+?)\s+-\s+(?P<class>.+?)\s+-\s+[\d.]+\s+-\s+Release\s*$";

#[derive(Debug, Clone)]
pub struct DetectedWindow {
    pub hwnd: isize,
    /// Kept for diagnostics / future cycler logic.
    #[allow(dead_code)]
    pub pid: u32,
    pub slot_key: String,
    pub class: Option<String>,
    /// Raw window title; useful for the temp_dir diag log when a regex doesn't match.
    #[allow(dead_code)]
    pub title: String,
}

struct EnumCtx<'a> {
    re: &'a Regex,
    out: &'a mut Vec<DetectedWindow>,
    exe_cache: &'a mut ExeCache,
    seen_pids: &'a mut HashSet<u32>,
}

pub fn enumerate_dofus_windows(
    re: &Regex,
    out: &mut Vec<DetectedWindow>,
    exe_cache: &mut ExeCache,
    seen_pids: &mut HashSet<u32>,
) {
    out.clear();
    seen_pids.clear();
    let mut ctx = EnumCtx {
        re,
        out,
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

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
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
        Some(name) => name.eq_ignore_ascii_case("Dofus.exe"),
        None => false,
    };
    if !is_dofus {
        return BOOL(1);
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
        hwnd: hwnd.0 as isize,
        pid,
        slot_key: name,
        class,
        title,
    });
    BOOL(1)
}
