//! Static asset bytes embedded into the binary at compile time.
//! Single source of truth so the same PNG / SVG isn't `include_bytes!`-ed
//! from multiple modules.

pub const ICON_PNG: &[u8] = include_bytes!("../resources/icons/icon.png");
pub const GITHUB_SVG: &[u8] = include_bytes!("../resources/icons/github.svg");
pub const FLAG_FR_SVG: &[u8] = include_bytes!("../resources/flags/fr.svg");
pub const FLAG_EN_SVG: &[u8] = include_bytes!("../resources/flags/en.svg");
pub const FLAG_ES_SVG: &[u8] = include_bytes!("../resources/flags/es.svg");

/// Class avatar PNGs keyed by canonical filename stem (lowercase, no
/// accent). The key is the `class_filename()` output and is reused as
/// the egui texture cache key. Order is alphabetical for diff stability.
pub const CLASS_PNGS: &[(&str, &[u8])] = &[
    ("cra", include_bytes!("../resources/classes/cra.png")),
    ("ecaflip", include_bytes!("../resources/classes/ecaflip.png")),
    ("eliotrope", include_bytes!("../resources/classes/eliotrope.png")),
    ("eniripsa", include_bytes!("../resources/classes/eniripsa.png")),
    ("enutrof", include_bytes!("../resources/classes/enutrof.png")),
    ("feca", include_bytes!("../resources/classes/feca.png")),
    ("forgelance", include_bytes!("../resources/classes/forgelance.png")),
    ("huppermage", include_bytes!("../resources/classes/huppermage.png")),
    ("iop", include_bytes!("../resources/classes/iop.png")),
    ("osamodas", include_bytes!("../resources/classes/osamodas.png")),
    ("ouginak", include_bytes!("../resources/classes/ouginak.png")),
    ("pandawa", include_bytes!("../resources/classes/pandawa.png")),
    ("roublard", include_bytes!("../resources/classes/roublard.png")),
    ("sacrieur", include_bytes!("../resources/classes/sacrieur.png")),
    ("sadida", include_bytes!("../resources/classes/sadida.png")),
    ("sram", include_bytes!("../resources/classes/sram.png")),
    ("steamer", include_bytes!("../resources/classes/steamer.png")),
    ("xelor", include_bytes!("../resources/classes/xelor.png")),
    ("zobal", include_bytes!("../resources/classes/zobal.png")),
];
