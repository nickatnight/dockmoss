use tauri::State;

use crate::dock::{self, DockItem};
use crate::geometry::Display;
use crate::macos::dock::DockError;
use crate::trust::{Trust, TrustState};
use crate::{Error, macos};

pub struct OverlayDisplay(pub Display);

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
