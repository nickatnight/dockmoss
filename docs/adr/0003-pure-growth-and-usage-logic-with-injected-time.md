# ADR-0003: Pure `growth/` and `usage/` logic, with time passed in

## Status

Accepted at Gate 1, 2026-11-02. Slice 1 creates the empty modules. The logic arrives in slices 6 to 8.

## Date

2026-11-02

## Context

Overgrowth depends on how many days have passed since an app was last used. Time-dependent logic is hard to test if it reads the clock itself. The last-used rules in `SUMMARY.md` also have edge cases that need exact tests:

- null Spotlight dates
- five or more apps sharing a timestamp within seconds (a migration signature)
- the first-run baseline, applied once
- apps already running at startup
- per-app jitter that must not change between launches

## Decision

- **No platform code in the logic:** `src-tauri/src/growth/` and `src-tauri/src/usage/` stay pure Rust, with no `tauri` and no `objc2`. Platform input reaches them as plain types from `macos/` (ADR-0001).
- **Time comes in as a parameter.** A clock is injected into all pure logic, and time is stored as `jiff::Timestamp` in UTC. It becomes local time only for display.
- **Domain values get newtypes:** `BundleId(String)`, and `Overgrowth(f32)` clamped to 0..=1 when constructed.
- **Stable jitter:** jitter is a fixed hash of the bundle ID (FNV or xxhash), in the range ±3 days. It does not use `std::hash::DefaultHasher` or an RNG.
- **How tests are written:** table-driven tests with fixed timestamps. They live in `<module>/tests.rs`, not inline, so the coverage figure measures only logic.

## Alternatives considered

### Reading the system clock inside the logic
- Cons: tests would depend on the wall clock, and fast-forward (1x to 1000x) would need a second code path.
- Rejected. Fast-forward becomes a factor on elapsed time computed from the injected clock.

### `DefaultHasher` or an RNG for jitter
- Cons: `DefaultHasher` output is not guaranteed across Rust versions, and an RNG changes on every launch. Either way, moss would shift between runs.
- Rejected.

## Consequences

- The coverage gate in `CONSTRAINTS.md` (80% of lines, excluding only `macos/`, `main.rs`, `lib.rs`, `tray.rs`, `commands.rs` and `tests.rs` files) applies fully to these modules.
- Callers in `lib.rs` and `commands.rs` own the real clock and the I/O. The tracker thread, the Tauri commands and the launch-log writer pass `Timestamp` values in and send results out.
- Golden-value tests pin the jitter, so a change to the hash shows up as a failing test, not as moss that moves.
