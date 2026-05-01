//! Static asset bytes embedded into the binary at compile time.
//! Single source of truth so the same PNG / SVG isn't `include_bytes!`-ed
//! from multiple modules.

pub const ICON_PNG: &[u8] = include_bytes!("../resources/icons/icon.png");
pub const GITHUB_SVG: &[u8] = include_bytes!("../resources/icons/github.svg");
pub const FLAG_FR_SVG: &[u8] = include_bytes!("../resources/flags/fr.svg");
pub const FLAG_EN_SVG: &[u8] = include_bytes!("../resources/flags/en.svg");
pub const FLAG_ES_SVG: &[u8] = include_bytes!("../resources/flags/es.svg");
