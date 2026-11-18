use crate::platform::MainWindow;

/// Inert: the position isn't tracked yet, the window opens centred.
pub fn position(_window: MainWindow) -> Option<(i32, i32)> {
    None
}

pub fn restore(_window: MainWindow, _pos: (i32, i32)) {}
