# DockMoss

macOS-only Tauri 2 app: a transparent, click-through overlay that grows moss on Dock icons for apps that haven't been used in a while. Product rules, the last-used algorithm, risks and milestones are in `SUMMARY.md`. Read it before touching usage tracking or growth logic, and update it when one of those rules changes.

Read CONSTRAINTS.md before writing code. Do not weaken it to make a change pass.

## Checks

Run from `src-tauri/`. All four must pass before a change is done:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

Run the app with `pnpm tauri dev` from the repo root. The package manager is **pnpm** (`pnpm install`, `pnpm add`, `pnpm <script>`), and `pnpm-lock.yaml` is the only lockfile. Frontend checks come from the scripts in `package.json`.

## Layout

```
src-tauri/src/
  main.rs        # one line: dockmoss_lib::run()
  lib.rs         # Builder, plugins, setup(), command registration, and the
                 #   dock-tracker, launch-log (usage) and spotlight-seed thread glue
  error.rs       # crate Error enum (thiserror + Serialize)
  commands.rs    # #[tauri::command] fns: thin, they delegate to modules
  tracker.rs     # pure: per Dock read -> what to send, log and show, and the poll rate
  geometry.rs    # pure: AX rects -> overlay-local logical pixels (the one conversion)
  dock.rs        # pure: raw Dock children -> DockItems (application items only)
  bundle_id.rs   # pure: BundleId newtype
  growth/        # pure: effective last-used -> 0..1 overgrowth, jitter, stages
  usage/         # launch log model and file format (log.rs), atomic store (store.rs),
                 #   save and recompute schedule (schedule.rs), running-app filter,
                 #   Spotlight parsing and migration-batch check (spotlight.rs), and
                 #   UsageState (state.rs): first run, first-seen, growth per Dock app
  macos/         # every objc2 / AppKit / AX call, the `mdls` subprocess, and every
                 #   `unsafe` block
src-tauri/capabilities/   # one capability file per window
src/                      # webview: canvas/WebGL moss renderer, settings UI
```

The boundary that matters is between `macos/` and everything else. `growth/`, `usage/`, `tracker.rs`, `geometry.rs`, `dock.rs` and `bundle_id.rs` stay pure Rust: no `tauri`, no `objc2`, and time passed in as a parameter. That keeps them unit-testable with fixed clocks and fixed displays, and the coverage gate counts them. `macos/` exposes safe functions that return plain Rust types such as `RawDockChild { subrole, rect, app_path, bundle_id }`, so the rest of the crate never sees a raw pointer.

## Rust conventions

- **Edition 2024**, with `rust-version` pinned in `Cargo.toml`. Use current std: `LazyLock`/`OnceLock`, let-else, let chains, `impl Trait` in argument position.
- **Errors:** each module has a `thiserror` enum. Commands return `Result<T, crate::Error>`, where `Error` implements `Serialize` as `{ kind, message }` so the frontend can branch on `kind`. `anyhow` is fine in `setup()` and in tests.
- **Panics:** use `?` in non-test code. Release builds abort on panic. `unwrap`, `expect` and slice indexing are denied crate-wide (CONSTRAINTS.md Floor). The only `.expect`s outside tests are named startup invariants: the `MainThreadMarker` in `setup()`, the Tauri build in `run()`, and `build.rs`. Each has its own `#[expect(clippy::expect_used, reason = "...")]`, and the reason states the invariant ("tauri runs setup() on the main thread").
- **Logging:** use `tracing` macros with structured fields (`tracing::debug!(bundle_id, days, "growth updated")`), wired up through `tauri-plugin-log`.
- **Types:** use newtypes for domain values: `BundleId(String)`, and `Overgrowth(f32)` clamped to 0..=1 when constructed. Store time as `jiff::Timestamp` (UTC) and convert to local time only for display.
- **Tests:** unit tests live in `<module>/tests.rs`, declared with `#[cfg(test)] mod tests;`, not in an inline `mod tests { ... }`. CONSTRAINTS.md excludes those files from coverage, so the number measures logic only.
- **Lints:** set them in `Cargo.toml` under `[lints]` instead of crate-level attributes. Enable `clippy::pedantic` as warn, plus `unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks`. When you allow a lint, add `#[expect(clippy::..., reason = "...")]` at the narrowest scope. A lint attribute covers everything inside the item or statement it sits on, closures included, so never put one on a statement that holds a closure such as `.setup(|app| ...)`; split the statement first.
- **Dependencies:** check that a new crate is maintained and supports Tauri 2 before you add it. Prefer an official `tauri-plugin-*` when one exists (autostart, store, log).

## Unsafe and objc2

- Use the `objc2` framework crates (`objc2-app-kit`, `objc2-foundation`, `objc2-application-services`, `objc2-core-foundation`). The older `cocoa` and `objc` crates are deprecated, so leave them out.
- Every `unsafe` block lives in `macos/` and has a `// SAFETY:` comment stating the invariant it depends on. Keep each block as small as the call it wraps. `unsafe_code` is denied crate-wide, so the item holding the block opts in with `#[expect(unsafe_code, reason = "...")]`.
- **AppKit runs on the main thread.** Take a `MainThreadMarker` in any function that touches `NSWindow`/`NSWorkspace`, and reach the main thread from elsewhere with `app.run_on_main_thread(...)`.
- **AX calls are synchronous IPC to the Dock process** and can block. Run Dock polling on a dedicated thread, or with `tauri::async_runtime::spawn_blocking`, and never on the main thread. Release CF objects through `CFRetained`/RAII wrappers instead of calling `CFRelease` by hand.
- Get the native window with `window.ns_window()` and cast it to `&NSWindow` inside `macos/overlay.rs` only.

## Tauri 2 conventions

- **Commands:** use `async fn` with owned arguments. Shared state goes in through `app.manage(...)` and comes out as `State<'_, T>`. Use `std::sync::Mutex` when the lock never crosses an `.await`; use `tokio::sync::Mutex` only when it does.
- **Spawning:** use `tauri::async_runtime::spawn`. A bare `tokio::spawn` panics inside `setup()` because no runtime context exists there yet.
- **Rust → webview:** stream state the webview needs from its first frame over a `tauri::ipc::Channel` that a `watch_*` command hands to the owning thread: Dock geometry with `watch_dock`, and each Dock app's growth with `watch_growth`. Send a frame only when it differs from the last one sent, so the webview render loop can stay idle (see Battery in `SUMMARY.md`). A new call replaces the channel and gets the current frame at once, so a reloaded webview resubscribes (ADR-0007). An event sent before the webview's listener exists is lost, so use `Emitter::emit_to("overlay", ...)` only for one-off events the webview can afford to miss.
- **Capabilities:** each window gets its own file in `capabilities/`, scoped by window label. The overlay webview only renders, so it gets the smallest permission set that lets it receive events, plus `allow-watch-dock` (the command that hands Rust its `Channel`). The settings window gets the plugin permissions it uses. App commands are listed in the app manifest in `build.rs`, which makes Tauri generate an `allow-<command>` permission for each one and deny any app command in a window whose capability doesn't grant it. A new command needs both its `build.rs` entry and a grant in the capability of the window that calls it.
- **Frontend:** import from `@tauri-apps/api/core` / `/event` (v2 paths). Keep every `invoke` call in one typed module (`src/ipc.ts`) that mirrors the Rust command signatures.

## macOS overlay gotchas

- **Transparency** needs `"macOSPrivateApi": true` under `app` in `tauri.conf.json`, plus `transparent: true` on the window and a transparent `body` background in CSS. Private API rules out the Mac App Store. Distribution is Developer ID only.
- **Overlay window:** `decorations: false`, `shadow: false`, `focus: false`, `skipTaskbar: true`, then `set_ignore_cursor_events(true)` and `set_visible_on_all_workspaces(true)`. Set the `NSWindow` level above the Dock (`kCGDockWindowLevel + 1`) and the collection behavior to `canJoinAllSpaces | stationary | fullScreenAuxiliary | ignoresCycle` through objc2. Tauri's `always_on_top` level sits below the Dock.
- **No Dock icon of our own:** call `app.set_activation_policy(ActivationPolicy::Accessory)` in `setup()`. Settings live in the menu-bar tray (`TrayIconBuilder`, `tray-icon` feature).
- **Coordinates:** AX reports global points with a top-left origin on the primary display. Cocoa uses a bottom-left origin. Tauri distinguishes `Logical` from `Physical` sizes. The one conversion function is the pure `geometry::to_overlay_local` in `src-tauri/src/geometry.rs`, outside `macos/` so the coverage gate counts it. `macos/` only supplies the raw AX rects, and `lib.rs` supplies the display values Tauri reports. Its tests use multi-display and Retina fixtures, and the overlay's own frame comes from the same `Display::frame`.
- **Accessibility grant:** TCC ties the grant to the code signature and to the *responsible process*. Under `tauri dev` started from a terminal, the grant goes to the terminal app. An ad-hoc-signed bundle loses the grant on every rebuild. Check with `AXIsProcessTrustedWithOptions` at startup and show a clear prompt instead of failing silently. Test the real flow with a bundle signed by a stable identity.
- **Bundle identifier** (`identifier` in `tauri.conf.json`) is permanent once chosen. Changing it resets the Accessibility grant and moves the app data directory.

## Usage data rules (code-level)

- The launch log is the source of truth (see `SUMMARY.md`). It lives at `app.path().app_data_dir()/launch_log.json`. Write it atomically (temp file in the same directory, then `rename`), so a crash mid-write can't destroy history. `usage::save` removes a stale temp file and creates the new one with `create_new` and mode 0600; do not change that to `File::create`, which would keep an old mode and follow a planted link.
- The file is versioned JSON (`version: 1`, ADR-0009, amended by ADR-0010). It holds `baseline_at` (set at the first run; it marks the first run as done and is not a last-used time), `last_launched`, `first_seen` and `spotlight_seed`. The last two are optional on read, so older files load.
- Every map value and `baseline_at` is read as a `serde_json::Value` and checked on its own. A value that is not a string, a bad bundle ID, an RFC 3339 timestamp that does not parse, or one more than `FUTURE_TOLERANCE` (5 minutes) after now is dropped and logged, and the rest of the file loads. jiff's `serde` feature stays off, so nothing panics. A file that does not decode at all is renamed to `launch_log.json.corrupt`, never deleted.
- When decode drops anything, `load_or_set_aside` copies the bytes it read to `launch_log.json.dropped` before the next save can rewrite the file. It uses the same `write_private` as the temp file (stale file removed, `create_new`, mode 0600).
- `BundleId::new` refuses an empty ID or one over 255 bytes, so the Dock, workspace events and decode all share one cap.
- A `SpotlightSeed` is made only by `usage::spotlight::sane_seeds` (future dates dropped, one date per bundle ID, then every date dropped on a migration batch) or, for a seed already stored, by log decode through `assume_sane`.
- Only the `launch-log` thread in `lib.rs` touches the `UsageState` (the log, the running set and the Dock list). Everything else sends it `WriterMessage::{Used, DockApps, WatchGrowth, Flush}` over its channel. The first-run Spotlight read runs on a short-lived `spotlight-seed` thread, one `mdls` per distinct bundle ID with a 2 s timeout, and returns its readings through its `JoinHandle`. The writer holds no sender to its own inbox.
- **Jitter must be stable across runs and Rust versions.** It is FNV-1a 64 of the bundle ID's UTF-8 bytes, mapped to whole seconds in ±1 day (`growth/jitter.rs`). Never use `std::hash::DefaultHasher` or an RNG. Changing the hash or the mapping moves every user's moss, and the golden-value tests in `growth/jitter/tests.rs` fail by design.
- Test migration detection and the growth curve with table-driven tests over fixed timestamps, including the edge cases from `SUMMARY.md`: null Spotlight dates, 5+ apps within seconds, first-run grace, and apps running at startup.
