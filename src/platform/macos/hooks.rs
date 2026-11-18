//! Global keyboard, mouse and wheel listener: a Quartz event tap on its own
//! thread and run loop.
//!
//! Like the Windows LL hooks, the callback is on the input path of the
//! whole session: it only reads atomics and the binding table, and hands
//! the actual focus to the focus worker.

use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRetained, CFRunLoop};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType,
};

use super::focus::focus_window;
use super::permission::input_permission_granted;
use super::watcher::frontmost_pid;
use crate::hooks;
use crate::triggers::{MouseBtn, Trigger, WheelDir};

/// Mouse button numbers of Quartz: 2 middle, 3 and 4 the side buttons.
const BUTTON_MIDDLE: i64 = 2;
const BUTTON_BACK: i64 = 3;
const BUTTON_FORWARD: i64 = 4;

/// Without the Accessibility grant the tap can't be created: wait for it.
const PERMISSION_POLL: Duration = Duration::from_secs(1);

/// The tap, for re-enabling it from its own callback.
static TAP: AtomicPtr<CFMachPort> = AtomicPtr::new(ptr::null_mut());

/// Keys whose press was swallowed: their autorepeats and release are
/// swallowed too, so the game never sees half a keystroke.
static SWALLOWED: [AtomicBool; 128] = [const { AtomicBool::new(false) }; 128];

/// Last key pressed while ROrganizer itself is in front, with its time,
/// packed as `code << 48 | ms since START`. The binding modal reads it:
/// egui can't tell some keys apart (see `keys::key_from_event`). Held in
/// memory only, overwritten by the next press, never stored.
static OWN_KEY: AtomicU64 = AtomicU64::new(0);
static START: LazyLock<Instant> = LazyLock::new(Instant::now);

/// How long after the tap saw it a key press is still the one egui reports.
const OWN_KEY_FRESH: Duration = Duration::from_millis(500);

/// Code of the key just pressed in ROrganizer, if the tap saw it.
pub(super) fn take_own_key() -> Option<u16> {
    let packed = OWN_KEY.swap(0, Ordering::Relaxed);
    let at = Duration::from_millis(packed & 0xFFFF_FFFF_FFFF);
    let code = (packed >> 48) as u16;
    (packed != 0 && START.elapsed().saturating_sub(at) <= OWN_KEY_FRESH).then_some(code)
}

pub fn spawn_hooks() {
    LazyLock::force(&START);
    std::thread::Builder::new()
        .name("rorg-hooks".into())
        .spawn(run)
        .expect("hooks thread");
}

fn run() {
    while !input_permission_granted() {
        std::thread::sleep(PERMISSION_POLL);
    }
    let mask = [
        CGEventType::KeyDown,
        CGEventType::KeyUp,
        CGEventType::OtherMouseDown,
        CGEventType::ScrollWheel,
    ]
    .iter()
    .fold(0u64, |m, t| m | 1 << t.0);
    let tap = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            mask,
            Some(on_event),
            ptr::null_mut(),
        )
    }
    .expect("create event tap");
    // `tap` outlives the run loop below, which never returns.
    TAP.store(CFRetained::as_ptr(&tap).as_ptr(), Ordering::Release);
    let source = CFMachPort::new_run_loop_source(None, Some(&tap), 0).expect("tap run loop source");
    let run_loop = CFRunLoop::current().expect("hooks run loop");
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    CFRunLoop::run();
}

unsafe extern "C-unwind" fn on_event(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    _info: *mut c_void,
) -> *mut CGEvent {
    let pass = event.as_ptr();
    if kind == CGEventType::TapDisabledByTimeout || kind == CGEventType::TapDisabledByUserInput {
        // macOS turns a tap off when a callback is slow; keep listening.
        if let Some(tap) = NonNull::new(TAP.load(Ordering::Acquire)) {
            CGEvent::tap_enable(unsafe { tap.as_ref() }, true);
        }
        return pass;
    }
    let event = unsafe { event.as_ref() };
    let field = |f: CGEventField| CGEvent::integer_value_field(Some(event), f);
    let own_front = frontmost_pid() == std::process::id() as i32;

    let trigger = match kind {
        CGEventType::KeyDown => {
            let code = field(CGEventField::KeyboardEventKeycode) as u16;
            if own_front {
                // As with the Windows hooks, nothing fires while ROrganizer
                // is in front: the binding modal gets the key.
                let now = START.elapsed().as_millis() as u64 & 0xFFFF_FFFF_FFFF;
                OWN_KEY.store(u64::from(code) << 48 | now.max(1), Ordering::Relaxed);
                return pass;
            }
            let swallowed = SWALLOWED.get(usize::from(code));
            if field(CGEventField::KeyboardEventAutorepeat) != 0 {
                let held = swallowed.is_some_and(|s| s.load(Ordering::Relaxed));
                return if held { ptr::null_mut() } else { pass };
            }
            if !hooks::is_enabled() {
                return pass;
            }
            match hooks::focus_target(Trigger::Key(u32::from(code))) {
                Some(target) => {
                    if let Some(s) = swallowed {
                        s.store(true, Ordering::Relaxed);
                    }
                    focus_window(target);
                    return ptr::null_mut();
                }
                None => return pass,
            }
        }
        CGEventType::KeyUp => {
            let code = field(CGEventField::KeyboardEventKeycode) as usize;
            let held = SWALLOWED.get(code).is_some_and(|s| s.swap(false, Ordering::Relaxed));
            return if held { ptr::null_mut() } else { pass };
        }
        CGEventType::OtherMouseDown => match field(CGEventField::MouseEventButtonNumber) {
            BUTTON_MIDDLE => Trigger::Mouse(MouseBtn::Middle),
            BUTTON_BACK => Trigger::Mouse(MouseBtn::X1),
            BUTTON_FORWARD => Trigger::Mouse(MouseBtn::X2),
            _ => return pass,
        },
        CGEventType::ScrollWheel => {
            // Trackpads and the Magic Mouse scroll continuously, dozens of
            // events per gesture: only a notched wheel makes a trigger.
            if field(CGEventField::ScrollWheelEventIsContinuous) != 0 {
                return pass;
            }
            match field(CGEventField::ScrollWheelEventDeltaAxis1) {
                d if d > 0 => Trigger::Wheel(WheelDir::Up),
                d if d < 0 => Trigger::Wheel(WheelDir::Down),
                _ => return pass,
            }
        }
        _ => return pass,
    };

    if own_front || !hooks::is_enabled() {
        return pass;
    }
    match hooks::focus_target(trigger) {
        Some(target) => {
            focus_window(target);
            ptr::null_mut()
        }
        None => pass,
    }
}
