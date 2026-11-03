use tauri::State;

use crate::dock::{self, DockItem};
use crate::geometry::Display;
use crate::macos::dock::DockError;
use crate::trust::{Trust, TrustState};
use crate::{Error, macos};

/// The display the overlay window covers, set once in `setup()`.
pub struct OverlayDisplay(pub Display);

/// Reads the Dock's application items in the overlay's coordinates.
///
/// The trust reading is advisory: when it says untrusted, the AX read is
/// skipped, and when it says trusted, an AX failure is still reported.
#[tauri::command]
pub async fn dock_items(
    trust: State<'_, TrustState>,
    overlay: State<'_, OverlayDisplay>,
) -> Result<Vec<DockItem>, Error> {
    let display = overlay.0;
    let read = if trust.current() == Trust::Untrusted {
        Err(DockError::ApiDisabled)
    } else {
        tauri::async_runtime::spawn_blocking(macos::dock::items).await?
    };
    match read {
        Ok(children) => {
            let items = dock::app_items(children, display);
            tracing::debug!(count = items.len(), "dock items read");
            Ok(items)
        }
        Err(error) => {
            tracing::warn!(%error, "dock items not read");
            Err(error.into())
        }
    }
}
