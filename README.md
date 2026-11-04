# DockMoss

A macOS menu-bar app that grows moss on Dock icons for apps you haven't opened in a while. The longer an app sits unused, the more overgrown its icon gets. Opening the app clears the moss. Product rules and the last-used algorithm are in [SUMMARY.md](SUMMARY.md).

## Status

MVP slice 10 of 13 (moss on every icon). The app starts with no Dock icon and puts a moss-tile glyph in the menu bar. At launch it checks whether it has the Accessibility grant. If not, macOS shows its own Accessibility dialog once, and the tray menu offers Grant Accessibility Access…, which opens the Accessibility list in System Settings, above Quit DockMoss. A background thread re-checks every 5 s and removes the item once access is granted, or puts it back if access is revoked. With the grant, the menu has Quit DockMoss alone.

A transparent, click-through window covers the main display one level above the Dock, on every Space. A `dock-tracker` thread reads the Dock through Accessibility:
- at 60 Hz while the pointer is within 150 pt of the Dock (or of the bottom edge while the Dock is hidden) and moving, or the Dock moved in the last 250 ms, and at 2 Hz otherwise;
- while the pointer rests in that band, it also checks the pointer position alone (no Dock read) 10 times a second, so a magnification started from rest goes back to 60 Hz within about 100 ms;
- it streams a frame to the overlay only when an icon moves by 0.5 pt or more, or the list changes.

The overlay draws moss on every Dock app whose overgrowth is above 0, all in one instanced WebGL draw. Overgrowth 0 draws nothing. At the Overgrown stage an icon also gets one to three small mushrooms or flowers, placed from its bundle ID and the same on every run. The moss grows from the icon's bottom edge and bottom corners, and at 1.0 it covers about the lower half of the icon in the middle, higher at the sides, so the top half of the app's glyph stays visible. It is clipped to the icon's rounded rect (corner radius 22.5% of its side), so it never reaches the Dock's tooltip above the icon. The icon art is taken to be a square centred in the Dock tile, 0.79 of the tile's width, measured on a bottom Dock at the default and magnified sizes. The moss follows magnification, auto-hide and Dock restarts, and the renderer draws one frame per visible change, with no frame loop while nothing changes. Each icon gets its own moss pattern and a shade between olive and blue-green, the same on every run; two Dock items for one app get the same overgrowth and a pattern each. While the Dock is hidden, or cannot be read (no grant, Dock restarting), the window is ordered out. It also follows changes to the main display's resolution or arrangement. Clicks, hovers and magnification pass through to the Dock.

The overlay's content is hidden from VoiceOver (`aria-hidden`). The overlay window itself is still in the Accessibility tree, as an empty web area.

### Debug layer (dev builds)

`pnpm tauri dev` also outlines each application icon, alternating magenta and cyan, and labels it with the app's overgrowth (0.00 to 1.00) over its stage, for example `0.66` over `patchy`. Every second label sits one label higher, so neighbouring labels don't cover each other at the default Dock size. While the Dock is magnified, labels near the pointer can still overlap. The labels sit above macOS's own Dock tooltip and can hide part of it. If the overlay's `watch_dock` or `watch_growth` call is rejected, a 2 pt red bar runs along the bottom of the screen. `pnpm build` leaves all of this out.

To see moss at a chosen overgrowth, set `VITE_DOCKMOSS_OVERGROWTH` to a number from 0 to 1. Every Dock icon then uses that value, and at 1 every icon is also overgrown, with sprites. Any other value is ignored with a console warning. `VITE_DOCKMOSS_SWAY=1` turns on the sway loop: tuft tips and sprites move gently, at 30 frames a second, while some icon is at least patchy. Sway is off in every other build: this loop costs about 19% CPU (job 010, M9). Slice 12 replaces it with a cheaper sway, under a 3% bar, and adds its setting.

```sh
VITE_DOCKMOSS_OVERGROWTH=0.6 pnpm tauri dev
```

Growth comes from DockMoss's own launch log, a Spotlight seed taken once at the first run, and a first-seen time for apps with neither (see "Launch log" below; the rules are in SUMMARY.md, "Last-used data" and "Growth curve"). The usage thread recomputes it after every launch, activation, quit and Dock change, and every 5 minutes, and sends it to the overlay over the `watch_growth` channel only when it changes. A running app is always clean.

### Fast-forward (debug builds)

To watch moss grow without waiting days, start a debug build with a speed factor:

```sh
DOCKMOSS_FAST_FORWARD=1000 pnpm tauri dev
```

The factor multiplies the time since each app was last used. `1`, `10`, `100` and `1000` are accepted; any other value logs a warning and runs at 1x. At 1000x an app quit at time t reaches Fuzz about 6 to 9 minutes later, depending on its jitter, and growth is recomputed every second. Release builds ignore the variable. A fast-forward setting arrives with the settings window in slices 12 and 13.

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

- **Format.** Version 1 JSON with four fields, each map sorted by bundle ID, and every time an RFC 3339 UTC string:

  ```json
  {
    "version": 1,
    "baseline_at": "2026-11-03T08:00:57Z",
    "last_launched": { "com.apple.Safari": "2026-11-03T09:14:03Z" },
    "first_seen":    { "com.apple.Chess": "2026-11-03T08:00:57Z" },
    "spotlight_seed": { "com.hnc.Discord": "2026-10-18T17:49:11Z" }
  }
  ```

  `last_launched` is the last launch, activation or quit. `spotlight_seed` holds the Spotlight last-used dates kept at the first run. `first_seen` holds the time DockMoss first listed an app that had no launch and no seed. `baseline_at` is when the first run happened. A file written before slice 8, without the two new maps and with `baseline_at: null`, still loads, and the next Dock read runs the first run.
- **First run.** The first time DockMoss reads a Dock that lists apps, it runs `mdls -name kMDItemLastUsedDate` once per distinct Dock app, so an app pinned twice is read once. A date later than now is ignored. If 5 or more apps have dates within 5 seconds of each other (an OS update or migration re-stamped them), every Spotlight date is thrown out. Startup then logs `first run: dock apps seeded and baselined` with the number of distinct Dock apps, dates read and seeds kept, and whether a migration batch was thrown out. Apps not running whose Spotlight date is more than about 5 days old start past clean, at fuzz or later; every other app starts clean. If the Dock is hidden (for example in a full-screen Space), the first run waits until it is shown.
- **Writes.** A change reaches the disk about half a second after the event: changes within 500 ms of the first one are written together. Quitting DockMoss from the menu bar writes anything still pending first. Each save writes a temp file beside the log (`launch_log.json.tmp`, mode 0600), syncs it and renames it over the log, so a crash leaves either the old file or the new one.
- **A bad entry.** A value that is not a string, a bundle ID that is empty or longer than 255 bytes, a time that does not parse, or a time more than 5 minutes in the future is left out with a warning, and the rest of the file loads. DockMoss first copies the file as read to `launch_log.json.dropped` (mode 0600, replacing an older copy), so the next save, which writes the log without the bad entry, loses nothing.
- **A bad file.** If the file cannot be read, is larger than 1 MiB, is not JSON of this shape, or has another `version`, DockMoss renames it to `launch_log.json.corrupt` (replacing an older one), logs a warning naming both paths, and starts from an empty log. The history is still in `launch_log.json.corrupt` for recovery by hand. A read error that would have cleared up on its own is treated the same way.
- **Logs.** Startup logs `launch log started` with the path, the entry count, how many running apps were recorded, whether a first run is still to come (`first_run`) and the fast-forward factor. Bundle IDs appear only at debug level (`tauri dev`), apart from the warning for a dropped entry.

To start over, quit DockMoss and delete the file. The next start runs the first run again. `launch_log.json.corrupt` and `launch_log.json.dropped` are only kept for recovery by hand and can be deleted too.

## Commands

| Command | What it runs |
|---|---|
| `pnpm tauri dev` | The app, with Vite on `http://localhost:5110` |
| `pnpm check` | Same as `check:fast` |
| `pnpm check:fast` | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, three grep guards (unit tests only in `tests.rs`, `unsafe_code` opt-ins only in `src/macos/`, no `@ts-ignore` or `@ts-expect-error` under `src/`), `tsc --noEmit` |
| `pnpm test` | vitest, once, over the frontend's pure modules (`src/**/*.test.ts`) |
| `pnpm check:task` | `check:fast`, then `pnpm test`, a `pnpm build` whose `dist/` must hold no debug layer, `cargo test`, the 80% line-coverage gate, `cargo deny`, `pnpm audit` and three gitleaks passes |

[CONSTRAINTS.md](CONSTRAINTS.md) defines what each check enforces. If the scripts and that file disagree, the file wins.

## Layout

```
src-tauri/src/
  main.rs, lib.rs   # entry point; Tauri builder, plugins, setup(); lib.rs also holds
                    #   mod dock_tracker, the thread that reads, decides and sends, and
                    #   mod launch_log_writer, the usage thread that owns and saves the
                    #   launch log, runs the first-run Spotlight read and sends growth
  error.rs          # crate Error, serialized as { kind, message }
  commands.rs       # Tauri commands watch_dock and watch_growth: hand the overlay's
                    #   Channels to the tracker and to the usage thread
  tracker.rs        # pure: per read, what to send, log and show, and the poll rate
  geometry.rs       # pure: AX rects to overlay-local logical pixels
  dock.rs           # pure: raw Dock children to DockItems (apps only), the trust gate
  bundle_id.rs      # BundleId newtype
  tray.rs           # menu-bar icon, Quit, and the Accessibility grant item
  trust.rs          # Trust (trusted or not) and the shared latest reading
  growth/           # pure: last-used sources, jitter and the curve -> overgrowth and stage
  usage/            # pure apart from std file I/O: the launch log and growth per Dock app
    log.rs          #   LaunchLog, and the version 1 file format (encode, decode)
    store.rs        #   load_or_set_aside, the .dropped copy and the atomic save
    schedule.rs     #   when the writer saves (500 ms debounce, 5 s retry) and recomputes
    running.rs      #   which running apps are recorded
    spotlight.rs    #   mdls output parsing, the migration-batch check, SpotlightSeed
    state.rs        #   UsageState: first run, first-seen apps, running set, growth
  macos/            # all objc2 / AppKit / AX code and every unsafe block
    overlay.rs      # raises the overlay NSWindow above the Dock, on every Space
    ax.rs           # Accessibility trust check (AXIsProcessTrustedWithOptions)
    dock.rs         # reads the Dock's item list through Accessibility
    cursor.rs       # pointer location (CGEvent), no permission needed
    screen.rs       # observes display changes (NSApplicationDidChangeScreenParameters)
    workspace.rs    # running apps, app launch/activate/quit notifications, and
                    #   opening the Accessibility list in System Settings
    spotlight.rs    # runs /usr/bin/mdls for one app, 2 s timeout
src-tauri/capabilities/
  overlay.json      # overlay window may listen for events and call watch_dock and watch_growth
src-tauri/icons/
  source/           # SVG masters for the app icon and the tray glyph
overlay.html        # Vite entry for the overlay window
src/                # webview frontend: overlay.ts (one subscription per channel, fanned
                    #   out to the renderer and, in dev builds, the debug layer), ipc.ts
                    #   (every invoke call, the watchDock and watchGrowth Channels),
                    #   debug.ts / debug.css (dev only: outlines, labels, the override)
  render/           # WebGL2 moss: model.ts (pure: mossy icons, keys and seeds, art box,
                    #   draw rule, sprites, the reach bound and the shader's constants;
                    #   model.test.ts), renderer.ts (draw and sway loop behind injected
                    #   schedule and MossGl; renderer.test.ts), gl.ts (one instanced
                    #   draw), index.ts (DOM wiring), moss.vert.glsl, moss.frag.glsl
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
