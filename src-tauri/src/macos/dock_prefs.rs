//! Local reads through `cfprefsd`; the Dock process is not asked.

use objc2_core_foundation::{
    CFBoolean, CFPreferencesAppSynchronize, CFPreferencesCopyAppValue, CFString,
};

const DOCK_DOMAIN: &str = "com.apple.dock";
const AUTOHIDE: &str = "autohide";

/// Whether "Automatically hide and show the Dock" is on. Synchronizes the
/// domain first, so a change made in System Settings since the last read is
/// seen. A missing value, or one that is not a boolean, is off: that is the
/// Dock's default.
pub fn auto_hide() -> bool {
    let domain = CFString::from_static_str(DOCK_DOMAIN);
    if !CFPreferencesAppSynchronize(&domain) {
        tracing::debug!("dock preferences not synchronized; reading the cached value");
    }
    CFPreferencesCopyAppValue(&CFString::from_static_str(AUTOHIDE), &domain).is_some_and(|value| {
        value
            .downcast_ref::<CFBoolean>()
            .is_some_and(CFBoolean::as_bool)
    })
}
