//! The Dock posts `AXSelectedChildrenChanged` when the pointer enters, crosses
//! or leaves one of its items, and no move or resize notification for them:
//! `AXMoved`, `AXResized` and `AXLayoutChanged` all return
//! `NotificationUnsupported` (macOS 27, job 016). So the notification marks the
//! start of a magnification, not each of its frames, and the reads stay
//! polled. Registered on the application element, it covers every element of
//! the Dock, so a rebuilt item list needs no new registration.

use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr::{self, NonNull};
use std::time::Duration;

use objc2::rc::autoreleasepool;
use objc2_application_services::{AXError, AXObserver, AXUIElement};
use objc2_core_foundation::{
    CFRetained, CFRunLoop, CFRunLoopMode, CFRunLoopSource, CFString, kCFRunLoopDefaultMode,
};

const HOVER_CHANGED: &str = "AXSelectedChildrenChanged";
const RECHECK: Duration = Duration::from_secs(2);

pub fn spawn(
    trusted: impl Fn() -> bool + Send + 'static,
    on_hover: impl Fn() + Send + 'static,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("dock-observer".into())
        .spawn(move || observe(&trusted, &on_hover))?;
    Ok(())
}

fn observe(trusted: &dyn Fn() -> bool, on_hover: &dyn Fn()) {
    let Some(mode) = default_mode() else {
        tracing::warn!("no default run loop mode; dock hover changes not observed");
        return;
    };
    loop {
        if trusted()
            && let Some(pid) = dock_pid()
        {
            match Registration::new(pid, &on_hover, mode) {
                Ok(registration) => {
                    tracing::debug!(pid, "dock hover observer registered");
                    while trusted() && dock_pid() == Some(pid) {
                        CFRunLoop::run_in_mode(Some(mode), RECHECK.as_secs_f64(), false);
                    }
                    drop(registration);
                    tracing::debug!(pid, "dock hover observer released");
                    continue;
                }
                Err(code) => tracing::debug!(pid, code, "dock hover observer not registered"),
            }
        }
        std::thread::sleep(RECHECK);
    }
}

fn dock_pid() -> Option<i32> {
    autoreleasepool(super::dock::system_dock_pid)
}

#[expect(unsafe_code, reason = "kCFRunLoopDefaultMode is a C static")]
fn default_mode() -> Option<&'static CFRunLoopMode> {
    // SAFETY: kCFRunLoopDefaultMode is an immutable CFString constant that
    // CoreFoundation initialises before any Rust code runs.
    unsafe { kCFRunLoopDefaultMode }
}

struct Registration<'a> {
    run_loop: CFRetained<CFRunLoop>,
    source: CFRetained<CFRunLoopSource>,
    mode: &'static CFRunLoopMode,
    _observer: CFRetained<AXObserver>,
    _callback: PhantomData<&'a &'a dyn Fn()>,
}

impl<'a> Registration<'a> {
    #[expect(
        unsafe_code,
        reason = "AXObserverCreate returns through an out-pointer and takes a C callback with a raw refcon"
    )]
    fn new(
        pid: i32,
        callback: &'a &'a dyn Fn(),
        mode: &'static CFRunLoopMode,
    ) -> Result<Self, i32> {
        let mut raw: *mut AXObserver = ptr::null_mut();
        // SAFETY: `hover_changed` has the AXObserverCallback signature and
        // `raw` is a live, writable out-pointer for the duration of the call.
        let code = unsafe { AXObserver::create(pid, Some(hover_changed), NonNull::from(&mut raw)) };
        if code != AXError::Success {
            return Err(code.0);
        }
        let raw = NonNull::new(raw).ok_or(AXError::Failure.0)?;
        // SAFETY: AXObserverCreate succeeded, and by the Create rule the
        // caller owns the +1 observer it stored, so `CFRetained` takes it over.
        let observer = unsafe { CFRetained::from_raw(raw) };
        let dock = super::dock::application(pid);
        let name = CFString::from_static_str(HOVER_CHANGED);
        let refcon = ptr::from_ref(callback).cast_mut().cast::<c_void>();
        // SAFETY: `refcon` points to `callback`, which outlives this
        // registration ('a), and the source is removed from the run loop
        // before the registration is dropped, so no callback reads it later.
        let code = unsafe { observer.add_notification(&dock, &name, refcon) };
        if code != AXError::Success {
            return Err(code.0);
        }
        // SAFETY: `observer` is a live AXObserver; the binding retains the
        // source it returns.
        let source = unsafe { observer.run_loop_source() };
        let run_loop = CFRunLoop::current().ok_or(AXError::Failure.0)?;
        run_loop.add_source(Some(&source), Some(mode));
        Ok(Self {
            run_loop,
            source,
            mode,
            _observer: observer,
            _callback: PhantomData,
        })
    }
}

impl Drop for Registration<'_> {
    fn drop(&mut self) {
        self.run_loop
            .remove_source(Some(&self.source), Some(self.mode));
    }
}

#[expect(
    unsafe_code,
    reason = "an AXObserver callback is a C function that receives its context as a raw pointer"
)]
unsafe extern "C-unwind" fn hover_changed(
    _observer: NonNull<AXObserver>,
    _element: NonNull<AXUIElement>,
    _notification: NonNull<CFString>,
    refcon: *mut c_void,
) {
    // SAFETY: `refcon` is the `&&dyn Fn()` that `Registration::new` passed.
    // The callback runs only inside `CFRunLoop::run_in_mode` on the observer
    // thread while the registration, and so the callback, is alive.
    let on_hover = unsafe { &*refcon.cast::<&dyn Fn()>() };
    on_hover();
}
