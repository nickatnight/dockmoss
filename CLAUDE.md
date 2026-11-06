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

Run the app with `pnpm tauri dev` from the repo root. Quit any other DockMoss first (an installed bundle, a debug bundle, another `tauri dev`): every copy shares the bundle identifier, so a second copy exits at once and the running one opens Settings instead. The package manager is **pnpm** (`pnpm install`, `pnpm add`, `pnpm <script>`), and `pnpm-lock.yaml` is the only lockfile. Frontend checks come from the scripts in `package.json`: `pnpm test` runs vitest over the pure TS (`src/**/*.test.ts`), and `pnpm check:task` also builds the frontend and fails if `dist/` carries the debug layer.

## Layout

```
src-tauri/src/
  main.rs        # one line: dockmoss_lib::run()
  lib.rs         # Builder, plugins (single-instance first, then private-log-dir), setup(),
                 #   command registration, the one-time data move (DataDir), the dock-tracker,
                 #   launch-log (usage) and spotlight-seed thread glue, loading settings,
                 #   the settings window (create or focus, focus hand-back), and what
                 #   opens it besides the tray (a second launch, Reopen)
  error.rs       # crate Error enum (thiserror + Serialize)
  commands.rs    # #[tauri::command] fns: thin, they delegate to modules (watch_dock,
                 #   watch_growth, get_settings, save_settings, set_launch_at_login)
  tracker.rs     # pure: per Dock read -> what to send, log and show; the poll rate,
                 #   Activity (Fast only while something moves), read deadlines, and
                 #   the hover cut (wait_for_read: a Dock hover change ends a long wait,
                 #   no sooner than one Fast interval after the last read)
  geometry.rs    # pure: AX rects -> overlay-local logical pixels (the one conversion),
                 #   and the settings window's height clamped to the work area
  dock.rs        # pure: raw Dock children -> DockItems (application items only),
                 #   ReadGate (no AX read while trust is revoked), and HoverObserverGate
                 #   (when the hover observer may register: trusted, not latched after
                 #   APIDisabled, not refused)
  bundle_id.rs   # pure: BundleId newtype
  settings.rs    # pure apart from std file I/O: Settings (thresholds, intensity,
                 #   exclusions, fast-forward), one decoder per field, load_file (capped,
                 #   per-field fallback), commit (write first, then adopt), applied (the
                 #   dev fast-forward override in place of the saved speed), SettingsView
  login_item.rs  # pure: LoginItemStatus, outcome (judged by the status macOS reports
                 #   after a change), is_app_bundle (only a .app may become a login item)
  bounded.rs     # std only: read_within, one read on its own thread with a deadline
  navigation.rs  # pure: which URLs a window may load (is_app_origin)
  focus.rs       # pure: which activated app Settings hands focus back to
                 #   (is_return_target), and what closing Settings does
                 #   (on_settings_closed, hides)
  growth/        # pure: effective last-used -> 0..1 overgrowth, jitter, stages
  usage/         # launch log model and file format (log.rs), the capped read and atomic
                 #   write both user-data files use (store.rs),
                 #   save and recompute schedule (schedule.rs), running-app filter,
                 #   Spotlight parsing and migration-batch check (spotlight.rs), and
                 #   UsageState (state.rs): first run, first-seen, growth per Dock app
                 #   under the applied settings; migrate.rs: the one-time move out of the
                 #   com.dockmoss.app folder (plan_move pure, move_old_data std I/O)
  macos/         # every objc2 / AppKit / AX call, the `mdls` subprocess, and every
                 #   `unsafe` block; app.rs: the front app, the app at a pid, and
                 #   activating one; login_item.rs: SMAppService.mainApp status and set;
                 #   dock_observer.rs: the dock-observer thread (AXObserver for the
                 #   Dock's AXSelectedChildrenChanged, its own CFRunLoop)
src-tauri/capabilities/   # one capability file per window: overlay.json, settings.json
overlay.html, settings.html   # one Vite entry per window
src/                      # webview: overlay.ts (one subscription per channel, fanned out),
                          #   ipc.ts, debug.ts / debug.css (dev builds only)
  render/                 # baked moss, drawn with WebGL2 (ADR-0018):
                          #   generator.ts (pure: the moss prototype's port, shapes for one
                          #   item at one of 24 level steps: anchors, cushions, fronds,
                          #   spore stalks, and an Overgrown item's mushroom and flower),
                          #   bake.ts (shapes to Canvas2D calls on a software-backed
                          #   OffscreenCanvas), atlas.ts (pure: bake keys, and the atlas
                          #   plan: one texture-array layer per bake, eviction, two bakes
                          #   a frame, an item's held bake kept while its new one waits),
                          #   model.ts (pure: mossy icons, item keys and seeds, art and bake
                          #   boxes, draw rule, shader constants, instance attribute layout,
                          #   and the shrivel: which items start one, its progress, and an
                          #   icon's instance at a progress), renderer.ts (draws on change,
                          #   bakes into the atlas, plus a 30 fps loop for the shrivel,
                          #   behind an injected schedule, bake and MossGl), gl.ts (the
                          #   texture-array atlas and one instanced draw), index.ts (DOM
                          #   wiring), moss.{vert,frag}.glsl
  settings.ts             # the settings form: DOM wiring and ⌘W (lazy window import)
  settings.css
  settings/model.ts       # pure: the next settings from one change, the one-at-a-time
                          #   saver, page feedback and re-announce rules, exclusion rows,
                          #   which day fields are wrong, the messages, and the launch at
                          #   login box (launchAtLoginBox, launchAtLoginShown) and its
                          #   one-at-a-time toggle (createLoginToggle) (model.test.ts)
```

The boundary that matters is between `macos/` and everything else. `growth/`, `usage/`, `settings.rs`, `login_item.rs`, `bounded.rs`, `navigation.rs`, `focus.rs`, `tracker.rs`, `geometry.rs`, `dock.rs` and `bundle_id.rs` stay pure Rust: no `tauri`, no `objc2`, and time passed in as a parameter. That keeps them unit-testable with fixed clocks and fixed displays. `macos/` exposes safe functions that return plain Rust types such as `RawDockChild { subrole, rect, app_path, bundle_id }`, so the rest of the crate never sees a raw pointer.

## Rust conventions

- **Edition 2024**, with `rust-version` pinned in `Cargo.toml`. Use current std: `LazyLock`/`OnceLock`, let-else, let chains, `impl Trait` in argument position.
- **Errors:** each module has a `thiserror` enum. Commands return `Result<T, crate::Error>`, where `Error` implements `Serialize` as `{ kind, message }` so the frontend can branch on `kind`. `anyhow` is fine in `setup()` and in tests.
- **Panics:** use `?` in non-test code. Release builds abort on panic. `unwrap`, `expect` and slice indexing are denied crate-wide (CONSTRAINTS.md Floor). The only `.expect`s outside tests are named startup invariants: the `MainThreadMarker` in `setup()`, the Tauri build in `run()`, and `build.rs`. Each has its own `#[expect(clippy::expect_used, reason = "...")]`, and the reason states the invariant ("tauri runs setup() on the main thread").
- **Logging:** use `tracing` macros with structured fields (`tracing::debug!(bundle_id, days, "growth updated")`), wired up through `tauri-plugin-log`. Logs go to `~/Library/Logs/app.dockmoss.DockMoss/DockMoss.log`. The `private-log-dir` plugin in `lib.rs` makes that folder 0700 (`usage::private_dir`) before the log plugin opens the file, so it is registered right after single-instance and before `tauri-plugin-log` (Tauri sets plugins up in registration order). The folder is the guard: the log plugin has no file-mode option and recreates `DockMoss.log` as 0644 when it rotates.
- **Types:** use newtypes for domain values: `BundleId(String)`, and `Overgrowth(f32)` clamped to 0..=1 when constructed. Store time as `jiff::Timestamp` (UTC) and convert to local time only for display.
- **Tests:** unit tests sit in one inline `#[cfg(test)] mod tests { ... }` per module, as its last item. Guards: `clippy::tests_outside_test_module` (denied) fails a `#[test]` outside a `#[cfg(test)]` module, and `clippy::items_after_test_module` fails an item after the test module. In `check:fast`, a grep allows `#[cfg(test)]` only on the line directly above `mod tests {` and refuses `#![cfg(test)]`, a second grep refuses `include!(`, and a `find` refuses any `tests.rs` and any file under a `tests/` folder in `src-tauri/src`, so a separate test file fails. There is no coverage gate: `pnpm coverage` reports line coverage on demand, with no threshold, and test lines count toward it, so it is a rough guide. Frontend tests sit beside their module as `*.test.ts` and run under vitest; keep what they test pure (no DOM, WebGL or clock), as `src/render/model.ts` and `src/settings/model.ts` are.
- **Lints:** set them in `Cargo.toml` under `[lints]` instead of crate-level attributes. Enable `clippy::pedantic` as warn, plus `unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks`. When you allow a lint, add `#[expect(clippy::..., reason = "...")]` at the narrowest scope. A lint attribute covers everything inside the item or statement it sits on, closures included, so never put one on a statement that holds a closure such as `.setup(|app| ...)`; split the statement first.
- **Dependencies:** check that a new crate is maintained and supports Tauri 2 before you add it. Prefer an official `tauri-plugin-*` when one exists and meets the requirement (log). `tauri-plugin-autostart` does not: its macOS launchers are a LaunchAgent (listed under Allow in the Background, not Open at Login) and AppleScript (prompts for Automation), so launch at login uses `SMAppService` (ADR-0016). Settings do not use `tauri-plugin-store`: they are written like the launch log (ADR-0014).

## Unsafe and objc2

- Use the `objc2` framework crates (`objc2-app-kit`, `objc2-foundation`, `objc2-application-services`, `objc2-core-foundation`, `objc2-core-graphics`, `objc2-service-management`). The older `cocoa` and `objc` crates are deprecated, so leave them out.
- Every `unsafe` block lives in `macos/` and has a `// SAFETY:` comment stating the invariant it depends on. Keep each block as small as the call it wraps. `unsafe_code` is denied crate-wide, so the item holding the block opts in with `#[expect(unsafe_code, reason = "...")]`.
- **AppKit runs on the main thread.** Take a `MainThreadMarker` in any function that touches `NSWindow`/`NSWorkspace`, and reach the main thread from elsewhere with `app.run_on_main_thread(...)`.
- **AX calls are synchronous IPC to the Dock process** and can block. Run Dock polling on a dedicated thread, or with `tauri::async_runtime::spawn_blocking`, and never on the main thread. Release CF objects through `CFRetained`/RAII wrappers instead of calling `CFRelease` by hand.
- **Every AX read costs the Dock process CPU** (about 0.4% at 2 Hz). The tracker reads at 60 Hz only while something moves: the pointer near the Dock, a frame sent within 250 ms, or a pointer waiting at a hidden Dock's edge (1 s after its last move). A pointer at rest in the band gets pointer-only checks at 10 Hz, with no AX read. Keep any new reason to poll fast bounded in time, and put it in `tracker.rs`'s `Activity` with table tests (ADR-0012).
- **A Dock hover change ends a long wait** (ADR-0017). The Dock posts no move or resize notification for its items, only `AXSelectedChildrenChanged` when the hovered item changes (entry, each item crossed, exit). So reads stay polled, and the notification only starts them early.
  - The `dock-observer` thread (`macos/dock_observer.rs`) registers for it on its own `CFRunLoop`, never on the main thread, and sends `TrackerMessage::DockHoverChanged`. It is optional: the tracker starts first, and polling is the fallback.
  - Whether it may register is decided by the pure `dock::HoverObserverGate`: the cached `TrustState` and a fresh, no-prompt `check_trust`, a latch after `APIDisabled` until trust is seen revoked, and a stop after `NotificationUnsupported`. Attempts are at least 2 s apart.
  - In `Activity::wait_for_read`, a hover ends a wait longer than one `Fast` interval, but never makes a read due sooner than one `Fast` interval after the last read. That caps hover-driven reads at 60 a second, two per selection change while the pointer is still, and adds no reason to stay at `Fast`.
  - It is the only `TrackerMessage` that cuts the wait short (ADR-0007, amended).
- Get the native window with `window.ns_window()` and cast it to `&NSWindow` inside `macos/overlay.rs` only.

## Tauri 2 conventions

- **Commands:** use `async fn` with owned arguments. Shared state goes in through `app.manage(...)` and comes out as `State<'_, T>`. Use `std::sync::Mutex` when the lock never crosses an `.await`; use `tokio::sync::Mutex` only when it does.
- **Spawning:** use `tauri::async_runtime::spawn`. A bare `tokio::spawn` panics inside `setup()` because no runtime context exists there yet.
- **Rust → webview:** stream state the webview needs from its first frame over a `tauri::ipc::Channel` that a `watch_*` command hands to the owning thread: Dock geometry with `watch_dock`, and each Dock app's growth with `watch_growth`. Send a frame only when it differs from the last one sent, so the webview render loop can stay idle (see Battery in `SUMMARY.md`). A new call replaces the channel and gets the current frame at once, so a reloaded webview resubscribes (ADR-0007). For the same reason `src/overlay.ts` subscribes once per channel and passes each frame to every consumer (the renderer, and the debug layer in dev builds). Send only what the webview draws: `DockItem::app_path` stays in Rust (`#[serde(skip_serializing)]`), because a path can hold the user's name. An event sent before the webview's listener exists is lost, so use `Emitter::emit_to("overlay", ...)` only for one-off events the webview can afford to miss.
- **Capabilities:** each window gets its own file in `capabilities/`, scoped by window label.
  - The overlay webview only renders, so it gets the smallest permission set that lets it receive events, plus `allow-watch-dock` and `allow-watch-growth` (the commands that hand Rust its `Channel`s).
  - The settings window gets `allow-get-settings`, `allow-save-settings`, `allow-set-launch-at-login` and `core:window:allow-close`, and no plugin permissions: Rust validates and writes every setting (ADR-0014) and changes the login item (ADR-0016).
  - `core:window:allow-close` lets a webview name any window's label, so the overlay refuses `CloseRequested` (`prevent_close`). Quitting goes through `app.exit`, which does not send it.
  - Neither window can call the other's commands. App commands are listed in the app manifest in `build.rs`, which makes Tauri generate an `allow-<command>` permission for each one and deny any app command in a window whose capability doesn't grant it. A new command needs both its `build.rs` entry and a grant in the capability of the window that calls it.
- **Frontend:** import from `@tauri-apps/api/core` / `/event` (v2 paths). Keep every `invoke` call in one typed module (`src/ipc.ts`) that mirrors the Rust command signatures. Dev-only code (the Dock item outlines, growth labels and `VITE_DOCKMOSS_OVERGROWTH`) lives in `src/debug.ts` and loads only through `import()` under `import.meta.env.DEV`, so `pnpm build` drops it; never import it statically (ADR-0011). A build-only Vite plugin fails `pnpm build` if `src/debug.ts` is anywhere in the module graph. The settings entry never imports `debug.ts`, the renderer or `overlay.ts`. Put Dock data into the DOM with `textContent` only.
- **Renderer constants have one source.** The moss's palette, layers, anchor layout, props and its 24 level steps live in `src/render/generator.ts`; the bake size and the two-bakes-a-frame budget in `atlas.ts`. The shader's constants (`MIP_LOD_BIAS`, `SHRIVEL_FRONT_WIDTH`, `WITHER_TINT`) live in `src/render/model.ts`, and `glslConstants()` writes them into `moss.frag.glsl` at `#pragma moss_constants`; never declare one again in GLSL, and its tests fail a constant the shader uses but nobody generated, or one generated that the shader does not use. The per-instance buffer is `INSTANCE_ATTRIBUTES` in `model.ts`: the bake box, the atlas layer, the wither and what a shrivel leaves (`a_box`, `a_layer`, `a_wither`, `a_remaining`), and its test pins it to `moss.vert.glsl`'s `in` list. Every mossy icon is one instance of one `drawArraysInstanced` call that samples its layer of one texture array (ADR-0012, ADR-0018), shrivelling ones included.
- **One DockMoss at a time** (ADR-0015). `tauri-plugin-single-instance` is registered first in the Builder. A second launch reaches the running copy's socket (`/tmp/app_dockmoss_DockMoss_si.sock`, derived from the identifier) and exits 0 inside the plugin's setup, before any other plugin and before `setup()` opens a file, builds the tray or creates a window, so it leaves no log of its own. The running copy opens or focuses Settings on a second launch (the plugin callback, sent to the main thread) and on `RunEvent::Reopen`, which is what a Finder double-click, Spotlight or `open` sends to a running app. The callback never logs its `argv` or `cwd`, which can hold the user's name. DockMoss never records its own bundle ID as used, so a second copy starting up is not a launch (`usage::tracked_bundle_id`).
- **Navigation guard:** both window builders pass `.on_navigation(is_app_url)`, which allows only the bundled `tauri:` origin, plus localhost in debug builds (`navigation::is_app_origin`, table-tested).
- **The shrivel is inferred, not sent.** The webview starts one when an icon it drew as mossy comes back clean in the growth stream; Rust sends no event for it (ADR-0013). It draws from the icon's bake, and the shader browns it (`WITHER_TINT`) while a front eases down over it to the bottom edge (ADR-0018). So any drop of a mossy icon to 0 shrivels, including one a setting causes: an exclusion, intensity 0, or higher thresholds (ADR-0016). Its 600 ms run is one more condition of `createRenderer`'s time-limited loop, and the loop stops when the last shrivel draws the icon clean.

## macOS overlay gotchas

- **Transparency** needs `"macOSPrivateApi": true` under `app` in `tauri.conf.json`, plus `transparent: true` on the window and a transparent `body` background in CSS. Private API rules out the Mac App Store. Distribution is Developer ID only.
- **Overlay window:** `decorations: false`, `shadow: false`, `focus: false`, `skipTaskbar: true`, then `set_ignore_cursor_events(true)` and `set_visible_on_all_workspaces(true)`. Set the `NSWindow` level above the Dock (`kCGDockWindowLevel + 1`) and the collection behavior to `canJoinAllSpaces | stationary | fullScreenAuxiliary | ignoresCycle` through objc2. Tauri's `always_on_top` level sits below the Dock.
- **No Dock icon of our own:** call `app.set_activation_policy(ActivationPolicy::Accessory)` in `setup()`. The menu-bar tray (`TrayIconBuilder`, `tray-icon` feature) holds Settings… and Quit DockMoss, plus Grant Accessibility Access… while untrusted. Only the main thread touches the tray: other threads look it up with `app.tray_by_id(TRAY_ID)` inside `run_on_main_thread`, because its handle is not thread-safe.
- **Settings window:** a normal titled window (label `settings`), created on Settings… and destroyed on close. It is 440 × 760 pt, or shorter to fit the primary display's work area, and then the form scrolls (`geometry::settings_window_height`). `set_focus()` already activates the Accessory app, so no extra activation call is needed. An Accessory app has no menu bar, so the page handles ⌘W itself.
- **Focus hand-back on close** (ADR-0014, amended). The return target has one source: `LastOtherApp`, the last activated regular app that is neither this process nor another copy of DockMoss (`focus::is_return_target`, fed by `macos::workspace::on_app_activated` and seeded with the front app at startup). Nothing is captured when Settings opens, so the tray, a second launch and Reopen all get the right target even when DockMoss or an exiting copy is already in front. When the window is destroyed, `focus::on_settings_closed` decides:
  - DockMoss is not the active app: leave the user where they are, so closing Settings from behind never pulls an old app forward.
  - The same bundle ID still runs at the remembered pid (`macos::app::app_at`): activate it.
  - Otherwise (no target, it quit, or macOS gave its pid to another app), or if the activation fails (`focus::hides`): hide DockMoss, so macOS brings the next app forward. A windowless DockMoss is never left in front. The overlay sets `canHide = false` (`macos/overlay.rs`), so hiding the app leaves the moss on screen.
- **Coordinates:** AX reports global points with a top-left origin on the primary display. Cocoa uses a bottom-left origin. Tauri distinguishes `Logical` from `Physical` sizes. The one conversion function is the pure `geometry::to_overlay_local` in `src-tauri/src/geometry.rs`, outside `macos/` so it can be unit-tested with fixed displays. `macos/` only supplies the raw AX rects, and `lib.rs` supplies the display values Tauri reports. Its tests use multi-display and Retina fixtures, and the overlay's own frame comes from the same `Display::frame`.
- **Accessibility grant:** TCC ties the grant to the code signature and to the *responsible process*. Under `tauri dev` started from a terminal, the grant goes to the terminal app. An ad-hoc-signed bundle loses the grant on every rebuild. Check with `AXIsProcessTrustedWithOptions` at startup and show a clear prompt instead of failing silently. Test the real flow with a bundle signed by a stable identity.
- **Bundle identifier** (`identifier` in `tauri.conf.json`) is `app.dockmoss.DockMoss` (job 014), and it is permanent from now on. Changing it resets the Accessibility grant (a new TCC entry), moves the app data and log folders, and moves the single-instance socket, so an old-identifier copy can run beside the new one. Code never hard-codes it: own-ID checks read `app.config().identifier`. The only literal is `usage::migrate`'s `OLD_IDENTIFIER` (`com.dockmoss.app`), the folder the one-time move reads (see "Usage data rules").
- **Launch at login** (ADR-0016) is `SMAppService.mainApp` through `objc2-service-management`, in `macos/login_item.rs`. DockMoss then shows under System Settings → General → Login Items → Open at Login, with no Automation or other prompt. The state lives in macOS, not in `settings.json`: `get_settings` reads it as `launchAtLogin` (below), and `set_launch_at_login(enabled)` returns what macOS reports after the change (`login_item::outcome`), or `login_item_not_changed` when that differs from what was asked. Judging by the status read afterwards matters: unregistering an item that was never registered fails, yet ends in the asked state. `set` refuses unless the main bundle's path ends in `.app` (`login_item::is_app_bundle`), so `tauri dev`'s bare binary is never registered and the toggle reports the failure. An `NSError` is kept and logged only as its domain and code (`RedactedNsError`), because its description can name a path. Leave the real login item off when you test it.
- **Release bundle:** `pnpm tauri build` builds an unsigned `DockMoss.app` only (`bundle.targets` is `["app"]`; DMG, signing and notarization are milestone 7), with release `opt-level = "s"`. No `Info.plist` is added: macOS has no usage-description key for Accessibility.

## Usage data rules (code-level)

- The launch log is the source of truth (see `SUMMARY.md`). It lives at `app.path().app_data_dir()/launch_log.json` (`~/Library/Application Support/app.dockmoss.DockMoss/`). Every data path comes from `DataDir::file` in `lib.rs`, and only `moved_app_data_dir` makes a `DataDir`, after the one-time move, so nothing can read or write app data before the move. Write it atomically (temp file in the same directory, then `rename`), so a crash mid-write can't destroy history. `usage::save` removes a stale temp file and creates the new one with `create_new` and mode 0600; do not change that to `File::create`, which would keep an old mode and follow a planted link.
- The file is versioned JSON (`version: 1`, ADR-0009, amended by ADR-0010). It holds `baseline_at` (set at the first run; it marks the first run as done and is not a last-used time), `last_launched`, `first_seen` and `spotlight_seed`. The last two are optional on read, so older files load.
- Every map value and `baseline_at` is read as a `serde_json::Value` and checked on its own. A value that is not a string, a bad bundle ID, an RFC 3339 timestamp that does not parse, or one more than `FUTURE_TOLERANCE` (5 minutes) after now is dropped and logged, and the rest of the file loads. jiff's `serde` feature stays off, so nothing panics. A file that does not decode at all is renamed to `launch_log.json.corrupt`, never deleted.
- When decode drops anything, `load_or_set_aside` copies the bytes it read to `launch_log.json.dropped` before the next save can rewrite the file. It uses the same `write_private` as the temp file (stale file removed, `create_new`, mode 0600).
- DockMoss never records its own bundle ID: `usage::tracked_bundle_id` drops it, so another copy of DockMoss (a second launch is a regular app until it exits) is never recorded as used. Lookalike IDs (`app.dockmoss.DockMoss.helper`) are other apps and are recorded, and so is a copy built with the old identifier `com.dockmoss.app`.
- `BundleId::new` refuses an empty ID or one over 255 bytes, so the Dock, workspace events and decode all share one cap.
- A `SpotlightSeed` is made only by `usage::spotlight::sane_seeds` (future dates dropped, one date per bundle ID, then every date dropped on a migration batch) or, for a seed already stored, by log decode through `assume_sane`.
- Only the `launch-log` thread in `lib.rs` touches the `UsageState` (the log, the running set and the Dock list). Everything else sends it `WriterMessage::{Used, DockApps, ListDockApps, WatchGrowth, Settings, Flush}` over its channel. `ListDockApps` carries a reply sender; `get_settings` waits for the reply on a `spawn_blocking` thread for up to 500 ms and treats no reply as an unknown Dock list. The first-run Spotlight read runs on a short-lived `spotlight-seed` thread, one `mdls` per distinct bundle ID with a 2 s timeout, and returns its readings through its `JoinHandle`. The writer holds no sender to its own inbox.
- **Jitter must be stable across runs and Rust versions.** It is FNV-1a 64 of the bundle ID's UTF-8 bytes, mapped to whole seconds in ±1 day (`growth/jitter.rs`). Never use `std::hash::DefaultHasher` or an RNG. Changing the hash or the mapping moves every user's moss, and the golden-value tests in `growth/jitter.rs`'s test module fail by design.
- Test migration detection and the growth curve with table-driven tests over fixed timestamps, including the edge cases from `SUMMARY.md`: null Spotlight dates, 5+ apps within seconds, first-run grace, and apps running at startup.

**The one-time data move** (job 014, ADR-0016):
- At startup, before `load_settings` and `start_launch_log`, `usage::move_old_data` moves the names in `usage::migrate::MOVED_FILES` (`launch_log.json`, `settings.json`, `launch_log.json.corrupt`, `launch_log.json.dropped`) from the sibling `com.dockmoss.app` folder into the new one. It never moves a `.tmp` or any other name. `LAUNCH_LOG_FILE` and `SETTINGS_FILE` are defined there and imported by `lib.rs`, so the guarded names have one source.
- Each move is `fs::hard_link`, then `fs::remove_file` of the old name. The link fails with `AlreadyExists` when the name is taken, so the move never overwrites. Only a regular file moves (`symlink_metadata`); a link or anything else stays, with a `warn`. A name already in the new folder is kept and the old file left. The new folder is made 0700 by `usage::private_dir`, which refuses a link. If it is not usable, every planned move counts as failed. The old folder is removed only when empty (`fs::remove_dir`). Do not change the move to `rename`, which replaces an existing name.
- Log lines name the file and the `io::ErrorKind`, never a path. A start with nothing to do logs only at `debug` (`MoveReport::worth_reporting`).
- A file whose move failed is **left behind**: `MoveReport::file_in` gives no path for it. With the launch log left behind, usage tracking stays off for that session; with `settings.json` left behind, settings are the defaults and saving is off. Nothing writes a replacement, which the next start would keep while the user's file stayed stranded, and the next start retries the move. The `.corrupt` and `.dropped` copies are not guarded: they are never read back.
- Known limits (ADR-0016): on a volume without hard links every move fails, so every start runs without usage tracking and only a `warn` says so. A failed `remove_file` after a successful link leaves a frozen copy in the old folder, which keeps that folder from being removed. The old log folder `~/Library/Logs/com.dockmoss.app` is left as it is.

**Settings file** (ADR-0014):
- Rust owns settings. The webview reads them with `get_settings` (settings, `limits` from the Rust constants, the Dock apps by name, or `null` while the Dock list is unknown, and `launchAtLogin` from macOS, or `null` when it does not answer within 500 ms) and writes them only through `save_settings`. The login status is read alongside the Dock list through `bounded::read_within`, so a stalled ServiceManagement disables only that box; the page then says why (`launchAtLoginBox`). There is no `tauri-plugin-store` and no store permission.
- `settings.json` lives in `app_data_dir`. It is versioned JSON (`version: 1`) with one key per setting in the webview's camelCase shape: `thresholds`, `intensityPercent`, `excluded` (sorted) and `fastForward`.
- Only `save_settings` writes it, through `usage::write_atomically`: the same `write_private` temp file as the launch log (stale file removed, `create_new`, mode 0600), synced, then `rename`. Nothing writes it at startup or at quit.
- Save is all or nothing. `Settings::parse` checks every field through its constructor (`Thresholds::new`, `Intensity::new`, `FastForward::from_factor`, `BundleId::new`, at most 256 exclusions, and lists over 4 × 256 entries refused before reading). Then `settings::commit` writes, and only after the write succeeds replaces the settings in memory. A failed write returns `settings_not_saved` with a fixed message; the cause, which can hold a path, is logged. The managed `Mutex<Settings>` is held through the write, on a `spawn_blocking` thread.
- Load never fails and never writes. `settings::load_file` reads through `usage::read_capped` at 256 KiB (`settings::MAX_FILE_BYTES`).
  - A missing file gives defaults silently.
  - An unreadable or oversized file, one that is not a JSON object, or one without `version: 1` (`{}` included) gives defaults with one `warn`.
  - Otherwise each field is checked on its own: a missing or invalid field gets its default with a `warn` naming the field, and a bad excluded bundle ID drops only that entry.
  - A `warn` never contains an excluded bundle ID.
  - A bad file stays on disk until the user's next save replaces it.
- No settings type derives a `Deserialize` that skips its constructor.
- Saved settings apply at once (ADR-0016). After `settings::commit` writes, `save_settings` sends `WriterMessage::Settings` to the launch-log thread while it still holds the settings lock (`apply_to_growth` takes the guard), so saves apply in the order they were written. The thread recomputes at once, resets the recompute timer for the new fast-forward, and sends a growth frame only if it changed. While usage tracking is off, the save persists and the send is skipped with a `debug`.
- `UsageState::growth(now, &Settings)` applies them, in Rust only; the webview, renderer and shaders are unchanged.
  - The thresholds and fast-forward drive the curve.
  - An excluded Dock app keeps its entry, with overgrowth 0 and stage `clean`.
  - Intensity scales the sent overgrowth (`Intensity::scale`, clamped through `Overgrowth::new`) and leaves the stage, so an Overgrown app keeps its props at any intensity above 0. Intensity 0 draws no moss.
- In debug builds, a valid `DOCKMOSS_FAST_FORWARD` replaces the saved speed (`Settings::applied`). An invalid one is ignored with a `warn`, and the saved speed applies. Release builds ignore the variable. The dev overrides (`DOCKMOSS_FAST_FORWARD`, `VITE_DOCKMOSS_*`) never read or write settings.
