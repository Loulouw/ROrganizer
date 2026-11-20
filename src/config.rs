use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;
use crate::overlay;
use crate::theme::Theme;
use crate::triggers::{BindingTarget, Trigger};

const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredConfig {
    pub version: u32,
    pub bindings: Vec<(BindingTarget, Trigger)>,
    pub cycle_order: Vec<String>,
    pub lang: Lang,
    pub theme: Theme,
    /// Title-matching regex used by the watcher to identify Dofus windows.
    /// `None` (or missing in older JSONs) → fall back to
    /// `crate::dofus::DEFAULT_TITLE_REGEX`. Lets users adapt to future Dofus
    /// title format changes without rebuilding.
    #[serde(default)]
    pub title_regex: Option<String>,
    /// Bindings only fire while a Dofus window is in front. Off when missing:
    /// existing users keep shortcuts that work from any application.
    #[serde(default)]
    pub shortcuts_only_in_dofus: bool,
    /// "–" and "Activer" minimize to the taskbar instead of hiding in the
    /// system tray.
    #[serde(default)]
    pub minimize_to_taskbar: bool,
    /// Main window kept above every other one. On when missing: it always
    /// was before the option existed.
    #[serde(default = "default_true")]
    pub always_on_top: bool,
    /// Top-left corner of the main window, in the screen coordinates of
    /// `platform::placement` (Windows: physical pixels, macOS: points).
    #[serde(default)]
    pub window_pos: Option<(i32, i32)>,
    /// Character overlay shown while ROrganizer is active. On when missing.
    #[serde(default = "default_true")]
    pub overlay_enabled: bool,
    #[serde(default)]
    pub overlay_layout: overlay::Layout,
    #[serde(default)]
    pub overlay_display: overlay::Display,
    /// Side of one head, in points.
    #[serde(default = "default_head_size")]
    pub overlay_head_size: f32,
    /// Top-left corner of the overlay, in the screen coordinates of
    /// `platform::overlay`.
    #[serde(default)]
    pub overlay_pos: Option<(i32, i32)>,
}

fn default_true() -> bool {
    true
}

fn default_head_size() -> f32 {
    overlay::HEAD_DEFAULT
}

/// `%APPDATA%\rorganizer\config.json` on Windows,
/// `~/Library/Application Support/rorganizer/config.json` on macOS. None
/// when the system reports no usable config dir.
pub fn default_path() -> Option<PathBuf> {
    use directories::BaseDirs;
    let base = BaseDirs::new()?;
    Some(base.config_dir().join("rorganizer").join("config.json"))
}

/// Reads + parses the config file. Any failure (missing, corrupted, version
/// mismatch) returns None — the caller falls back to defaults silently.
pub fn load(path: &Path) -> Option<StoredConfig> {
    let text = std::fs::read_to_string(path).ok()?;
    let cfg: StoredConfig = serde_json::from_str(&text).ok()?;
    if cfg.version != CURRENT_VERSION {
        return None;
    }
    Some(cfg)
}

/// Sibling path: `<file>.tmp` next to `path`, in the same directory so the
/// final `rename` stays on a single filesystem (atomic on Windows + Linux).
fn tmp_path(path: &Path) -> std::path::PathBuf {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config".into());
    path.with_file_name(format!("{name}.tmp"))
}

/// Writes the config to a sibling `.tmp` then renames to the final path so a
/// crash mid-write never corrupts the existing file.
///
/// Calls `sync_all()` on the tmp before the rename to flush the file's
/// data + metadata to the disk controller; without it, a power loss
/// between write and rename can leave a zero-byte tmp on disk and an
/// older valid `config.json` — annoying but recoverable. With it, the
/// final state on disk is either "old config" or "new config", never
/// half-written.
pub fn save_atomic(path: &Path, cfg: &StoredConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = tmp_path(path);
    let text = serde_json::to_string_pretty(cfg).map_err(std::io::Error::other)?;
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(tmp, path)?;
    Ok(())
}

pub const fn current_version() -> u32 {
    CURRENT_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triggers::{BindingTarget, MouseBtn, Trigger, WheelDir};
    use tempfile::tempdir;

    fn sample_config() -> StoredConfig {
        StoredConfig {
            version: CURRENT_VERSION,
            bindings: vec![
                (BindingTarget::Account("Dowoop".into()), Trigger::Key(0x70)),
                (BindingTarget::CycleNext, Trigger::Mouse(MouseBtn::X1)),
                (BindingTarget::CyclePrev, Trigger::Wheel(WheelDir::Down)),
            ],
            cycle_order: vec!["Dowoop".into(), "Dowoip".into()],
            lang: Lang::Fr,
            theme: Theme::Dark,
            title_regex: None,
            shortcuts_only_in_dofus: true,
            minimize_to_taskbar: true,
            always_on_top: false,
            window_pos: Some((-1200, 340)),
            overlay_enabled: false,
            overlay_layout: overlay::Layout::Vertical,
            overlay_display: overlay::Display::Initials,
            overlay_head_size: 56.0,
            overlay_pos: Some((-900, 12)),
        }
    }

    /// A config.json as ROrganizer 1.4.0 writes it.
    const CONFIG_1_4_0: &str = r#"{
  "version": 1,
  "bindings": [
    [{ "Account": "Dowoop" }, { "Mouse": "X1" }],
    [{ "Account": "Dowoup" }, { "Key": 112 }],
    [{ "Account": "Dowoap" }, { "Mouse": "Middle" }],
    [{ "Account": "Dosac" }, { "Key": 222 }],
    ["CycleNext", { "Wheel": "Up" }],
    ["CyclePrev", { "Key": 9 }]
  ],
  "cycle_order": ["Dowoop", "Dowoup", "Dowoap", "Dosac"],
  "lang": "Fr",
  "theme": "Dark",
  "title_regex": null,
  "shortcuts_only_in_dofus": false,
  "minimize_to_taskbar": false,
  "always_on_top": true,
  "window_pos": [1124, 450]
}"#;

    #[test]
    fn existing_config_loads_and_saves_unchanged() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, CONFIG_1_4_0).unwrap();
        let cfg = load(&path).expect("1.4.0 config loads");
        assert_eq!(
            cfg.bindings,
            vec![
                (BindingTarget::Account("Dowoop".into()), Trigger::Mouse(MouseBtn::X1)),
                (BindingTarget::Account("Dowoup".into()), Trigger::Key(112)),
                (BindingTarget::Account("Dowoap".into()), Trigger::Mouse(MouseBtn::Middle)),
                (BindingTarget::Account("Dosac".into()), Trigger::Key(222)),
                (BindingTarget::CycleNext, Trigger::Wheel(WheelDir::Up)),
                (BindingTarget::CyclePrev, Trigger::Key(9)),
            ]
        );
        assert_eq!(cfg.cycle_order, ["Dowoop", "Dowoup", "Dowoap", "Dosac"]);
        assert_eq!(cfg.window_pos, Some((1124, 450)));

        save_atomic(&path, &cfg).unwrap();
        let mut saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        // Every 1.4.0 value is written back as is; the overlay keys are added
        // with their defaults.
        let obj = saved.as_object_mut().unwrap();
        assert_eq!(obj.remove("overlay_enabled"), Some(serde_json::json!(true)));
        assert_eq!(obj.remove("overlay_layout"), Some(serde_json::json!("Horizontal")));
        assert_eq!(obj.remove("overlay_display"), Some(serde_json::json!("ClassHead")));
        assert_eq!(obj.remove("overlay_head_size"), Some(serde_json::json!(40.0)));
        assert_eq!(obj.remove("overlay_pos"), Some(serde_json::Value::Null));
        let original: serde_json::Value = serde_json::from_str(CONFIG_1_4_0).unwrap();
        assert_eq!(saved, original);
    }

    #[test]
    fn config_written_before_the_settings_keeps_previous_behaviour() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut json = serde_json::to_value(sample_config()).unwrap();
        let obj = json.as_object_mut().unwrap();
        for key in [
            "shortcuts_only_in_dofus",
            "minimize_to_taskbar",
            "always_on_top",
            "window_pos",
            "overlay_enabled",
            "overlay_layout",
            "overlay_display",
            "overlay_head_size",
            "overlay_pos",
        ] {
            obj.remove(key);
        }
        std::fs::write(&path, json.to_string()).unwrap();
        let cfg = load(&path).expect("old config loads");
        assert!(!cfg.shortcuts_only_in_dofus);
        assert!(!cfg.minimize_to_taskbar);
        assert!(cfg.always_on_top);
        assert_eq!(cfg.window_pos, None);
        assert!(cfg.overlay_enabled);
        assert_eq!(cfg.overlay_layout, overlay::Layout::Horizontal);
        assert_eq!(cfg.overlay_display, overlay::Display::ClassHead);
        assert_eq!(cfg.overlay_head_size, overlay::HEAD_DEFAULT);
        assert_eq!(cfg.overlay_pos, None);
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let cfg = sample_config();
        save_atomic(&path, &cfg).unwrap();
        let loaded = load(&path).expect("load should succeed");
        assert_eq!(loaded.version, cfg.version);
        assert_eq!(loaded.bindings, cfg.bindings);
        assert_eq!(loaded.cycle_order, cfg.cycle_order);
        assert_eq!(loaded.lang, cfg.lang);
        assert_eq!(loaded.theme, cfg.theme);
        assert_eq!(loaded.shortcuts_only_in_dofus, cfg.shortcuts_only_in_dofus);
        assert_eq!(loaded.minimize_to_taskbar, cfg.minimize_to_taskbar);
        assert_eq!(loaded.always_on_top, cfg.always_on_top);
        assert_eq!(loaded.window_pos, cfg.window_pos);
        assert_eq!(loaded.overlay_enabled, cfg.overlay_enabled);
        assert_eq!(loaded.overlay_layout, cfg.overlay_layout);
        assert_eq!(loaded.overlay_display, cfg.overlay_display);
        assert_eq!(loaded.overlay_head_size, cfg.overlay_head_size);
        assert_eq!(loaded.overlay_pos, cfg.overlay_pos);
    }

    #[test]
    fn load_returns_none_on_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nope.json");
        assert!(load(&path).is_none());
    }

    #[test]
    fn load_returns_none_on_corrupt_json() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "not json {{{").unwrap();
        assert!(load(&path).is_none());
    }

    #[test]
    fn load_returns_none_on_version_mismatch() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut cfg = sample_config();
        cfg.version = CURRENT_VERSION + 1;
        save_atomic(&path, &cfg).unwrap();
        assert!(load(&path).is_none());
    }

    #[test]
    fn save_atomic_does_not_leave_tmp_on_success() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        save_atomic(&path, &sample_config()).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["config.json"], "no .tmp leftover, got {entries:?}");
    }

    #[test]
    fn save_creates_parent_directory() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("deeper").join("config.json");
        save_atomic(&path, &sample_config()).unwrap();
        assert!(path.exists());
    }
}
