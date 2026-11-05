use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

use crate::focus::{ActiveApp, Pid};

pub fn own_pid() -> Pid {
    Pid(NSRunningApplication::currentApplication().processIdentifier())
}

pub fn front_app(_mtm: MainThreadMarker) -> Option<ActiveApp> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| active_app(&app))
}

pub(super) fn active_app(app: &NSRunningApplication) -> ActiveApp {
    ActiveApp {
        pid: Pid(app.processIdentifier()),
        bundle_id: app.bundleIdentifier().map(|id| id.to_string()),
    }
}

pub fn activate_app(Pid(pid): Pid, _mtm: MainThreadMarker) -> bool {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .is_some_and(|app| app.activateWithOptions(NSApplicationActivationOptions::empty()))
}

pub fn is_active(_mtm: MainThreadMarker) -> bool {
    NSRunningApplication::currentApplication().isActive()
}
