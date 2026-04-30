use std::sync::mpsc::Sender;
use std::sync::OnceLock;

use eframe::egui;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, SetForegroundWindow, ShowWindow, SW_SHOW, WM_CLOSE,
};

#[derive(Debug, Clone, Copy)]
pub enum TrayEvent {
    ShowRequested,
    QuitRequested,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum TrayCommand {
    SetActiveCount(usize),
    SetActive(bool),
}

/// Main window HWND, captured during App::new so the tray handlers can
/// directly drive ShowWindow / WM_CLOSE while the eframe loop is parked
/// (which happens whenever the viewport is set to Visible(false) — at that
/// point Context::request_repaint() does not wake update()).
static MAIN_HWND: OnceLock<isize> = OnceLock::new();

pub fn register_main_hwnd(hwnd: isize) {
    let _ = MAIN_HWND.set(hwnd);
}

#[cfg(windows)]
fn show_main_window() {
    if let Some(&raw) = MAIN_HWND.get() {
        let hwnd = HWND(raw as *mut _);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(windows)]
fn close_main_window() {
    if let Some(&raw) = MAIN_HWND.get() {
        let hwnd = HWND(raw as *mut _);
        unsafe {
            // When the viewport is Visible(false) eframe parks its main loop
            // and Context::request_repaint() does not wake it. PostMessage(WM_CLOSE)
            // queues the close but it sits there until something else wakes winit.
            // Showing the window first delivers a window-event that wakes the loop;
            // the queued WM_CLOSE then drains immediately and eframe exits cleanly.
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(not(windows))]
fn show_main_window() {}
#[cfg(not(windows))]
fn close_main_window() {}

pub fn install(
    ctx: egui::Context,
    tx: Sender<TrayEvent>,
) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let menu = Menu::new();
    let show_item = MenuItem::new(rust_i18n::t!("tray.show").as_ref(), true, None);
    let quit_item = MenuItem::new(rust_i18n::t!("tray.quit").as_ref(), true, None);
    menu.append(&show_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit_item)?;

    let show_id = show_item.id().clone();
    let quit_id = quit_item.id().clone();

    let ctx_menu = ctx.clone();
    let tx_menu = tx.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        if ev.id == show_id {
            show_main_window();
            let _ = tx_menu.send(TrayEvent::ShowRequested);
            ctx_menu.request_repaint();
        } else if ev.id == quit_id {
            close_main_window();
            let _ = tx_menu.send(TrayEvent::QuitRequested);
            ctx_menu.request_repaint();
        }
    }));

    let ctx_tray = ctx;
    let tx_tray = tx;
    TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
        let is_show = match ev {
            TrayIconEvent::DoubleClick { .. } => true,
            TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } => true,
            _ => false,
        };
        if is_show {
            show_main_window();
            let _ = tx_tray.send(TrayEvent::ShowRequested);
            ctx_tray.request_repaint();
        }
    }));

    let icon = build_icon()?;
    let tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_tooltip("ROrganizer")
        .build()?;
    Ok(tray)
}

fn build_icon() -> Result<Icon, tray_icon::BadIcon> {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let edge = x < 2 || x >= SIZE - 2 || y < 2 || y >= SIZE - 2;
            let (r, g, b) = if edge {
                (0x97u8, 0xC4u8, 0x59u8)
            } else {
                (0x26u8, 0x26u8, 0x1Fu8)
            };
            rgba.extend_from_slice(&[r, g, b, 0xFFu8]);
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE)
}
