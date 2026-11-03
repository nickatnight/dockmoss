use tauri::State;

use crate::dock::{self, DockItem};
use crate::geometry::Display;
use crate::trust::TrustState;
use crate::{Error, macos};

pub struct OverlayDisplay(pub Display);

#[tauri::command]
pub async fn dock_items(
    trust: State<'_, TrustState>,
    overlay: State<'_, OverlayDisplay>,
) -> Result<Vec<DockItem>, Error> {
    let display = overlay.0;
    let trust = trust.current();
    let read = tauri::async_runtime::spawn_blocking(move || {
        dock::read(trust, || macos::dock::DockReader::new().read())
    })
    .await?;
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
