use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSRunningApplication, NSWorkspace,
};

/// Activates this app, so the window it shows next comes up in front of
/// the app the user was in and takes the keyboard. Showing a window does not
/// activate an Accessory app on its own.
pub fn activate(mtm: MainThreadMarker) {
    NSApplication::sharedApplication(mtm).activate();
}

/// The process ID of the active app, when that app is not this one.
pub fn active_other_app(_mtm: MainThreadMarker) -> Option<i32> {
    let active = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = active.processIdentifier();
    (pid != NSRunningApplication::currentApplication().processIdentifier()).then_some(pid)
}

/// Activates the app with process ID `pid`. False when it has quit or macOS
/// refused.
pub fn activate_app(pid: i32, _mtm: MainThreadMarker) -> bool {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .is_some_and(|app| app.activateWithOptions(NSApplicationActivationOptions::empty()))
}
