use objc2_application_services::{AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};

use crate::trust::Trust;

/// Reads Accessibility trust, prompting if untrusted.
///
/// When untrusted, macOS also shows its own Accessibility dialog
/// asynchronously and adds the app to the list in System Settings. The
/// result does not wait for the user's answer.
pub fn request_trust() -> Trust {
    read_trust(true)
}

pub fn check_trust() -> Trust {
    read_trust(false)
}

#[expect(
    unsafe_code,
    reason = "the Accessibility trust check is a C function and its option key a C static"
)]
fn read_trust(prompt: bool) -> Trust {
    // SAFETY: `kAXTrustedCheckOptionPrompt` is an immutable `CFStringRef`
    // constant that ApplicationServices initialises before any Rust code runs.
    let key: &CFString = unsafe { kAXTrustedCheckOptionPrompt };
    let options =
        CFDictionary::<CFString, CFBoolean>::from_slices(&[key], &[CFBoolean::new(prompt)]);
    // SAFETY: `options` maps `CFString` keys to `CFBoolean` values, the types
    // the function documents for `kAXTrustedCheckOptionPrompt`, and it stays
    // alive (owned by `CFRetained`) for the duration of the call.
    let trusted = unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) };
    Trust::from_trusted(trusted)
}
