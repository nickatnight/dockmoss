<div align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="128" height="128" alt="DockMoss app icon: a grey stone tile with moss growing up from its lower edge">
  <h1>DockMoss</h1>
  <p>A macOS menu-bar app that grows moss on Dock icons for apps you haven't opened in a while.<br>
  The longer an app sits unused, the more overgrown its icon gets. Open the app and the moss shrivels away.</p>
  <p>
    <a href="https://github.com/nickatnight/dockmoss/blob/main/package.json"><img src="https://img.shields.io/github/package-json/v/nickatnight/dockmoss" alt="Version"></a>
    <a href="https://github.com/nickatnight/dockmoss/actions/workflows/ci.yml"><img src="https://github.com/nickatnight/dockmoss/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI status"></a>
    <a href="https://codecov.io/gh/nickatnight/dockmoss"><img src="https://codecov.io/gh/nickatnight/dockmoss/graph/badge.svg?branch=main" alt="Rust line coverage"></a>
    <a href="LICENSE"><img src="https://img.shields.io/github/license/nickatnight/dockmoss" alt="Licence: MIT"></a>
    <a href="#requirements"><img src="https://img.shields.io/badge/macOS-15%2B-3E5B4C" alt="Platform: macOS 15 or later"></a>
  </p>
  <p><img src="docs/readme/banner.svg" width="900" alt="Animated illustration: a laptop on a kitchen counter with six app tiles in its Dock. As a wall calendar turns from May 1 to May 30, moss creeps over the tiles of unused apps, stage by stage, until the Notes tile is overgrown with a mushroom and a flower. The pointer opens Notes: its tile bounces, the moss browns and shrivels away, and the Notes window opens."></p>
</div>

**Status:** MVP complete (slice 13 of 13). Builds are unsigned; signing, notarization and a DMG are milestone 7.

## Requirements

- macOS 15 or later
- Rust 1.99+, Node 22, pnpm
- Optional: `cargo-deny` and gitleaks 8.30.1 for `pnpm check:task`, `cargo-llvm-cov` for `pnpm coverage`

## Quick start

```sh
pnpm install
pnpm tauri dev
```

Quit any other DockMoss first (an installed copy, a debug bundle, another `tauri dev`). Only one copy runs at a time: a second one hands over to the running copy, which opens Settings, and exits.

DockMoss has no Dock icon. It lives in the menu bar as a moss tile, with Settings… and Quit DockMoss.

## Accessibility access

DockMoss reads the Dock through Accessibility. Which app needs the grant depends on how you start it:

- **`pnpm tauri dev`:** your terminal app.
- **An app bundle:** DockMoss itself. Build one with `pnpm tauri build --debug --bundles app` and run `open src-tauri/target/debug/bundle/macos/DockMoss.app`. macOS asks once, and the menu offers Grant Accessibility Access… until it's granted.

Local builds are ad-hoc signed, so each rebuild counts as a new app and needs a new grant. If the old DockMoss entry in System Settings → Privacy & Security → Accessibility does nothing, remove it with − and relaunch. Granting or revoking takes effect within 5 s, no restart.

## Settings

Open **Settings…** from the menu bar. Every change saves as you make it and reaches the Dock within a second.

| Setting | Default | What it does |
|---|---|---|
| Growth stages | 5 / 10 / 30 days | Days unused before Fuzz, Patchy and Overgrown. Each 2–3650 and longer than the one before. |
| Intensity | 100% | How much of each icon the moss covers. 0% draws none. |
| Fast-forward | 1× | 1×, 10×, 100× or 1000×, to preview growth. |
| Excluded apps | none | Apps that never get moss (up to 256). |
| Launch at login | off | Stored by macOS under System Settings → General → Login Items. Only works from an app bundle. |

## Your data

Everything lives in `~/Library/Application Support/app.dockmoss.DockMoss/`:

- `launch_log.json`: when each app was last used. Quit DockMoss and delete it to start over.
- `settings.json`: quit and delete it to reset every setting.
- `launch_log.json.corrupt`, `launch_log.json.dropped`: copies kept when a file or entry couldn't be read, for recovery by hand. Safe to delete.

Logs go to `~/Library/Logs/app.dockmoss.DockMoss/DockMoss.log`.

Builds before job 014 used the identifier `com.dockmoss.app`. A newer build moves that data over on its first start; quit the old build before you start the new one.

## Development

```sh
VITE_DOCKMOSS_OVERGROWTH=0.6 pnpm tauri dev   # draw every icon at one overgrowth (0 to 1)
DOCKMOSS_FAST_FORWARD=1000 pnpm tauri dev     # 1, 10, 100 or 1000; overrides the saved speed
```

Dev builds also outline each Dock icon and label it with its overgrowth and stage. `pnpm build` leaves all of this out.

| Command | Runs |
|---|---|
| `pnpm tauri dev` | The app, with Vite on `localhost:5110` |
| `pnpm check` | fmt, clippy in the dev and release profiles, layout guards, `tsc` |
| `pnpm test` | vitest over the frontend's pure modules |
| `pnpm check:task` | `check` plus tests, a clean `pnpm build`, `cargo test`, `cargo deny`, `pnpm audit`, gitleaks |
| `pnpm coverage` | Rust line coverage, on demand, no threshold |
| `pnpm coverage:lcov` | The same, written to `src-tauri/target/lcov.info` |
| `pnpm tauri build` | Unsigned `src-tauri/target/release/bundle/macos/DockMoss.app` |

CI runs on every PR to `main` and every push to `main`, with Rust pinned to 1.99.1. Its jobs start at once. The check jobs run the named parts of `pnpm check:task` from `package.json`, so CI and the local bar run the same commands. Lint (`check:fast`), Rust tests and coverage, and Release build run on a macOS 26 arm64 runner. Web (`check:web`) and Deps and secrets (`check:deps`, `check:secrets`) run on Ubuntu 24.04. Release build runs `cargo build` and `pnpm tauri build --no-bundle`, and fails on any warning. Rust tests and coverage runs the Rust tests once, under `pnpm coverage:lcov`. Except on PRs from forks, it then uploads the report to Codecov for the coverage badge; a failed upload never fails the run. The upload uses a Codecov CLI checked against a sha256 kept in `ci.yml`; to move to a new CLI, change its version and hash together, taking the hash from both Codecov's `SHA256SUM` and the GitHub release asset. A last job, `CI`, passes only when every other job passed, so it is the one check for branch protection to require. The workflow is [.github/workflows/ci.yml](.github/workflows/ci.yml).

`tauri dev` skips the Content Security Policy. To test with it, run `pnpm tauri build --debug --no-bundle` and start `src-tauri/target/debug/dockmoss`. That build has the Web Inspector on, so keep it to your own machine.

## Docs

- [ARCHITECTURE.md](ARCHITECTURE.md): how it works, threads, data files, source layout
- [SUMMARY.md](SUMMARY.md): product rules, the last-used algorithm and growth curve
- [CONSTRAINTS.md](CONSTRAINTS.md): the quality bar and what each check enforces
- [CLAUDE.md](CLAUDE.md): engineering conventions
- [src-tauri/icons/README.md](src-tauri/icons/README.md): regenerating the app icon and tray glyph
