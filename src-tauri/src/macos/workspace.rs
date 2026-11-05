use std::ptr::NonNull;
use std::sync::Arc;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey,
    NSWorkspaceDidActivateApplicationNotification, NSWorkspaceDidLaunchApplicationNotification,
    NSWorkspaceDidTerminateApplicationNotification,
};
use objc2_foundation::{NSNotification, NSOperationQueue, NSString, NSURL};

use crate::focus::ActiveApp;
use crate::usage::{AppEvent, AppEventKind, RunningApp};

const ACCESSIBILITY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("{0:?} is not a valid URL")]
    InvalidUrl(String),
    #[error("macOS could not open {0:?}")]
    NotOpened(String),
}

pub fn open_accessibility_settings(mtm: MainThreadMarker) -> Result<(), WorkspaceError> {
    open_url(ACCESSIBILITY_SETTINGS_URL, mtm)
}

fn open_url(url: &str, _mtm: MainThreadMarker) -> Result<(), WorkspaceError> {
    let ns_url = NSURL::URLWithString(&NSString::from_str(url))
        .ok_or_else(|| WorkspaceError::InvalidUrl(url.to_owned()))?;
    if NSWorkspace::sharedWorkspace().openURL(&ns_url) {
        Ok(())
    } else {
        Err(WorkspaceError::NotOpened(url.to_owned()))
    }
}

pub fn running_apps(_mtm: MainThreadMarker) -> Vec<RunningApp> {
    NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .map(|app| running_app(&app))
        .collect()
}

#[expect(
    unsafe_code,
    reason = "the notification names are AppKit extern statics"
)]
pub fn on_app_events(on_event: impl Fn(AppEvent) + Send + Sync + 'static, mtm: MainThreadMarker) {
    let on_event = Arc::new(on_event);
    // SAFETY: the names are immutable NSString constants that AppKit
    // initialises before any Rust code runs.
    let names = unsafe {
        [
            (
                NSWorkspaceDidLaunchApplicationNotification,
                AppEventKind::Launched,
            ),
            (
                NSWorkspaceDidActivateApplicationNotification,
                AppEventKind::Activated,
            ),
            (
                NSWorkspaceDidTerminateApplicationNotification,
                AppEventKind::Terminated,
            ),
        ]
    };
    for (name, kind) in names {
        let on_event = Arc::clone(&on_event);
        observe(
            name,
            move |app| {
                on_event(AppEvent {
                    kind,
                    app: running_app(app),
                });
            },
            mtm,
        );
    }
}

#[expect(
    unsafe_code,
    reason = "the notification name is an AppKit extern static"
)]
pub fn on_app_activated(
    on_activated: impl Fn(ActiveApp) + Send + Sync + 'static,
    mtm: MainThreadMarker,
) {
    // SAFETY: the name is an immutable NSString constant that AppKit
    // initialises before any Rust code runs.
    let name = unsafe { NSWorkspaceDidActivateApplicationNotification };
    observe(
        name,
        move |app| on_activated(super::app::active_app(app)),
        mtm,
    );
}

/// Calls `on_app` on the main thread with the application of each workspace
/// notification named `name`.
#[expect(
    unsafe_code,
    reason = "the observer API takes a block, and the block dereferences the delivered notification"
)]
fn observe(
    name: &NSString,
    on_app: impl Fn(&NSRunningApplication) + Send + Sync + 'static,
    _mtm: MainThreadMarker,
) {
    // Workspace notifications are posted only to NSWorkspace's own center,
    // never to NSNotificationCenter.defaultCenter.
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let queue = NSOperationQueue::mainQueue();
    let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
        // SAFETY: Foundation calls the block with the notification being
        // delivered, which stays alive for the duration of the call.
        let notification = unsafe { notification.as_ref() };
        if let Some(app) = notified_app(notification) {
            on_app(&app);
        } else {
            tracing::debug!(name = %notification.name(), "workspace notification without an application");
        }
    });
    // SAFETY: `obj` is None (any sender). The block is sendable, as the
    // binding requires: it captures only `on_app`, which is
    // `Send + Sync + 'static`, so Foundation may copy, call or release it on
    // any thread. The main queue runs it on the main thread, where `_mtm`
    // shows the registration also happens.
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(name), None, Some(&queue), &block)
    };
    drop(observer);
}

#[expect(unsafe_code, reason = "the user-info key is an AppKit extern static")]
fn notified_app(notification: &NSNotification) -> Option<Retained<NSRunningApplication>> {
    let info = notification.userInfo()?;
    // SAFETY: the key is an immutable NSString constant that AppKit
    // initialises before any Rust code runs.
    let key = unsafe { NSWorkspaceApplicationKey };
    info.objectForKey(key.as_ref())?
        .downcast::<NSRunningApplication>()
        .ok()
}

fn running_app(app: &NSRunningApplication) -> RunningApp {
    RunningApp {
        bundle_id: app.bundleIdentifier().map(|id| id.to_string()),
        can_have_dock_icon: app.activationPolicy() == NSApplicationActivationPolicy::Regular,
    }
}
