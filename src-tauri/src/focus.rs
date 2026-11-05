#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub i32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveApp {
    pub pid: Pid,
    pub bundle_id: Option<String>,
}

/// Settings hands focus back to the last app activated that is neither this
/// process nor another copy of it: a second launch is in front until it
/// exits, and macOS activates this copy itself on a reopen.
pub fn is_return_target(app: &ActiveApp, own_pid: Pid, own_bundle_id: &str) -> bool {
    app.pid != own_pid && app.bundle_id.as_deref() != Some(own_bundle_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnSettingsClosed {
    /// Settings was closed from behind: leave the user in the app in front.
    Leave,
    ReturnTo(Pid),
    /// No app to return to: hiding lets macOS bring the next app forward,
    /// rather than leave this app in front with no window.
    Hide,
}

pub fn on_settings_closed(dockmoss_active: bool, return_to: Option<Pid>) -> OnSettingsClosed {
    match (dockmoss_active, return_to) {
        (false, _) => OnSettingsClosed::Leave,
        (true, Some(pid)) => OnSettingsClosed::ReturnTo(pid),
        (true, None) => OnSettingsClosed::Hide,
    }
}

/// Whether to hide once the decision has run: a target that would not
/// activate (it quit) leaves this app in front, as no target does.
pub fn hides(decision: OnSettingsClosed, activated: bool) -> bool {
    match decision {
        OnSettingsClosed::Leave => false,
        OnSettingsClosed::ReturnTo(_) => !activated,
        OnSettingsClosed::Hide => true,
    }
}

#[cfg(test)]
mod tests;
