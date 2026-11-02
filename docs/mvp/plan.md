# Plan: Dock Moss MVP

Read `spec.md` (and `SUMMARY.md`) for what is built, and `CLAUDE.md` for how. Slices run in order. High-risk platform work comes first so a dead end surfaces before the logic and rendering are built on top of it. Each slice is one conventional commit and leaves the app launchable.

**Every slice's verification includes the four `CLAUDE.md` checks** (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build` in `src-tauri/`) plus `pnpm check` (tsc + frontend lint). They are listed below only where a slice adds something beyond them. A slice is done when its acceptance criteria hold and every verification line passes. **(M)** marks a manual check: do it with `screencapture` / Activity Monitor where possible, and record the result in `build.md`.

## Dependency graph

```
1 scaffold ─┬─ 2 overlay ── 4 dock-geometry ── 5 tracking loop ─┐
            └─ 3 AX trust ──┘                                    │
1 ── 6 growth model (pure) ──┐                                   ├─ 9 moss on one icon ── 10 all icons ── 11 shrivel
1 ── 7 launch log ── 8 spotlight seed + baseline ── wire ────────┘                                         │
                                                              12 settings window ── 13 live settings + autostart
```

## Job index

Each slice is one emgee job, started with `/emgee` after the previous job is merged to `main`. A job's `spec.md` is the line `Source: docs/mvp/spec.md and slice N of docs/mvp/plan.md`, plus anything specific to that slice. A checkpoint is checked at the Gate 2 of the job it follows, before that job merges.

| Job | Slice | Gate 2 also checks |
|---|---|---|
| 001-scaffold | 1 (also commits `docs/mvp/`, `CONSTRAINTS.md`) | |
| 002-overlay-window | 2 | |
| 003-ax-trust | 3 | |
| 004-dock-geometry | 4 | |
| 005-tracking-loop | 5 | Checkpoint A |
| 006-growth-model | 6 (may run in parallel with 002–005) | |
| 007-launch-log | 7 | |
| 008-spotlight-seed | 8 | Checkpoint B |
| 009-moss-one-icon | 9 | |
| 010-moss-all-icons | 10 | |
| 011-shrivel | 11 | Checkpoint C |
| 012-settings-window | 12 | |
| 013-live-settings | 13 | Checkpoint D |

Slice 6 touches only `growth/` and one `mod` line, so it is the one slice that can safely run in a parallel session. Every other slice edits shared files (`lib.rs`, `commands.rs`, `src/main.ts`) and runs in sequence.

## Phase 1: Platform risk

### 1. Scaffold
Tauri 2 + Vite + vanilla TS via `pnpm create tauri-app`, restructured to the `CLAUDE.md` layout (`error.rs`, `commands.rs`, empty `growth/`, `usage/`, `macos/`). Includes `[lints]` in `Cargo.toml`, edition 2024 with `rust-version`, `tracing` via `tauri-plugin-log`, an accessory activation policy, and a tray with Quit. Adds the `check` scripts from CONSTRAINTS.md to `package.json`.
- Accept: `pnpm tauri dev` launches; a tray icon appears; Dock Moss has no Dock icon; Quit exits.
- Verify: standard checks; (M) tray visible and no Dock icon.
- Files: `package.json`, `src-tauri/{Cargo.toml,tauri.conf.json,src/lib.rs,src/main.rs,src/error.rs}`, `src-tauri/capabilities/*.json`

### 2. Overlay window (highest risk)
An `overlay` window that is transparent, undecorated and click-through, with `macOSPrivateApi`. In `macos/overlay.rs`: NSWindow level `kCGDockWindowLevel + 1` and collection behaviour `canJoinAllSpaces | stationary | fullScreenAuxiliary | ignoresCycle`. The window is sized to the main display's frame. The webview draws a 2 px debug border along the bottom 120 px.
- Accept: the border is visible above the Dock; clicks and magnification reach the Dock; the window shows on every Space; it is absent from Cmd-Tab.
- Verify: (M) `screencapture` with the border over the Dock; click a Dock icon through the overlay and check the app launches; switch Spaces.
- If the window level cannot be raised above the Dock, the slice is **BLOCKED**: the Electron fallback decision belongs to the human.
- Files: `src-tauri/src/macos/{mod.rs,overlay.rs}`, `src-tauri/src/lib.rs`, `tauri.conf.json`, `src/main.ts`, `index.html`

### 3. Accessibility trust
`macos::ax::is_trusted(prompt: bool)` wraps `AXIsProcessTrustedWithOptions`. Without trust, the tray shows "Grant Accessibility…", which opens `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`. Trust is re-checked every 5 s until it is granted.
- Accept: the untrusted path shows the menu item and nothing panics; the trusted path hides it.
- Verify: unit test for the menu-state reducer; (M) both states.
- Files: `src-tauri/src/macos/ax.rs`, `src-tauri/src/tray.rs`, `lib.rs`

### 4. Dock geometry
`macos::dock::items() -> Result<Vec<DockItem>>` reads the Dock process's AX tree. It keeps only children with subrole `AXApplicationDockItem` and reads `AXPosition`/`AXSize`/`AXURL`, resolving the bundle ID from the URL. The pure function `geometry::to_overlay_local(rect, display) -> LogicalRect` has unit tests covering the primary display, a secondary display with negative origin, and 2x scale. A debug command draws a 1 px rect per icon.
- Accept: the debug rects align with the icons (within 1 px at rest).
- Verify: `cargo test geometry`; (M) screenshot with the rects aligned.
- Files: `src-tauri/src/macos/dock.rs`, `src-tauri/src/geometry.rs`, `commands.rs`, `src/debug.ts`

### 5. Tracking loop
A dedicated thread polls `dock::items()` (60 Hz when the cursor is within 150 px of the Dock edge, 2 Hz otherwise). It diffs against the last set with a 0.5 px tolerance and sends changes over a `tauri::ipc::Channel<Vec<DockItem>>`. An auto-hidden, offscreen Dock sends an empty list. The webview requests frames only on change.
- Accept: the debug rects follow magnification; no channel messages arrive when the Dock is idle; CPU <1% at idle.
- Verify: unit tests for the diff and tolerance; (M) magnification screenshot; Activity Monitor reading.
- Files: `src-tauri/src/tracker.rs`, `commands.rs`, `src/ipc.ts`, `src/main.ts`

**Checkpoint A:** the overlay follows every Dock icon through magnification and auto-hide, is click-through, and idles at <1% CPU. If this fails, stop and escalate before any logic is written.

## Phase 2: Usage data and growth (pure logic first)

### 6. Growth model
`growth/`: `BundleId`, `Overgrowth`, `Stage`, `Thresholds`, `jitter(bundle_id) -> Days` (stable hash, ±3 d), `effective_last_used(...)`, and `overgrowth(now, last_used, jitter, thresholds, fast_forward)`.
- Accept: monotonic and clamped; stage boundaries at 30/90/180 days; jitter is identical across runs (golden values in a test); unknown apps start at 0; fast-forward scales elapsed time.
- Verify: `cargo test growth`, table-driven tests with fixed timestamps.
- Files: `src-tauri/src/growth/{mod.rs,curve.rs,jitter.rs}`

### 7. Launch log
`usage/log.rs` holds `LaunchLog` (bundle ID → `Timestamp`, plus `baseline_at`) with load and atomic save (temp file + rename in `app_data_dir`). A corrupt or missing file is replaced with an empty log and a warning. `macos/workspace.rs` subscribes to didLaunch/didActivate and records running apps at startup. Events reach the log via an mpsc channel; saves are debounced to ≤1 s.
- Accept: launching an app updates the log on disk within 1 s; a corrupt file is recovered from; running apps count as used now.
- Verify: unit tests for load/save/corrupt/recovery using `tempfile`; (M) launch TextEdit and check the file updates.
- Files: `src-tauri/src/usage/{mod.rs,log.rs}`, `src-tauri/src/macos/workspace.rs`, `lib.rs`

### 8. Spotlight seed and first-run baseline
`usage/spotlight.rs` holds `parse_mdls`, the pure `is_migration_batch(dates)` (≥5 within 5 s), and `seed(...)`, which runs `mdls -raw` per Dock app once on first run. The baseline is applied once. Growth is wired end to end: tracker items + log + growth → `Vec<{bundle_id, rect, overgrowth, stage}>` sent to the webview. Recompute runs every 5 minutes and on usage events.
- Accept: a migration batch is discarded; `(null)` is handled; the baseline is set once; the webview receives overgrowth per icon.
- Verify: `cargo test usage`; (M) delete the app data dir, relaunch, and confirm every icon reads 0 (shown as numbers in the debug overlay).
- Files: `src-tauri/src/usage/spotlight.rs`, `src-tauri/src/state.rs`, `commands.rs`, `src/debug.ts`

**Checkpoint B:** with fast-forward hard-coded to 1000× in a debug build, the debug overlay shows overgrowth values rising per icon and resetting when an app launches.

## Phase 3: Rendering

### 9. Moss on one icon
`src/render/`: a WebGL2 context, one fullscreen quad, and a fragment shader with fbm noise. Moss grows from the bottom edge and corners, coverage is driven by overgrowth, and it is clipped to a rounded-rect SDF (radius 22.5%). Hard-coded to the first Dock icon.
- Accept: the visual reads as moss at 0.3, 0.6 and 1.0; nothing is drawn outside the icon's rounded rect.
- Verify: (M) three screenshots, one per value, saved under `docs/screenshots/` (git-ignored) and listed in `build.md`.
- Files: `src/render/{gl.ts,moss.frag.glsl,moss.vert.glsl,index.ts}`

### 10. All icons, idle loop, Overgrown sprites
Per-icon data goes in a uniform array/UBO (rect, overgrowth, seed), still in one draw call. Mushroom/flower sprites appear at the Overgrown stage. `requestAnimationFrame` runs only while sway is enabled and an icon is ≥Patchy, capped at 30 fps; otherwise frames are drawn on change only. Debug drawing is removed from release builds.
- Accept: every Dock app shows moss matching its overgrowth; idle CPU <1% when no icon is ≥Patchy and <3% with sway.
- Verify: (M) full-Dock screenshot at 1000× fast-forward; Activity Monitor readings.
- Files: `src/render/*`, `src/main.ts`

### 11. Shrivel animation
A usage event for a mossy app makes Rust emit `moss-cleared {bundle_id}`. The webview animates that icon's coverage to 0 with a desaturate/shrink over ~600 ms, then settles at the new overgrowth (0).
- Accept: launching a mossy app plays the animation once, then the icon is clean.
- Verify: (M) screen recording or a frame sequence via `screencapture -V`.
- Files: `src-tauri/src/state.rs`, `src/render/index.ts`, `src/ipc.ts`

**Checkpoint C:** a full visual pass on a real Dock, reviewed against SUMMARY.md's concept section.

## Phase 4: Settings

### 12. Settings window
A tray item, Settings…, opens a `settings` window (its own capability file with store and autostart permissions only). The form holds thresholds (three numbers, validated ascending), intensity slider, exclusions (checkbox list of current Dock apps), launch at login, and fast-forward (1/10/100/1000×). Values persist with `tauri-plugin-store`.
- Accept: values survive a restart; invalid thresholds are rejected with an inline message.
- Verify: Rust unit tests for `Settings` validation and serde defaults; (M) restart persistence.
- Files: `settings.html`, `src/settings.ts`, `src-tauri/src/settings.rs`, `src-tauri/capabilities/settings.json`, `lib.rs`

### 13. Live settings and launch at login
A settings change makes Rust recompute and push to the overlay immediately. Excluded apps render nothing. Intensity scales coverage. Launch at login uses `tauri-plugin-autostart`. Bundle config is prepared (`identifier` finalised, `Info.plist` usage string for Accessibility, `minimumSystemVersion`), without signing.
- Accept: every setting visibly changes the overlay within 1 s with no restart; the login item appears in System Settings → General → Login Items.
- Verify: (M) toggle each setting and screenshot; `pnpm tauri build` produces an unsigned `.app`.
- Files: `src-tauri/src/state.rs`, `src-tauri/src/settings.rs`, `src/render/index.ts`, `tauri.conf.json`, `src-tauri/Info.plist`

**Checkpoint D (done):** every acceptance criterion in spec.md holds, CONSTRAINTS.md checks pass, and the unsigned `pnpm tauri build` app runs from `/Applications`.

## Risks

| Risk | Impact | Mitigation |
|---|---|---|
| NSWindow level can't sit above the Dock, or click-through breaks on macOS 27 | High: kills the approach | Slice 2 is first and BLOCKs on failure; the Electron decision goes to the human |
| AX grant unavailable to an unattended builder | High: stalls slices 4+ | Gate 1 Q1: pre-grant to the terminal |
| AX polling at 60 Hz is costly | Med | Proximity-gated rate (slice 5), measured at Checkpoint A |
| Manual visual checks are subjective for an agent | Med | Screenshots in `build.md`; Checkpoint C plus the UI Designer reviewer |
| macOS 27 API changes vs objc2 crate bindings | Med | Pin crate versions; slice 2–4 surface it early |

## Follow-ups from job 001 reviews

These are optional findings from the job 001 reviews (round 1 and round 2 in that job's folder), grouped by the job that should pick them up. Tags like "round-2 perf #1" give the review round, the reviewer and the finding number. A job that edits a check edits CONSTRAINTS.md and `package.json` together.

### Job 002 (overlay window)
- Replace `"csp": null` in `tauri.conf.json` with a restrictive policy before the first window loads, with no `'unsafe-eval'` and no remote origin (round-2 security #1, round-2 code-review #1).
- Before Checkpoint A, the CONSTRAINTS.md idle-CPU check sums `%CPU` of dock-moss plus its WebKit WebContent and GPU processes, and records idle wakeups/s (round-2 perf #1).
- Give the overlay and settings windows separate HTML entries. Each has a transparent `html, body`, `margin: 0` and `overflow: hidden` (round-2 UI #5).
- Set Vite `build.modulePreload.polyfill: false`, and `build.target` to the WebKit version that matches `minimumSystemVersion` (round-2 perf #3, #4).
- Add a grep guard to `check:fast`, run from `src-tauri/`, so unit tests live only in `tests.rs`: `! grep -rlE --include='*.rs' '#\[([a-z_]+::)?test\]' src | grep -v '/tests[.]rs$'`. Note "(enforced by a grep in `check:fast`)" on the CONSTRAINTS.md Floor bullet (round-2 test-engineer A).
- Add `unused_result_ok = "deny"` under `[lints.clippy]` (round-2 test-engineer B).
- Add a grep guard to `check:fast` that keeps `unsafe_code` opt-ins inside `src/macos/`: `! grep -rl --include='*.rs' 'unsafe_code' src | grep -v '^src/macos/'` (round-2 test-engineer B, round-2 security #2).

### Job 003 (AX trust)
- Replace the stock Tauri bundle icons with a Dock Moss app icon before the Accessibility pane flow, because that pane shows the bundle icon (round-2 UI #2).
- Re-export the tray glyph at about 40 px tall on the 44 px canvas, centred, with the vein at least 3 px wide or dropped (round-2 UI #1, #3).

### Job 013 (live settings and bundle)
- Set `opt-level = "s"` in the release profile. It measured 20% smaller (3.38 MB against 4.24 MB) with the same idle CPU. Recheck the Checkpoint A CPU figure after switching (round-2 perf #2).
- Set bundle `targets` to `["app", "dmg"]` (round-2 code-review #4).
- Add `.env` and `.env.*` to `.gitignore` before any signing or notarisation credentials exist (round-2 security #4).
- Run cargo-deny `sources` (and `bans`), with `unknown-registry = "deny"` and `unknown-git = "deny"` in `deny.toml` (round-2 security #3).

### Later, or when relevant
- Run `gitleaks dir --redact --no-banner` on `src` and `src-tauri/src` so untracked files are scanned. Never point it at the repo root, because `target/` is several GB (round-2 test-engineer C).
- Once `src/ipc.ts` lands, add a ts-comment check: a grep for `@ts-(ignore|expect-error)` or ESLint `ban-ts-comment` (round-2 test-engineer D).
- `check:task` runs the tests twice, in `cargo test` and again in `cargo llvm-cov`. If test time grows, drop `cargo test`, because llvm-cov already fails on a failing test (round-2 code-review #2).
- The CLAUDE.md Lints bullet should list the four deny lints added in job 001 (`unsafe_code`, `allow_attributes`, `allow_attributes_without_reason`, `let_underscore_must_use`) or point to `Cargo.toml` (round-2 code-review #6).
- The ⌘Q accelerator on the tray's Quit item works only while the menu is open, because an Accessory app has no app menu (round-2 code-review #5).
- A tray glyph drawn from the product (a moss cushion, or an icon outline with moss along its bottom edge) would identify the app better than a leaf (round-2 UI #4).
