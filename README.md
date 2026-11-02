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

`tauri dev` loads the page straight from Vite, so the Content Security Policy in `tauri.conf.json` is not applied there. To run with the real CSP and bundled assets, build with `pnpm tauri build --debug --no-bundle` and run `src-tauri/target/debug/dock-moss`. That debug build has the Web Inspector enabled, so use it only on your own machine and never hand it to anyone.

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

After any change to `tray.svg`, decode the alpha channel of the 44×44 `tray.png` and check that:

- the glyph's bounding box spans at least 40 px on its longer axis, with at most 2 px of padding on that axis;
- it is centred within 1 px on both axes;
- every visible pixel is black (0,0,0);
- no stroke, gap or notch is thinner than 3 px, so a 3×3 morphological opening and a 3×3 closing each change no pixel;
- each valley in the moss edge is at least 4 px below the tuft tops on both sides (tuft relief).

The current glyph sits exactly on the relief floor. In `tray.svg` the centre tuft tops out at y=24, the outer shoulders at y=25 and the valley floors at y=29, so the shoulder side gives exactly 4 px. Any edit that lowers a shoulder or raises a floor fails the check. If the relief has to grow, raise the tops rather than lowering the floors, so the moss stays in the bottom third. Raising the centre alone does not help, because the shoulder side is the shallower one; move the centre to y=23 and the shoulders to y=24 together, which gives 5 px and keeps the centre highest. The tuft arcs are tangent to 3 px valley fillets, so recompute the tangent points instead of nudging single coordinates.

`pnpm tauri icon` writes the chunks of `icon.icns` in a different order on each run, so a regenerated `icon.icns` differs byte-for-byte even when the artwork is identical. Commit it only when `app-icon.svg` changed.

## Docs

- [CLAUDE.md](CLAUDE.md): engineering conventions (Rust, Tauri, objc2, overlay gotchas)
- [CONSTRAINTS.md](CONSTRAINTS.md): the quality bar and the checks that enforce it
