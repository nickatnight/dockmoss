use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior, NSWindowLevel};
use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey};
use tauri::WebviewWindow;

#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    #[error("overlay has no native window")]
    NativeWindow(#[source] tauri::Error),
    #[error("overlay's native window pointer is null")]
    NullWindow,
    #[error("overlay window level {overlay} is not above the Dock's level {dock}")]
    LevelNotRaised {
        dock: NSWindowLevel,
        overlay: NSWindowLevel,
    },
}

#[derive(Debug)]
pub struct PinnedLevels {
    pub dock: NSWindowLevel,
    pub overlay: NSWindowLevel,
}

pub fn pin_above_dock(
    window: &WebviewWindow,
    mtm: MainThreadMarker,
) -> Result<PinnedLevels, OverlayError> {
    let ns_window = as_ns_window(window, mtm)?;

    let dock = CGWindowLevelForKey(CGWindowLevelKey::DockWindowLevelKey) as NSWindowLevel;
    ns_window.setLevel(dock + 1);
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );

    let overlay = ns_window.level();
    ensure_raised(dock, overlay)?;
    Ok(PinnedLevels { dock, overlay })
}

fn ensure_raised(dock: NSWindowLevel, overlay: NSWindowLevel) -> Result<(), OverlayError> {
    if overlay > dock {
        Ok(())
    } else {
        Err(OverlayError::LevelNotRaised { dock, overlay })
    }
}

#[expect(
    unsafe_code,
    reason = "Tauri hands out the NSWindow only as a raw pointer"
)]
fn as_ns_window(
    window: &WebviewWindow,
    _mtm: MainThreadMarker,
) -> Result<Retained<NSWindow>, OverlayError> {
    let raw = window.ns_window().map_err(OverlayError::NativeWindow)?;
    // SAFETY: `raw` is tao's live NSWindow or null. tao frees the window only
    // from the event loop, which cannot run during this synchronous call on
    // the main thread, so the pointer is valid to retain here. The returned
    // `Retained` then owns its own reference.
    let ns_window = unsafe { Retained::retain(raw.cast::<NSWindow>()) };
    ns_window.ok_or(OverlayError::NullWindow)
}

#[cfg(test)]
mod tests;
