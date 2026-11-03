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
pub fn on_screen_parameters_change(on_change: impl Fn() + 'static, _mtm: MainThreadMarker) {
    let block = RcBlock::new(move |_notification: NonNull<NSNotification>| on_change());
    // SAFETY: the name is an immutable NSString constant that AppKit
    // initialises before any Rust code runs.
    let name = unsafe { NSApplicationDidChangeScreenParametersNotification };
    let queue = NSOperationQueue::mainQueue();
    // SAFETY: `obj` is None (any sender), the main queue runs the block on
    // the main thread, where `_mtm` shows this registration also happens, and
    // the block only runs `on_change`, which needs no other thread.
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
