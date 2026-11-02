use objc2::MainThreadMarker;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};

/// Opens `url` with its default handler. Returns `false` when the string is
/// not a valid URL or no handler accepted it.
pub fn open_url(url: &str, _mtm: MainThreadMarker) -> bool {
    NSURL::URLWithString(&NSString::from_str(url))
        .is_some_and(|url| NSWorkspace::sharedWorkspace().openURL(&url))
}
