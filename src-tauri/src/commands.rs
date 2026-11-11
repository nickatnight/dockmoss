use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde_json::{Map, Value};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::Error;
use crate::bounded::{self, AloneRead};
use crate::dock::DockItem;
use crate::launch_log_writer::{LaunchLogInbox, WriterMessage};
use crate::login_item::{self, LaunchAtLogin};
use crate::macos;
use crate::settings::{self, NotSaved, Settings, SettingsView};
use crate::usage::{self, AppGrowth, DistinctDockApps};

const DOCK_APPS_TIMEOUT: Duration = Duration::from_millis(500);
const LOGIN_STATUS_TIMEOUT: Duration = Duration::from_millis(500);
static LOGIN_STATUS_READ: AtomicBool = AtomicBool::new(false);

pub enum TrackerMessage {
    Watch(Channel<Vec<DockItem>>),
    ScreenParametersChanged,
    OverlayOrderFailed,
    DockHoverChanged,
    AppLaunching,
}

pub struct TrackerInbox(pub Sender<TrackerMessage>);

pub struct SettingsPath(pub PathBuf);

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
    let login = tauri::async_runtime::spawn(launch_at_login());
    let dock = dock_apps(&app).await;
    let launch_at_login = login.await.unwrap_or_else(|error| {
        tracing::warn!(%error, "launch at login status not read");
        None
    });
    let settings = settings
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    Ok(SettingsView::new(settings, dock.as_ref(), launch_at_login))
}

async fn launch_at_login() -> Option<LaunchAtLogin> {
    let waited = tauri::async_runtime::spawn_blocking(|| {
        bounded::read_alone(&LOGIN_STATUS_READ, LOGIN_STATUS_TIMEOUT, || {
            macos::login_item::status().launch_at_login()
        })
    })
    .await;
    match waited {
        Ok(AloneRead::Answered(status)) => Some(status),
        Ok(AloneRead::NoAnswer) => {
            tracing::warn!("launch at login status not read in time");
            None
        }
        Ok(AloneRead::Busy) => {
            tracing::warn!("launch at login status not read: an earlier read has not answered");
            None
        }
        Err(error) => {
            tracing::warn!(%error, "launch at login status not read");
            None
        }
    }
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
) -> Result<Settings, Error> {
    tauri::async_runtime::spawn_blocking(move || save_settings_blocking(&app, &settings)).await?
}

/// Writes and syncs the file, so it runs off the async runtime.
fn save_settings_blocking(app: &AppHandle, value: &Map<String, Value>) -> Result<Settings, Error> {
    let next = Settings::parse(value).inspect_err(|error| {
        tracing::info!(field = error.field().key(), %error, "settings not saved: invalid");
    })?;
    let path = app
        .try_state::<SettingsPath>()
        .ok_or_else(|| NotSaved::new(std::io::Error::other("the app data directory is unknown")))?;
    let state = app.state::<Mutex<Settings>>();
    let mut saved = state.lock().unwrap_or_else(PoisonError::into_inner);
    settings::commit(&mut saved, next, |bytes| {
        usage::write_atomically(&path.0, bytes)
    })
    .inspect_err(|error| tracing::warn!(%error, "settings not saved"))?;
    saved.log("settings saved");
    apply_to_growth(app, &saved);
    Ok(saved.clone())
}

/// Takes the guard, so a send can only happen under the settings lock: the
/// usage thread then applies saves in the order they were written.
fn apply_to_growth(app: &AppHandle, saved: &MutexGuard<'_, Settings>) {
    let Some(inbox) = app.try_state::<LaunchLogInbox>() else {
        tracing::debug!("settings saved; usage tracking is off, so nothing to apply");
        return;
    };
    if inbox
        .0
        .send(WriterMessage::Settings(Settings::clone(saved)))
        .is_err()
    {
        tracing::warn!("saved settings not applied: the usage thread stopped");
    }
}

#[tauri::command]
pub async fn set_launch_at_login(enabled: bool) -> Result<LaunchAtLogin, Error> {
    tauri::async_runtime::spawn_blocking(move || set_launch_at_login_blocking(enabled)).await?
}

fn set_launch_at_login_blocking(wanted: bool) -> Result<LaunchAtLogin, Error> {
    if let Err(error) = macos::login_item::set(wanted) {
        tracing::warn!(%error, wanted, "launch at login change refused");
    }
    // The change itself cannot be abandoned, but the read after it can.
    let read = bounded::read_alone(
        &LOGIN_STATUS_READ,
        LOGIN_STATUS_TIMEOUT,
        macos::login_item::status,
    );
    let now = login_item::after_change(wanted, read).inspect_err(|failed| {
        tracing::warn!(
            wanted,
            ?failed,
            "launch at login not changed, or not read after the change"
        );
    })?;
    tracing::info!(launch_at_login = ?now, "launch at login set");
    Ok(now)
}
