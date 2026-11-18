use std::sync::mpsc::Receiver;

use eframe::egui;

use crate::dofus::WindowsSnapshot;
use crate::platform::WindowId;

/// Inert: Dofus clients are not detected yet, the snapshot stays empty.
pub fn spawn_watcher(
    _snapshot: WindowsSnapshot,
    _ctx: egui::Context,
    _regex_src: String,
    _refresh_rx: Receiver<()>,
) {
}

pub fn is_dofus_window(_id: WindowId) -> bool {
    false
}
