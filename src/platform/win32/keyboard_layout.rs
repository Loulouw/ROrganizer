//! Queries against the keyboard layout of the calling thread.
//!
//! The binding modal reads keys through egui, which names them after their
//! US-QWERTY legend; the LL hook receives the VK the user's layout assigns
//! to the scan code (AZERTY `²` is VK_OEM_7 0xDE, not the US backtick
//! 0xC0). Capture and display therefore ask Windows. Called on the UI
//! thread, whose layout is the one the user types with while our window
//! has focus.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, MAPVK_VK_TO_CHAR, MAPVK_VSC_TO_VK,
};

/// VK the active layout produces for set-1 `scan_code`.
pub fn scan_code_to_vk(scan_code: u32) -> Option<u32> {
    let vk = unsafe { MapVirtualKeyW(scan_code, MAPVK_VSC_TO_VK) };
    (vk != 0).then_some(vk)
}

/// Unshifted character printed by `vk` on the active layout.
pub fn vk_to_char(vk: u32) -> Option<char> {
    let raw = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_CHAR) };
    // Dead keys (AZERTY `^`, `¨`) come back with the top bit set.
    let c = char::from_u32(raw & 0x7FFF_FFFF)?;
    (c != '\0' && !c.is_control()).then_some(c)
}
