# ADR-0001: Tauri 2 with objc2, and every `unsafe` block in `macos/`

## Status

Accepted at Gate 1, 2026-11-02. Slice 2 (overlay window) tests the riskiest part. If the overlay cannot sit above the Dock, slice 2 stops as BLOCKED and the human decides on the Electron fallback, which would supersede this ADR.

## Date

2026-11-02

## Context

Dock Moss draws on top of the macOS Dock. It needs:

- a transparent, click-through window above the Dock on every Space, raised to `kCGDockWindowLevel + 1`. Tauri's `always_on_top` level sits below the Dock, so this needs AppKit calls Tauri doesn't expose.
- Dock icon geometry, read from the Dock process through the Accessibility API (`AXUIElement`).
- `NSWorkspace` launch and activation notifications for the launch log.
- no Dock icon of its own, with settings in a menu-bar tray.
- an idle cost low enough for the "Battery" rule in `SUMMARY.md`.

Each of those native calls is `unsafe` FFI. Spread across the crate, they would put raw pointers and main-thread rules next to the growth logic.

## Decision

- **Shell:** Tauri 2 on Rust edition 2024, with a webview for drawing (see ADR-0002). macOS only, with no cross-platform `cfg` scaffolding.
- **Native calls:** the `objc2` framework crates (`objc2-app-kit`, `objc2-foundation`, `objc2-application-services`, `objc2-core-foundation`).
- **One module for unsafe code:** every objc2, AppKit and AX call, and every `unsafe` block, lives in `src-tauri/src/macos/`. `macos/` exposes safe functions that return plain Rust types such as `DockItem { bundle_id, rect }`.
- **Enforcement** (in `Cargo.toml` and `CONSTRAINTS.md`):
  - `unsafe_code` is denied crate-wide, so an item in `macos/` opts in with `#[expect(unsafe_code, reason = "...")]`.
  - `clippy::undocumented_unsafe_blocks` is denied, so every block needs a `// SAFETY:` comment.
  - `unsafe_op_in_unsafe_fn` is on.
- **Menu-bar only:** the app uses `ActivationPolicy::Accessory` and a `TrayIconBuilder` tray. Slice 1 ships this.

## Alternatives considered

### Electron with a small native helper for Dock positions
- Pros: `SUMMARY.md` names it as the fallback if Tauri's transparent overlay proves fiddly.
- Cons: it ships a full Chromium runtime and adds a second process to keep in sync with the helper.
- Status: not pursued unless slice 2 fails. The human makes that call.

### The `cocoa` and `objc` crates
- Cons: both are deprecated (CLAUDE.md).
- Rejected in favour of the `objc2` framework crates.

### Allowing `unsafe` wherever it is needed
- Cons: raw pointers and main-thread rules would reach `growth/` and `usage/`, which must stay pure and testable (ADR-0003).
- Rejected. The `unsafe_code` deny lint plus the opt-in makes every exception visible in review.

## Consequences

- Transparency needs `"macOSPrivateApi": true`, which rules out the Mac App Store. Distribution is Developer ID only.
- The Accessibility grant is tied to the code signature and the responsible process. Under `tauri dev` the grant goes to the terminal, and an ad-hoc-signed bundle loses the grant on every rebuild. A stable signing identity is needed to test the real flow (milestone 7, out of MVP scope).
- AppKit calls run on the main thread, with a `MainThreadMarker`. AX calls block, so Dock polling runs on its own thread, never the main thread.
- `bundle.identifier` (`com.dockmoss.app`) is permanent once users have the app: changing it resets the Accessibility grant and moves the app data directory.
- The `unsafe_code` opt-in can still be written outside `macos/`. Until a `check:fast` grep guard lands (follow-up for job 002 in `docs/mvp/plan.md`), review is the only thing that keeps it inside `macos/`.
