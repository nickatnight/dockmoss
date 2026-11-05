use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(i32);

pub fn active_other_app(_mtm: MainThreadMarker) -> Option<Pid> {
    let active = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = active.processIdentifier();
    (pid != NSRunningApplication::currentApplication().processIdentifier()).then_some(Pid(pid))
}

/// Whether the app in front is another copy of this one (a second launch that
/// has not exited yet) or no app at all, as while one hands activation on.
pub fn another_copy_or_nothing_in_front(own_bundle_id: &str, _mtm: MainThreadMarker) -> bool {
    let Some(front) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
        return true;
    };
    front.processIdentifier() != NSRunningApplication::currentApplication().processIdentifier()
        && front
            .bundleIdentifier()
            .is_some_and(|id| id.to_string() == own_bundle_id)
}

pub fn activate_app(Pid(pid): Pid, _mtm: MainThreadMarker) -> bool {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .is_some_and(|app| app.activateWithOptions(NSApplicationActivationOptions::empty()))
}

pub fn is_active(_mtm: MainThreadMarker) -> bool {
    NSRunningApplication::currentApplication().isActive()
}
