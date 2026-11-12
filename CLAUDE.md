# DockMoss

macOS-only Tauri 2 app: a transparent, click-through overlay that grows moss on Dock icons for apps that haven't been used in a while.

- Read `SUMMARY.md` before touching usage tracking or growth logic, and update it when one of those rules changes.
- Read `CONSTRAINTS.md` before writing code. Do not weaken it to make a change pass.
- `ARCHITECTURE.md` has the threads, data flow and source layout. Update it when they change.

## Checks

Run from `src-tauri/`. All four must pass before a change is done:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

Frontend: `pnpm test` (vitest), and `pnpm check:task`, which also fails if `dist/` carries the debug layer. The package manager is **pnpm** only.

Run the app with `pnpm tauri dev`. Quit any other DockMoss first: every copy shares the bundle identifier, so a second copy exits at once.

## The boundary

All objc2, AppKit and AX calls, the `mdls` subprocess, and every `unsafe` block live in `macos/`, which returns plain Rust types. Everything else (`growth/`, `usage/`, `tracker.rs`, `geometry.rs`, `dock.rs`, `settings.rs`, `focus.rs`, `login_item.rs`, `navigation.rs`, `bounded.rs`, `bundle_id.rs`) stays pure: no `tauri`, no `objc2`, time passed in as a parameter, so it is unit-testable with fixed clocks and displays.

## Rust

- **Edition 2024.** Use current std: `LazyLock`/`OnceLock`, let-else, let chains.
- **Errors:** a `thiserror` enum per module. Commands return `Result<T, crate::Error>`, serialized as `{ kind, message }`. `anyhow` only in `setup()` and tests.
- **No panics:** `unwrap`, `expect` and slice indexing are denied. The only `.expect`s outside tests are named startup invariants (`MainThreadMarker` in `setup()`, the Tauri build in `run()`, `build.rs`), each with its own `#[expect(clippy::expect_used, reason = "...")]`.
- **Logging:** `tracing` macros with structured fields. The `private-log-dir` plugin must be registered right after single-instance and before `tauri-plugin-log`, because the folder's 0700 mode is the only guard on the log file.
- **Types:** newtypes for domain values (`BundleId`, `Overgrowth` clamped on construction). Time is `jiff::Timestamp` in UTC.
- **Tests:** one inline `#[cfg(test)] mod tests { ... }` per module, as its last item. No `tests.rs`, no `tests/` folder, no `include!` (`check:fast` refuses them). Frontend tests sit beside their module as `*.test.ts` and test only pure code (no DOM, WebGL or clock).
- **Lints:** set in `Cargo.toml` under `[lints]`. Allow a lint with `#[expect(..., reason = "...")]` at the narrowest scope. Never put one on a statement holding a closure (such as `.setup(|app| ...)`); split it first.
- **Dependencies:** check a crate is maintained and supports Tauri 2. Don't use `tauri-plugin-autostart` (launch at login is `SMAppService`, ADR-0016) or `tauri-plugin-store` (settings are written like the launch log, ADR-0014).

## Unsafe and objc2

- Use the `objc2-*` framework crates, never `cocoa` or `objc`.
- Every `unsafe` block is in `macos/`, as small as the call it wraps, with a `// SAFETY:` comment. The item holding it opts in with `#[expect(unsafe_code, reason = "...")]`.
- AppKit runs on the main thread: take a `MainThreadMarker`, and reach it from elsewhere with `app.run_on_main_thread`.
- AX calls are blocking IPC to the Dock. Never make them on the main thread. Release CF objects through `CFRetained`, never `CFRelease`.
- Every AX read costs the Dock CPU. Read fast only while something moves. Any new reason to poll fast must be bounded in time and go in `tracker.rs`'s `Activity` with table tests (ADR-0012). Only `DockHoverChanged` and `AppLaunching` may cut a wait short, and never to less than one `Fast` interval after the last read (ADR-0007, ADR-0017).
- The `dock-observer` is optional (polling is the fallback), runs on its own `CFRunLoop`, and registers only when `dock::HoverObserverGate` allows.
- Cast `window.ns_window()` to `&NSWindow` only in `macos/overlay.rs`.

## Tauri

- **Commands:** `async fn` with owned arguments, thin, delegating to modules. `std::sync::Mutex` unless a lock crosses an `.await`.
- **Spawning:** `tauri::async_runtime::spawn`, never bare `tokio::spawn` (panics in `setup()`).
- **Rust → webview:** stream over a `tauri::ipc::Channel` handed over by a `watch_*` command. Send a frame only when it changed. A new call replaces the channel and gets the current frame at once. `src/overlay.ts` subscribes once per channel and fans out. Use `emit_to` only for one-off events the webview can miss.
- **Capabilities:** one file per window. A new command needs both its `build.rs` app-manifest entry and a grant in the calling window's capability. `core:window:allow-close` can name any window, so the overlay must keep refusing `CloseRequested`.
- **Frontend:** v2 import paths (`@tauri-apps/api/core`, `/event`). Every `invoke` lives in `src/ipc.ts`. Dev-only code (`src/debug.ts`, `src/icon/`) loads only via `import()` under `import.meta.env.DEV`, never statically; `pnpm build` fails otherwise. The settings entry never imports `debug.ts`, the renderer or `overlay.ts`. Put Dock data into the DOM with `textContent` only.
- **Renderer constants have one source.** Shader constants live in `src/render/model.ts` and are written into `moss.frag.glsl` at `#pragma moss_constants`; never declare one in GLSL. `INSTANCE_ATTRIBUTES` in `model.ts` is pinned to `moss.vert.glsl` by test. All mossy icons are one `drawArraysInstanced` call.
- **The shrivel is inferred** by the webview from the growth stream. Don't add a Rust event for it (ADR-0013).
- **One copy at a time:** `tauri-plugin-single-instance` is registered first. Its callback never logs `argv` or `cwd`.
- **Navigation:** both windows pass `.on_navigation(is_app_url)`.

## macOS gotchas

- **Overlay window:** transparency needs `macOSPrivateApi` (no Mac App Store; Developer ID only). Set the level to `kCGDockWindowLevel + 1` and the collection behavior through objc2; Tauri's `always_on_top` sits below the Dock.
- **Tray:** the app is `ActivationPolicy::Accessory`. Only the main thread touches the tray (`app.tray_by_id(TRAY_ID)` inside `run_on_main_thread`). An Accessory app has no menu bar, so the Settings page handles ⌘W itself.
- **Focus:** closing Settings never leaves a windowless DockMoss in front. Rules are in `focus.rs` (ADR-0014).
- **Coordinates:** AX is top-left global, Cocoa bottom-left. `geometry::to_overlay_local` is the one conversion.
- **Accessibility grant:** under `tauri dev` it goes to the terminal; an ad-hoc-signed bundle loses it on every rebuild. Check with `AXIsProcessTrustedWithOptions` and prompt clearly, never fail silently.
- **Bundle identifier** `app.dockmoss.DockMoss` is permanent. Changing it resets the Accessibility grant and moves the data, logs and single-instance socket. Never hard-code it: read `app.config().identifier`. The only literal is `usage::migrate`'s `OLD_IDENTIFIER`.
- **Launch at login** is `SMAppService.mainApp` in `macos/login_item.rs`. Its state lives in macOS, not `settings.json`. Judge a change by the status read afterwards (`login_item::outcome`), not by the call's result. Only a `.app` may register. Leave the real login item off when testing.

## Privacy

Paths can contain the user's name. Never log or send a path, `argv` or `cwd`. Log an `NSError` only as domain and code. `DockItem::app_path` stays in Rust (`#[serde(skip_serializing)]`). A settings `warn` never contains an excluded bundle ID.

## Usage data

- The launch log is the source of truth. Every data path comes from `DataDir::file` in `lib.rs`, which exists only after the one-time data move.
- Write user data atomically through `usage::store` (stale temp removed, `create_new`, mode 0600, sync, `rename`). Never change it to `File::create`.
- Decode checks each value on its own and drops bad ones; nothing panics (jiff's `serde` feature stays off). A file that doesn't decode is renamed to `.corrupt`, never deleted. When anything is dropped, the bytes read are copied to `.dropped` before the next save.
- DockMoss never records its own bundle ID (`usage::tracked_bundle_id`).
- A `SpotlightSeed` is made only by `spotlight::sane_seeds` or, on decode, `assume_sane`.
- Only the `launch-log` thread touches `UsageState`. Everything else sends it a `WriterMessage`.
- **Jitter must stay stable** across runs and Rust versions: FNV-1a 64 of the bundle ID. Never use `DefaultHasher` or an RNG. Its golden-value tests fail by design if it changes.
- Test migration detection and the growth curve with table tests over fixed timestamps, covering `SUMMARY.md`'s edge cases.

**The one-time data move** (`usage::migrate`, ADR-0016) runs before anything reads app data. It must never overwrite: `hard_link` then `remove_file`, or a `create_new` copy where links are unsupported. Never change it to `rename`. A file that fails to move is left behind with no replacement written, and the next start retries.

**Settings** (ADR-0014):
- Only `save_settings` writes `settings.json`. A save is all or nothing: validate every field through its constructor, write, then replace the in-memory settings.
- Load never fails and never writes: a bad file gives defaults, a bad field its default.
- No settings type derives a `Deserialize` that skips its constructor.
- `save_settings` sends `WriterMessage::Settings` while still holding the settings lock, so saves apply in order. Settings are applied to growth in Rust only.
- `DOCKMOSS_FAST_FORWARD` works in debug builds only and never reads or writes settings.
