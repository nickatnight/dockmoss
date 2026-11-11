use objc2::rc::autoreleasepool;
use objc2_foundation::{NSBundle, NSError};
use objc2_service_management::{SMAppService, SMAppServiceStatus};

use crate::login_item::{LoginItemStatus, is_app_bundle};

#[derive(Debug, thiserror::Error)]
pub enum LoginItemError {
    #[error("this copy of DockMoss is not an app bundle")]
    NotAnAppBundle,
    #[error("ServiceManagement refused the change ({0})")]
    Refused(RedactedNsError),
}

/// An `NSError` cut down to its domain and code. Its description can name a
/// path, which holds the user's name, so it is never kept or logged.
#[derive(Debug, thiserror::Error)]
#[error("{domain} error {code}")]
pub struct RedactedNsError {
    domain: String,
    code: isize,
}

impl RedactedNsError {
    fn new(error: &NSError) -> Self {
        Self {
            domain: error.domain().to_string(),
            code: error.code(),
        }
    }
}

/// The status macOS reports for this app's login item. Runs in its own
/// autorelease pool: it is called on a short-lived blocking thread that has
/// none, so the objects `ServiceManagement` autoreleases are freed when it
/// returns instead of when the thread is reaped.
pub fn status() -> LoginItemStatus {
    autoreleasepool(|_| status_in_pool())
}

#[expect(
    unsafe_code,
    reason = "the ServiceManagement bindings mark every SMAppService method unsafe"
)]
fn status_in_pool() -> LoginItemStatus {
    // SAFETY: `mainAppService` takes no arguments and returns a retained
    // object for this app's own bundle.
    let service = unsafe { SMAppService::mainAppService() };
    // SAFETY: `service` is a valid, retained `SMAppService` that lives for
    // the call; `status` takes no arguments and only reads it.
    let status = unsafe { service.status() };
    match status {
        SMAppServiceStatus::Enabled => LoginItemStatus::Enabled,
        SMAppServiceStatus::RequiresApproval => LoginItemStatus::RequiresApproval,
        SMAppServiceStatus::NotRegistered => LoginItemStatus::NotRegistered,
        _ => LoginItemStatus::NotFound,
    }
}

/// Registers or unregisters this app as a login item, in its own autorelease
/// pool, as `status` is.
pub fn set(open_at_login: bool) -> Result<(), LoginItemError> {
    autoreleasepool(|_| set_in_pool(open_at_login))
}

#[expect(
    unsafe_code,
    reason = "the ServiceManagement bindings mark every SMAppService method unsafe"
)]
fn set_in_pool(open_at_login: bool) -> Result<(), LoginItemError> {
    let bundle_path = NSBundle::mainBundle().bundlePath().to_string();
    if !is_app_bundle(std::path::Path::new(&bundle_path)) {
        return Err(LoginItemError::NotAnAppBundle);
    }
    let service = {
        // SAFETY: `mainAppService` takes no arguments and returns a retained
        // object for this app's own bundle.
        unsafe { SMAppService::mainAppService() }
    };
    let result = if open_at_login {
        // SAFETY: `service` is a valid, retained `SMAppService` that lives
        // for the call; the method takes no other arguments.
        unsafe { service.registerAndReturnError() }
    } else {
        // SAFETY: as above.
        unsafe { service.unregisterAndReturnError() }
    };
    result.map_err(|error| LoginItemError::Refused(RedactedNsError::new(&error)))
}
