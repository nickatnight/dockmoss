use objc2_application_services::{AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};

/// With `prompt` set and the process untrusted, macOS shows its own
/// Accessibility dialog asynchronously and adds the app to the list in
/// System Settings. The return value does not wait for the user's answer.
#[expect(
    unsafe_code,
    reason = "the Accessibility trust check is a C function and its option key a C static"
)]
pub fn is_trusted(prompt: bool) -> bool {
    // SAFETY: `kAXTrustedCheckOptionPrompt` is an immutable `CFStringRef`
    // constant that ApplicationServices initialises before any Rust code runs.
    let key: &CFString = unsafe { kAXTrustedCheckOptionPrompt };
    let options =
        CFDictionary::<CFString, CFBoolean>::from_slices(&[key], &[CFBoolean::new(prompt)]);
    // SAFETY: `options` maps `CFString` keys to `CFBoolean` values, the types
    // the function documents for `kAXTrustedCheckOptionPrompt`, and it stays
    // alive (owned by `CFRetained`) for the duration of the call.
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) }
}
