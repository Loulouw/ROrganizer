use windows::Win32::Globalization::GetUserDefaultLocaleName;

use crate::i18n::Lang;

pub fn detect_system_lang() -> Lang {
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
