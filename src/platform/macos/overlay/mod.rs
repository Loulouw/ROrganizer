//! macOS side of the character overlay: native panels drawn with AppKit
//! (`OVERLAY_IS_EGUI` is false), driven by the same `crate::overlay::State`
//! as the egui viewports on Windows. App calls `show` on every pass while
//! the bar should show, `hide` when it shouldn't, `close` when the overlay
//! goes off.
//!
//! Why not egui (spike, 2026-11): a click on any winit window activates the
//! app, which blinds the event tap; a winit window never shows over another
//! app's fullscreen Space; `Visible(true)` from another thread never reaches
//! a hidden viewport. A `NonactivatingPanel` does none of that.
//!
//! Coordinates: points, y downwards from the top of the primary screen,
//! as `placement` (the saved `window_pos` and `overlay_pos` alike).

mod bar;
mod draw;
mod tip;

use std::cell::RefCell;

use eframe::egui::{self, Vec2};
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSFloatingWindowLevel, NSPanel,
    NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use self::bar::BarView;
use super::placement::{self, Rect};
use crate::overlay::{ScreenRect, Shared};
use crate::platform::OverlayWindow;

/// The bar's panel and view, alive while the overlay is on.
struct Bar {
    panel: Retained<NSPanel>,
    view: Retained<BarView>,
}

thread_local! {
    static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
}

/// Runs `f` on the bar's view, if there is one. Main thread only.
fn with_view(f: impl FnOnce(&BarView)) {
    let view = BAR.with_borrow(|bar| bar.as_ref().map(|b| b.view.clone()));
    if let Some(view) = view {
        f(&view);
    }
}

/// Whether `view` is the open bar's, not one App closed since.
fn is_current(view: &BarView) -> bool {
    BAR.with_borrow(|bar| bar.as_ref().is_some_and(|b| std::ptr::eq(&*b.view, view)))
}

pub fn show(shared: &Shared, ctx: &egui::Context) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let view = BAR.with_borrow_mut(|bar| {
        bar.get_or_insert_with(|| Bar::new(mtm, shared.clone(), ctx.clone()))
            .view
            .clone()
    });
    view.refresh(true);
}

pub fn hide(_shared: &Shared) {
    with_view(|v| v.hide());
}

pub fn close(_shared: &Shared) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    if let Some(bar) = BAR.take() {
        bar.view.close();
        bar.panel.close();
    }
}

/// Another app came to the front (watcher, main thread): the lit head
/// follows right away rather than on App's next pass, which waits up to
/// 100 ms while the main window is hidden.
pub(super) fn foreground_changed() {
    with_view(|v| {
        if v.window().is_some_and(|w| w.isVisible()) {
            v.refresh(false);
        }
    });
}

impl Bar {
    fn new(mtm: MainThreadMarker, shared: Shared, ctx: egui::Context) -> Self {
        let (tip_panel, tip_view) = tip::panel(mtm);
        let view = BarView::new(mtm, shared, ctx, tip_panel, tip_view);
        let panel = new_panel(mtm);
        panel.setContentView(Some(&view));
        Self { panel, view }
    }
}

/// A borderless panel that never activates the app, floats over every
/// Space including fullscreen ones, and stays when the app deactivates
/// (it does all the time: the panel's whole point is to sit over Dofus).
fn new_panel(mtm: MainThreadMarker) -> Retained<NSPanel> {
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(10.0, 10.0));
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        frame,
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setBecomesKeyOnlyIfNeeded(true);
    // Mandatory: the app turns `.accessory`, hence inactive for good,
    // whenever the main window is hidden in the menu bar.
    panel.setHidesOnDeactivate(false);
    panel.setLevel(NSFloatingWindowLevel);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(false);
    // Shown and hidden at once, as on Windows: the default fade kept a
    // hidden bar on screen ~200 ms longer.
    panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
    panel
}

fn overlay_window(panel: &NSWindow) -> OverlayWindow {
    OverlayWindow(panel as *const NSWindow as usize)
}

impl OverlayWindow {
    /// The panel behind `self`, while `BAR` holds it: `State::window` is
    /// cleared when App closes the overlay.
    fn ns_window(self, _mtm: MainThreadMarker) -> Option<Retained<NSWindow>> {
        unsafe { Retained::retain(self.0 as *mut NSWindow) }
    }
}

/// Resizes `panel` to `size` points, its top-left corner staying put.
fn resize(panel: &NSWindow, size: Vec2) {
    let frame = panel.frame();
    let top = frame.origin.y + frame.size.height;
    let (w, h) = (f64::from(size.x), f64::from(size.y));
    let frame = NSRect::new(NSPoint::new(frame.origin.x, top - h), NSSize::new(w, h));
    panel.setFrame_display(frame, false);
}

/// AppKit frame of a `size` window whose top-left corner is at `pos`.
fn appkit_frame(pos: (i32, i32), size: Vec2) -> Option<NSRect> {
    let primary_height = placement::primary_height(MainThreadMarker::new()?)?;
    let (w, h) = (f64::from(size.x), f64::from(size.y));
    let y = primary_height - f64::from(pos.1) - h;
    Some(NSRect::new(NSPoint::new(f64::from(pos.0), y), NSSize::new(w, h)))
}

fn screen_rect(r: Rect) -> ScreenRect {
    (r.left, r.top, r.right, r.bottom)
}

pub fn rect(window: OverlayWindow) -> Option<ScreenRect> {
    let mtm = MainThreadMarker::new()?;
    let frame = window.ns_window(mtm)?.frame();
    Some(screen_rect(Rect::from_appkit(frame, placement::primary_height(mtm)?)))
}

pub fn move_to(window: OverlayWindow, pos: (i32, i32)) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(primary_height) = placement::primary_height(mtm) else { return };
    let Some(ns_window) = window.ns_window(mtm) else { return };
    let top_left = NSPoint::new(f64::from(pos.0), primary_height - f64::from(pos.1));
    ns_window.setFrameTopLeftPoint(top_left);
}

pub fn work_area(window: OverlayWindow) -> Option<ScreenRect> {
    let mtm = MainThreadMarker::new()?;
    let visible = window.ns_window(mtm)?.screen()?.visibleFrame();
    Some(screen_rect(Rect::from_appkit(visible, placement::primary_height(mtm)?)))
}

pub fn work_area_of(rect: ScreenRect) -> Option<ScreenRect> {
    let mtm = MainThreadMarker::new()?;
    let rect = Rect { left: rect.0, top: rect.1, right: rect.2, bottom: rect.3 };
    let work = placement::visible_area_showing(mtm, rect, placement::primary_height(mtm)?)?;
    Some(screen_rect(work))
}

pub fn main_work_area() -> Option<ScreenRect> {
    let mtm = MainThreadMarker::new()?;
    let primary = objc2_app_kit::NSScreen::screens(mtm).firstObject()?;
    Some(screen_rect(Rect::from_appkit(primary.visibleFrame(), primary.frame().size.height)))
}

pub fn cursor_position() -> (i32, i32) {
    let p = NSEvent::mouseLocation();
    let primary_height = MainThreadMarker::new().and_then(placement::primary_height).unwrap_or(0.0);
    (p.x.round() as i32, (primary_height - p.y).round() as i32)
}

/// Never called: the overlay is native here (`OVERLAY_IS_EGUI`).
pub fn attach(_title: &str, _activates: bool) -> Option<OverlayWindow> {
    None
}

/// Never called, as `attach`.
pub fn activate(_window: OverlayWindow) {}
