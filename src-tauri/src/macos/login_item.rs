use objc2_foundation::{NSBundle, NSError};
use objc2_service_management::{SMAppService, SMAppServiceStatus};

use crate::login_item::LoginItemStatus;

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

#[expect(
    unsafe_code,
    reason = "the ServiceManagement bindings mark every SMAppService method unsafe"
)]
pub fn status() -> LoginItemStatus {
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

#[expect(
    unsafe_code,
    reason = "the ServiceManagement bindings mark every SMAppService method unsafe"
)]
pub fn set(open_at_login: bool) -> Result<(), LoginItemError> {
    if !in_app_bundle() {
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

fn in_app_bundle() -> bool {
    let path = NSBundle::mainBundle().bundlePath().to_string();
    std::path::Path::new(&path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}
