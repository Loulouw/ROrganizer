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
            Lang::Fr => include_bytes!("../resources/flags/fr.svg"),
            Lang::En => include_bytes!("../resources/flags/en.svg"),
            Lang::Es => include_bytes!("../resources/flags/es.svg"),
        }
    }
}

#[cfg(windows)]
pub fn detect_system_lang() -> Lang {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buf = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if len > 0 {
        let used = (len as usize).saturating_sub(1);
        let s = String::from_utf16_lossy(&buf[..used]);
        match s.split('-').next().unwrap_or("") {
            "fr" => Lang::Fr,
            "es" => Lang::Es,
            _ => Lang::En,
        }
    } else {
        Lang::En
    }
}

#[cfg(not(windows))]
pub fn detect_system_lang() -> Lang {
    Lang::En
}

pub fn set_lang(lang: Lang) {
    rust_i18n::set_locale(lang.code());
}
