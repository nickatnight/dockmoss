# DockMoss

A macOS menu-bar app that grows moss on Dock icons for apps you haven't opened in a while. The longer an app sits unused, the more overgrown its icon gets. Opening the app clears the moss. Product rules and the last-used algorithm are in [SUMMARY.md](SUMMARY.md).

## Status

MVP slice 7 of 13 (launch log). The app starts with no Dock icon and puts a moss-tile glyph in the menu bar. At launch it checks whether it has the Accessibility grant. If not, macOS shows its own Accessibility dialog once, and the tray menu offers Grant Accessibility Access…, which opens the Accessibility list in System Settings, above Quit DockMoss. A background thread re-checks every 5 s and removes the item once access is granted, or puts it back if access is revoked. With the grant, the menu has Quit DockMoss alone.

A transparent, click-through window covers the main display one level above the Dock, on every Space. A `dock-tracker` thread reads the Dock through Accessibility:
- at 60 Hz while the pointer is within 150 pt of the Dock (or of the bottom edge while the Dock is hidden), and at 2 Hz otherwise;
- it streams a frame to the overlay only when an icon moves by 0.5 pt or more, or the list changes.

The overlay draws a 1 px debug outline around each application icon, alternating magenta and cyan; folders, Trash and separators get none. The outlines follow magnification, auto-hide and Dock restarts. While the Dock is hidden, or cannot be read (no grant, Dock restarting), the window is ordered out. It also follows changes to the main display's resolution or arrangement. There is no moss yet. Clicks, hovers and magnification pass through to the Dock.

The growth model in `growth/` computes each app's moss without drawing it yet. It takes an app's last-used sources, its bundle ID, the stage thresholds and the fast-forward factor, and returns an overgrowth from 0 to 1 and a stage. The rules are in SUMMARY.md, "Growth curve". Nothing outside its tests calls it yet. Slice 8 connects it to the tracker.

DockMoss keeps its own launch log of when each app was last used (see "Launch log" below). Nothing reads it for moss yet; slice 8 adds the Spotlight seed and the first-run baseline, and feeds the log to the growth model.

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

`tauri dev` loads the page straight from Vite, so the Content Security Policy in `tauri.conf.json` is not applied there. To run with the real CSP and bundled assets, build with `pnpm tauri build --debug --no-bundle` and run `src-tauri/target/debug/dockmoss`. That debug build has the Web Inspector enabled, so use it only on your own machine and never hand it to anyone.

## Accessibility access

DockMoss needs the Accessibility grant to read the Dock. macOS gives the grant to the *responsible process*, so which app needs it depends on how DockMoss starts:

- `pnpm tauri dev` uses the grant of the terminal you start it from. If your terminal is in System Settings → Privacy & Security → Accessibility, DockMoss runs trusted and the tray shows only Quit DockMoss.
- An app bundle launched from Finder or with `open` is its own responsible process. Build one with `pnpm tauri build --debug --bundles app` and run `open src-tauri/target/debug/bundle/macos/DockMoss.app`. Without a grant, macOS shows its Accessibility dialog once at launch, and the tray menu offers Grant Accessibility Access…, which opens the Accessibility list. macOS 27 titles that list "Device Control and Data Access".
- Local bundles are ad-hoc signed, so each rebuild has a new signature and macOS treats it as a different app: a grant does not carry over. Signing with a stable identity is milestone 7.

While it runs, the app re-checks every 5 s without prompting and rebuilds the tray menu when the result changes, so a grant or revoke needs no restart. If access is revoked while the app runs, the Dock tracker notices on its next read, stops reading the Dock, and re-checks trust each tick (without prompting) until access returns. It logs `accessibility trust checked` at startup and `accessibility trust changed` on each change, to `~/Library/Logs/com.dockmoss.app/DockMoss.log`. `tauri dev` also prints them to the terminal.

## Launch log

DockMoss records the last time each app was used in `~/Library/Application Support/com.dockmoss.app/launch_log.json`. An app counts as used when it launches, becomes frontmost or quits, and every app already running when DockMoss starts counts as used at that moment. Only regular apps with a bundle ID are recorded (the apps that can have a Dock icon), so agents, background helpers and DockMoss itself are left out. The rule is in SUMMARY.md, "Last-used data", rule 1.

- **Format.** Version 1 JSON: `{ "version": 1, "baseline_at": null, "last_launched": { "<bundle ID>": "<RFC 3339 UTC time>" } }`, with entries sorted by bundle ID. `baseline_at` stays `null` until slice 8.
- **Writes.** A change reaches the disk about half a second after the event: changes within 500 ms of the first one are written together. Quitting DockMoss from the menu bar writes anything still pending first. Each save writes a temp file beside the log (`launch_log.json.tmp`, mode 0600), syncs it and renames it over the log, so a crash leaves either the old file or the new one.
- **A bad entry.** An empty bundle ID, a time that does not parse, or a time more than 5 minutes in the future is left out with a warning, and the rest of the file loads. The next save writes the log without it.
- **A bad file.** If the file cannot be read, is larger than 1 MiB, is not JSON of this shape, or has another `version`, DockMoss renames it to `launch_log.json.corrupt` (replacing an older one), logs a warning naming both paths, and starts from an empty log. The history is still in `launch_log.json.corrupt` for recovery by hand. A read error that would have cleared up on its own is treated the same way.
- **Logs.** Startup logs `launch log started` with the path, the entry count and how many running apps were recorded. Bundle IDs appear only at debug level (`tauri dev`), apart from the warning for a dropped entry.

To start over, quit DockMoss and delete the file.

## Commands

| Command | What it runs |
|---|---|
| `pnpm tauri dev` | The app, with Vite on `http://localhost:5110` |
| `pnpm check` | Same as `check:fast` |
| `pnpm check:fast` | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, three grep guards (unit tests only in `tests.rs`, `unsafe_code` opt-ins only in `src/macos/`, no `@ts-ignore` or `@ts-expect-error` under `src/`), `tsc --noEmit` |
| `pnpm check:task` | `check:fast`, then tests, the 80% line-coverage gate, `cargo deny`, `pnpm audit` and three gitleaks passes |

[CONSTRAINTS.md](CONSTRAINTS.md) defines what each check enforces. If the scripts and that file disagree, the file wins.

## Layout

```
src-tauri/src/
  main.rs, lib.rs   # entry point; Tauri builder, plugins, setup(); lib.rs also holds
                    #   mod dock_tracker, the thread that reads, decides and sends, and
                    #   mod launch_log_writer, the thread that owns and saves the launch log
  error.rs          # crate Error, serialized as { kind, message }
  commands.rs       # Tauri command watch_dock: hands the overlay's Channel to the tracker
  tracker.rs        # pure: per read, what to send, log and show, and the poll rate
  geometry.rs       # pure: AX rects to overlay-local logical pixels
  dock.rs           # pure: raw Dock children to DockItems (apps only), the trust gate
  bundle_id.rs      # BundleId newtype
  tray.rs           # menu-bar icon, Quit, and the Accessibility grant item
  trust.rs          # Trust (trusted or not) and the shared latest reading
  growth/           # pure: last-used sources, jitter and the curve -> overgrowth and stage
  usage/            # pure apart from std file I/O: the launch log
    log.rs          #   LaunchLog, and the version 1 file format (encode, decode)
    store.rs        #   load_or_set_aside and the atomic save
    schedule.rs     #   when the writer saves (500 ms debounce, 5 s retry)
    running.rs      #   which running apps are recorded
  macos/            # all objc2 / AppKit / AX code and every unsafe block
    overlay.rs      # raises the overlay NSWindow above the Dock, on every Space
    ax.rs           # Accessibility trust check (AXIsProcessTrustedWithOptions)
    dock.rs         # reads the Dock's item list through Accessibility
    cursor.rs       # pointer location (CGEvent), no permission needed
    screen.rs       # observes display changes (NSApplicationDidChangeScreenParameters)
    workspace.rs    # running apps, app launch/activate/quit notifications, and
                    #   opening the Accessibility list in System Settings
src-tauri/capabilities/
  overlay.json      # overlay window may listen for events and call watch_dock
src-tauri/icons/
  source/           # SVG masters for the app icon and the tray glyph
overlay.html        # Vite entry for the overlay window
src/                # webview frontend: overlay.ts / overlay.css, ipc.ts (every invoke call, watchDock's Channel), debug.ts (outlines; renderer from slice 9)
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
