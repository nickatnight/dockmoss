use std::ptr::NonNull;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior, NSWindowLevel};
use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey};
use tauri::WebviewWindow;

#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    #[error("overlay has no native window")]
    NativeWindow(#[source] tauri::Error),
    #[error("overlay's native window pointer is null")]
    NullWindow,
}

#[derive(Debug, Clone, Copy)]
pub struct PinnedLevels {
    pub dock: NSWindowLevel,
    pub overlay: NSWindowLevel,
}

pub fn pin_above_dock(
    window: &WebviewWindow,
    _mtm: MainThreadMarker,
) -> Result<PinnedLevels, OverlayError> {
    let raw = window.ns_window().map_err(OverlayError::NativeWindow)?;
    let ptr = NonNull::new(raw.cast::<NSWindow>()).ok_or(OverlayError::NullWindow)?;
    let ns_window = as_ns_window(ptr);

    let dock = CGWindowLevelForKey(CGWindowLevelKey::DockWindowLevelKey) as NSWindowLevel;
    ns_window.setLevel(dock + 1);
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );

    Ok(PinnedLevels {
        dock,
        overlay: ns_window.level(),
    })
}

#[expect(
    unsafe_code,
    reason = "Tauri hands out the NSWindow only as a raw pointer"
)]
fn as_ns_window<'a>(ptr: NonNull<NSWindow>) -> &'a NSWindow {
    // SAFETY: `ptr` is non-null and comes from `WebviewWindow::ns_window()`,
    // which returns the live `NSWindow` that tao retains for as long as the
    // Tauri window exists. The caller holds a `&WebviewWindow`, so the window
    // outlives the borrow, and the caller is on the main thread.
    unsafe { ptr.as_ref() }
}
