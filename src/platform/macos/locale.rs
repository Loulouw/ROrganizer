use objc2_foundation::NSLocale;

use crate::i18n::Lang;

/// First language of the user's ordered list (System Settings > Language &
/// Region), e.g. `fr-FR`.
pub fn detect_system_lang() -> Lang {
    let preferred = NSLocale::preferredLanguages();
    let first = preferred.firstObject().map(|s| s.to_string()).unwrap_or_default();
    match first.split(['-', '_']).next().unwrap_or("") {
        "fr" => Lang::Fr,
        "es" => Lang::Es,
        _ => Lang::En,
    }
}
