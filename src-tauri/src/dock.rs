use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::bundle_id::BundleId;
use crate::geometry::{Display, LogicalRect, ScreenRect, to_overlay_local};
use crate::trust::Trust;

const APPLICATION_SUBROLE: &str = "AXApplicationDockItem";
/// A latched gate re-checks trust at most this often: the tracker's `Slow`
/// interval, so a revoked grant costs one query per slow poll.
const RECHECK_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DockError {
    #[error("the Dock process is not running")]
    NotRunning,
    #[error("Accessibility access is not granted, so the Dock cannot be read")]
    ApiDisabled,
    #[error("the Dock did not answer an Accessibility request")]
    CannotComplete,
    #[error("the Dock's Accessibility tree has no item list")]
    NoItemList,
    #[error("reading {attribute} from the Dock failed with AX error {code}")]
    Ax { attribute: &'static str, code: i32 },
}

impl DockError {
    /// True when no part of the Dock can be read: access is not granted, or
    /// the Dock is not answering. Any other error belongs to one child.
    pub fn is_dock_wide(&self) -> bool {
        matches!(self, Self::ApiDisabled | Self::CannotComplete)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockItem {
    pub bundle_id: BundleId,
    /// Kept in Rust for the Spotlight read. The path can hold the user's
    /// name, and the overlay only draws, so it is not sent to the webview.
    #[serde(skip_serializing)]
    pub app_path: PathBuf,
    pub rect: LogicalRect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawDockChild {
    pub subrole: Option<String>,
    pub rect: Option<ScreenRect>,
    pub app_path: Option<PathBuf>,
    pub bundle_id: Option<String>,
}

/// Runs the Dock read only when trusted. On macOS 27 an untrusted AX read
/// raises the system Accessibility dialog by itself, so a poll must never
/// reach `read` while untrusted.
///
/// `TrustState` is refreshed only every few seconds, so trust revoked while
/// running shows up first as a read that returns `ApiDisabled`. From then on
/// the gate skips the read and calls `recheck` (a trust query that never
/// prompts) at most once per `Slow` interval, until it or `TrustState`
/// reports trust again. The latch holds only while `check_trust` agrees with
/// AX.
#[derive(Debug, Default)]
pub struct ReadGate {
    revoked: bool,
    last_recheck: Option<Instant>,
}

impl ReadGate {
    pub fn read(
        &mut self,
        trust: Trust,
        now: Instant,
        recheck: impl FnOnce() -> Trust,
        read: impl FnOnce() -> Result<Vec<RawDockChild>, DockError>,
    ) -> Result<Vec<RawDockChild>, DockError> {
        match trust {
            Trust::Untrusted => {
                self.revoked = false;
                self.last_recheck = None;
                return Err(DockError::ApiDisabled);
            }
            Trust::Trusted if self.revoked => {
                let due = self
                    .last_recheck
                    .is_none_or(|last| now.saturating_duration_since(last) >= RECHECK_INTERVAL);
                if !due {
                    return Err(DockError::ApiDisabled);
                }
                self.last_recheck = Some(now);
                if recheck() == Trust::Untrusted {
                    return Err(DockError::ApiDisabled);
                }
            }
            Trust::Trusted => {}
        }
        let result = read();
        self.revoked = result == Err(DockError::ApiDisabled);
        if !self.revoked {
            self.last_recheck = None;
        }
        result
    }
}

pub fn app_items(children: Vec<RawDockChild>, display: Display) -> Vec<DockItem> {
    children
        .into_iter()
        .filter(|child| child.subrole.as_deref() == Some(APPLICATION_SUBROLE))
        .filter_map(|child| app_item(child, display))
        .collect()
}

fn app_item(child: RawDockChild, display: Display) -> Option<DockItem> {
    let Some(app_path) = child.app_path else {
        tracing::debug!(bundle_id = child.bundle_id, "dock app item has no URL");
        return None;
    };
    if app_path.to_str().is_none() {
        tracing::debug!(path = %app_path.display(), "dock app item path is not UTF-8");
        return None;
    }
    let Some(bundle_id) = child.bundle_id.and_then(BundleId::new) else {
        tracing::debug!(path = %app_path.display(), "dock app item has no bundle id");
        return None;
    };
    let Some(rect) = child.rect else {
        tracing::debug!(path = %app_path.display(), "dock app item has no position or size");
        return None;
    };
    let rect = to_overlay_local(rect, display)?;
    Some(DockItem {
        bundle_id,
        app_path,
        rect,
    })
}

/// How one registration of the Dock hover observer (`macos::dock_observer`)
/// ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverRegistration {
    Registered,
    /// The Dock refuses the notification (`kAXErrorNotificationUnsupported`),
    /// so no later attempt can succeed.
    Unsupported,
    /// Access is refused (`kAXErrorAPIDisabled`) though trust was cached as
    /// granted: a revoked grant, or a TCC entry that disagrees with the check.
    ApiDisabled,
    /// Anything else, such as a Dock still starting: worth another attempt.
    Failed,
}

/// Whether the hover observer may register with the Dock. Its thread waits
/// between attempts; this decides whether an attempt is made at all. Like
/// `ReadGate`, it makes no AX call while trust is revoked: an attempt needs
/// the cached trust and a fresh, no-prompt `check`, and after access was
/// refused it waits until the cached trust has been seen revoked.
#[derive(Debug, Default)]
pub struct HoverObserverGate {
    refused: bool,
    latched: bool,
}

impl HoverObserverGate {
    /// Seeing the cached trust revoked releases the latch.
    pub fn may_register(&mut self, trust: Trust, check: impl FnOnce() -> Trust) -> bool {
        if self.refused {
            return false;
        }
        if !trust.is_trusted() {
            self.latched = false;
            return false;
        }
        !self.latched && check().is_trusted()
    }

    pub fn record(&mut self, registration: HoverRegistration) {
        match registration {
            HoverRegistration::Unsupported => self.refused = true,
            HoverRegistration::ApiDisabled => self.latched = true,
            HoverRegistration::Registered | HoverRegistration::Failed => {}
        }
    }

    /// The Dock refused the notification: the observer can stop, and entry
    /// into a magnification is found by polling alone.
    pub fn gave_up(&self) -> bool {
        self.refused
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::{
        DockError, DockItem, HoverObserverGate, HoverRegistration, RawDockChild, ReadGate,
        app_items,
    };
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
        thread_local! {
            static CLOCK: Cell<Option<Instant>> = const { Cell::new(None) };
        }
        let now = CLOCK.with(|clock| {
            let now = clock
                .get()
                .map_or_else(Instant::now, |last| last + Duration::from_secs(1));
            clock.set(Some(now));
            now
        });
        let read = Cell::new(false);
        let rechecked = Cell::new(false);
        let result = gate.read(
            trust,
            now,
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
    fn the_recheck_interval_is_the_trackers_slow_interval() {
        assert_eq!(
            super::RECHECK_INTERVAL,
            crate::tracker::PollRate::Slow.interval()
        );
    }

    #[test]
    fn a_latched_gate_rechecks_trust_at_most_once_per_slow_interval() {
        let mut gate = ReadGate::default();
        let start = Instant::now();
        let revoked = gate.read(
            Trust::Trusted,
            start,
            || Trust::Untrusted,
            || Err(DockError::ApiDisabled),
        );
        assert_eq!(revoked, Err(DockError::ApiDisabled));
        let rechecks = Cell::new(0);

        for tick in 1..=300_u32 {
            let now = start + Duration::from_nanos(16_666_667) * tick;
            let result = gate.read(
                Trust::Trusted,
                now,
                || {
                    rechecks.set(rechecks.get() + 1);
                    Trust::Untrusted
                },
                || panic!("read while access is revoked"),
            );
            assert_eq!(result, Err(DockError::ApiDisabled));
        }

        assert_eq!(rechecks.get(), 10);
    }

    fn latched_and_rechecked(gate: &mut ReadGate, start: Instant) {
        let latched = gate.read(
            Trust::Trusted,
            start,
            || panic!("re-checked before the gate latched"),
            || Err(DockError::ApiDisabled),
        );
        assert_eq!(latched, Err(DockError::ApiDisabled));
        let rechecked = Cell::new(false);
        let still_revoked = gate.read(
            Trust::Trusted,
            start + Duration::from_millis(600),
            || {
                rechecked.set(true);
                Trust::Untrusted
            },
            || panic!("read while access is revoked"),
        );
        assert_eq!(still_revoked, Err(DockError::ApiDisabled));
        assert!(rechecked.get(), "the first re-check was held back");
    }

    fn rechecks_at(gate: &mut ReadGate, now: Instant) -> bool {
        let rechecked = Cell::new(false);
        let result = gate.read(
            Trust::Trusted,
            now,
            || {
                rechecked.set(true);
                Trust::Untrusted
            },
            || Err(DockError::ApiDisabled),
        );
        assert_eq!(result, Err(DockError::ApiDisabled));
        rechecked.get()
    }

    #[test]
    fn a_new_latch_after_a_good_read_rechecks_at_once() {
        let mut gate = ReadGate::default();
        let start = Instant::now();
        latched_and_rechecked(&mut gate, start);
        let good = gate.read(
            Trust::Trusted,
            start + Duration::from_millis(1100),
            || Trust::Trusted,
            || Ok(vec![safari()]),
        );
        assert!(good.is_ok());
        let relatched = gate.read(
            Trust::Trusted,
            start + Duration::from_millis(1200),
            || panic!("re-checked before the gate latched again"),
            || Err(DockError::ApiDisabled),
        );
        assert_eq!(relatched, Err(DockError::ApiDisabled));

        assert!(rechecks_at(&mut gate, start + Duration::from_millis(1300)));
    }

    #[test]
    fn a_new_latch_after_trust_state_reports_untrusted_rechecks_at_once() {
        let mut gate = ReadGate::default();
        let start = Instant::now();
        latched_and_rechecked(&mut gate, start);
        let untrusted = gate.read(
            Trust::Untrusted,
            start + Duration::from_millis(650),
            || panic!("re-checked while untrusted"),
            || panic!("read while untrusted"),
        );
        assert_eq!(untrusted, Err(DockError::ApiDisabled));
        let relatched = gate.read(
            Trust::Trusted,
            start + Duration::from_millis(700),
            || panic!("re-checked before the gate latched again"),
            || Err(DockError::ApiDisabled),
        );
        assert_eq!(relatched, Err(DockError::ApiDisabled));

        assert!(rechecks_at(&mut gate, start + Duration::from_millis(800)));
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

    /// One earlier attempt: the cached trust, the fresh check (if the gate
    /// makes one), and the outcome recorded if the attempt was allowed.
    type Step = (Trust, Trust, HoverRegistration);

    #[derive(Debug, PartialEq)]
    struct GateAnswer {
        allowed: bool,
        fresh_checks: u32,
        gave_up: bool,
    }

    const ALLOWED: GateAnswer = GateAnswer {
        allowed: true,
        fresh_checks: 1,
        gave_up: false,
    };
    const BLOCKED_UNCHECKED: GateAnswer = GateAnswer {
        allowed: false,
        fresh_checks: 0,
        gave_up: false,
    };
    const BLOCKED_BY_CHECK: GateAnswer = GateAnswer {
        allowed: false,
        fresh_checks: 1,
        gave_up: false,
    };
    const GAVE_UP: GateAnswer = GateAnswer {
        allowed: false,
        fresh_checks: 0,
        gave_up: true,
    };

    /// Replays `history`, then asks once more with `trust` and `check`.
    fn hover_gate_after(history: &[Step], trust: Trust, check: Trust) -> GateAnswer {
        let mut gate = HoverObserverGate::default();
        for &(trust, check, outcome) in history {
            if gate.may_register(trust, || check) {
                gate.record(outcome);
            }
        }
        let made = Cell::new(0);
        let allowed = gate.may_register(trust, || {
            made.set(made.get() + 1);
            check
        });
        GateAnswer {
            allowed,
            fresh_checks: made.get(),
            gave_up: gate.gave_up(),
        }
    }

    const TRUSTED: Trust = Trust::Trusted;
    const UNTRUSTED: Trust = Trust::Untrusted;

    #[test]
    fn the_hover_observer_registers_only_when_cached_and_fresh_trust_agree() {
        let cases = [
            (
                "untrusted: no fresh check",
                UNTRUSTED,
                TRUSTED,
                BLOCKED_UNCHECKED,
            ),
            ("trusted, checked trusted", TRUSTED, TRUSTED, ALLOWED),
            (
                "trusted, but the fresh check says untrusted",
                TRUSTED,
                UNTRUSTED,
                BLOCKED_BY_CHECK,
            ),
        ];

        for (case, trust, check, expected) in cases {
            assert_eq!(hover_gate_after(&[], trust, check), expected, "{case}");
        }
    }

    #[test]
    fn the_hover_observer_retries_a_failure_and_stops_after_the_dock_refuses() {
        let cases = [
            (
                "after a failure worth a retry",
                HoverRegistration::Failed,
                ALLOWED,
            ),
            (
                "after a registration that held",
                HoverRegistration::Registered,
                ALLOWED,
            ),
            (
                "after the Dock refused",
                HoverRegistration::Unsupported,
                GAVE_UP,
            ),
        ];

        for (case, outcome, expected) in cases {
            let history = [(TRUSTED, TRUSTED, outcome)];
            assert_eq!(
                hover_gate_after(&history, TRUSTED, TRUSTED),
                expected,
                "{case}"
            );
        }
    }

    #[test]
    fn refused_access_latches_the_hover_observer_until_trust_is_seen_revoked() {
        let refused = (TRUSTED, TRUSTED, HoverRegistration::ApiDisabled);
        let held = (TRUSTED, TRUSTED, HoverRegistration::Registered);
        let revoked = (UNTRUSTED, TRUSTED, HoverRegistration::Registered);
        let cases: [(&str, &[Step], GateAnswer); 3] = [
            (
                "refused while cached as trusted: no fresh check",
                &[refused],
                BLOCKED_UNCHECKED,
            ),
            ("still latched later", &[refused, held], BLOCKED_UNCHECKED),
            (
                "revoked, then granted again: released",
                &[refused, revoked],
                ALLOWED,
            ),
        ];

        for (case, history, expected) in cases {
            assert_eq!(
                hover_gate_after(history, TRUSTED, TRUSTED),
                expected,
                "{case}"
            );
        }
    }
}
