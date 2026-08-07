use std::collections::HashSet;

use regex::Regex;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

use super::process::{self, ExeCache};

/// Matches both client families, which format their title differently:
///
/// - Unity — `Pseudo - Iop - 2.75.4.13 - Release`
/// - Retro — `Pseudo - Dofus Retro v1.48.21`
///
/// Retro titles carry no class, so `class` only exists in the Unity branch
/// and `DetectedWindow::class` stays `None` for Retro. The Unity branch is
/// tried first, leaving its behaviour untouched.
///
/// The Retro branch is deliberately lenient (optional `v`, optional trailing
/// segment): the in-game title could not be observed first hand. Widening it
/// is safe because `is_dofus_exe` has already restricted the search to the
/// two Dofus executables.
pub const DEFAULT_TITLE_REGEX: &str = concat!(
    r"^(?P<name>.+?)\s+-\s+(?:",
    r"(?P<class>.+?)\s+-\s+[\d.]+\s+-\s+.+",
    r"|Dofus\s+Retro\s+v?[\d.]+(?:\s+-\s+.+)?",
    r")$",
);

/// Retro ships as an Electron app whose executable is named with a space.
/// Matching `Dofus.exe` alone silently excludes it, before the title is even
/// read.
const DOFUS_EXES: [&str; 2] = ["Dofus.exe", "Dofus Retro.exe"];

fn is_dofus_exe(name: &str) -> bool {
    DOFUS_EXES.iter().any(|exe| name.eq_ignore_ascii_case(exe))
}

#[derive(Debug, Clone, Hash)]
pub struct DetectedWindow {
    pub hwnd: isize,
    pub slot_key: String,
    pub class: Option<String>,
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
        slot_key: name,
        class,
    });
    BOOL(1)
}}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors the extraction `enum_proc` performs on a matching title.
    fn parse(title: &str) -> Option<(String, Option<String>)> {
        let re = Regex::new(DEFAULT_TITLE_REGEX).expect("default regex compiles");
        let caps = re.captures(title)?;
        let name = caps.name("name")?.as_str().trim().to_string();
        let class = caps
            .name("class")
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        Some((name, class))
    }

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

    #[test]
    fn unity_title_yields_name_and_class() {
        assert_eq!(
            parse("Pseudo - Iop - 2.75.4.13 - Release"),
            Some(("Pseudo".into(), Some("Iop".into())))
        );
        assert_eq!(
            parse("Pseudo - Cra - 2.76.0.1 - Beta"),
            Some(("Pseudo".into(), Some("Cra".into())))
        );
    }

    #[test]
    fn retro_title_yields_name_without_class() {
        assert_eq!(
            parse("Ajim-farming - Dofus Retro v1.48.21"),
            Some(("Ajim-farming".into(), None))
        );
    }

    #[test]
    fn retro_name_may_contain_a_spaced_dash() {
        // The lazy `name` group has to give ground until the Retro branch
        // matches the tail, otherwise a nickname with " - " loses characters.
        assert_eq!(
            parse("Ajim - farming - Dofus Retro v1.48.21"),
            Some(("Ajim - farming".into(), None))
        );
    }

    #[test]
    fn retro_version_tolerates_shape_variations() {
        assert_eq!(
            parse("Pseudo - Dofus Retro 1.48.21"),
            Some(("Pseudo".into(), None))
        );
        assert_eq!(
            parse("Pseudo - Dofus Retro v1.48.21 - Serveur"),
            Some(("Pseudo".into(), None))
        );
    }

    #[test]
    fn retro_login_window_is_not_an_account() {
        // Observed live: before a character is picked the window is titled
        // with the product name only. Matching it would create a phantom row.
        assert_eq!(parse("Dofus Retro v1.48.21"), None);
    }

    #[test]
    fn unrelated_titles_are_rejected() {
        assert_eq!(parse("Ankama Launcher"), None);
        assert_eq!(parse("MSCTFIME UI"), None);
        assert_eq!(parse("Bloc-notes - Sans titre"), None);
        assert_eq!(parse(""), None);
    }
}
