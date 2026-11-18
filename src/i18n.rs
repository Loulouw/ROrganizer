use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    Fr,
    En,
    Es,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Fr, Lang::En, Lang::Es];

    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
            Lang::Es => "Español",
        }
    }

    pub fn flag_uri(self) -> &'static str {
        match self {
            Lang::Fr => "bytes://flag-fr.svg",
            Lang::En => "bytes://flag-en.svg",
            Lang::Es => "bytes://flag-es.svg",
        }
    }

    pub fn flag_bytes(self) -> &'static [u8] {
        match self {
            Lang::Fr => crate::assets::FLAG_FR_SVG,
            Lang::En => crate::assets::FLAG_EN_SVG,
            Lang::Es => crate::assets::FLAG_ES_SVG,
        }
    }
}

pub fn set_lang(lang: Lang) {
    rust_i18n::set_locale(lang.code());
}
