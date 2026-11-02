Source: SUMMARY.md. Read it alongside this file; it wins on any conflict.

# Spec: Dock Moss MVP

This file adds only what `SUMMARY.md` lacks: scope for this job, decisions taken since, acceptance criteria, out-of-scope items and open questions. Engineering conventions live in `CLAUDE.md`, and the quality bar lives in `CONSTRAINTS.md`.

## Scope of this job

SUMMARY.md milestones 1–6: the overlay, Dock geometry, the launch log with Spotlight seeding, the growth model, moss rendering, the shrivel animation and menu-bar settings. Milestone 7 (signing, notarization) is out, because this machine has no Developer ID identity (`security find-identity` shows 0 valid identities). Bundle config is prepared, but the work stops before signing.

## Decisions already taken

| Area | Decision |
|---|---|
| Shell | Tauri 2, Rust edition 2024, `objc2` framework crates for AppKit/AX. Electron fallback is not pursued unless slice 2 fails. |
| Package manager | pnpm, `pnpm-lock.yaml` only |
| Frontend | Vite + vanilla TypeScript, raw WebGL2 with one shader program for all icons. No UI framework and no three.js. The settings window is a plain HTML form. |
| Persistence | Launch log: JSON in `app_data_dir`, written atomically. Settings: `tauri-plugin-store`. |
| Login item | `tauri-plugin-autostart` |
| Time | `jiff::Timestamp`, UTC. A clock is injected into all pure logic. |
| Jitter | Stable hash of the bundle ID (FNV/xxhash), in the range ±3 days. |
| Platform | macOS only (dev machine is macOS 27). No cross-platform `cfg` scaffolding. |

## Capabilities and acceptance criteria

Each criterion is checkable: by `cargo test`, by a command, or by a named manual check (M).

### overlay
- An `overlay` window covers the Dock's display. It is transparent, has no shadow, never takes focus, and does not appear in Cmd-Tab or Mission Control.
- Clicks, drags and hovers pass through to the Dock (M: clicking an icon under the overlay launches the app; magnification still works).
- It renders above the Dock on every Space and over full-screen apps' Dock reveal (M).
- Dock Moss itself has no Dock icon. A tray icon offers Settings… and Quit.

### dock-geometry
- On startup, without Accessibility trust: the tray shows "Grant Accessibility…", which opens the correct System Settings pane. The overlay draws nothing. Nothing panics.
- With trust: a list of `DockItem { bundle_id, app_path, rect }` for application Dock items only (no folders, Trash, separators or minimized windows).
- Rects are converted to overlay-local logical pixels by one tested function. Unit tests cover the primary display, a secondary display with a negative origin, and 2x scale.
- Geometry follows magnification. Updates reach the webview only when a rect changes by ≥0.5 px.
- When the Dock is auto-hidden and offscreen, the item list sent to the webview is empty.

### usage
- `NSWorkspace` didLaunch/didActivate events update `last_launched[bundle_id]`. The update is persisted atomically within 1 s.
- Apps already running at startup count as used now.
- First run: every app with no history is baselined to "now". The baseline is stored, so it is applied once only.
- Spotlight seed: `kMDItemLastUsedDate` is used only when present and the batch is sane. When ≥5 apps share a timestamp within 5 s, Spotlight dates are discarded for that batch. `kMDItemDateAdded` is never read.
- A corrupt or missing log file is replaced with an empty one plus a re-baseline, and logged with `tracing::warn!`. The app does not crash.

### growth
- `effective_last_used = max(launch_log, spotlight_seed, baseline)`
- `overgrowth(days, jitter, thresholds) -> Overgrowth` is monotonic non-decreasing in days and clamped to 0..=1. Default stage thresholds are 30/90/180 days, mapped to stages Clean/Fuzz/Patchy/Overgrown.
- Unknown apps start at 0 and follow the normal curve.
- Recomputed every 5 minutes and on any usage event. Fast-forward multiplies elapsed days by a settings factor (1×, 10×, 100×, 1000×).
- Table-driven tests cover all of the above with fixed clocks.

### render
- A single WebGL2 draw renders moss on every icon. Coverage scales with overgrowth: moss grows from the bottom edge and corners, clipped to the icon's rounded rect (corner radius ≈ 22.5% of icon size). The Overgrown stage adds small mushroom/flower sprites.
- Idle cost: when geometry and overgrowth are unchanged, no frames are requested (`requestAnimationFrame` loop stops). M: Activity Monitor shows Dock Moss at <1% CPU with the Dock idle.
- An app launch plays a ~600 ms shrivel animation on that icon, then moss is cleared.

### settings
- The settings window (opened from the tray) holds stage thresholds, intensity (0–100%), per-app exclusions picked from current Dock apps, launch at login, and fast-forward.
- Changes apply live to the overlay without a restart, and persist across restarts.
- Excluded apps render no moss.

## Out of scope

- Signing, notarization and DMG (milestone 7). A stable signing identity is needed first.
- Seasonal variants, critters, the "garden" view (SUMMARY.md "Later ideas")
- `AXObserver`-based geometry (polling is accepted for MVP)
- Left/right Dock orientations (moss always grows from the icon's bottom edge)
- Windows/Linux, App Store distribution
- Auto-update

## Assumptions

1. Spotlight is queried by running `mdls -raw -name kMDItemLastUsedDate <path>` once per app at first run. This avoids binding CoreServices `MDItem` for a one-time seed.
2. Dock geometry polling runs at 60 Hz while the cursor is within 150 px of the Dock edge, and at 2 Hz otherwise.

## Gate 1 decisions (2026-11-02)

- **Accessibility and Screen Recording** are granted to the user's terminal app. Builders run `pnpm tauri dev` from a terminal, so macOS attributes both grants to it, and builders may use `screencapture` for verification.
- **Dock orientation:** bottom only for MVP. Rects stay correct for left/right Docks (AX is orientation-agnostic), but moss always grows from each icon's bottom edge, and left/right support is out of scope.
- **Tool installs:** builders may run `cargo install cargo-llvm-cov cargo-deny` and `brew install gitleaks`.
- **Delivery:** one emgee job per plan slice, run one after another. Each job branches from `main` after the previous one is merged. See the job index in `plan.md`.
