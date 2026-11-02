# Dock Moss

A macOS menu-bar app that grows moss on Dock icons for apps you haven't opened in a while. The longer an app sits unused, the more overgrown its icon gets. Opening the app clears the moss. Product rules and the last-used algorithm are in [SUMMARY.md](SUMMARY.md).

## Status

MVP slice 2 of 13 (the overlay window). The app starts with no Dock icon and puts a moss-tile glyph in the menu bar. The tray menu has one item, Quit Dock Moss. A transparent, click-through window covers the main display one level above the Dock, on every Space. For now it draws only a 2 px magenta debug band around the bottom 120 px; there is no moss yet. Clicks, hovers and magnification pass through to the Dock.

## Requirements

- macOS 15 or later (the only platform; the bundle's `minimumSystemVersion` is 15.0)
- Rust 1.99 or later
- Node 22 and pnpm
- For `pnpm check:task`: `cargo install cargo-llvm-cov cargo-deny` and gitleaks 8.30.1. On first run, cargo-llvm-cov installs the `llvm-tools-preview` rustup component.

## Quick start

```sh
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` starts Vite on port 5110, then builds and runs the app. Quit it from the moss tile in the menu bar.

`tauri dev` loads the page straight from Vite, so the Content Security Policy in `tauri.conf.json` is not applied there. To run with the real CSP and bundled assets, build with `pnpm tauri build --debug --no-bundle` and run `src-tauri/target/debug/dock-moss`.

## Commands

| Command | What it runs |
|---|---|
| `pnpm tauri dev` | The app, with Vite on `http://localhost:5110` |
| `pnpm check` | Same as `check:fast` |
| `pnpm check:fast` | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, two grep guards (unit tests only in `tests.rs`, `unsafe_code` opt-ins only in `src/macos/`), `tsc --noEmit` |
| `pnpm check:task` | `check:fast`, then tests, the 80% line-coverage gate, `cargo deny`, `pnpm audit` and three gitleaks passes |

[CONSTRAINTS.md](CONSTRAINTS.md) defines what each check enforces. If the scripts and that file disagree, the file wins.

## Layout

```
src-tauri/src/
  main.rs, lib.rs   # entry point; Tauri builder, plugins, setup()
  error.rs          # crate Error, serialized as { kind, message }
  commands.rs       # Tauri commands (none yet)
  tray.rs           # menu-bar icon and Quit
  growth/           # pure growth model (slice 6)
  usage/            # launch log and Spotlight seed (slices 7-8)
  macos/            # all objc2 / AppKit / AX code and every unsafe block
    overlay.rs      # raises the overlay NSWindow above the Dock, on every Space
src-tauri/capabilities/
  overlay.json      # overlay window may only listen for events
src-tauri/icons/
  source/           # SVG masters for the app icon and the tray glyph
overlay.html        # Vite entry for the overlay window
src/                # webview frontend: overlay.ts / overlay.css (renderer from slice 9)
```

## Icons

The bundle icons and the tray glyph are generated from the SVG masters in `src-tauri/icons/source/`:

```sh
pnpm tauri icon src-tauri/icons/source/app-icon.svg
# then delete the non-macOS outputs (android/, ios/, icon.ico, Square*Logo.png, StoreLogo.png, 64x64.png)

pnpm tauri icon src-tauri/icons/source/tray.svg --png 44 -o src-tauri/icons
mv src-tauri/icons/44x44.png src-tauri/icons/tray.png
```

The tray glyph is a template image: pure black plus alpha, which macOS tints for light and dark menu bars.

## Docs

- [CLAUDE.md](CLAUDE.md): engineering conventions (Rust, Tauri, objc2, overlay gotchas)
- [CONSTRAINTS.md](CONSTRAINTS.md): the quality bar and the checks that enforce it
