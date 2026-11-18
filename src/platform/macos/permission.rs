//! The Accessibility grant. Reading Dofus window titles, the global event
//! tap and bringing another app forward all depend on it.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2_foundation::{NSDictionary, NSNumber, NSString};

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
    static kAXTrustedCheckOptionPrompt: *const NSString;
}

pub fn input_permission_granted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Shows the system prompt that leads to the Accessibility settings pane.
/// macOS shows it once per app; afterwards the grant is only changed from
/// the settings.
pub fn request_input_permission() {
    // SAFETY: a constant CFString, toll-free bridged to NSString.
    let key = unsafe { &*kAXTrustedCheckOptionPrompt };
    let yes = NSNumber::new_bool(true);
    let options = NSDictionary::from_slices(&[key], &[&*yes]);
    unsafe { AXIsProcessTrustedWithOptions(Retained::as_ptr(&options).cast()) };
}
