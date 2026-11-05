use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(i32);

pub fn active_other_app(_mtm: MainThreadMarker) -> Option<Pid> {
    let active = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = active.processIdentifier();
    (pid != NSRunningApplication::currentApplication().processIdentifier()).then_some(Pid(pid))
}

pub fn activate_app(Pid(pid): Pid, _mtm: MainThreadMarker) -> bool {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .is_some_and(|app| app.activateWithOptions(NSApplicationActivationOptions::empty()))
}
