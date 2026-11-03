use objc2_application_services::AXError;

use super::{DockError, Reading, classify};

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
