use objc2::MainThreadMarker;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};

const ACCESSIBILITY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("{0:?} is not a valid URL")]
    InvalidUrl(String),
    #[error("macOS could not open {0:?}")]
    NotOpened(String),
}

pub fn open_accessibility_settings(mtm: MainThreadMarker) -> Result<(), WorkspaceError> {
    open_url(ACCESSIBILITY_SETTINGS_URL, mtm)
}

fn open_url(url: &str, _mtm: MainThreadMarker) -> Result<(), WorkspaceError> {
    let ns_url = NSURL::URLWithString(&NSString::from_str(url))
        .ok_or_else(|| WorkspaceError::InvalidUrl(url.to_owned()))?;
    if NSWorkspace::sharedWorkspace().openURL(&ns_url) {
        Ok(())
    } else {
        Err(WorkspaceError::NotOpened(url.to_owned()))
    }
}
