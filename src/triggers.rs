use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::platform;

pub type SlotKey = String;

/// What the user is binding a Trigger to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BindingTarget {
    Account(SlotKey),
    CycleNext,
    CyclePrev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Trigger {
    /// Platform-native key code: Win32 virtual-key on Windows (VK_F1 = 0x70,
    /// VK_A = 0x41, …), virtual key code `kVK_*` on macOS (F1 = 0x7A).
    Key(u32),
    Mouse(MouseBtn),
    Wheel(WheelDir),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseBtn {
    Middle,
    /// XBUTTON1 — "mb4" / side back.
    X1,
    /// XBUTTON2 — "mb5" / side forward.
    X2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WheelDir {
    Up,
    Down,
}

impl Trigger {
    /// Key press seen by egui, coded as the platform hook will report it.
    pub fn from_key_event(key: egui::Key, physical_key: Option<egui::Key>) -> Option<Self> {
        platform::key_from_event(key, physical_key).map(Trigger::Key)
    }

    pub fn display_label(&self) -> String {
        match self {
            Trigger::Key(code) => platform::key_label(*code),
            Trigger::Mouse(MouseBtn::Middle) => "MWheel".into(),
            Trigger::Mouse(MouseBtn::X1) => "Mb4".into(),
            Trigger::Mouse(MouseBtn::X2) => "Mb5".into(),
            Trigger::Wheel(WheelDir::Up) => "MWh\u{2191}".into(),
            Trigger::Wheel(WheelDir::Down) => "MWh\u{2193}".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_display_label() {
        assert_eq!(Trigger::Mouse(MouseBtn::Middle).display_label(), "MWheel");
        assert_eq!(Trigger::Mouse(MouseBtn::X1).display_label(), "Mb4");
        assert_eq!(Trigger::Mouse(MouseBtn::X2).display_label(), "Mb5");
        assert_eq!(Trigger::Wheel(WheelDir::Up).display_label(), "MWh\u{2191}");
        assert_eq!(Trigger::Wheel(WheelDir::Down).display_label(), "MWh\u{2193}");
    }
}
