use std::ptr::NonNull;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2_app_kit::NSApplicationDidChangeScreenParametersNotification;
use objc2_foundation::{NSNotification, NSNotificationCenter, NSOperationQueue};

/// Calls `on_change` on the main thread whenever a display's resolution,
/// arrangement or the main display changes. The observer stays registered
/// for the life of the process: the notification center holds the returned
/// token until it is removed, and it never is.
#[expect(
    unsafe_code,
    reason = "the notification name is an AppKit extern static and the observer API takes a block"
)]
pub fn on_screen_parameters_change(
    on_change: impl Fn() + Send + Sync + 'static,
    _mtm: MainThreadMarker,
) {
    let block = RcBlock::new(move |_notification: NonNull<NSNotification>| on_change());
    // SAFETY: the name is an immutable NSString constant that AppKit
    // initialises before any Rust code runs.
    let name = unsafe { NSApplicationDidChangeScreenParametersNotification };
    let queue = NSOperationQueue::mainQueue();
    // SAFETY: `obj` is None (any sender). The block is sendable, as the
    // binding requires: it only calls `on_change`, which is `Send + Sync +
    // 'static`, so Foundation may copy, call or release it on any thread. The
    // main queue runs it on the main thread, where `_mtm` shows the
    // registration also happens.
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(name),
            None,
            Some(&queue),
            &block,
        )
    };
    drop(observer);
}
