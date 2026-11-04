use std::sync::mpsc::Sender;

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::Error;
use crate::dock::DockItem;
use crate::launch_log_writer::{LaunchLogInbox, WriterMessage};
use crate::usage::AppGrowth;

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

/// Streams each Dock app's growth to `on_growth`, starting with the current
/// frame. A later call replaces the channel. `usage_stopped` when usage
/// tracking did not start or its thread has stopped.
#[tauri::command]
pub async fn watch_growth(on_growth: Channel<Vec<AppGrowth>>, app: AppHandle) -> Result<(), Error> {
    let inbox = app
        .try_state::<LaunchLogInbox>()
        .ok_or(Error::UsageStopped)?;
    inbox
        .0
        .send(WriterMessage::WatchGrowth(on_growth))
        .map_err(|_| Error::UsageStopped)?;
    tracing::debug!("growth watcher subscribing");
    Ok(())
}
