use std::cell::Cell;
use std::path::PathBuf;

use super::{DockError, DockItem, RawDockChild, ReadGate, app_items};
use crate::bundle_id::BundleId;
use crate::geometry::{Display, LogicalRect, ScreenRect};
use crate::trust::Trust;

const PRIMARY_2X: Display = Display {
    physical_x: 0,
    physical_y: 0,
    physical_width: 3024,
    physical_height: 1964,
    scale_factor: 2.0,
};

const ICON: ScreenRect = ScreenRect {
    x: 700.0,
    y: 920.0,
    width: 56.0,
    height: 56.0,
};

fn safari() -> RawDockChild {
    RawDockChild {
        subrole: Some("AXApplicationDockItem".into()),
        rect: Some(ICON),
        app_path: Some(PathBuf::from("/Applications/Safari.app")),
        bundle_id: Some("com.apple.Safari".into()),
    }
}

#[test]
fn application_item_becomes_a_dock_item_in_overlay_pixels() {
    let secondary = Display {
        physical_x: -1920,
        physical_y: -200,
        physical_width: 1920,
        physical_height: 1080,
        scale_factor: 1.0,
    };
    let child = RawDockChild {
        rect: Some(ScreenRect {
            x: -1000.0,
            y: 820.0,
            width: 64.0,
            height: 64.0,
        }),
        ..safari()
    };

    let items = app_items(vec![child], secondary);

    assert_eq!(
        items,
        vec![DockItem {
            bundle_id: BundleId::new("com.apple.Safari").expect("non-empty bundle id"),
            app_path: PathBuf::from("/Applications/Safari.app"),
            rect: LogicalRect {
                x: 920.0,
                y: 1020.0,
                width: 64.0,
                height: 64.0,
            },
        }]
    );
}

#[test]
fn only_application_items_are_kept() {
    let cases = [
        ("application", Some("AXApplicationDockItem"), 1),
        ("folder", Some("AXFolderDockItem"), 0),
        ("trash", Some("AXTrashDockItem"), 0),
        ("separator", Some("AXSeparatorDockItem"), 0),
        ("minimized window", Some("AXMinimizedWindowDockItem"), 0),
        ("document", Some("AXDocumentDockItem"), 0),
        ("url", Some("AXURLDockItem"), 0),
        ("unknown subrole", Some("AXSomethingNewDockItem"), 0),
        ("no subrole", None, 0),
    ];

    for (case, subrole, kept) in cases {
        let child = RawDockChild {
            subrole: subrole.map(str::to_owned),
            ..safari()
        };

        assert_eq!(app_items(vec![child], PRIMARY_2X).len(), kept, "{case}");
    }
}

#[test]
fn application_items_missing_data_are_dropped() {
    let cases = [
        (
            "no URL",
            RawDockChild {
                app_path: None,
                ..safari()
            },
        ),
        (
            "no bundle id",
            RawDockChild {
                bundle_id: None,
                ..safari()
            },
        ),
        (
            "empty bundle id",
            RawDockChild {
                bundle_id: Some(String::new()),
                ..safari()
            },
        ),
        (
            "no position or size",
            RawDockChild {
                rect: None,
                ..safari()
            },
        ),
        (
            "below the display, as an auto-hidden Dock",
            RawDockChild {
                rect: Some(ScreenRect { y: 990.0, ..ICON }),
                ..safari()
            },
        ),
    ];

    for (case, child) in cases {
        assert_eq!(app_items(vec![child], PRIMARY_2X), vec![], "{case}");
    }
}

#[test]
fn kept_items_stay_in_dock_order() {
    let finder = RawDockChild {
        app_path: Some(PathBuf::from("/System/Library/CoreServices/Finder.app")),
        bundle_id: Some("com.apple.finder".into()),
        rect: Some(ScreenRect { x: 640.0, ..ICON }),
        ..safari()
    };
    let trash = RawDockChild {
        subrole: Some("AXTrashDockItem".into()),
        ..safari()
    };

    let ids: Vec<BundleId> = app_items(vec![finder, trash, safari()], PRIMARY_2X)
        .into_iter()
        .map(|item| item.bundle_id)
        .collect();

    assert_eq!(
        ids,
        [
            BundleId::new("com.apple.finder").expect("non-empty bundle id"),
            BundleId::new("com.apple.Safari").expect("non-empty bundle id"),
        ]
    );
}

/// The app path can hold the user's name (`~/Applications`), and the
/// webview only draws, so it gets the bundle ID and the rect and no more.
#[test]
fn dock_item_serializes_only_its_bundle_id_and_rect_in_camel_case() {
    let item = DockItem {
        bundle_id: BundleId::new("com.apple.Safari").expect("non-empty bundle id"),
        app_path: PathBuf::from("/Users/someone/Applications/Safari.app"),
        rect: LogicalRect {
            x: 700.5,
            y: 920.0,
            width: 56.0,
            height: 56.0,
        },
    };

    let json = serde_json::to_value(&item).expect("DockItem serializes to JSON");

    assert_eq!(
        json,
        serde_json::json!({
            "bundleId": "com.apple.Safari",
            "rect": { "x": 700.5, "y": 920.0, "width": 56.0, "height": 56.0 }
        })
    );
}

#[test]
fn two_items_with_one_bundle_id_are_both_kept_in_order() {
    let left = RawDockChild {
        app_path: Some(PathBuf::from("/Applications/Visual Studio Code.app")),
        bundle_id: Some("com.microsoft.VSCode".into()),
        rect: Some(ScreenRect { x: 640.0, ..ICON }),
        ..safari()
    };
    let right = RawDockChild {
        rect: Some(ScreenRect { x: 760.0, ..ICON }),
        ..left.clone()
    };

    let xs: Vec<(BundleId, f64)> = app_items(vec![left, right], PRIMARY_2X)
        .into_iter()
        .map(|item| (item.bundle_id, item.rect.x))
        .collect();

    let vscode = BundleId::new("com.microsoft.VSCode").expect("non-empty bundle id");
    assert_eq!(xs, [(vscode.clone(), 640.0), (vscode, 760.0)]);
}

/// One gated tick: returns the result, whether the AX read ran and whether
/// trust was re-checked.
fn gated(
    gate: &mut ReadGate,
    trust: Trust,
    recheck: Trust,
    outcome: Result<Vec<RawDockChild>, DockError>,
) -> (Result<Vec<RawDockChild>, DockError>, bool, bool) {
    let read = Cell::new(false);
    let rechecked = Cell::new(false);
    let result = gate.read(
        trust,
        || {
            rechecked.set(true);
            recheck
        },
        || {
            read.set(true);
            outcome
        },
    );
    (result, read.get(), rechecked.get())
}

#[test]
fn untrusted_read_returns_api_disabled_without_reading() {
    let mut gate = ReadGate::default();

    let (result, read, rechecked) = gated(
        &mut gate,
        Trust::Untrusted,
        Trust::Trusted,
        Ok(vec![safari()]),
    );

    assert_eq!(result, Err(DockError::ApiDisabled));
    assert!(!read, "the AX read ran while untrusted");
    assert!(!rechecked, "trust was re-checked while untrusted");
}

#[test]
fn trusted_read_passes_the_result_through() {
    let cases = [
        ("children", Ok(vec![safari()])),
        ("error", Err(DockError::CannotComplete)),
    ];

    for (case, outcome) in cases {
        let mut gate = ReadGate::default();

        let (result, read, rechecked) =
            gated(&mut gate, Trust::Trusted, Trust::Trusted, outcome.clone());

        assert_eq!(result, outcome, "{case}");
        assert!(read, "{case}: the AX read did not run");
        assert!(!rechecked, "{case}: trust was re-checked on a normal read");
    }
}

#[test]
fn trust_revoked_while_running_stops_reads_until_trust_returns() {
    let mut gate = ReadGate::default();
    let steps = [
        (
            "read finds access revoked",
            Trust::Trusted,
            Trust::Untrusted,
            Err(DockError::ApiDisabled),
            true,
            false,
        ),
        (
            "state still says trusted; re-check says no",
            Trust::Trusted,
            Trust::Untrusted,
            Ok(vec![safari()]),
            false,
            true,
        ),
        (
            "re-check says no again",
            Trust::Trusted,
            Trust::Untrusted,
            Ok(vec![safari()]),
            false,
            true,
        ),
        (
            "re-check says trusted: read again",
            Trust::Trusted,
            Trust::Trusted,
            Ok(vec![safari()]),
            true,
            true,
        ),
        (
            "normal read, no re-check",
            Trust::Trusted,
            Trust::Trusted,
            Ok(vec![safari()]),
            true,
            false,
        ),
    ];

    for (case, trust, recheck, outcome, expect_read, expect_recheck) in steps {
        let (_, read, rechecked) = gated(&mut gate, trust, recheck, outcome);

        assert_eq!((read, rechecked), (expect_read, expect_recheck), "{case}");
    }
}

#[test]
fn untrusted_state_clears_the_revoked_latch() {
    let mut gate = ReadGate::default();
    let (revoked, ..) = gated(
        &mut gate,
        Trust::Trusted,
        Trust::Untrusted,
        Err(DockError::ApiDisabled),
    );
    let (untrusted, ..) = gated(
        &mut gate,
        Trust::Untrusted,
        Trust::Untrusted,
        Ok(vec![safari()]),
    );
    assert_eq!(
        (revoked, untrusted),
        (Err(DockError::ApiDisabled), Err(DockError::ApiDisabled))
    );

    let (result, read, rechecked) = gated(
        &mut gate,
        Trust::Trusted,
        Trust::Untrusted,
        Ok(vec![safari()]),
    );

    assert_eq!(result, Ok(vec![safari()]));
    assert!(read, "the watcher saw trust return, so the read runs");
    assert!(!rechecked);
}

#[test]
fn application_item_with_a_non_utf8_path_is_dropped() {
    use std::os::unix::ffi::OsStrExt;
    let child = RawDockChild {
        app_path: Some(PathBuf::from(std::ffi::OsStr::from_bytes(
            b"/Applications/\xff.app",
        ))),
        ..safari()
    };

    assert_eq!(app_items(vec![child], PRIMARY_2X), vec![]);
}

#[test]
fn only_disabled_access_and_an_unanswering_dock_are_dock_wide() {
    let cases = [
        (DockError::ApiDisabled, true),
        (DockError::CannotComplete, true),
        (DockError::NotRunning, false),
        (DockError::NoItemList, false),
        (
            DockError::Ax {
                attribute: "AXURL",
                code: -25202,
            },
            false,
        ),
    ];

    for (error, dock_wide) in cases {
        assert_eq!(error.is_dock_wide(), dock_wide, "{error}");
    }
}
