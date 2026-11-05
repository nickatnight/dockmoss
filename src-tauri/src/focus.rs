#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub i32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveApp {
    pub pid: Pid,
    pub bundle_id: Option<String>,
    /// Has a regular activation policy (can have a Dock icon and windows).
    pub regular: bool,
}

/// Settings hands focus back to the last regular app activated that is
/// neither this process nor another copy of it: a second launch is in front
/// until it exits, macOS activates this copy itself on a reopen, and an agent
/// (a launcher that activated to open this app) has no window to take the
/// keyboard.
pub fn is_return_target(app: &ActiveApp, own_pid: Pid, own_bundle_id: &str) -> bool {
    app.regular && app.pid != own_pid && app.bundle_id.as_deref() != Some(own_bundle_id)
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

/// `target` is the remembered app and `at_its_pid` the app running under its
/// pid now. Focus goes back only while that is still the same app, so a pid
/// that macOS reused for another app after the target quit is not activated.
pub fn on_settings_closed(
    dockmoss_active: bool,
    target: Option<&ActiveApp>,
    at_its_pid: Option<&ActiveApp>,
) -> OnSettingsClosed {
    if !dockmoss_active {
        return OnSettingsClosed::Leave;
    }
    match (target, at_its_pid) {
        (Some(target), Some(now)) if target.pid == now.pid && target.bundle_id == now.bundle_id => {
            OnSettingsClosed::ReturnTo(target.pid)
        }
        _ => OnSettingsClosed::Hide,
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
