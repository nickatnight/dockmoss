use objc2_application_services::AXError;

use std::path::PathBuf;

use super::{Reading, classify, keep_child};
use crate::dock::{DockError, RawDockChild};

#[test]
fn ax_result_codes_map_to_a_reading_or_a_dock_error() {
    let cases = [
        ("success", AXError::Success, Ok(Reading::Present)),
        ("no value", AXError::NoValue, Ok(Reading::Missing)),
        (
            "attribute unsupported",
            AXError::AttributeUnsupported,
            Ok(Reading::Missing),
        ),
        (
            "api disabled",
            AXError::APIDisabled,
            Err(DockError::ApiDisabled),
        ),
        (
            "cannot complete",
            AXError::CannotComplete,
            Err(DockError::CannotComplete),
        ),
        (
            "invalid element",
            AXError::InvalidUIElement,
            Err(DockError::Ax {
                attribute: "AXSubrole",
                code: -25202,
            }),
        ),
        (
            "failure",
            AXError::Failure,
            Err(DockError::Ax {
                attribute: "AXSubrole",
                code: -25200,
            }),
        ),
    ];

    for (case, code, expected) in cases {
        assert_eq!(classify(code, "AXSubrole"), expected, "{case}");
    }
}

#[test]
fn only_dock_wide_errors_fail_the_whole_read() {
    let child = RawDockChild {
        subrole: Some("AXApplicationDockItem".into()),
        rect: None,
        app_path: Some(PathBuf::from("/Applications/Safari.app")),
        bundle_id: Some("com.apple.Safari".into()),
    };
    let stale = DockError::Ax {
        attribute: "AXSubrole, AXPosition, AXSize and AXURL",
        code: -25202,
    };
    let cases = [
        ("child read", Ok(child.clone()), Ok(Some(child))),
        (
            "api disabled",
            Err(DockError::ApiDisabled),
            Err(DockError::ApiDisabled),
        ),
        (
            "cannot complete",
            Err(DockError::CannotComplete),
            Err(DockError::CannotComplete),
        ),
        ("invalid element", Err(stale), Ok(None)),
        (
            "failure",
            Err(DockError::Ax {
                attribute: "AXURL",
                code: -25200,
            }),
            Ok(None),
        ),
    ];

    for (case, read, expected) in cases {
        assert_eq!(keep_child(read), expected, "{case}");
    }
}
