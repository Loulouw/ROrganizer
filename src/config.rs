use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;
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
}

impl Default for StoredConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            bindings: Vec::new(),
            cycle_order: Vec::new(),
            lang: Lang::En,
            theme: Theme::Dark,
        }
    }
}

/// `%APPDATA%\rorganizer\config.json` on Windows. Returns None on platforms
/// where no usable config dir is reported (extremely rare on Windows).
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

/// Writes the config to a `.tmp` then renames to the final path so a crash
/// mid-write never corrupts the existing file.
pub fn save_atomic(path: &Path, cfg: &StoredConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(cfg).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

pub const fn current_version() -> u32 {
    CURRENT_VERSION
}
