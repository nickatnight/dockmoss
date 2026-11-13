mod bounded;
mod bundle_id;
mod commands;
mod dock;
mod error;
mod focus;
mod geometry;
mod growth;
mod login_item;
mod macos;
mod navigation;
mod settings;
mod tracker;
mod tray;
mod trust;
mod usage;

pub use error::Error;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError, mpsc};
use std::time::Duration;

use jiff::Timestamp;
use tauri::{
    AppHandle, Manager, RunEvent, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_log::log::LevelFilter;

use commands::{SettingsPath, TrackerInbox, TrackerMessage};
use focus::OnSettingsClosed;
use geometry::Display;
use growth::FastForward;
use launch_log_writer::{LaunchLogInbox, WriterMessage};
use settings::Settings;
use tracker::Visibility;
use trust::TrustState;
use usage::{LAUNCH_LOG_FILE, SETTINGS_FILE};

const OVERLAY_LABEL: &str = "overlay";
const SETTINGS_LABEL: &str = "settings";
const SETTINGS_HEIGHT: f64 = 760.0;
const FLUSH_TIMEOUT: Duration = Duration::from_millis(500);
const TRUST_POLL_INTERVAL: Duration = Duration::from_secs(5);
const APP_LOG_LEVEL: LevelFilter = if cfg!(debug_assertions) {
    LevelFilter::Debug
} else {
    LevelFilter::Info
};

/// Builds and runs the Tauri application.
///
/// # Panics
///
/// Panics if the Tauri runtime fails to build.
pub fn run() {
    let built = tauri::Builder::default()
        // First: a second launch exits inside this plugin's setup, before
        // any other plugin and before our setup() opens a file or a window.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            on_second_launch(app);
        }))
        .plugin(private_log_dir())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(LevelFilter::Info)
                .level_for("dockmoss_lib", APP_LOG_LEVEL)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::watch_dock,
            commands::watch_growth,
            commands::get_settings,
            commands::save_settings,
            commands::set_launch_at_login
        ])
        .setup(|app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            if let Some(error) = app.try_state::<LogDirNotPrivate>() {
                tracing::warn!(kind = %error.0, "log folder not made private");
            }
            app.manage(LastOtherApp::default());
            let data_dir = moved_app_data_dir(app.handle());
            let (settings, settings_path) = load_settings(data_dir.as_ref());
            app.manage(Mutex::new(settings.clone()));
            if let Some(path) = settings_path {
                app.manage(SettingsPath(path));
            }
            start_trust_and_tray(app.handle(), data_dir.as_ref())?;
            let display = primary_display(app.handle())?;
            let (inbox, messages) = mpsc::channel();
            let tracker_inbox = inbox.clone();
            app.manage(TrackerInbox(inbox));
            let overlay = build_overlay(app.handle(), display)?;
            #[expect(
                clippy::expect_used,
                reason = "startup invariant: tauri runs setup() on the main thread"
            )]
            let mtm =
                macos::MainThreadMarker::new().expect("tauri runs setup() on the main thread");
            track_last_other_app(app.handle(), mtm);
            let launch_log = data_dir
                .as_ref()
                .and_then(|data_dir| data_dir.file(LAUNCH_LOG_FILE));
            if let Some(path) = launch_log {
                if let Err(error) = start_launch_log(app.handle(), mtm, &path, &settings) {
                    tracing::warn!(%error, "usage tracking not started; running without growth");
                }
            } else {
                tracing::warn!(
                    "usage tracking not started: the launch log is unknown or still in the old app folder"
                );
            }
            let levels = macos::overlay::pin_above_dock(&overlay, mtm).map_err(Error::from)?;
            tracing::info!(
                dock_level = levels.dock,
                overlay_level = levels.overlay,
                "overlay pinned above the dock"
            );
            overlay.show()?;
            let shown = if overlay.is_visible()? {
                Visibility::Shown
            } else {
                Visibility::Hidden
            };
            let inbox = tracker_inbox.clone();
            dock_tracker::spawn(app.handle(), messages, inbox, display, shown, mtm)?;
            macos::screen::on_screen_parameters_change(
                move || {
                    if tracker_inbox
                        .send(TrackerMessage::ScreenParametersChanged)
                        .is_err()
                    {
                        tracing::warn!("screen change not passed on: the dock tracker stopped");
                    }
                },
                mtm,
            );
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "dockmoss started");
            Ok(())
        })
        .build(tauri::generate_context!());
    #[expect(
        clippy::expect_used,
        reason = "startup invariant: the runtime builds with the context built into the binary"
    )]
    let app = built.expect("tauri runtime builds with the bundled context");
    app.run(|app, event| match event {
        RunEvent::Exit => flush_launch_log(app),
        // A Finder double-click, Spotlight or `open` on the running copy
        // starts no second process: macOS activates this one and reopens it.
        RunEvent::Reopen { .. } => {
            tracing::info!("dockmoss reopened; opening settings");
            show_settings_window(app);
        }
        _ => {}
    });
}

/// The log plugin's setup creates the log folder and opens the file, so this
/// runs before it (plugins set up in the order they are registered).
fn private_log_dir() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("private-log-dir")
        .setup(|app, _api| {
            let made = app
                .path()
                .app_log_dir()
                .map_err(|error| std::io::Error::other(error.to_string()))
                .and_then(|dir| usage::private_dir(&dir));
            if let Err(error) = made {
                app.manage(LogDirNotPrivate(error.kind()));
            }
            Ok(())
        })
        .build()
}

struct LogDirNotPrivate(std::io::ErrorKind);

/// The plugin calls this on a tokio task. Main-thread tasks run only once the
/// event loop runs, which is after `setup()` has managed the state Settings
/// needs.
fn on_second_launch(app: &AppHandle) {
    tracing::info!("dockmoss launched again; opening settings");
    let handle = app.clone();
    if let Err(error) = app.run_on_main_thread(move || show_settings_window(&handle)) {
        tracing::warn!(%error, "settings not opened for the second launch");
    }
}

fn start_launch_log(
    app: &AppHandle,
    mtm: macos::MainThreadMarker,
    path: &Path,
    saved: &Settings,
) -> tauri::Result<()> {
    let log = usage::load_or_set_aside(path, Timestamp::now());
    let entries = log.len();
    let first_run = log.baseline_at().is_none();
    let fast_forward_override = fast_forward_override();
    let fast_forward = saved.applied(fast_forward_override).fast_forward;
    let inbox = launch_log_writer::spawn(path.to_path_buf(), log, saved, fast_forward_override)?;
    let own_bundle_id = app.config().identifier.clone();
    let now = Timestamp::now();
    let mut running = 0_usize;
    for bundle_id in macos::workspace::running_apps(mtm)
        .into_iter()
        .filter_map(|running| usage::tracked_bundle_id(running, &own_bundle_id))
    {
        tracing::debug!(bundle_id = bundle_id.as_str(), "running at startup");
        let used = WriterMessage::Used {
            bundle_id,
            at: now,
            kind: usage::AppEventKind::Launched,
        };
        if inbox.send(used).is_err() {
            tracing::warn!("running apps not recorded: the launch log writer stopped");
            break;
        }
        running += 1;
    }
    let events = inbox.clone();
    macos::workspace::on_app_events(
        move |event| record_app_event(&events, event, &own_bundle_id),
        mtm,
    );
    app.manage(LaunchLogInbox(inbox));
    tracing::info!(
        path = %path.display(),
        entries,
        running_apps = running,
        first_run,
        fast_forward = fast_forward.factor(),
        "launch log started"
    );
    Ok(())
}

/// The app data folder once the old folder's files are in it. Only
/// `moved_app_data_dir` makes one, so nothing that reads app data can run
/// before the move.
struct DataDir {
    dir: PathBuf,
    moved: usage::MoveReport,
}

impl DataDir {
    fn file(&self, name: &str) -> Option<PathBuf> {
        self.moved.file_in(&self.dir, name)
    }
}

fn moved_app_data_dir(app: &AppHandle) -> Option<DataDir> {
    let new_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(error) => {
            tracing::warn!(%error, "app data folder unknown; old data not moved");
            return None;
        }
    };
    let moved = usage::old_data_dir(&new_dir)
        .map(|old_dir| usage::move_old_data(&old_dir, &new_dir))
        .unwrap_or_default();
    moved.log();
    Some(DataDir {
        dir: new_dir,
        moved,
    })
}

fn load_settings(data_dir: Option<&DataDir>) -> (Settings, Option<PathBuf>) {
    let Some(path) = data_dir.and_then(|data_dir| data_dir.file(SETTINGS_FILE)) else {
        tracing::warn!(
            "settings file unknown or still in the old app folder; using defaults, saving off"
        );
        return (Settings::default(), None);
    };
    let loaded = settings::load_file(&path);
    for problem in &loaded.problems {
        tracing::warn!(%problem, "stored setting not loaded");
    }
    loaded.settings.log("settings loaded");
    (loaded.settings, Some(path))
}

#[cfg(debug_assertions)]
fn fast_forward_override() -> Option<FastForward> {
    const VARIABLE: &str = "DOCKMOSS_FAST_FORWARD";
    let value = std::env::var_os(VARIABLE)?;
    let parsed = value.to_str().and_then(FastForward::parse);
    if parsed.is_none() {
        tracing::warn!(
            value = %value.to_string_lossy(),
            "{VARIABLE} is not 1, 10, 100 or 1000; using the saved fast-forward"
        );
    }
    parsed
}

#[cfg(not(debug_assertions))]
fn fast_forward_override() -> Option<FastForward> {
    None
}

fn record_app_event(
    inbox: &mpsc::Sender<WriterMessage>,
    event: usage::AppEvent,
    own_bundle_id: &str,
) {
    let kind = event.kind;
    let Some(bundle_id) = usage::tracked_bundle_id(event.app, own_bundle_id) else {
        return;
    };
    tracing::debug!(bundle_id = bundle_id.as_str(), ?kind, "app used");
    let used = WriterMessage::Used {
        bundle_id,
        at: Timestamp::now(),
        kind,
    };
    if inbox.send(used).is_err() {
        tracing::warn!(?kind, "app use not recorded: the launch log writer stopped");
    }
}

fn flush_launch_log(app: &AppHandle) {
    let Some(inbox) = app.try_state::<LaunchLogInbox>() else {
        return;
    };
    let (ack, acked) = mpsc::channel();
    if inbox.0.send(WriterMessage::Flush(ack)).is_err() {
        tracing::warn!("launch log not flushed on quit: the writer stopped");
        return;
    }
    match acked.recv_timeout(FLUSH_TIMEOUT) {
        Ok(()) => tracing::debug!("launch log flushed on quit"),
        Err(error) => tracing::warn!(%error, "launch log flush on quit not confirmed"),
    }
}

mod launch_log_writer {
    use std::path::PathBuf;
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use jiff::Timestamp;
    use tauri::ipc::Channel;

    use crate::bundle_id::BundleId;
    use crate::growth::FastForward;
    use crate::macos;
    use crate::settings::Settings;
    use crate::usage::{
        self, AppEventKind, AppGrowth, Change, DistinctDockApps, DockApp, DockUpdate, LaunchLog,
        SaveSchedule, UsageState,
    };

    pub enum WriterMessage {
        Used {
            bundle_id: BundleId,
            at: Timestamp,
            kind: AppEventKind,
        },
        DockApps(Vec<DockApp>),
        ListDockApps(Sender<DistinctDockApps>),
        WatchGrowth(Channel<Vec<AppGrowth>>),
        Settings(Settings),
        Flush(Sender<()>),
    }

    pub struct LaunchLogInbox(pub Sender<WriterMessage>);

    type Readings = Vec<(BundleId, Option<Timestamp>)>;

    /// How often the writer checks whether the first-run Spotlight read has
    /// finished, while one is running.
    const SEED_READ_POLL: Duration = Duration::from_millis(100);

    pub fn spawn(
        path: PathBuf,
        log: LaunchLog,
        saved: &Settings,
        fast_forward_override: Option<FastForward>,
    ) -> std::io::Result<Sender<WriterMessage>> {
        let (inbox, messages) = mpsc::channel();
        let writer = Writer {
            path,
            state: UsageState::new(log),
            schedule: SaveSchedule::default(),
            seed_read: None,
            applied: saved.applied(fast_forward_override),
            fast_forward_override,
            watcher: None,
            last_sent: None,
            next_recompute: Instant::now(),
        };
        std::thread::Builder::new()
            .name("launch-log".into())
            .spawn(move || writer.run(&messages))?;
        Ok(inbox)
    }

    struct Writer {
        path: PathBuf,
        state: UsageState,
        schedule: SaveSchedule,
        seed_read: Option<JoinHandle<Readings>>,
        applied: Settings,
        fast_forward_override: Option<FastForward>,
        watcher: Option<Channel<Vec<AppGrowth>>>,
        last_sent: Option<Vec<AppGrowth>>,
        next_recompute: Instant,
    }

    impl Writer {
        fn run(mut self, messages: &Receiver<WriterMessage>) {
            loop {
                let mut deadline = self
                    .schedule
                    .deadline()
                    .map_or(self.next_recompute, |save| save.min(self.next_recompute));
                if self.seed_read.is_some() {
                    let now = Instant::now();
                    deadline = deadline.min(now.checked_add(SEED_READ_POLL).unwrap_or(now));
                }
                match messages.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(message) => self.handle(message),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => {
                        if self.schedule.deadline().is_some() {
                            self.save(SaveTrigger::Disconnected);
                        }
                        tracing::debug!("launch log writer stopped: no senders left");
                        return;
                    }
                }
                self.collect_seed_read();
                let now = Instant::now();
                if self.schedule.is_due(now) {
                    self.save(SaveTrigger::Deadline);
                }
                if self.next_recompute <= now {
                    self.recompute();
                }
            }
        }

        fn handle(&mut self, message: WriterMessage) {
            match message {
                WriterMessage::Used {
                    bundle_id,
                    at,
                    kind,
                } => {
                    let change = self.state.used(bundle_id, at, kind);
                    self.apply(change);
                }
                WriterMessage::DockApps(apps) => {
                    match self.state.dock_apps(apps, Timestamp::now()) {
                        DockUpdate::Seed(apps) => self.start_seed_read(apps),
                        DockUpdate::Applied(change) => self.apply(change),
                    }
                }
                WriterMessage::ListDockApps(reply) => {
                    if reply.send(self.state.distinct_dock_apps()).is_err() {
                        tracing::debug!(
                            "dock apps listed after the settings window stopped waiting"
                        );
                    }
                }
                WriterMessage::WatchGrowth(channel) => {
                    self.watcher = Some(channel);
                    self.last_sent = None;
                    tracing::debug!("growth watcher subscribed");
                    self.recompute();
                }
                WriterMessage::Settings(saved) => {
                    self.applied = saved.applied(self.fast_forward_override);
                    tracing::debug!(
                        fast_forward = self.applied.fast_forward.factor(),
                        "settings applied to growth"
                    );
                    self.recompute();
                }
                WriterMessage::Flush(ack) => {
                    if self.schedule.deadline().is_some() {
                        self.save(SaveTrigger::Flush);
                    }
                    if ack.send(()).is_err() {
                        tracing::debug!("launch log flushed after the quit stopped waiting");
                    }
                }
            }
        }

        fn apply(&mut self, change: Change) {
            if change.saves() {
                self.schedule.changed(Instant::now());
            }
            if change.moves_growth() {
                self.recompute();
            }
        }

        fn start_seed_read(&mut self, apps: Vec<DockApp>) {
            let bundle_ids: Vec<BundleId> = apps.iter().map(|app| app.bundle_id.clone()).collect();
            match spawn_seed_read(apps) {
                Ok(read) => {
                    self.seed_read = Some(read);
                    tracing::debug!(apps = bundle_ids.len(), "spotlight read started");
                }
                Err(error) => {
                    tracing::warn!(%error, "spotlight read not started; every Dock app starts now");
                    self.finish_seeding(bundle_ids.into_iter().map(|id| (id, None)).collect());
                }
            }
            self.recompute();
        }

        fn collect_seed_read(&mut self) {
            if !self.seed_read.as_ref().is_some_and(JoinHandle::is_finished) {
                return;
            }
            let Some(read) = self.seed_read.take() else {
                return;
            };
            let readings = read.join().unwrap_or_else(|_| {
                tracing::warn!(
                    "spotlight read stopped without a result; every Dock app starts now"
                );
                Vec::new()
            });
            self.finish_seeding(readings);
        }

        fn finish_seeding(&mut self, readings: Readings) {
            let Some(report) = self.state.seeded(readings, Timestamp::now()) else {
                tracing::warn!("spotlight dates arrived with no first run in flight; ignored");
                return;
            };
            tracing::info!(
                dock_apps = report.dock_apps,
                spotlight_dates = report.dates_read,
                seeds_kept = report.seeds_kept,
                migration_batch_discarded = report.migration_batch,
                "first run: dock apps seeded and baselined"
            );
            self.apply(Change::Log);
        }

        fn recompute(&mut self) {
            let now = Instant::now();
            self.next_recompute = now
                .checked_add(usage::recompute_interval(self.applied.fast_forward))
                .unwrap_or(now);
            let frame = self.state.growth(Timestamp::now(), &self.applied);
            if self.last_sent.as_ref() == Some(&frame) {
                tracing::debug!(apps = frame.len(), "growth recomputed; unchanged");
                return;
            }
            let Some(watcher) = self.watcher.as_ref() else {
                tracing::debug!(apps = frame.len(), "growth recomputed with no watcher");
                return;
            };
            let summary = summarize(&frame);
            match watcher.send(frame.clone()) {
                Ok(()) => {
                    tracing::debug!(apps = frame.len(), growth = %summary, "growth sent");
                    self.last_sent = Some(frame);
                }
                Err(error) => {
                    tracing::debug!(%error, "growth watcher dropped after a failed send");
                    self.watcher = None;
                }
            }
        }

        fn save(&mut self, trigger: SaveTrigger) {
            let log = self.state.log();
            match usage::save(&self.path, log) {
                Ok(()) => {
                    self.schedule.saved();
                    tracing::debug!(entries = log.len(), ?trigger, "launch log saved");
                }
                Err(error) => {
                    self.schedule.save_failed(Instant::now());
                    tracing::warn!(%error, ?trigger, "launch log not saved; retrying in 5 s");
                }
            }
        }
    }

    fn summarize(frame: &[AppGrowth]) -> String {
        frame
            .iter()
            .map(|entry| {
                format!(
                    "{}={:.2}/{:?}",
                    entry.bundle_id.as_str(),
                    entry.overgrowth.get(),
                    entry.stage
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn spawn_seed_read(apps: Vec<DockApp>) -> std::io::Result<JoinHandle<Readings>> {
        std::thread::Builder::new()
            .name("spotlight-seed".into())
            .spawn(move || {
                apps.into_iter()
                    .map(|app| {
                        let date = macos::spotlight::last_used_date(&app.app_path)
                            .as_deref()
                            .and_then(usage::parse_mdls);
                        tracing::debug!(
                            bundle_id = app.bundle_id.as_str(),
                            path = %app.app_path.display(),
                            ?date,
                            "spotlight date read"
                        );
                        (app.bundle_id, date)
                    })
                    .collect()
            })
    }

    #[derive(Debug, Clone, Copy)]
    enum SaveTrigger {
        Deadline,
        Flush,
        Disconnected,
    }
}

fn start_trust_and_tray(
    app: &AppHandle,
    data_dir: Option<&DataDir>,
) -> Result<(), Box<dyn std::error::Error>> {
    let trust = macos::ax::request_trust();
    tracing::info!(trusted = trust.is_trusted(), "accessibility trust checked");
    let left_behind = data_dir.map_or_else(Vec::new, |data_dir| data_dir.moved.left_behind_data());
    tray::build(app, trust, &left_behind)?;
    app.manage(TrustState::new(trust));
    spawn_trust_watcher(app.clone(), left_behind)?;
    Ok(())
}

fn spawn_trust_watcher(app: AppHandle, left_behind: Vec<usage::LeftBehind>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("ax-trust".into())
        .spawn(move || {
            loop {
                std::thread::sleep(TRUST_POLL_INTERVAL);
                let state = app.state::<TrustState>();
                if let Some(trust) = state.replace_if_changed(macos::ax::check_trust()) {
                    tracing::info!(trusted = trust.is_trusted(), "accessibility trust changed");
                    refresh_tray_on_main_thread(&app, &left_behind);
                }
            }
        })?;
    Ok(())
}

/// The tray is looked up on the main thread because its handle is not
/// thread-safe: cloning it elsewhere would race the clones Tauri makes there.
fn refresh_tray_on_main_thread(app: &AppHandle, left_behind: &[usage::LeftBehind]) {
    let main = app.clone();
    let left_behind = left_behind.to_vec();
    let scheduled = app.run_on_main_thread(move || {
        let Some(tray) = main.tray_by_id(tray::TRAY_ID) else {
            tracing::warn!("tray not updated after a trust change: the tray is gone");
            return;
        };
        let trust = main.state::<TrustState>().current();
        if let Err(error) = tray::show_trust(&tray, trust, &left_behind) {
            tracing::warn!(%error, "tray update after a trust change failed");
        }
    });
    if let Err(error) = scheduled {
        tracing::warn!(%error, "could not schedule the tray update");
    }
}

mod dock_tracker {
    use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
    use std::time::{Duration, Instant};

    use tauri::ipc::Channel;
    use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager};

    use crate::commands::{TrackerInbox, TrackerMessage};
    use crate::dock::{self, DockItem};
    use crate::geometry::Display;
    use crate::launch_log_writer::{LaunchLogInbox, WriterMessage};
    use crate::tracker::{
        self, Activity, PendingLaunch, PollRate, Tick, TickLog, Tracker, Visibility, Woken,
    };
    use crate::trust::{Trust, TrustState};
    use crate::usage::DockApp;
    use crate::{OVERLAY_LABEL, macos, primary_display};

    pub(super) fn spawn(
        app: &AppHandle,
        messages: Receiver<TrackerMessage>,
        inbox: Sender<TrackerMessage>,
        display: Display,
        overlay: Visibility,
        mtm: macos::MainThreadMarker,
    ) -> std::io::Result<()> {
        let tracked = app.clone();
        std::thread::Builder::new()
            .name("dock-tracker".into())
            .spawn(move || track_dock(&tracked, &messages, display, overlay))?;
        watch_launches(inbox.clone(), mtm);
        watch_space_changes(inbox.clone(), mtm);
        if let Err(error) = spawn_observer(app.clone(), inbox) {
            tracing::warn!(%error, "dock hover observer not started; entry is polled");
        }
        Ok(())
    }

    fn watch_launches(tracker: Sender<TrackerMessage>, mtm: macos::MainThreadMarker) {
        macos::workspace::on_app_launching(
            move |app| {
                if app.can_have_dock_icon && tracker.send(TrackerMessage::AppLaunching).is_err() {
                    tracing::debug!("app launch not passed on: the dock tracker stopped");
                }
            },
            mtm,
        );
    }

    fn watch_space_changes(tracker: Sender<TrackerMessage>, mtm: macos::MainThreadMarker) {
        macos::workspace::on_active_space_changed(
            move || {
                if tracker.send(TrackerMessage::SpaceChanged).is_err() {
                    tracing::debug!("space change not passed on: the dock tracker stopped");
                }
            },
            mtm,
        );
    }

    fn spawn_observer(app: AppHandle, tracker: Sender<TrackerMessage>) -> std::io::Result<()> {
        macos::dock_observer::spawn(
            move || {
                app.try_state::<TrustState>()
                    .map_or(Trust::Untrusted, |trust| trust.current())
            },
            move || {
                if tracker.send(TrackerMessage::DockHoverChanged).is_err() {
                    tracing::debug!("dock hover change not passed on: the dock tracker stopped");
                }
            },
        )
    }

    fn track_dock(
        app: &AppHandle,
        messages: &Receiver<TrackerMessage>,
        display: Display,
        overlay: Visibility,
    ) {
        let Some(trust) = app.try_state::<TrustState>() else {
            tracing::warn!("dock tracker started before the trust state; not tracking");
            return;
        };
        let mut reader = macos::dock::DockReader::new();
        let mut gate = dock::ReadGate::default();
        let mut tracker = Tracker::new(display, overlay);
        let mut activity = Activity::default();
        let mut watcher = None;
        let mut rate = PollRate::Slow;
        let mut screen_changed = false;
        let mut launched = PendingLaunch::default();
        let mut deadline = Instant::now();
        loop {
            let started = Instant::now();
            if std::mem::take(&mut screen_changed) {
                follow_display(app, &mut tracker);
            }
            let cursor = macos::cursor::location();
            let read = gate.read(trust.current(), started, macos::ax::check_trust, || {
                reader.read()
            });
            tracing::debug!(elapsed_us = micros(started.elapsed()), ?rate, "dock read");
            let tick = tracker.tick(read, cursor, started);
            activity.departure(tick.left_dock, tracker.overlay_shown(), started);
            let next_rate = activity.rate(
                tick.proximity_rate,
                tick.awaits_reveal,
                cursor,
                tick.send.is_some(),
                started,
            );
            if next_rate != rate {
                tracing::debug!(rate = ?next_rate, "dock poll rate changed");
                rate = next_rate;
            }
            perform(app, tick, &mut watcher);
            let now = Instant::now();
            deadline = activity.wait_for_read(
                tracker.read_due(tracker::next_deadline(deadline, rate, now)),
                now,
                |until| {
                    drain_inbox_until(
                        until,
                        messages,
                        &mut tracker,
                        &mut watcher,
                        &mut screen_changed,
                        &mut launched,
                    )
                },
                macos::cursor::location,
            );
            launched.hand_to(&mut activity, tracker.overlay_shown());
        }
    }

    fn drain_inbox_until(
        deadline: Instant,
        messages: &Receiver<TrackerMessage>,
        tracker: &mut Tracker,
        watcher: &mut Option<Channel<Vec<DockItem>>>,
        screen_changed: &mut bool,
        launched: &mut PendingLaunch,
    ) -> Woken {
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match messages.recv_timeout(left) {
                Ok(TrackerMessage::Watch(channel)) => {
                    *watcher = Some(channel);
                    tracker.resend_on_next_tick();
                }
                Ok(TrackerMessage::ScreenParametersChanged) => *screen_changed = true,
                Ok(TrackerMessage::OverlayOrderFailed) => tracker.order_failed(),
                Ok(TrackerMessage::DockHoverChanged) => {
                    tracker.record_switch_end();
                    return Woken::DockHover { at: Instant::now() };
                }
                Ok(TrackerMessage::AppLaunching) => {
                    let at = Instant::now();
                    launched.record(at);
                    return Woken::AppLaunch { at };
                }
                Ok(TrackerMessage::SpaceChanged) => {
                    if tracker.awaits_switch_end() {
                        tracker.record_switch_end();
                        return Woken::SpaceChanged { at: Instant::now() };
                    }
                }
                Err(RecvTimeoutError::Timeout) => return Woken::Due,
                Err(RecvTimeoutError::Disconnected) => {
                    std::thread::sleep(left);
                    return Woken::Due;
                }
            }
        }
    }

    fn follow_display(app: &AppHandle, tracker: &mut Tracker) {
        let display = match primary_display(app) {
            Ok(display) => display,
            Err(error) => {
                tracing::warn!(%error, "display change not followed");
                return;
            }
        };
        if !tracker.set_display(display) {
            tracing::debug!("screen parameters changed; primary display unchanged");
            return;
        }
        let frame = display.frame();
        let scale = display.scale_factor;
        let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
            tracing::warn!("display changed but the overlay window is gone");
            return;
        };
        let moved = window
            .set_position(LogicalPosition::new(frame.x, frame.y))
            .and_then(|()| window.set_size(LogicalSize::new(frame.width, frame.height)));
        match moved {
            Ok(()) => tracing::info!(
                x = frame.x,
                y = frame.y,
                width = frame.width,
                height = frame.height,
                scale,
                "overlay moved to the new display frame"
            ),
            Err(error) => tracing::warn!(%error, "overlay not moved to the new display frame"),
        }
    }

    fn perform(app: &AppHandle, tick: Tick, watcher: &mut Option<Channel<Vec<DockItem>>>) {
        match tick.log {
            Some(TickLog::ReadFailed(error)) => tracing::warn!(%error, "dock read failed"),
            Some(TickLog::Recovered) => tracing::info!("dock read recovered"),
            None => {}
        }
        if let Some(visibility) = tick.overlay {
            order_overlay(app, visibility);
        }
        if let Some(apps) = tick.apps {
            report_dock_apps(app, apps);
        }
        let Some(frame) = tick.send else {
            return;
        };
        let count = frame.len();
        let Some(channel) = watcher.as_ref() else {
            tracing::debug!(count, "dock items changed with no watcher");
            return;
        };
        match channel.send(frame) {
            Ok(()) => tracing::debug!(count, "dock items sent"),
            Err(error) => {
                tracing::debug!(%error, "dock watcher dropped after a failed send");
                *watcher = None;
            }
        }
    }

    fn report_dock_apps(app: &AppHandle, apps: Vec<DockApp>) {
        let Some(inbox) = app.try_state::<LaunchLogInbox>() else {
            tracing::debug!("dock apps changed; usage tracking is off");
            return;
        };
        let bundle_ids: Vec<&str> = apps.iter().map(|app| app.bundle_id.as_str()).collect();
        tracing::debug!(?bundle_ids, "dock apps changed");
        if inbox.0.send(WriterMessage::DockApps(apps)).is_err() {
            tracing::warn!("dock apps not passed on: the usage thread stopped");
        }
    }

    fn order_overlay(app: &AppHandle, visibility: Visibility) {
        let main = app.clone();
        let scheduled = app.run_on_main_thread(move || {
            // No retry: run_on_main_thread always runs this on the main thread.
            let Some(mtm) = macos::MainThreadMarker::new() else {
                tracing::warn!("overlay ordering ran off the main thread");
                return;
            };
            // No retry: with the window gone there is nothing left to order.
            let Some(window) = main.get_webview_window(OVERLAY_LABEL) else {
                tracing::warn!("overlay window is gone");
                return;
            };
            match macos::overlay::set_visible(&window, visibility == Visibility::Shown, mtm) {
                Ok(state) => tracing::debug!(
                    ?visibility,
                    visible = state.visible,
                    level = state.level,
                    ignores_mouse_events = state.ignores_mouse_events,
                    collection_behavior = state.collection_behavior,
                    "overlay ordered"
                ),
                Err(error) => {
                    tracing::warn!(%error, ?visibility, "overlay not ordered");
                    report_order_failed(&main);
                }
            }
        });
        if let Err(error) = scheduled {
            tracing::warn!(%error, "could not schedule the overlay ordering");
            report_order_failed(app);
        }
    }

    fn report_order_failed(app: &AppHandle) {
        let Some(inbox) = app.try_state::<TrackerInbox>() else {
            tracing::warn!("overlay ordering not retried: no tracker inbox");
            return;
        };
        if inbox.0.send(TrackerMessage::OverlayOrderFailed).is_err() {
            tracing::warn!("overlay ordering not retried: the dock tracker stopped");
        }
    }

    fn micros(duration: Duration) -> u64 {
        u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
    }

    #[cfg(test)]
    mod tests {
        use std::path::PathBuf;
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        use tauri::ipc::Channel;

        use super::drain_inbox_until;
        use crate::commands::TrackerMessage;
        use crate::dock::RawDockChild;
        use crate::geometry::{Display, ScreenPoint, ScreenRect};
        use crate::tracker::{PendingLaunch, Tracker, Visibility, Woken};

        const PRIMARY_2X: Display = Display {
            physical_x: 0,
            physical_y: 0,
            physical_width: 3024,
            physical_height: 1964,
            scale_factor: 2.0,
        };

        fn safari() -> RawDockChild {
            RawDockChild {
                subrole: Some("AXApplicationDockItem".into()),
                rect: Some(ScreenRect {
                    x: 700.0,
                    y: 920.0,
                    width: 56.0,
                    height: 56.0,
                }),
                app_path: Some(PathBuf::from("/Applications/Safari.app")),
                bundle_id: Some("com.apple.Safari".into()),
            }
        }

        #[test]
        fn a_burst_of_screen_changes_does_not_cut_the_wait_short() {
            let (inbox, messages) = mpsc::channel();
            for _ in 0..1000 {
                inbox
                    .send(TrackerMessage::ScreenParametersChanged)
                    .expect("the receiver is alive");
            }
            let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
            let mut watcher = None;
            let mut screen_changed = false;
            let mut launched = PendingLaunch::default();
            let delay = Duration::from_millis(50);

            let started = Instant::now();
            drain_inbox_until(
                started + delay,
                &messages,
                &mut tracker,
                &mut watcher,
                &mut screen_changed,
                &mut launched,
            );

            assert!(
                started.elapsed() >= delay,
                "returned after {:?}",
                started.elapsed()
            );
            assert!(screen_changed);
            assert!(messages.try_recv().is_err(), "every message was taken");
        }

        #[test]
        fn a_dock_hover_change_ends_the_wait_at_once_after_taking_earlier_messages() {
            let (inbox, messages) = mpsc::channel();
            inbox
                .send(TrackerMessage::ScreenParametersChanged)
                .expect("the receiver is alive");
            inbox
                .send(TrackerMessage::DockHoverChanged)
                .expect("the receiver is alive");
            let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
            let mut watcher = None;
            let mut screen_changed = false;
            let mut launched = PendingLaunch::default();

            let started = Instant::now();
            let woken = drain_inbox_until(
                started + Duration::from_secs(5),
                &messages,
                &mut tracker,
                &mut watcher,
                &mut screen_changed,
                &mut launched,
            );

            let returned = Instant::now();

            assert!(matches!(woken, Woken::DockHover { at } if at >= started && at <= returned));
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "returned after {:?}",
                started.elapsed()
            );
            assert!(screen_changed);
        }

        const FAR: Option<ScreenPoint> = Some(ScreenPoint { x: 720.0, y: 300.0 });

        /// A tracker whose overlay is held after a switch from a full-screen
        /// Space: the Dock read off screen at `start`, back 300 ms later.
        fn held_after_a_switch(start: Instant) -> Tracker {
            let off_screen = RawDockChild {
                rect: Some(ScreenRect {
                    y: 990.0,
                    ..safari().rect.expect("a rect")
                }),
                ..safari()
            };
            let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
            tracker.tick(Ok(vec![off_screen]), FAR, start);
            let held = tracker.tick(Ok(vec![safari()]), FAR, start + Duration::from_millis(300));
            assert_eq!(held.overlay, None, "held");
            tracker
        }

        #[test]
        fn a_dock_hover_change_brings_a_held_overlay_in_on_the_next_read() {
            let (inbox, messages) = mpsc::channel();
            let start = Instant::now();
            let mut tracker = held_after_a_switch(start);
            inbox
                .send(TrackerMessage::DockHoverChanged)
                .expect("the receiver is alive");
            let mut watcher = None;
            let mut screen_changed = false;
            let mut launched = PendingLaunch::default();

            drain_inbox_until(
                Instant::now() + Duration::from_secs(5),
                &messages,
                &mut tracker,
                &mut watcher,
                &mut screen_changed,
                &mut launched,
            );
            let next = tracker.tick(Ok(vec![safari()]), FAR, start + Duration::from_millis(900));

            assert_eq!(next.overlay, Some(Visibility::Shown));
        }

        #[test]
        fn a_space_change_ends_the_wait_only_while_the_overlay_is_held() {
            let start = Instant::now();
            let mut shown = Tracker::new(PRIMARY_2X, Visibility::Shown);
            shown.tick(Ok(vec![safari()]), FAR, start);
            let mut held = held_after_a_switch(start);
            let delay = Duration::from_millis(50);

            for (case, tracker, ends_at_once) in [
                ("Dock shown: no cut", &mut shown, false),
                ("overlay held: read now", &mut held, true),
            ] {
                let (inbox, messages) = mpsc::channel();
                inbox
                    .send(TrackerMessage::SpaceChanged)
                    .expect("the receiver is alive");
                let mut watcher = None;
                let mut screen_changed = false;
                let mut launched = PendingLaunch::default();

                let waited = Instant::now();
                let woken = drain_inbox_until(
                    waited + delay,
                    &messages,
                    tracker,
                    &mut watcher,
                    &mut screen_changed,
                    &mut launched,
                );

                assert_eq!(
                    matches!(woken, Woken::SpaceChanged { .. }),
                    ends_at_once,
                    "{case}: {woken:?}"
                );
                if !ends_at_once {
                    assert!(waited.elapsed() >= delay, "{case}: the wait ran out");
                }
            }
            let next = held.tick(Ok(vec![safari()]), FAR, start + Duration::from_millis(900));
            assert_eq!(next.overlay, Some(Visibility::Shown), "the next read");
        }

        #[test]
        fn a_space_change_while_nothing_is_held_does_not_cancel_the_next_hold() {
            let start = Instant::now();
            let off_screen = RawDockChild {
                rect: Some(ScreenRect {
                    y: 990.0,
                    ..safari().rect.expect("a rect")
                }),
                ..safari()
            };
            let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
            tracker.tick(Ok(vec![off_screen]), FAR, start);
            let (inbox, messages) = mpsc::channel();
            inbox
                .send(TrackerMessage::SpaceChanged)
                .expect("the receiver is alive");
            let mut watcher = None;
            let mut screen_changed = false;
            let mut launched = PendingLaunch::default();

            let woken = drain_inbox_until(
                Instant::now() + Duration::from_millis(20),
                &messages,
                &mut tracker,
                &mut watcher,
                &mut screen_changed,
                &mut launched,
            );
            let back = tracker.tick(Ok(vec![safari()]), FAR, start + Duration::from_millis(400));

            assert_eq!(woken, Woken::Due, "not held: no wake");
            assert_eq!(back.overlay, None, "the switch back is held");
            assert!(tracker.awaits_switch_end());
        }

        #[test]
        fn an_app_launch_ends_the_wait_at_once_and_is_kept_for_the_next_tick() {
            let (inbox, messages) = mpsc::channel();
            inbox
                .send(TrackerMessage::AppLaunching)
                .expect("the receiver is alive");
            let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
            let mut watcher = None;
            let mut screen_changed = false;
            let mut launched = PendingLaunch::default();

            let started = Instant::now();
            let woken = drain_inbox_until(
                started + Duration::from_secs(5),
                &messages,
                &mut tracker,
                &mut watcher,
                &mut screen_changed,
                &mut launched,
            );

            let returned = Instant::now();

            assert!(matches!(woken, Woken::AppLaunch { at } if at >= started && at <= returned));
            let kept = launched.take();
            assert!(
                kept.is_some_and(|at| at >= started && at <= returned),
                "{kept:?}"
            );
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "returned after {:?}",
                started.elapsed()
            );
        }

        #[test]
        fn a_later_launch_in_the_same_wait_keeps_the_first_launch_time() {
            let (inbox, messages) = mpsc::channel();
            inbox
                .send(TrackerMessage::AppLaunching)
                .expect("the receiver is alive");
            let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
            let mut watcher = None;
            let mut screen_changed = false;
            let mut launched = PendingLaunch::default();
            let deadline = Instant::now() + Duration::from_secs(5);
            drain_inbox_until(
                deadline,
                &messages,
                &mut tracker,
                &mut watcher,
                &mut screen_changed,
                &mut launched,
            );
            std::thread::sleep(Duration::from_millis(1));
            let between = Instant::now();
            inbox
                .send(TrackerMessage::AppLaunching)
                .expect("the receiver is alive");

            drain_inbox_until(
                deadline,
                &messages,
                &mut tracker,
                &mut watcher,
                &mut screen_changed,
                &mut launched,
            );

            assert!(launched.take().is_some_and(|at| at < between));
        }

        #[test]
        fn a_new_watcher_gets_the_current_frame_on_the_next_tick() {
            let now = Instant::now();
            let (inbox, messages) = mpsc::channel();
            let mut tracker = Tracker::new(PRIMARY_2X, Visibility::Shown);
            let first = tracker.tick(Ok(vec![safari()]), None, now);
            let unchanged = tracker.tick(Ok(vec![safari()]), None, now);
            inbox
                .send(TrackerMessage::Watch(Channel::new(|_| Ok(()))))
                .expect("the receiver is alive");
            let mut watcher = None;
            let mut screen_changed = false;
            let mut launched = PendingLaunch::default();

            drain_inbox_until(
                Instant::now() + Duration::from_millis(5),
                &messages,
                &mut tracker,
                &mut watcher,
                &mut screen_changed,
                &mut launched,
            );
            let after_watch = tracker.tick(Ok(vec![safari()]), None, now);

            assert!(first.send.is_some() && unchanged.send.is_none(), "baseline");
            assert!(watcher.is_some(), "the watcher is stored");
            assert!(!screen_changed);
            assert_eq!(after_watch.send.map(|frame| frame.len()), Some(1));
        }
    }
}

fn primary_display(app: &AppHandle) -> Result<Display, Error> {
    let monitor = app.primary_monitor()?.ok_or(Error::NoMainDisplay)?;
    let origin = monitor.position();
    let size = monitor.size();
    Ok(Display {
        physical_x: origin.x,
        physical_y: origin.y,
        physical_width: size.width,
        physical_height: size.height,
        scale_factor: monitor.scale_factor(),
    })
}

fn build_overlay(app: &AppHandle, display: Display) -> Result<WebviewWindow, Error> {
    let frame = display.frame();
    let scale = display.scale_factor;
    let window =
        WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("overlay.html".into()))
            .title("DockMoss Overlay")
            .position(frame.x, frame.y)
            .inner_size(frame.width, frame.height)
            .transparent(true)
            .decorations(false)
            .shadow(false)
            .resizable(false)
            .focused(false)
            .focusable(false)
            .skip_taskbar(true)
            .visible_on_all_workspaces(true)
            .visible(false)
            .on_navigation(is_app_url)
            .build()?;
    window.set_ignore_cursor_events(true)?;
    // Any webview granted core:window:allow-close (Settings) may name another
    // window's label; quitting goes through app.exit, not CloseRequested.
    window.on_window_event(|event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            tracing::warn!("overlay close request refused");
        }
    });

    tracing::info!(
        x = frame.x,
        y = frame.y,
        width = frame.width,
        height = frame.height,
        scale,
        "overlay window created"
    );
    Ok(window)
}

pub(crate) fn show_settings_window(app: &AppHandle) {
    let window = if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        window
    } else {
        match build_settings_window(app) {
            Ok(window) => window,
            Err(error) => {
                tracing::warn!(%error, "settings window not created");
                return;
            }
        }
    };
    let shown = window
        .unminimize()
        .and_then(|()| window.show())
        .and_then(|()| window.set_focus());
    match shown {
        Ok(()) => tracing::debug!("settings window shown"),
        Err(error) => tracing::warn!(%error, "settings window not brought to the front"),
    }
}

fn build_settings_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let window =
        WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("settings.html".into()))
            .title("DockMoss Settings")
            .inner_size(440.0, settings_window_height(app))
            .resizable(false)
            .maximizable(false)
            .center()
            .on_navigation(is_app_url)
            .build()?;
    let app = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            return_focus(&app);
        }
    });
    Ok(window)
}

/// The primary display's work area is the one Settings opens centred on.
fn settings_window_height(app: &AppHandle) -> f64 {
    let work_area_height = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| f64::from(monitor.work_area().size.height) / monitor.scale_factor());
    geometry::settings_window_height(SETTINGS_HEIGHT, work_area_height)
}

/// The last app activated that is not this one or another copy of it, which
/// is where closing Settings hands focus back.
#[derive(Default)]
struct LastOtherApp(Mutex<Option<focus::ActiveApp>>);

fn track_last_other_app(app: &AppHandle, mtm: macos::MainThreadMarker) {
    let own_pid = macos::app::own_pid();
    let own_bundle_id = app.config().identifier.clone();
    let app = app.clone();
    let remember = move |active: focus::ActiveApp| {
        if focus::is_return_target(&active, own_pid, &own_bundle_id) {
            let last = app.state::<LastOtherApp>();
            *last.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(active);
        }
    };
    if let Some(front) = macos::app::front_app(mtm) {
        remember(front);
    }
    macos::workspace::on_app_activated(remember, mtm);
}

fn return_focus(app: &AppHandle) {
    let Some(mtm) = macos::MainThreadMarker::new() else {
        tracing::warn!("settings window closed off the main thread; focus not returned");
        return;
    };
    let target = app
        .state::<LastOtherApp>()
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let at_its_pid = target
        .as_ref()
        .and_then(|target| macos::app::app_at(target.pid, mtm));
    let decision = focus::on_settings_closed(
        macos::app::is_active(mtm),
        target.as_ref(),
        at_its_pid.as_ref(),
    );
    let activated = match decision {
        OnSettingsClosed::ReturnTo(pid) => macos::app::activate_app(pid, mtm),
        OnSettingsClosed::Leave | OnSettingsClosed::Hide => false,
    };
    let hidden = focus::hides(decision, activated);
    if hidden && let Err(error) = app.hide() {
        tracing::warn!(%error, "dockmoss not hidden after settings closed");
    }
    tracing::debug!(?decision, activated, hidden, "settings window closed");
}

fn is_app_url(url: &Url) -> bool {
    navigation::is_app_origin(url.scheme(), url.host_str(), cfg!(debug_assertions))
}
