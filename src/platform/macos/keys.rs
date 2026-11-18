use eframe::egui;

/// Inert: keyboard shortcuts can't be captured yet; mouse and wheel
/// triggers are platform-independent and still work in the binding modal.
pub fn key_from_event(_key: egui::Key, _physical_key: Option<egui::Key>) -> Option<u32> {
    None
}

pub fn key_label(code: u32) -> String {
    format!("0x{code:02X}")
}
