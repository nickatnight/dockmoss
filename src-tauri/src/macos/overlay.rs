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

    Ok(PinnedLevels {
        dock,
        overlay: ns_window.level(),
    })
}

#[expect(
    unsafe_code,
    reason = "Tauri hands out the NSWindow only as a raw pointer"
)]
fn as_ns_window(window: &WebviewWindow, _mtm: MainThreadMarker) -> Result<&NSWindow, OverlayError> {
    let raw = window.ns_window().map_err(OverlayError::NativeWindow)?;
    let ptr = NonNull::new(raw.cast::<NSWindow>()).ok_or(OverlayError::NullWindow)?;
    // SAFETY: `ns_window()` returns the NSWindow that tao retains for as long
    // as `window` exists, and the returned borrow cannot outlive `window`.
    let ns_window = unsafe { ptr.as_ref() };
    Ok(ns_window)
}
