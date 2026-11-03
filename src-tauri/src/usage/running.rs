use crate::bundle_id::BundleId;

/// An app as `NSWorkspace` reports it, in plain Rust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningApp {
    pub bundle_id: Option<String>,
    /// Its activation policy is `Regular`: it can have a Dock icon.
    pub regular: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEventKind {
    Launched,
    Activated,
    Terminated,
}

/// An `NSWorkspace` notification about one app. Each kind counts as use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEvent {
    pub kind: AppEventKind,
    pub app: RunningApp,
}

/// The bundle ID to record for `app`, or `None` for an app that cannot be a
/// Dock app item: one without a bundle ID, or an agent or background helper
/// (this app runs as an accessory, so it is skipped too).
pub fn tracked_bundle_id(app: RunningApp) -> Option<BundleId> {
    if !app.regular {
        return None;
    }
    BundleId::new(app.bundle_id?)
}

#[cfg(test)]
mod tests;
