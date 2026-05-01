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
    ToggleRequested,
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
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
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

/// Holds the live tray icon plus mutable handles on every item whose label
/// can change at runtime — either through Activer/Désactiver toggling or
/// through a language switch.
pub struct TrayController {
    icon: TrayIcon,
    icon_active: Icon,
    icon_inactive: Icon,
    show_item: MenuItem,
    toggle_item: MenuItem,
    status_item: MenuItem,
    quit_item: MenuItem,
    /// Last (active, count) pushed via `set_status`. Cached so `relocalize`
    /// can rebuild the localized status label without the caller having to
    /// re-pass it.
    last_status: (bool, usize),
}

impl TrayController {
    pub fn set_active(&mut self, active: bool) {
        let label = if active {
            rust_i18n::t!("tray.deactivate")
        } else {
            rust_i18n::t!("tray.activate")
        };
        self.toggle_item.set_text(label.as_ref());
        // Both icons are built once at install — `Icon` is cheap to clone
        // (it's an Arc-wrapped Win32 HICON internally).
        let icon = if active {
            self.icon_active.clone()
        } else {
            self.icon_inactive.clone()
        };
        let _ = self.icon.set_icon(Some(icon));
    }

    pub fn set_status(&mut self, active: bool, count: usize) {
        self.last_status = (active, count);
        self.status_item.set_text(format_status_label(active, count));
    }

    /// Re-applies the current locale to every dynamic-label item. Called by
    /// `App::set_lang` after `i18n::set_lang` has been pushed. Status is
    /// rebuilt from `last_status` so the new locale takes effect immediately
    /// rather than waiting for the next watcher tick.
    pub fn relocalize(&mut self) {
        self.show_item.set_text(rust_i18n::t!("tray.show").as_ref());
        self.quit_item.set_text(rust_i18n::t!("tray.quit").as_ref());
        let (active, count) = self.last_status;
        let toggle_label = if active {
            rust_i18n::t!("tray.deactivate")
        } else {
            rust_i18n::t!("tray.activate")
        };
        self.toggle_item.set_text(toggle_label.as_ref());
        self.set_status(active, count);
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

    let icon_inactive = build_icon(false)?;
    let icon_active = build_icon(true)?;
    let tray = TrayIconBuilder::new()
        .with_icon(icon_inactive.clone())
        .with_menu(Box::new(menu))
        .with_tooltip("ROrganizer")
        .build()?;

    Ok(TrayController {
        icon: tray,
        icon_active,
        icon_inactive,
        show_item,
        toggle_item,
        status_item,
        quit_item,
        // Status item is initialized with the inactive label above; mirror
        // that here so the first `relocalize` produces the same string.
        last_status: (false, 0),
    })
}

const ICON_SIZE: u32 = 32;

static ACTIVE_RGBA: OnceLock<Vec<u8>> = OnceLock::new();
static INACTIVE_RGBA: OnceLock<Vec<u8>> = OnceLock::new();

fn ensure_icons() -> (&'static [u8], &'static [u8]) {
    let active = ACTIVE_RGBA.get_or_init(|| {
        let img = image::load_from_memory(crate::assets::ICON_PNG).expect("decode icon.png");
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

/// Pure formatter for the status menu item label, factored out of
/// `TrayController` so it can be unit-tested without instantiating the
/// underlying Win32 tray icon.
fn format_status_label(active: bool, count: usize) -> String {
    if active {
        rust_i18n::t!("tray.status_active", count = count.to_string()).to_string()
    } else {
        rust_i18n::t!("tray.status_inactive").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::format_status_label;

    /// Tests touch the rust_i18n global locale, so they must run sequentially
    /// to avoid clobbering each other when `cargo test` runs threads in
    /// parallel. We serialize via a process-wide mutex.
    fn locale_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn status_label_inactive_uses_locale_string() {
        let _g = locale_lock();
        rust_i18n::set_locale("en");
        assert_eq!(format_status_label(false, 0), "Inactive");
        // count is irrelevant when inactive.
        assert_eq!(format_status_label(false, 99), "Inactive");
    }

    #[test]
    fn status_label_active_interpolates_count() {
        let _g = locale_lock();
        rust_i18n::set_locale("en");
        assert_eq!(format_status_label(true, 3), "Active · 3 accounts");
    }

    #[test]
    fn status_label_follows_locale_switch() {
        // Guards against Fix B regressions: relocalize must produce a label
        // in the new locale, not the locale at TrayController construction.
        let _g = locale_lock();
        rust_i18n::set_locale("fr");
        let fr = format_status_label(true, 5);
        rust_i18n::set_locale("en");
        let en = format_status_label(true, 5);
        assert_ne!(fr, en, "active label must differ between fr and en");
        // Sanity: each contains the count.
        assert!(fr.contains('5'));
        assert!(en.contains('5'));
    }
}
