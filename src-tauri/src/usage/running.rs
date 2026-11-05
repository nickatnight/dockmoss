use crate::bundle_id::BundleId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningApp {
    pub bundle_id: Option<String>,
    pub can_have_dock_icon: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEventKind {
    Launched,
    Activated,
    Terminated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEvent {
    pub kind: AppEventKind,
    pub app: RunningApp,
}

pub fn tracked_bundle_id(app: RunningApp, own_bundle_id: &str) -> Option<BundleId> {
    if !app.can_have_dock_icon {
        return None;
    }
    let bundle_id = app.bundle_id?;
    if bundle_id == own_bundle_id {
        return None;
    }
    BundleId::new(bundle_id)
}

#[cfg(test)]
mod tests;
