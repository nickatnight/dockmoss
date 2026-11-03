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

impl DockError {
    /// True when no part of the Dock can be read: access is not granted, or
    /// the Dock is not answering. Any other error belongs to one child.
    pub fn is_dock_wide(&self) -> bool {
        matches!(self, Self::ApiDisabled | Self::CannotComplete)
    }
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
///
/// `TrustState` is refreshed only every few seconds, so trust revoked while
/// running shows up first as a read that returns `ApiDisabled`. From then on
/// the gate skips the read and calls `recheck` (a trust query that never
/// prompts) each tick, until it or `TrustState` reports trust again. The
/// latch holds only while `check_trust` agrees with AX.
#[derive(Debug, Default)]
pub struct ReadGate {
    revoked: bool,
}

impl ReadGate {
    pub fn read(
        &mut self,
        trust: Trust,
        recheck: impl FnOnce() -> Trust,
        read: impl FnOnce() -> Result<Vec<RawDockChild>, DockError>,
    ) -> Result<Vec<RawDockChild>, DockError> {
        match trust {
            Trust::Untrusted => {
                self.revoked = false;
                return Err(DockError::ApiDisabled);
            }
            Trust::Trusted if self.revoked && recheck() == Trust::Untrusted => {
                return Err(DockError::ApiDisabled);
            }
            Trust::Trusted => {}
        }
        let result = read();
        self.revoked = result == Err(DockError::ApiDisabled);
        result
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
    if app_path.to_str().is_none() {
        tracing::debug!(path = %app_path.display(), "dock app item path is not UTF-8");
        return None;
    }
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
