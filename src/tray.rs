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
    ToggleRequested,
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
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(not(windows))]
fn show_main_window() {}
#[cfg(not(windows))]
fn close_main_window() {}

/// Public entry point used by `single_instance` to wake the existing
/// instance when a second launch is detected.
pub fn show_main_window_external() {
    show_main_window();
}

/// Holds the live tray icon plus mutable handles on items whose label
/// changes during the session (Activer/Désactiver toggle + status line).
pub struct TrayController {
    icon: TrayIcon,
    toggle_item: MenuItem,
    status_item: MenuItem,
}

impl TrayController {
    pub fn set_active(&mut self, active: bool) {
        let label = if active {
            rust_i18n::t!("tray.deactivate")
        } else {
            rust_i18n::t!("tray.activate")
        };
        self.toggle_item.set_text(label.as_ref());
        if let Ok(icon) = build_icon(active) {
            let _ = self.icon.set_icon(Some(icon));
        }
    }

    pub fn set_status(&mut self, active: bool, count: usize) {
        let label = if active {
            rust_i18n::t!("tray.status_active", count = count.to_string()).to_string()
        } else {
            rust_i18n::t!("tray.status_inactive").to_string()
        };
        self.status_item.set_text(label);
    }
}

pub fn install(
    ctx: egui::Context,
    tx: Sender<TrayEvent>,
) -> Result<TrayController, Box<dyn std::error::Error>> {
    let menu = Menu::new();
    let show_item = MenuItem::new(rust_i18n::t!("tray.show").as_ref(), true, None);
    let toggle_item = MenuItem::new(rust_i18n::t!("tray.activate").as_ref(), true, None);
    let status_item = MenuItem::new(
        rust_i18n::t!("tray.status_inactive").as_ref(),
        false, // disabled — informational only
        None,
    );
    let quit_item = MenuItem::new(rust_i18n::t!("tray.quit").as_ref(), true, None);

    menu.append(&show_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&status_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&toggle_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit_item)?;

    let show_id = show_item.id().clone();
    let toggle_id = toggle_item.id().clone();
    let quit_id = quit_item.id().clone();

    let ctx_menu = ctx.clone();
    let tx_menu = tx.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        if ev.id == show_id {
            show_main_window();
            let _ = tx_menu.send(TrayEvent::ShowRequested);
            ctx_menu.request_repaint();
        } else if ev.id == toggle_id {
            // Always show the window before forwarding the event. If the
            // toggle is going to "active" (hide), update() will send
            // Visible(false) right after — net effect = hide. If it's going
            // to "inactive" (show), this call wakes the parked eframe loop
            // so the ToggleRequested event can be drained.
            show_main_window();
            let _ = tx_menu.send(TrayEvent::ToggleRequested);
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
        let is_show = matches!(
            ev,
            TrayIconEvent::DoubleClick { .. }
                | TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                }
        );
        if is_show {
            show_main_window();
            let _ = tx_tray.send(TrayEvent::ShowRequested);
            ctx_tray.request_repaint();
        }
    }));

    let icon = build_icon(false)?;
    let tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_tooltip("ROrganizer")
        .build()?;

    Ok(TrayController {
        icon: tray,
        toggle_item,
        status_item,
    })
}

const ICON_PNG: &[u8] = include_bytes!("../resources/icons/icon.png");
const ICON_SIZE: u32 = 32;

static ACTIVE_RGBA: OnceLock<Vec<u8>> = OnceLock::new();
static INACTIVE_RGBA: OnceLock<Vec<u8>> = OnceLock::new();

fn ensure_icons() -> (&'static [u8], &'static [u8]) {
    let active = ACTIVE_RGBA.get_or_init(|| {
        let img = image::load_from_memory(ICON_PNG).expect("decode icon.png");
        let resized =
            img.resize_exact(ICON_SIZE, ICON_SIZE, image::imageops::FilterType::Lanczos3);
        resized.to_rgba8().into_raw()
    });
    let inactive = INACTIVE_RGBA.get_or_init(|| {
        let mut bytes = active.clone();
        // Rec.709 luma; alpha unchanged so the inactive icon stays visible.
        for px in bytes.chunks_exact_mut(4) {
            let l = (px[0] as f32 * 0.2126 + px[1] as f32 * 0.7152 + px[2] as f32 * 0.0722) as u8;
            px[0] = l;
            px[1] = l;
            px[2] = l;
        }
        bytes
    });
    (active.as_slice(), inactive.as_slice())
}

fn build_icon(active: bool) -> Result<Icon, tray_icon::BadIcon> {
    let (a, i) = ensure_icons();
    let bytes = if active { a } else { i };
    Icon::from_rgba(bytes.to_vec(), ICON_SIZE, ICON_SIZE)
}
