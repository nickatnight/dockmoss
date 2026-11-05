use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Map, Value};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::Error;
use crate::dock::DockItem;
use crate::launch_log_writer::{LaunchLogInbox, WriterMessage};
use crate::settings::{Settings, SettingsView};
use crate::usage::{AppGrowth, DistinctDockApps};

const DOCK_APPS_TIMEOUT: Duration = Duration::from_millis(500);

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

#[tauri::command]
pub async fn get_settings(
    app: AppHandle,
    settings: State<'_, Mutex<Settings>>,
) -> Result<SettingsView, Error> {
    let dock = dock_apps(&app).await;
    let settings = settings
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    Ok(SettingsView::new(settings, dock.as_ref()))
}

async fn dock_apps(app: &AppHandle) -> Option<DistinctDockApps> {
    let Some(inbox) = app.try_state::<LaunchLogInbox>() else {
        tracing::debug!("settings opened with usage tracking off; no dock apps listed");
        return None;
    };
    let (reply, replied) = mpsc::channel();
    if inbox.0.send(WriterMessage::ListDockApps(reply)).is_err() {
        tracing::warn!("dock apps not listed: the usage thread stopped");
        return None;
    }
    let waited =
        tauri::async_runtime::spawn_blocking(move || replied.recv_timeout(DOCK_APPS_TIMEOUT)).await;
    match waited {
        Ok(Ok(apps)) => Some(apps),
        Ok(Err(error)) => {
            tracing::warn!(%error, "dock apps not listed in time");
            None
        }
        Err(error) => {
            tracing::warn!(%error, "dock apps not listed");
            None
        }
    }
}

#[tauri::command]
pub async fn save_settings(
    settings: Map<String, Value>,
    app: AppHandle,
    state: State<'_, Mutex<Settings>>,
) -> Result<Settings, Error> {
    let settings = Settings::parse(&settings).inspect_err(|error| {
        tracing::info!(field = error.field().key(), %error, "settings not saved: invalid");
    })?;
    let mut saved = state.lock().unwrap_or_else(PoisonError::into_inner);
    if let Err(error) = crate::store_settings(&app, &settings, &saved) {
        tracing::warn!(%error, "settings not saved: the store write failed");
        return Err(Error::SettingsNotSaved);
    }
    saved.clone_from(&settings);
    crate::log_settings(&settings, "settings saved");
    Ok(settings)
}
