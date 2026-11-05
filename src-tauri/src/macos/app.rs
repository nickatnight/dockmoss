use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

pub fn active_other_app(_mtm: MainThreadMarker) -> Option<i32> {
    let active = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = active.processIdentifier();
    (pid != NSRunningApplication::currentApplication().processIdentifier()).then_some(pid)
}

pub fn activate_app(pid: i32, _mtm: MainThreadMarker) -> bool {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .is_some_and(|app| app.activateWithOptions(NSApplicationActivationOptions::empty()))
}
