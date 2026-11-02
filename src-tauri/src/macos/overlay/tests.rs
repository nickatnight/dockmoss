use super::{OverlayError, ensure_raised};

#[test]
fn overlay_one_level_above_the_dock_is_raised() {
    assert!(ensure_raised(20, 21).is_ok());
}

#[test]
fn overlay_level_equal_to_the_dock_is_not_raised() {
    let error = ensure_raised(20, 20).expect_err("equal levels are not raised");

    assert!(matches!(
        error,
        OverlayError::LevelNotRaised {
            dock: 20,
            overlay: 20
        }
    ));
}

#[test]
fn overlay_level_below_the_dock_is_not_raised() {
    let error = ensure_raised(20, 3).expect_err("a lower level is not raised");

    assert!(matches!(
        error,
        OverlayError::LevelNotRaised {
            dock: 20,
            overlay: 3
        }
    ));
}
