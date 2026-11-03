use crate::bundle_id::BundleId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningApp {
    pub bundle_id: Option<String>,
    pub regular: bool,
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

pub fn tracked_bundle_id(app: RunningApp) -> Option<BundleId> {
    if !app.regular {
        return None;
    }
    BundleId::new(app.bundle_id?)
}

#[cfg(test)]
mod tests;
