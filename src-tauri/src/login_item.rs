use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginItemStatus {
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
}

impl LoginItemStatus {
    pub fn opens_at_login(self) -> bool {
        self == Self::Enabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("launch at login was not changed")]
pub struct NotChanged;

/// Whether the app now opens at login, judged by the status read after the
/// change, not by whether the call reported an error: unregistering an item
/// that was never registered fails, yet ends where it was asked to.
pub fn outcome(wanted: bool, after: LoginItemStatus) -> Result<bool, NotChanged> {
    if after.opens_at_login() == wanted {
        Ok(wanted)
    } else {
        Err(NotChanged)
    }
}

/// `tauri dev` runs a bare binary, which must never become a login item:
/// only a path ending in `.app` is an app bundle.
pub fn is_app_bundle(bundle_path: &Path) -> bool {
    bundle_path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}

#[cfg(test)]
mod tests;
