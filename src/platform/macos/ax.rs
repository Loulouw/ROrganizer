//! Accessibility API (ApplicationServices): reading the windows of another
//! app and bringing them forward. Every call fails without the
//! Accessibility grant, which callers treat as "nothing found".
//!
//! AX elements and observers are Core Foundation objects: safe to retain,
//! release and query from any thread.

use std::ffi::c_void;
use std::ptr::{self, NonNull};

use objc2_core_foundation::{
    kCFRunLoopDefaultMode, CFBoolean, CFRetained, CFRunLoop, CFRunLoopSource, CFString, CFType,
};

type AXError = i32;
const AX_SUCCESS: AXError = 0;
/// `kAXErrorNotificationAlreadyRegistered`: the notification still fires.
const AX_ALREADY_REGISTERED: AXError = -25209;

pub type ObserverCallback =
    unsafe extern "C" fn(observer: *mut c_void, element: *mut c_void, notification: *const CFString, refcon: *mut c_void);

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
    fn AXUIElementCopyAttributeValue(
        element: *mut c_void,
        attribute: &CFString,
        value: *mut *const CFType,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: *mut c_void,
        attribute: &CFString,
        value: &CFType,
    ) -> AXError;
    fn AXUIElementPerformAction(element: *mut c_void, action: &CFString) -> AXError;
    fn AXUIElementGetTypeID() -> usize;
    fn AXObserverCreate(pid: i32, callback: ObserverCallback, observer: *mut *mut c_void) -> AXError;
    fn AXObserverAddNotification(
        observer: *mut c_void,
        element: *mut c_void,
        notification: &CFString,
        refcon: *mut c_void,
    ) -> AXError;
    fn AXObserverGetRunLoopSource(observer: *mut c_void) -> *const CFRunLoopSource;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFGetTypeID(cf: *const c_void) -> usize;
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
}

/// An owned `AXUIElementRef`.
pub struct Element(NonNull<c_void>);

// SAFETY: see the module doc.
unsafe impl Send for Element {}

impl Drop for Element {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0.as_ptr()) };
    }
}

impl Element {
    pub fn application(pid: i32) -> Option<Self> {
        NonNull::new(unsafe { AXUIElementCreateApplication(pid) }).map(Self)
    }

    fn copy(&self, attribute: &str) -> Option<CFRetained<CFType>> {
        let name = CFString::from_str(attribute);
        let mut value: *const CFType = ptr::null();
        let err = unsafe { AXUIElementCopyAttributeValue(self.0.as_ptr(), &name, &mut value) };
        let value = NonNull::new(value.cast_mut())?;
        // SAFETY: a Copy-rule result, owned by us from here on.
        let value = unsafe { CFRetained::from_raw(value) };
        (err == AX_SUCCESS).then_some(value)
    }

    pub fn string(&self, attribute: &str) -> Option<String> {
        let value = self.copy(attribute)?;
        value.downcast_ref::<CFString>().map(|s| s.to_string())
    }

    pub fn bool(&self, attribute: &str) -> Option<bool> {
        let value = self.copy(attribute)?;
        value.downcast_ref::<CFBoolean>().map(|b| b.as_bool())
    }

    pub fn set_bool(&self, attribute: &str, value: bool) {
        let name = CFString::from_str(attribute);
        let value = CFBoolean::new(value);
        unsafe { AXUIElementSetAttributeValue(self.0.as_ptr(), &name, value) };
    }

    pub fn perform(&self, action: &str) {
        let name = CFString::from_str(action);
        unsafe { AXUIElementPerformAction(self.0.as_ptr(), &name) };
    }

    /// Windows of an application element, front to back.
    pub fn windows(&self) -> Vec<Element> {
        let Some(array) = self.copy("AXWindows") else { return Vec::new() };
        let array: *const c_void = CFRetained::as_ptr(&array).as_ptr().cast();
        let element_type = unsafe { AXUIElementGetTypeID() };
        let count = unsafe { CFArrayGetCount(array) };
        (0..count)
            .filter_map(|i| {
                let item = unsafe { CFArrayGetValueAtIndex(array, i) };
                if item.is_null() || unsafe { CFGetTypeID(item) } != element_type {
                    return None;
                }
                // Get-rule value: retain it to outlive the array.
                NonNull::new(unsafe { CFRetain(item) }.cast_mut()).map(Element)
            })
            .collect()
    }

    /// The game window: the first standard window, else the first window.
    /// Dofus Unity has one; the other entries of `AXWindows`, if any, are
    /// dialogs and panels.
    pub fn game_window(&self) -> Option<Element> {
        let mut windows = self.windows();
        let standard = windows
            .iter()
            .position(|w| w.string("AXSubrole").as_deref() == Some("AXStandardWindow"));
        match standard {
            Some(i) => Some(windows.swap_remove(i)),
            None => windows.into_iter().next(),
        }
    }
}

/// An `AXObserver` scheduled on the run loop of the thread that created it.
/// Dropping it unschedules it, on that same thread.
pub struct Observer {
    observer: NonNull<c_void>,
    run_loop: CFRetained<CFRunLoop>,
}

impl Observer {
    pub fn new(pid: i32, callback: ObserverCallback) -> Option<Self> {
        let mut raw: *mut c_void = ptr::null_mut();
        if unsafe { AXObserverCreate(pid, callback, &mut raw) } != AX_SUCCESS {
            return None;
        }
        let observer = NonNull::new(raw)?;
        let run_loop = CFRunLoop::current()?;
        let this = Self { observer, run_loop };
        run_loop_mode(|mode| this.run_loop.add_source(Some(this.source()), mode));
        Some(this)
    }

    fn source(&self) -> &CFRunLoopSource {
        // SAFETY: Get rule; the source lives as long as the observer.
        unsafe { &*AXObserverGetRunLoopSource(self.observer.as_ptr()) }
    }

    /// Whether `notification` from `element` (or its descendants) will now
    /// reach the callback.
    pub fn watch(&self, element: &Element, notification: &str) -> bool {
        let name = CFString::from_str(notification);
        let err = unsafe {
            AXObserverAddNotification(
                self.observer.as_ptr(),
                element.0.as_ptr(),
                &name,
                ptr::null_mut(),
            )
        };
        err == AX_SUCCESS || err == AX_ALREADY_REGISTERED
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        run_loop_mode(|mode| self.run_loop.remove_source(Some(self.source()), mode));
        unsafe { CFRelease(self.observer.as_ptr()) };
    }
}

fn run_loop_mode(f: impl FnOnce(Option<&objc2_core_foundation::CFRunLoopMode>)) {
    // SAFETY: an immutable constant exported by Core Foundation.
    f(unsafe { kCFRunLoopDefaultMode })
}
