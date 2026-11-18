use std::sync::Arc;

use arc_swap::ArcSwap;
use regex::Regex;

use crate::platform::WindowId;

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

#[derive(Debug, Clone, Hash)]
pub struct DetectedWindow {
    pub hwnd: WindowId,
    pub slot_key: String,
    pub class: Option<String>,
}

impl DetectedWindow {
    /// The account shown in `title`, when it matches the title regex `re`
    /// (`name` group, plus `class` for Unity clients).
    pub fn from_title(re: &Regex, title: &str, hwnd: WindowId) -> Option<Self> {
        let caps = re.captures(title)?;
        let slot_key = caps.name("name")?.as_str().trim().to_string();
        let class = caps
            .name("class")
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        Some(Self { hwnd, slot_key, class })
    }
}

/// Lock-free snapshot of detected Dofus windows. Single writer (the watcher
/// thread), many readers (UI / hook callbacks). Readers call `.load_full()`
/// and get an `Arc<Vec<...>>` that's a stable view for the duration of the
/// borrow — even if the watcher publishes a new snapshot mid-read.
pub type WindowsSnapshot = Arc<ArcSwap<Vec<DetectedWindow>>>;

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(title: &str) -> Option<(String, Option<String>)> {
        let re = Regex::new(DEFAULT_TITLE_REGEX).expect("default regex compiles");
        DetectedWindow::from_title(&re, title, WindowId::test(1)).map(|w| (w.slot_key, w.class))
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
