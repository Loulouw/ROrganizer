//! Single instance through a named Mach service (`CFMessagePort`): the first
//! launch registers the name, a later one finds it, sends one message and
//! exits. No file and no socket are involved.
//!
//! Opening the app again from the Finder, the Dock or Spotlight starts no
//! second process: LaunchServices sends the running one a reopen Apple
//! event, which winit leaves to AppKit (deminiaturize only, nothing for a
//! window hidden in the menu bar). That event is turned into the same
//! message, so every path shows the window.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr;
use std::thread;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{class, define_class, msg_send, sel, ClassType, MainThreadMarker};
use objc2_core_foundation::{
    kCFRunLoopDefaultMode, CFData, CFMessagePort, CFMessagePortContext, CFRetained, CFRunLoop,
    CFString,
};

const PORT_NAME: &str = "fr.loulouw-labs.rorganizer.instance";
const MSG_SHOW: i32 = 1;
/// `kCFMessagePortSuccess`.
const SEND_OK: i32 = 0;

pub struct SingleInstanceGuard {
    port: CFRetained<CFMessagePort>,
}

// SAFETY: Core Foundation objects may be retained, released and scheduled
// from any thread; the port is only scheduled once, on the waiter's run loop.
unsafe impl Send for SingleInstanceGuard {}

pub enum AcquireResult {
    /// We are the first instance. Hold the guard for the rest of the
    /// process lifetime: the name stays registered while the port lives.
    First(SingleInstanceGuard),
    /// Another instance is already running and was successfully signaled.
    SignalledExisting,
}

/// Exits(1) when the name can be neither registered nor reached: booting a
/// second instance would silently bypass the protection.
pub fn acquire_or_signal_existing() -> AcquireResult {
    let name = CFString::from_str(PORT_NAME);
    if signal_existing(&name) {
        return AcquireResult::SignalledExisting;
    }
    let mut context = CFMessagePortContext {
        version: 0,
        info: ptr::null_mut(),
        retain: None,
        release: None,
        copyDescription: None,
    };
    let port = unsafe {
        CFMessagePort::new_local(None, Some(&name), Some(on_message), &mut context, ptr::null_mut())
    };
    match port {
        Some(port) => AcquireResult::First(SingleInstanceGuard { port }),
        // Lost the race against a launch started at the same moment.
        None if signal_existing(&name) => AcquireResult::SignalledExisting,
        None => {
            #[cfg(debug_assertions)]
            eprintln!("CFMessagePortCreateLocal failed for {PORT_NAME}");
            std::process::exit(1);
        }
    }
}

fn signal_existing(name: &CFString) -> bool {
    let Some(remote) = CFMessagePort::new_remote(None, Some(name)) else {
        return false;
    };
    // One-way message: no reply mode, no reply data.
    let status =
        unsafe { remote.send_request(MSG_SHOW, None, 1.0, 0.0, None, ptr::null_mut()) };
    status == SEND_OK
}

thread_local! {
    /// The waiter's callback, read by `on_message`, which the run loop calls
    /// on the same thread.
    static ON_SIGNAL: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

unsafe extern "C-unwind" fn on_message(
    _port: *mut CFMessagePort,
    msgid: i32,
    _data: *const CFData,
    _info: *mut c_void,
) -> *const CFData {
    if msgid == MSG_SHOW {
        ON_SIGNAL.with(|cb| {
            if let Some(cb) = cb.borrow().as_ref() {
                cb();
            }
        });
    }
    ptr::null()
}

/// Calls `on_signal` each time another launch asks to be shown. A message
/// sent before this runs waits in the port's queue. Must be called on the
/// main thread (from `App::new`): the reopen handler is registered there.
pub fn spawn_waiter<F: Fn() + Send + 'static>(guard: SingleInstanceGuard, on_signal: F) {
    handle_reopen_event(MainThreadMarker::new().expect("spawn_waiter runs on the main thread"));
    thread::Builder::new()
        .name("rorg-singleton-waiter".into())
        .spawn(move || {
            // Whole-guard capture: a closure would otherwise capture only
            // `guard.port`, which isn't Send on its own.
            let guard = guard;
            ON_SIGNAL.with(|cb| *cb.borrow_mut() = Some(Box::new(on_signal)));
            let source = CFMessagePort::new_run_loop_source(None, Some(&guard.port), 0)
                .expect("message port run loop source");
            let run_loop = CFRunLoop::current().expect("waiter run loop");
            run_loop.add_source(Some(&source), unsafe { kCFRunLoopDefaultMode });
            CFRunLoop::run();
        })
        .expect("spawn singleton waiter");
}

/// `kCoreEventClass` / `kAEReopenApplication`.
const AE_CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
const AE_REOPEN_APPLICATION: u32 = u32::from_be_bytes(*b"rapp");

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class has
    // no ivars and no Drop.
    #[unsafe(super(NSObject))]
    #[name = "RorgReopenHandler"]
    struct ReopenHandler;

    impl ReopenHandler {
        #[unsafe(method(handleReopen:withReplyEvent:))]
        fn handle_reopen(&self, _event: *mut AnyObject, _reply: *mut AnyObject) {
            // One-way message to our own port: the waiter shows the window.
            signal_existing(&CFString::from_str(PORT_NAME));
        }
    }
);

/// Replaces AppKit's reopen handler. It only deminiaturized the window;
/// `on_signal` restores it from the Dock as well.
fn handle_reopen_event(_mtm: MainThreadMarker) {
    let handler: Retained<ReopenHandler> = unsafe { msg_send![ReopenHandler::class(), new] };
    let manager: Retained<AnyObject> =
        unsafe { msg_send![class!(NSAppleEventManager), sharedAppleEventManager] };
    let () = unsafe {
        msg_send![
            &manager,
            setEventHandler: &*handler,
            andSelector: sel!(handleReopen:withReplyEvent:),
            forEventClass: AE_CORE_EVENT_CLASS,
            andEventID: AE_REOPEN_APPLICATION
        ]
    };
    // The event manager does not retain its handlers: it lives as long as
    // the process.
    std::mem::forget(handler);
}
