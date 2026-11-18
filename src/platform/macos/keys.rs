//! `Trigger::Key` holds a macOS virtual key code (`kVK_*`): the physical
//! key, whatever the layout. The event tap receives it as is.

use std::ffi::c_void;

use eframe::egui;
use objc2::MainThreadMarker;

use super::hooks::take_own_key;

/// Virtual key code the event tap will receive for this key press.
///
/// The code the tap itself saw takes precedence: egui can't tell every key
/// apart. winit names both the ISO key left of `1` (`@` on a French Mac)
/// and the one left of `Z` `Backquote`, and reports numpad digits as the
/// top-row ones. Without a fresh tap reading (no Accessibility grant yet),
/// the key is looked up by its US-QWERTY position.
pub fn key_from_event(key: egui::Key, physical_key: Option<egui::Key>) -> Option<u32> {
    let egui_code = code_of(physical_key.unwrap_or(key)).or_else(|| code_of(key));
    pick(take_own_key(), egui_code).map(u32::from)
}

/// The tap holds only the latest press: with two keys down in the same
/// frame, it already reads the second while egui reports the first. Its
/// code is kept only when it is the key egui names, or one egui confuses
/// with it.
fn pick(own: Option<u16>, egui_code: Option<u16>) -> Option<u16> {
    let same_key = |own: u16| {
        egui_code.is_none_or(|e| e == own || CONFUSED.contains(&(own, e)))
    };
    own.filter(|&c| same_key(c)).or(egui_code)
}

/// Keys egui reports under another key's name: (code of the key, code of
/// the key egui names instead).
const CONFUSED: &[(u16, u16)] = &[
    // ISO key left of `Z`, named `Backtick`.
    (0x0A, 0x32),
    // Numpad digits, named as the top-row ones.
    (0x52, 0x1D), (0x53, 0x12), (0x54, 0x13), (0x55, 0x14), (0x56, 0x15),
    (0x57, 0x17), (0x58, 0x16), (0x59, 0x1A), (0x5B, 0x1C), (0x5C, 0x19),
    // Numpad `-`, `/`, Enter, `=`; the decimal key by the character it
    // prints, `.` or `,` depending on the layout.
    (0x4E, 0x1B), (0x4B, 0x2C), (0x4C, 0x24), (0x51, 0x18), (0x41, 0x2F), (0x41, 0x2B),
];

pub fn key_label(code: u32) -> String {
    let Ok(code) = u16::try_from(code) else { return format!("0x{code:X}") };
    if let Some(name) = named_label(code) {
        return name.to_string();
    }
    layout_char(code)
        .map(|c| c.to_uppercase().collect())
        .or_else(|| key_of(code).map(|k| k.symbol_or_name().to_string()))
        .unwrap_or_else(|| format!("0x{code:02X}"))
}

/// Physical keys by their US-QWERTY name, as egui reports them.
const KEYS: &[(egui::Key, u16)] = {
    use egui::Key::*;
    &[
        (A, 0x00), (S, 0x01), (D, 0x02), (F, 0x03), (H, 0x04), (G, 0x05), (Z, 0x06),
        (X, 0x07), (C, 0x08), (V, 0x09), (B, 0x0B), (Q, 0x0C), (W, 0x0D), (E, 0x0E),
        (R, 0x0F), (Y, 0x10), (T, 0x11), (Num1, 0x12), (Num2, 0x13), (Num3, 0x14),
        (Num4, 0x15), (Num6, 0x16), (Num5, 0x17), (Equals, 0x18), (Num9, 0x19),
        (Num7, 0x1A), (Minus, 0x1B), (Num8, 0x1C), (Num0, 0x1D), (CloseBracket, 0x1E),
        (O, 0x1F), (U, 0x20), (OpenBracket, 0x21), (I, 0x22), (P, 0x23), (Enter, 0x24),
        (L, 0x25), (J, 0x26), (Quote, 0x27), (K, 0x28), (Semicolon, 0x29),
        (Backslash, 0x2A), (Comma, 0x2B), (Slash, 0x2C), (N, 0x2D), (M, 0x2E),
        (Period, 0x2F), (Tab, 0x30), (Space, 0x31), (Backtick, 0x32), (Backspace, 0x33),
        (Escape, 0x35), (F17, 0x40), (Plus, 0x45), (F18, 0x4F), (F19, 0x50), (F20, 0x5A),
        (F5, 0x60), (F6, 0x61), (F7, 0x62), (F3, 0x63), (F8, 0x64), (F9, 0x65),
        (F11, 0x67), (F13, 0x69), (F16, 0x6A), (F14, 0x6B), (F10, 0x6D), (F12, 0x6F),
        (F15, 0x71), (Insert, 0x72), (Home, 0x73), (PageUp, 0x74), (Delete, 0x75),
        (F4, 0x76), (End, 0x77), (F2, 0x78), (PageDown, 0x79), (F1, 0x7A),
        (ArrowLeft, 0x7B), (ArrowRight, 0x7C), (ArrowDown, 0x7D), (ArrowUp, 0x7E),
    ]
};

fn code_of(key: egui::Key) -> Option<u16> {
    KEYS.iter().find(|(k, _)| *k == key).map(|(_, c)| *c)
}

fn key_of(code: u16) -> Option<egui::Key> {
    KEYS.iter().find(|(_, c)| *c == code).map(|(k, _)| *k)
}

/// Keys whose name doesn't depend on the layout. Same names as on Windows.
fn named_label(code: u16) -> Option<&'static str> {
    Some(match code {
        0x7A => "F1",
        0x78 => "F2",
        0x63 => "F3",
        0x76 => "F4",
        0x60 => "F5",
        0x61 => "F6",
        0x62 => "F7",
        0x64 => "F8",
        0x65 => "F9",
        0x6D => "F10",
        0x67 => "F11",
        0x6F => "F12",
        0x69 => "F13",
        0x6B => "F14",
        0x71 => "F15",
        0x6A => "F16",
        0x40 => "F17",
        0x4F => "F18",
        0x50 => "F19",
        0x5A => "F20",
        0x52 => "Num0",
        0x53 => "Num1",
        0x54 => "Num2",
        0x55 => "Num3",
        0x56 => "Num4",
        0x57 => "Num5",
        0x58 => "Num6",
        0x59 => "Num7",
        0x5B => "Num8",
        0x5C => "Num9",
        0x41 => "Num.",
        0x43 => "Num*",
        0x45 => "Num+",
        0x4B => "Num/",
        0x4C => "NumEnter",
        0x4E => "Num-",
        0x51 => "Num=",
        0x47 => "Clear",
        0x24 => "Enter",
        0x30 => "Tab",
        0x31 => "Space",
        0x33 => "Backspace",
        0x35 => "Esc",
        0x72 => "Ins",
        0x75 => "Del",
        0x73 => "Home",
        0x77 => "End",
        0x74 => "PgUp",
        0x79 => "PgDn",
        0x7B => "\u{2190}",
        0x7E => "\u{2191}",
        0x7C => "\u{2192}",
        0x7D => "\u{2193}",
        _ => return None,
    })
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut c_void;
    fn TISGetInputSourceProperty(source: *mut c_void, key: *const c_void) -> *const c_void;
    static kTISPropertyUnicodeKeyLayoutData: *const c_void;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        layout: *const c_void,
        code: u16,
        action: u16,
        modifiers: u32,
        keyboard_type: u32,
        options: u32,
        dead_key_state: *mut u32,
        max_length: usize,
        length: *mut usize,
        chars: *mut u16,
    ) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
}

const KUC_KEY_ACTION_DISPLAY: u16 = 3;
const KUC_KEY_TRANSLATE_NO_DEAD_KEYS: u32 = 1;

/// Unshifted character printed by `code` on the active layout (`²`, `@`,
/// `&` on a French Mac). The input source API must be called on the main
/// thread: elsewhere, no character.
fn layout_char(code: u16) -> Option<char> {
    MainThreadMarker::new()?;
    unsafe {
        let source = TISCopyCurrentKeyboardLayoutInputSource();
        if source.is_null() {
            return None;
        }
        let data = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
        let mut chars = [0u16; 4];
        let mut len = 0usize;
        let mut dead = 0u32;
        let status = if data.is_null() {
            -1
        } else {
            UCKeyTranslate(
                CFDataGetBytePtr(data).cast(),
                code,
                KUC_KEY_ACTION_DISPLAY,
                0,
                u32::from(LMGetKbdType()),
                KUC_KEY_TRANSLATE_NO_DEAD_KEYS,
                &mut dead,
                chars.len(),
                &mut len,
                chars.as_mut_ptr(),
            )
        };
        CFRelease(source);
        if status != 0 || len == 0 {
            return None;
        }
        let c = char::decode_utf16(chars[..len].iter().copied()).next()?.ok()?;
        (!c.is_control()).then_some(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_physical_key_has_its_own_code() {
        let mut seen = std::collections::HashMap::new();
        for (key, code) in KEYS {
            if let Some(other) = seen.insert(*code, *key) {
                panic!("{key:?} and {other:?} share code {code:#04X}");
            }
        }
    }

    #[test]
    fn egui_keys_map_to_their_physical_codes() {
        // No tap reading in tests: the US-position table answers.
        assert_eq!(key_from_event(egui::Key::A, Some(egui::Key::Q)), Some(0x0C));
        assert_eq!(key_from_event(egui::Key::F1, None), Some(0x7A));
        assert_eq!(key_from_event(egui::Key::Backtick, Some(egui::Key::Backtick)), Some(0x32));
        assert_eq!(key_from_event(egui::Key::F21, None), None);
    }

    #[test]
    fn tap_reading_wins_only_for_the_key_egui_reports() {
        // ISO key left of `Z` and numpad 1: egui names them wrongly.
        assert_eq!(pick(Some(0x0A), Some(0x32)), Some(0x0A));
        assert_eq!(pick(Some(0x53), Some(0x12)), Some(0x53));
        // Two keys in one frame: the tap already holds the second (`S`)
        // while egui reports the first (`A`).
        assert_eq!(pick(Some(0x01), Some(0x00)), Some(0x00));
        // A key missing from the table: only the tap knows it.
        assert_eq!(pick(Some(0x5F), None), Some(0x5F));
        assert_eq!(pick(None, Some(0x00)), Some(0x00));
    }

    #[test]
    fn named_keys_share_the_windows_labels() {
        assert_eq!(key_label(0x7A), "F1");
        assert_eq!(key_label(0x5A), "F20");
        assert_eq!(key_label(0x53), "Num1");
        assert_eq!(key_label(0x31), "Space");
        assert_eq!(key_label(0x7B), "\u{2190}");
    }
}
