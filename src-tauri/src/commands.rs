use std::sync::mpsc::Sender;

use tauri::State;
use tauri::ipc::Channel;

use crate::Error;
use crate::dock::DockItem;

pub enum TrackerMessage {
    Watch(Channel<Vec<DockItem>>),
    ScreenParametersChanged,
    OverlayOrderFailed,
}

pub struct TrackerInbox(pub Sender<TrackerMessage>);

/// Streams the Dock items to `on_items`, starting with the current frame.
/// A later call replaces the channel, so a reloaded webview resubscribes.
#[tauri::command]
pub async fn watch_dock(
    on_items: Channel<Vec<DockItem>>,
    inbox: State<'_, TrackerInbox>,
) -> Result<(), Error> {
    inbox
        .0
        .send(TrackerMessage::Watch(on_items))
        .map_err(|_| Error::TrackerStopped)?;
    tracing::debug!("dock watcher subscribed");
    Ok(())
}
