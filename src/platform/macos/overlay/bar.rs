//! The bar's view: paints `crate::overlay::State` like
//! `ui::overlay::paint_bar`, runs the gestures of `crate::overlay` from its
//! mouse events, drives the tooltip and pops the right-click menu.
//!
//! Every method runs on the main thread, from AppKit's event dispatch or
//! from `App::logic` (`refresh`). None holds the state lock across a call
//! that can paint synchronously (`orderFrontRegardless`) or spin a nested
//! run loop (the menu): `drawRect:` takes that lock.

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::TryLockError;
use std::time::Instant;

use dispatch2::{DispatchQueue, DispatchTime};
use eframe::egui::{self, Color32, Pos2, Vec2};
use objc2::rc::Retained;
use objc2::{
    define_class, msg_send, sel, AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
    Message,
};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSControlStateValueOn, NSEvent, NSGraphicsContext, NSImage, NSImageInterpolation, NSMenu,
    NSMenuItem, NSFont, NSPanel, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindow,
};
use objc2_foundation::{NSData, NSPoint, NSRect, NSString};

use super::tip::{self, TipView};
use crate::overlay::{self, Display, Menu, MenuAction, MenuEntry, Shared, State, Tip};
use crate::platform::OverlayWindow;
use crate::theme::{self, Theme};

pub(super) struct BarIvars {
    shared: Shared,
    ctx: egui::Context,
    tip_panel: Retained<NSPanel>,
    tip_view: Retained<TipView>,
    /// Pointer over the bar, in view points; None outside.
    pointer: Cell<Option<Pos2>>,
    /// Head drawn hovered, and whether the resize corner shows.
    hovered: Cell<Option<usize>>,
    on_bar: Cell<bool>,
    /// What the last paint showed of `State`, to repaint only on change.
    drawn: Cell<u64>,
    /// Class heads, decoded on first use.
    heads: RefCell<HashMap<&'static str, Retained<NSImage>>>,
    /// The open menu, for `hide` to close it, and the item picked in it.
    menu: RefCell<Option<Retained<NSMenu>>>,
    picked: Cell<Option<usize>>,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements; no Drop.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "ROrganizerOverlayBar"]
    #[ivars = BarIvars]
    pub(super) struct BarView;

    impl BarView {
        /// y downwards, as `crate::overlay::Geometry`.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        /// The click that lands on the inactive app's panel is handled,
        /// not swallowed as a focus click.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.paint();
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.press(event);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.drag(event);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.release(event);
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.open_menu(event);
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.ivars().pointer.set(Some(self.location(event)));
            self.update_hover();
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, event: &NSEvent) {
            self.ivars().pointer.set(Some(self.location(event)));
            self.update_hover();
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.ivars().pointer.set(None);
            self.update_hover();
        }

        /// Target of the menu items.
        #[unsafe(method(pick:))]
        fn pick(&self, item: &NSMenuItem) {
            self.ivars().picked.set(usize::try_from(item.tag()).ok());
        }
    }
);

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, State> {
    overlay::lock(shared)
}

impl BarView {
    pub(super) fn new(
        mtm: MainThreadMarker,
        shared: Shared,
        ctx: egui::Context,
        tip_panel: Retained<NSPanel>,
        tip_view: Retained<TipView>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(BarIvars {
            shared,
            ctx,
            tip_panel,
            tip_view,
            pointer: Cell::new(None),
            hovered: Cell::new(None),
            on_bar: Cell::new(false),
            drawn: Cell::new(0),
            heads: RefCell::new(HashMap::new()),
            menu: RefCell::new(None),
            picked: Cell::new(None),
        });
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        // The app is inactive whenever Dofus is in front: the hover has to
        // be tracked regardless.
        let options = NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                NSRect::ZERO,
                options,
                Some(&view),
                None,
            )
        };
        view.addTrackingArea(&area);
        view
    }

    fn panel(&self) -> Option<Retained<NSWindow>> {
        self.window()
    }

    fn overlay_window(&self) -> Option<OverlayWindow> {
        self.panel().map(|p| super::overlay_window(&p))
    }

    fn location(&self, event: &NSEvent) -> Pos2 {
        let p = self.convertPoint_fromView(event.locationInWindow(), None);
        Pos2::new(p.x as f32, p.y as f32)
    }

    fn inside(&self, p: Pos2) -> bool {
        let b = self.bounds();
        f64::from(p.x) >= 0.0
            && f64::from(p.y) >= 0.0
            && f64::from(p.x) < b.size.width
            && f64::from(p.y) < b.size.height
    }

    /// Brings the panel in line with the state App just wrote: placed on
    /// its first showing, resized, the lit head following the foreground.
    /// `order_front`: also shows it.
    pub(super) fn refresh(&self, order_front: bool) {
        // A view whose bar App closed meanwhile (during the menu's run loop)
        // must not put its panel back into `State::window`: App cleared it,
        // and the panel goes away with the last reference.
        if !super::is_current(self) {
            return;
        }
        let iv = self.ivars();
        let Some(panel) = self.panel() else { return };
        let window = super::overlay_window(&panel);
        let mut st = lock(&iv.shared);
        if st.window.is_none() {
            let size = st.geometry().size();
            super::resize(&panel, size);
            let pos = overlay::place(window, st.saved_pos, overlay::physical(size, 1.0));
            st.window = Some(window);
            st.tip_window = Some(super::overlay_window(&iv.tip_panel));
            st.sent_size = Some((size, 1.0));
            st.events.push(overlay::Event::Moved(pos));
        }
        apply_size(&mut st, &panel, window);
        if let Some(again) = overlay::update_front(&mut st) {
            iv.ctx.request_repaint_after(again);
        }
        let drawn = fingerprint(&st);
        let events = !st.events.is_empty();
        drop(st);

        if iv.drawn.replace(drawn) != drawn {
            self.setNeedsDisplay(true);
        }
        if events {
            iv.ctx.request_repaint();
        }
        if order_front && !panel.isVisible() {
            panel.orderFrontRegardless();
        }
    }

    /// Hides everything, then closes the tooltip panel: the overlay is off.
    pub(super) fn close(&self) {
        self.hide();
        self.ivars().tip_panel.close();
    }

    /// Hides the bar, its tooltip and its menu.
    pub(super) fn hide(&self) {
        let iv = self.ivars();
        {
            let mut st = lock(&iv.shared);
            st.tip = None;
            st.hover = None;
            st.gesture = None;
        }
        if let Some(menu) = iv.menu.borrow().as_ref() {
            menu.cancelTracking();
        }
        iv.pointer.set(None);
        iv.hovered.set(None);
        iv.tip_panel.orderOut(None);
        if let Some(panel) = self.panel() {
            panel.orderOut(None);
        }
    }

    // -----------------------------------------------------------------------
    // Gestures.

    fn press(&self, event: &NSEvent) {
        let iv = self.ivars();
        let Some(window) = self.overlay_window() else { return };
        let p = self.location(event);
        iv.pointer.set(Some(p));
        {
            let mut st = lock(&iv.shared);
            let hit = st.geometry().hit(p);
            overlay::start_gesture(&mut st, window, Some(hit));
        }
        self.update_hover();
        self.setNeedsDisplay(true);
    }

    fn drag(&self, event: &NSEvent) {
        let iv = self.ivars();
        let Some(panel) = self.panel() else { return };
        let window = super::overlay_window(&panel);
        let p = self.location(event);
        iv.pointer.set(self.inside(p).then_some(p));
        let mut st = lock(&iv.shared);
        overlay::follow_gesture(&mut st, window, 1.0);
        let resized = apply_size(&mut st, &panel, window);
        drop(st);
        if resized {
            self.setNeedsDisplay(true);
        }
    }

    fn release(&self, event: &NSEvent) {
        let iv = self.ivars();
        let Some(window) = self.overlay_window() else { return };
        let p = self.location(event);
        let inside = self.inside(p);
        iv.pointer.set(inside.then_some(p));
        {
            let mut st = lock(&iv.shared);
            let hit = inside.then(|| st.geometry().hit(p));
            overlay::end_gesture(&mut st, window, hit, 1.0);
        }
        iv.ctx.request_repaint();
        self.update_hover();
        self.setNeedsDisplay(true);
    }

    /// Hover and tooltip, from `pointer`. No cursor shapes: AppKit ignores
    /// `NSCursor::set` from an inactive app (measured), and Dofus is in
    /// front whenever the bar is used.
    fn update_hover(&self) {
        // A closed bar would order its closed tooltip back in.
        if !super::is_current(self) {
            return;
        }
        let iv = self.ivars();
        let pointer = iv.pointer.get();
        let mut st = lock(&iv.shared);
        let g = st.geometry();
        let hit = pointer.map(|p| g.hit(p));
        let hovered = overlay::hovered_slot(&st, hit).filter(|_| st.menu.is_none());
        let (wanted, again) = overlay::wanted_tip(&mut st, hovered);
        let tip_change = (wanted != st.tip).then(|| {
            st.tip = wanted.clone();
            wanted.map(|tip| (tip, st.theme()))
        });
        let gesture = st.gesture;
        drop(st);

        if let Some(again) = again {
            let when = DispatchTime::try_from(again).unwrap_or(DispatchTime::NOW);
            let _ = DispatchQueue::main().after(when, || super::with_view(|v| v.update_hover()));
        }
        let on_bar = pointer.is_some() || gesture.is_some();
        if iv.hovered.replace(hovered) != hovered || iv.on_bar.replace(on_bar) != on_bar {
            self.setNeedsDisplay(true);
        }
        match tip_change {
            Some(Some((tip, theme))) => self.show_tip(&tip, theme, &g),
            Some(None) => iv.tip_panel.orderOut(None),
            None => {}
        }
    }

    fn show_tip(&self, tip: &Tip, theme: Theme, g: &overlay::Geometry) {
        let iv = self.ivars();
        let Some(window) = self.overlay_window() else { return };
        let (Some(bar), Some(work)) = (super::rect(window), super::work_area(window)) else {
            return;
        };
        let layout = tip::Layout::new(tip, theme);
        let size = overlay::physical(layout.size, 1.0);
        let pos = overlay::tip_position(bar, work, g, tip.slot, size, 1.0);
        let Some(frame) = super::appkit_frame(pos, layout.size) else { return };
        iv.tip_panel.setFrame_display(frame, false);
        iv.tip_view.set(layout);
        iv.tip_panel.orderFrontRegardless();
    }

    // -----------------------------------------------------------------------
    // Right-click menu.

    /// A native menu in the app's theme, popped from the inactive app: the
    /// foreground stays with Dofus while it is open. A pick gives the
    /// foreground back to the client that had it, as on Windows.
    fn open_menu(&self, event: &NSEvent) {
        // App may close the panel while the menu's run loop spins.
        let this = self.retain();
        let iv = this.ivars();
        let mtm = this.mtm();
        let (items, previous, theme) = {
            let mut st = lock(&iv.shared);
            if st.window.is_none() {
                return;
            }
            let items = overlay::menu(st.layout, st.display, st.visibility);
            let previous = crate::platform::foreground_window();
            st.menu = Some(Menu {
                items: items.clone(),
                previous,
                opened: Instant::now(),
                activated: true,
                focused: false,
            });
            st.gesture = None;
            (items, previous, st.theme())
        };
        this.update_hover();

        let menu = build_menu(mtm, &this, &items, theme);
        iv.picked.set(None);
        *iv.menu.borrow_mut() = Some(menu.clone());
        let at = this.location(event);
        let at = NSPoint::new(f64::from(at.x), f64::from(at.y));
        menu.popUpMenuPositioningItem_atLocation_inView(None, at, Some(&this));
        *iv.menu.borrow_mut() = None;

        let picked = iv.picked.take().and_then(|i| items.get(i)).and_then(|(_, a)| *a);
        let give_back = {
            let mut st = lock(&iv.shared);
            st.menu = None;
            match picked {
                Some(action) => overlay::pick(&mut st, action, previous),
                // Dismissed: only undo an activation the menu caused.
                None => previous.filter(|_| {
                    crate::platform::foreground_window().is_some_and(crate::platform::is_own_window)
                }),
            }
        };
        overlay::give_back(give_back);
        iv.ctx.request_repaint();
        // A new layout resizes the panel now, not after App's next pass.
        this.refresh(false);
        // The menu swallowed the exit from the bar: read where the mouse is.
        let at = this.window().map(|w| w.mouseLocationOutsideOfEventStream());
        let at = at.map(|p| this.convertPoint_fromView(p, None));
        let at = at.map(|p| Pos2::new(p.x as f32, p.y as f32)).filter(|&p| this.inside(p));
        iv.pointer.set(at);
        this.update_hover();
        this.setNeedsDisplay(true);
    }

    // -----------------------------------------------------------------------
    // Painting.

    fn head_image(&self, class: Option<&str>) -> Option<Retained<NSImage>> {
        let name = class.and_then(crate::class_icon::class_filename)?;
        let mut heads = self.ivars().heads.borrow_mut();
        if let Some(image) = heads.get(name) {
            return Some(image.clone());
        }
        let (_, bytes) = crate::assets::CLASS_PNGS.iter().find(|(n, _)| *n == name)?;
        let data = NSData::with_bytes(bytes);
        let image = NSImage::initWithData(NSImage::alloc(), &data)?;
        heads.insert(name, image.clone());
        Some(image)
    }

    fn paint(&self) {
        let iv = self.ivars();
        let st = match iv.shared.try_lock() {
            Ok(st) => st,
            Err(TryLockError::Poisoned(e)) => e.into_inner(),
            // Never expected (see the module doc): paint on the next pass
            // rather than deadlock the main thread.
            Err(TryLockError::WouldBlock) => {
                DispatchQueue::main().exec_async(|| super::with_view(|v| v.setNeedsDisplay(true)));
                return;
            }
        };
        if let Some(context) = NSGraphicsContext::currentContext() {
            context.setImageInterpolation(NSImageInterpolation::High);
        }
        let theme = st.theme();
        let g = st.geometry();
        let hovered = iv.hovered.get();
        let draw = super::draw::rounded_rect;

        let (fill, border) = overlay::bar_colors(theme);
        let bar = egui::Rect::from_min_size(Pos2::ZERO, g.size());
        // The window server sends clicks on fully transparent pixels to the
        // window below (measured: the rounded corners clicked into the game).
        // An invisible fill keeps the whole rectangle the bar's, as on Windows.
        draw(bar, 0, Color32::from_black_alpha(1), None);
        draw(bar, overlay::BAR_RADIUS, fill, Some((1.0, border)));

        let dot = theme::text_tertiary(theme);
        for center in g.grip_dots() {
            super::draw::circle(center, 1.4, dot);
        }

        let active = overlay::active_color(theme);
        let lit = st.lit();
        for (i, slot) in st.slots.iter().enumerate() {
            let rect = g.slot(i);
            let on = lit == Some(&slot.key);
            let bright = on || hovered == Some(i);
            let radius = overlay::head_radius(g.head);
            if on {
                draw(rect, radius, active.gamma_multiply(0.16), Some((2.0, active)));
            } else if hovered == Some(i) {
                draw(rect, radius, theme::hover_bg(theme).gamma_multiply(1.5), None);
            }

            let head = match st.display {
                Display::ClassHead => self.head_image(slot.class.as_deref()),
                Display::Initials => None,
            };
            match head {
                Some(image) => {
                    let opacity = if bright { 1.0 } else { f32::from(overlay::DIM_ALPHA) / 255.0 };
                    super::draw::image(&image, rect.shrink(g.head * 0.07), opacity);
                }
                None => {
                    let disc = overlay::initials_disc(theme);
                    let text = theme::text_primary(theme);
                    let (disc, text) = if bright {
                        (disc, text)
                    } else {
                        (disc.gamma_multiply(0.6), text.gamma_multiply(0.55))
                    };
                    super::draw::circle(rect.center(), g.head * 0.4, disc);
                    let label = super::draw::Text::new(
                        &slot.initials,
                        super::draw::Font::Proportional(g.head * 0.36),
                        text,
                    );
                    let half = Vec2::new(label.width(), label.height()) / 2.0;
                    label.draw(rect.center() - half);
                }
            }
        }

        if iv.on_bar.get() || st.gesture.is_some() {
            for [a, b] in g.resize_marks() {
                super::draw::line(a, b, 1.2, dot);
            }
        }
    }
}

/// Resizes the panel when the head size or the slot count changed, keeping
/// its top-left corner, and pulls it back whole onto its screen. Returns
/// whether it changed.
fn apply_size(st: &mut State, panel: &NSWindow, window: OverlayWindow) -> bool {
    let size = st.geometry().size();
    if st.sent_size == Some((size, 1.0)) {
        return false;
    }
    super::resize(panel, size);
    if let Some(pos) = overlay::keep_on_screen(window, overlay::physical(size, 1.0)) {
        st.events.push(overlay::Event::Moved(pos));
    }
    st.sent_size = Some((size, 1.0));
    true
}

/// What `paint` reads from `State`, hashed.
fn fingerprint(st: &State) -> u64 {
    let mut h = DefaultHasher::new();
    for slot in &st.slots {
        (&slot.key, &slot.initials, &slot.class).hash(&mut h);
    }
    std::mem::discriminant(&st.layout).hash(&mut h);
    std::mem::discriminant(&st.display).hash(&mut h);
    std::mem::discriminant(&st.theme()).hash(&mut h);
    st.head.to_bits().hash(&mut h);
    st.lit().hash(&mut h);
    h.finish()
}

fn build_menu(
    mtm: MainThreadMarker,
    target: &BarView,
    items: &[(MenuEntry, Option<overlay::MenuAction>)],
    theme: Theme,
) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    let appearance = match theme {
        Theme::Dark => unsafe { NSAppearanceNameDarkAqua },
        Theme::Light => unsafe { NSAppearanceNameAqua },
    };
    menu.setAppearance(NSAppearance::appearanceNamed(appearance).as_deref());
    for (i, (entry, action)) in items.iter().enumerate() {
        let item = match entry {
            MenuEntry::Separator => NSMenuItem::separatorItem(mtm),
            // Greyed-out, as the egui menu's headers.
            MenuEntry::Header(label) => {
                let item = new_item(mtm, label);
                item.setEnabled(false);
                item
            }
            MenuEntry::Item { label, checked } => {
                let item = new_item(mtm, label);
                unsafe {
                    item.setAction(Some(sel!(pick:)));
                    item.setTarget(Some(target));
                }
                item.setTag(i as isize);
                if *checked {
                    item.setState(NSControlStateValueOn);
                }
                // In the conflict red, as the egui menu.
                if *action == Some(MenuAction::Disable) {
                    let font = NSFont::menuFontOfSize(0.0);
                    let red = theme::conflict_palette(theme).3;
                    item.setAttributedTitle(Some(&super::draw::attributed(label, &font, red)));
                }
                item
            }
        };
        menu.addItem(&item);
    }
    menu
}

fn new_item(mtm: MainThreadMarker, label: &str) -> Retained<NSMenuItem> {
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(label),
            None,
            &NSString::new(),
        )
    }
}
