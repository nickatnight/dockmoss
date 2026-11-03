use std::path::PathBuf;

use serde::Serialize;

use crate::bundle_id::BundleId;
use crate::geometry::{Display, LogicalRect, ScreenRect, to_overlay_local};
use crate::trust::Trust;

const APPLICATION_SUBROLE: &str = "AXApplicationDockItem";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DockError {
    #[error("the Dock process is not running")]
    NotRunning,
    #[error("Accessibility access is not granted, so the Dock cannot be read")]
    ApiDisabled,
    #[error("the Dock did not answer an Accessibility request")]
    CannotComplete,
    #[error("the Dock's Accessibility tree has no item list")]
    NoItemList,
    #[error("reading {attribute} from the Dock failed with AX error {code}")]
    Ax { attribute: &'static str, code: i32 },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockItem {
    pub bundle_id: BundleId,
    pub app_path: PathBuf,
    pub rect: LogicalRect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawDockChild {
    pub subrole: Option<String>,
    pub rect: Option<ScreenRect>,
    pub app_path: Option<PathBuf>,
    pub bundle_id: Option<String>,
}

/// Runs the Dock read only when trusted. On macOS 27 an untrusted AX read
/// raises the system Accessibility dialog by itself, so a poll must never
/// reach `read` while untrusted.
pub fn read(
    trust: Trust,
    read: impl FnOnce() -> Result<Vec<RawDockChild>, DockError>,
) -> Result<Vec<RawDockChild>, DockError> {
    match trust {
        Trust::Trusted => read(),
        Trust::Untrusted => Err(DockError::ApiDisabled),
    }
}

pub fn app_items(children: Vec<RawDockChild>, display: Display) -> Vec<DockItem> {
    children
        .into_iter()
        .filter(|child| child.subrole.as_deref() == Some(APPLICATION_SUBROLE))
        .filter_map(|child| app_item(child, display))
        .collect()
}

fn app_item(child: RawDockChild, display: Display) -> Option<DockItem> {
    let Some(app_path) = child.app_path else {
        tracing::debug!(bundle_id = child.bundle_id, "dock app item has no URL");
        return None;
    };
    let Some(bundle_id) = child.bundle_id.and_then(BundleId::new) else {
        tracing::debug!(path = %app_path.display(), "dock app item has no bundle id");
        return None;
    };
    let Some(rect) = child.rect else {
        tracing::debug!(path = %app_path.display(), "dock app item has no position or size");
        return None;
    };
    let rect = to_overlay_local(rect, display)?;
    Some(DockItem {
        bundle_id,
        app_path,
        rect,
    })
}

#[cfg(test)]
mod tests;
