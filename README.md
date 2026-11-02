# Dock Moss

A macOS menu-bar app that grows moss on Dock icons for apps you haven't opened in a while. The longer an app sits unused, the more overgrown its icon gets. Opening the app clears the moss. Product rules and the last-used algorithm are in [SUMMARY.md](SUMMARY.md).

## Status

MVP slice 1 of 13 (the scaffold). The app starts with no Dock icon and puts a leaf in the menu bar. The tray menu has one item, Quit Dock Moss. There is no overlay and no moss yet.

## Requirements

- macOS (the only platform)
- Rust 1.99 or later
- Node 22 and pnpm
- For `pnpm check:task`: `cargo install cargo-llvm-cov cargo-deny` and gitleaks 8.30.1. On first run, cargo-llvm-cov installs the `llvm-tools-preview` rustup component.

## Quick start

```sh
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` starts Vite on port 5110, then builds and runs the app. Quit it from the leaf in the menu bar.

## Commands

| Command | What it runs |
|---|---|
| `pnpm tauri dev` | The app, with Vite on `http://localhost:5110` |
| `pnpm check` | Same as `check:fast` |
| `pnpm check:fast` | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `tsc --noEmit` |
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
src/                # webview frontend (renderer from slice 9)
```

## Docs

- [CLAUDE.md](CLAUDE.md): engineering conventions (Rust, Tauri, objc2, overlay gotchas)
- [CONSTRAINTS.md](CONSTRAINTS.md): the quality bar and the checks that enforce it
