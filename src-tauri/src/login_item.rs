//! Launch at login: what macOS reports, and what a change ends in.

/// macOS's state for this app as a login item (`SMAppService.mainApp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginItemStatus {
    NotRegistered,
    Enabled,
    /// Registered, but the user must allow it in System Settings first.
    RequiresApproval,
    NotFound,
}

impl LoginItemStatus {
    /// Only an enabled item opens at login.
    pub fn opens_at_login(self) -> bool {
        self == Self::Enabled
    }
}

/// macOS did not end in the state asked for.
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

#[cfg(test)]
mod tests;
