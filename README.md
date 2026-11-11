# DockMoss

A macOS menu-bar app that grows moss on Dock icons for apps you haven't opened in a while. The longer an app sits unused, the more overgrown its icon gets. Opening the app clears the moss. Product rules and the last-used algorithm are in [SUMMARY.md](SUMMARY.md).

## Status

MVP slice 13 of 13: settings apply live, launch at login, and the release bundle. Checkpoint D's last manual checks, running the release build from `/Applications` and measuring idle CPU with the Dock visible, are still open. The app starts with no Dock icon and puts a moss-tile glyph in the menu bar. At launch it checks whether it has the Accessibility grant. If not, macOS shows its own Accessibility dialog once, and the tray menu offers Grant Accessibility Access…, which opens the Accessibility list in System Settings, above Settings… and Quit DockMoss. A background thread re-checks every 5 s and removes the item once access is granted, or puts it back if access is revoked. With the grant, the menu has Settings… and Quit DockMoss.

A transparent, click-through window covers the main display one level above the Dock, on every Space. A `dock-tracker` thread reads the Dock through Accessibility:
- at 60 Hz while the pointer is within 150 pt of the Dock (or of the bottom edge while the Dock is hidden) and moving, or the Dock moved in the last 250 ms, and at 2 Hz otherwise;
- while the pointer rests in that band, it also checks the pointer position alone (no Dock read) 10 times a second, so a magnification started from rest goes back to 60 Hz within about 100 ms;
- a second thread listens for the Dock's own notice that the item under the pointer changed. The Dock sends it when the pointer enters the Dock, crosses an icon or leaves. It makes the next read due at once, so the moss follows a magnification from its first frames (about 17 ms) instead of waiting up to 500 ms for the next 2 Hz read. Those reads are never closer than 1/60 s apart;
- when a regular app starts launching, macOS's launch notice keeps 60 Hz, wherever the pointer is, until 1 s after the Dock stops moving and never more than 2 s after the latest launch, so the app's bouncing icon (and any tile a new app adds) is followed from about 40 ms into the bounce. The notice also makes the next read due at once, under the same 1/60 s spacing;
- with the Dock hidden, a pointer within 4 pt of the bottom edge keeps 60 Hz until 1 s after it last moved, longer than the Dock's default auto-hide delay, so the Dock sliding in is read at once rather than at the next 2 Hz read. With `com.apple.dock autohide-delay` set above 1 s, the reveal waits for that 2 Hz read again;
- it streams a frame to the overlay only when an icon moves by 0.5 pt or more, or the list changes.

The overlay draws moss on every Dock app whose overgrowth is above 0, all in one instanced WebGL draw. Overgrowth 0 draws nothing. The moss is made of lit cushions, fronds and, from about halfway, spore stalks. It starts as a thin strip along the icon's bottom edge, climbs the sides, and reaches the top edge last: at 1.0 it is a thick frame around the app's glyph, whose centre stays mostly clear. At the Overgrown stage an icon also gets a small mushroom and a flower, placed from its bundle ID inside the icon, on the moss near its bottom; at a low intensity they are smaller, at least 70% of full size. The moss may overhang the icon by half the gap to its neighbours, so it never reaches the next icon, and it stays inside the Dock tile, below the tooltip the Dock shows above it. The icon art is taken to be a square centred in the Dock tile, 0.79 of the tile's width, measured on a bottom Dock at the default and magnified sizes; a side Dock is not supported. Each icon's moss is baked once per growth step (24 steps) and then drawn from a cache, so a magnification only moves it. The moss follows magnification, auto-hide and Dock restarts, and the renderer draws one frame per visible change, with no frame loop while nothing changes, except during a shrivel or while new bakes are made (two a frame). Each icon gets its own moss pattern, the same on every run; two Dock items for one app get the same overgrowth and a pattern each. While the Dock is hidden, or cannot be read (no grant, Dock restarting), the window is ordered out. It also follows changes to the main display's resolution or arrangement. Clicks, hovers and magnification pass through to the Dock.

When you open an app whose icon has moss, the moss shrivels over 600 ms: it turns brown and its top edge sinks to the bottom of the icon, taking any mushroom or flower with it, then the icon is clean. It plays once, on the icon's tile as it moves, and several icons can shrivel at once. Any drop of a mossy icon to clean plays it: opening the app, or a settings change that clears the icon (an exclusion, intensity 0, or higher thresholds). An icon that leaves the Dock, a Dock that hides, a webview reload, or moss growing back ends a shrivel at once. While a shrivel runs, the renderer runs a frame loop at 30 frames a second, and stops it once the icon is clean. With `VITE_DOCKMOSS_OVERGROWTH` set, nothing shrivels, because the override pins every icon's overgrowth.

The overlay's content is hidden from VoiceOver (`aria-hidden`). The overlay window itself is still in the Accessibility tree, as an empty web area.

### Debug layer (dev builds)

`pnpm tauri dev` also outlines each application icon, alternating magenta and cyan, and labels it with the app's overgrowth (0.00 to 1.00) over its stage, for example `0.66` over `patchy`. Every second label sits one label higher, so neighbouring labels don't cover each other at the default Dock size. While the Dock is magnified, labels near the pointer can still overlap. The labels sit above macOS's own Dock tooltip and can hide part of it. If the overlay's `watch_dock` or `watch_growth` call is rejected, a 2 pt red bar runs along the bottom of the screen. `pnpm build` leaves all of this out.

To see moss at a chosen overgrowth, set `VITE_DOCKMOSS_OVERGROWTH` to a number from 0 to 1. Every Dock icon then uses that value, and at 1 every icon is also overgrown, with a mushroom and a flower. Any other value is ignored with a console warning. There is no sway: job 017 removed the old sway loop (about 19% CPU, job 010), and a later job adds a cheaper sway with its setting.

```sh
VITE_DOCKMOSS_OVERGROWTH=0.6 pnpm tauri dev
```

Growth comes from DockMoss's own launch log, a Spotlight seed taken once at the first run, and a first-seen time for apps with neither (see "Launch log" below; the rules are in SUMMARY.md, "Last-used data" and "Growth curve"). It follows your settings (see "Settings"). The usage thread recomputes it after every launch, activation, quit, Dock change and settings change, and every 5 minutes at 1× (5 minutes divided by the fast-forward factor otherwise, at most once a second), and sends it to the overlay over the `watch_growth` channel only when it changes. A running app is always clean.

### Fast-forward (debug builds)

To watch moss grow without waiting days, start a debug build with a speed factor:

```sh
DOCKMOSS_FAST_FORWARD=1000 pnpm tauri dev
```

The factor multiplies the time since each app was last used. `1`, `10`, `100` and `1000` are accepted; any other value logs a warning, and the saved fast-forward applies. At 1000x an app quit at time t reaches Fuzz about 6 to 9 minutes later, depending on its jitter, and growth is recomputed every second. Release builds ignore the variable. The settings window has a fast-forward setting too, which applies in every build. When the variable is set to a valid value it wins over the setting, and it never reads or changes it.

## Settings

Choose Settings… from the moss tile in the menu bar. The window opens in front of the app you were in, with the keyboard in it. Choosing Settings… again while it is open brings it back to the front.

Launching DockMoss again while it runs does the same: a double-click in Finder, Spotlight, `open`, or `open -n` opens Settings, or brings it to the front if it is already open. Only one copy of DockMoss runs at a time; a second copy hands over to the running one and exits before it touches your launch log or settings.

- **Launch at login:** opens DockMoss when you log in to your Mac. Turning it on adds DockMoss to System Settings → General → Login Items → Open at Login, with no other permission prompt. macOS keeps this setting, not DockMoss: the box shows what macOS reports, so turning it off in System Settings shows as off the next time Settings opens. If macOS wants your approval first, the box shows off and a note says to approve DockMoss in System Settings › General › Login Items. When macOS does not make the change, the notice says so and points there instead of asking you to try again. Only an app bundle can be a login item, so under `pnpm tauri dev` a change fails with that notice. If macOS does not answer within half a second when Settings opens, the box stays disabled and says so; close and reopen Settings to ask again. If macOS does not report how a change ended within half a second, the box goes to that same state.
- **Growth stages:** the days without use before Fuzz, Patchy and Overgrown begin (5, 10 and 30 by default). Each is a whole number from 2 to 3650, and each stage must be longer than the one before. A value that breaks the rule turns its field red with a message under the fields, and is not saved. The other settings still save.
- **Intensity:** how strongly moss is drawn, 0% to 100% (100% by default). It scales how much of each icon the moss covers. An overgrown app keeps its mushrooms or flowers at any intensity above 0%, and at 0% no icon has moss.
- **Fast-forward:** 1×, 10×, 100× or 1000× (1× by default). It multiplies the time since each app was last used, to preview growth. Use 1× for normal use.
- **Excluded apps:** one checkbox per Dock app, by name, in Dock order. An excluded app never has moss. An excluded app that is no longer in the Dock stays in the list by bundle ID, marked "Not in the Dock", so it can be cleared. Without the Accessibility grant, the list says it fills in once DockMoss can read the Dock. At most 256 apps can be excluded.

There is no Save button. A change is saved as soon as you make it: a number when you leave the field or press Return, the slider when you let go, a checkbox or speed when you click it. If a save fails, a notice at the top says so, and the control keeps your value until it saves. Close the window with its close button or ⌘W. The keyboard goes back to the last app you were in (a regular app, not a launcher such as Spotlight). If that app has quit, DockMoss steps back and macOS brings the next app forward. The window follows the system's light or dark appearance, and every control works from the keyboard.

**Changes apply at once.** A saved change reaches the moss on the Dock within a second, with no restart. A change that clears a mossy icon shrivels it, as opening the app would.

Settings are stored in `~/Library/Application Support/app.dockmoss.DockMoss/settings.json` (mode 0600). Launch at login is not in this file. DockMoss writes it only when you change a setting, the same way as the launch log: a temp file beside it, then a rename. It never writes it at startup or quit. If the file is missing, larger than 256 KiB, unreadable or not a settings file, DockMoss uses the defaults and leaves the file alone until your next change. A single bad value falls back to its default and the rest load. The log notes each case with a warning. To reset every setting, quit DockMoss and delete the file.

## Requirements

- macOS 15 or later (the only platform; the bundle's `minimumSystemVersion` is 15.0)
- Rust 1.99 or later
- Node 22 and pnpm
- For `pnpm check:task`: `cargo install cargo-deny` and gitleaks 8.30.1.
- For `pnpm coverage`: `cargo install cargo-llvm-cov`. On first run, it installs the `llvm-tools-preview` rustup component.

## Quick start

```sh
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` starts Vite on port 5110, then builds and runs the app. Quit it from the moss tile in the menu bar.

Quit any other DockMoss before you start it: an installed copy, a debug bundle, or another `tauri dev`. They all share the bundle identifier, so the dev copy would exit at once (and `tauri dev` with it), and the running copy would open its Settings instead.

`pnpm tauri build` builds an unsigned release `src-tauri/target/release/bundle/macos/DockMoss.app` and nothing else (no DMG). Signing and notarization are milestone 7.

`tauri dev` loads the page straight from Vite, so the Content Security Policy in `tauri.conf.json` is not applied there. To run with the real CSP and bundled assets, build with `pnpm tauri build --debug --no-bundle` and run `src-tauri/target/debug/dockmoss`. That debug build has the Web Inspector enabled, so use it only on your own machine and never hand it to anyone.

## Accessibility access

DockMoss needs the Accessibility grant to read the Dock. macOS gives the grant to the *responsible process*, so which app needs it depends on how DockMoss starts:

- `pnpm tauri dev` uses the grant of the terminal you start it from. If your terminal is in System Settings → Privacy & Security → Accessibility, DockMoss runs trusted and the tray shows Settings… and Quit DockMoss.
- An app bundle launched from Finder or with `open` is its own responsible process. Build one with `pnpm tauri build --debug --bundles app` and run `open src-tauri/target/debug/bundle/macos/DockMoss.app`. Without a grant, macOS shows its Accessibility dialog once at launch, and the tray menu offers Grant Accessibility Access…, which opens the Accessibility list. macOS 27 titles that list "Device Control and Data Access".
- Local bundles are ad-hoc signed, so each rebuild has a new signature and macOS treats it as a different app: a grant does not carry over. Signing with a stable identity is milestone 7.
- The bundle identifier changed from `com.dockmoss.app` to `app.dockmoss.DockMoss` in job 014, so a new build is a new entry in the Accessibility list. Old "DockMoss" entries can be removed there.

While it runs, the app re-checks every 5 s without prompting and rebuilds the tray menu when the result changes, so a grant or revoke needs no restart. If access is revoked while the app runs, the Dock tracker notices on its next read, stops reading the Dock, and re-checks trust at most every 500 ms (without prompting) until access returns. It logs `accessibility trust checked` at startup and `accessibility trust changed` on each change, to `~/Library/Logs/app.dockmoss.DockMoss/DockMoss.log`. DockMoss makes that folder private (mode 0700) before the first line is written. `tauri dev` also prints them to the terminal.

## Launch log

DockMoss records the last time each app was used in `~/Library/Application Support/app.dockmoss.DockMoss/launch_log.json`. An app counts as used when it launches, becomes frontmost or quits, and every app already running when DockMoss starts counts as used at that moment. Only regular apps with a bundle ID are recorded (the apps that can have a Dock icon), so agents, background helpers and DockMoss itself (any copy of it, including a second launch that is about to exit) are left out. The rule is in SUMMARY.md, "Last-used data", rule 1.

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

## Moving from `com.dockmoss.app`

Builds before job 014 used the bundle identifier `com.dockmoss.app`. On its first start, a build with `app.dockmoss.DockMoss` moves `launch_log.json`, `settings.json`, `launch_log.json.corrupt` and `launch_log.json.dropped` from `~/Library/Application Support/com.dockmoss.app/` into `~/Library/Application Support/app.dockmoss.DockMoss/`, so your usage history and settings carry over. It does this before it reads either file.

- Each file moves on its own, and only if it is a regular file and the new folder has no file of that name. No file is overwritten or merged, and no other file moves. The old folder is removed once it is empty. Later starts find nothing to move.
- If a file cannot be moved, it stays in the old folder and the log names it (never its path). For that session DockMoss runs without it: with the launch log left behind it records no use and draws no moss, and with the settings left behind it uses the defaults and cannot save a change. The next start tries the move again. While a file is left behind, the menu-bar menu says so in a line at the top, one line per file: "Usage history wasn’t moved, so no moss is shown" and "Saved settings weren’t moved, so changes won’t save". On a volume without hard links, such as exFAT or some network home folders, each file is copied into the new folder instead, never over a file of that name, and the old one is removed after the copy is on disk.
- If the log warns `old data file moved; its old copy was not removed`, the file in the old folder is a stale copy, and the old folder can be deleted by hand.
- Logs now go to `~/Library/Logs/app.dockmoss.DockMoss/`. The old `~/Library/Logs/com.dockmoss.app/` folder is left as it was, and you may delete it.
- Quit any DockMoss built with the old identifier before the first start of a new build. The two use different single-instance locks, so both could run and draw two overlays.

## Commands

| Command | What it runs |
|---|---|
| `pnpm tauri dev` | The app, with Vite on `http://localhost:5110` |
| `pnpm check` | Same as `check:fast` |
| `pnpm check:fast` | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` (which also fails a `#[test]` outside a `#[cfg(test)]` module), the test layout guards (two greps and a `find` that keep each module's unit tests in one inline `mod tests {` and refuse a separate test file), two more grep guards (`unsafe_code` opt-ins only in `src/macos/`, no `@ts-ignore` or `@ts-expect-error` under `src/`), `tsc --noEmit` |
| `pnpm test` | vitest, once, over the frontend's pure modules (`src/**/*.test.ts`) |
| `pnpm check:task` | `check:fast`, then `pnpm test`, a `pnpm build` whose `dist/` must hold no debug layer, `cargo test`, `cargo deny`, `pnpm audit` and three gitleaks passes |
| `pnpm coverage` | `cargo llvm-cov` over `src-tauri/src`, leaving out `macos/`, `main.rs`, `lib.rs`, `tray.rs` and `commands.rs`. On demand, with no threshold, and not part of any check. Unit tests sit inline, so their lines count too, and the figure is a rough guide. Extra arguments pass through: `pnpm coverage --html`, or `pnpm coverage --lcov --output-path target/lcov.info` (written to `src-tauri/target/lcov.info`) |

[CONSTRAINTS.md](CONSTRAINTS.md) defines what each check enforces. If the scripts and that file disagree, the file wins.

## Layout

```
src-tauri/src/
  main.rs, lib.rs   # entry point; Tauri builder, plugins, setup(); lib.rs also holds
                    #   mod dock_tracker, the thread that reads, decides and sends, and
                    #   mod launch_log_writer, the usage thread that owns and saves the
                    #   launch log, runs the first-run Spotlight read and sends growth;
                    #   setup() moves the old app data first and a small plugin makes the
                    #   log folder 0700
  error.rs          # crate Error, serialized as { kind, message }
  commands.rs       # Tauri commands: watch_dock and watch_growth hand the overlay's
                    #   Channels to the tracker and to the usage thread; get_settings,
                    #   save_settings and set_launch_at_login serve the settings window
  tracker.rs        # pure: per read, what to send, log and show; the poll rate and
                    #   Activity (60 Hz only while something moves, the 10 Hz rest
                    #   check, the reveal wait, the launch hold) and the absolute
                    #   read deadlines
  geometry.rs       # pure: AX rects to overlay-local logical pixels, and the settings
                    #   window's height on short displays
  dock.rs           # pure: raw Dock children to DockItems (apps only), and ReadGate,
                    #   which stops AX reads while trust is revoked
  bundle_id.rs      # BundleId newtype
  settings.rs       # the four settings, their checks, settings.json load and save, and
                    #   the settings growth applies (with the debug fast-forward override)
  login_item.rs     # pure: whether a login-item change worked, and the .app-only rule
  bounded.rs        # a read on its own thread with a deadline (the login status)
  navigation.rs     # which URLs the windows may load
  focus.rs          # pure: which app Settings hands the keyboard back to, and whether
                    #   DockMoss hides when it closes
  tray.rs           # menu-bar icon: Settings…, Quit, and the Accessibility grant item
  trust.rs          # Trust (trusted or not) and the shared latest reading
  growth/           # pure: last-used sources, jitter and the curve -> overgrowth and stage
  usage/            # pure apart from std file I/O: the launch log and growth per Dock app
    log.rs          #   LaunchLog, and the version 1 file format (encode, decode)
    store.rs        #   load_or_set_aside, the .dropped copy, the capped read and the
                    #   atomic write (settings.json uses both too)
    schedule.rs     #   when the writer saves (500 ms debounce, 5 s retry) and recomputes
    running.rs      #   which running apps are recorded
    spotlight.rs    #   mdls output parsing, the migration-batch check, SpotlightSeed
    state.rs        #   UsageState: first run, first-seen apps, running set, growth
                    #   under the settings (thresholds, speed, exclusions, intensity)
    migrate.rs      #   the one-time move out of the com.dockmoss.app folder
  macos/            # all objc2 / AppKit / AX code and every unsafe block
    app.rs          # the front app, the app at a pid, and activating one (settings
                    #   focus hand-back)
    overlay.rs      # raises the overlay NSWindow above the Dock, on every Space
    ax.rs           # Accessibility trust check (AXIsProcessTrustedWithOptions)
    dock.rs         # reads the Dock's item list through Accessibility
    cursor.rs       # pointer location (CGEvent), no permission needed
    screen.rs       # observes display changes (NSApplicationDidChangeScreenParameters)
    workspace.rs    # running apps, app launch/activate/quit notifications, and
                    #   opening the Accessibility list in System Settings
    spotlight.rs    # runs /usr/bin/mdls for one app, 2 s timeout
    login_item.rs   # launch at login through SMAppService.mainApp
src-tauri/capabilities/
  overlay.json      # overlay window may listen for events and call watch_dock and watch_growth
  settings.json     # settings window may call get_settings, save_settings and
                    #   set_launch_at_login, and close
src-tauri/icons/
  source/           # SVG masters for the app icon and the tray glyph
overlay.html        # Vite entry for the overlay window
settings.html       # Vite entry for the settings window
src/                # webview frontend: overlay.ts (one subscription per channel, fanned
                    #   out to the renderer and, in dev builds, the debug layer), ipc.ts
                    #   (every invoke call, the watchDock and watchGrowth Channels),
                    #   debug.ts / debug.css (dev only: outlines, labels, the override)
  render/           # baked moss drawn with WebGL2: generator.ts (pure: the moss's
                    #   shapes for one item at one growth step; generator.test.ts),
                    #   bake.ts (shapes onto a Canvas2D canvas; bake.test.ts),
                    #   atlas.ts (pure: which bakes the texture atlas holds, two new
                    #   bakes a frame; atlas.test.ts), model.ts (pure: mossy icons,
                    #   keys and seeds, art and bake boxes, draw rule, the shader's
                    #   constants and the instance attribute layout, and the shrivel
                    #   model; model.test.ts), renderer.ts (draws on change, bakes
                    #   into the atlas, plus a 30 fps loop for the shrivel, behind
                    #   injected schedule, bake and MossGl; renderer.test.ts), gl.ts
                    #   (the texture-array atlas and one instanced draw), index.ts
                    #   (DOM wiring), moss.vert.glsl, moss.frag.glsl
  settings.ts, settings.css  # the settings form
  settings/model.ts # pure: next settings, the one-at-a-time saver, feedback rules,
                    #   exclusion rows, the launch at login box and its one-at-a-time
                    #   toggle (model.test.ts)
```

## Icons

The bundle icons and the tray glyph are generated from the SVG masters in `src-tauri/icons/source/`.

### App icon

`src-tauri/icons/source/app-icon.svg` is generated, so do not edit it by hand. `appIconMaster()` in `src/icon/app-icon.ts` writes it. The master is the old stone tile with the moss generator's output (`src/render/generator.ts`) drawn over it. The moss is grown at level step 16 of 24 from the seed `fnv1a32("dockmoss#0")`, without props, and laid out as the overlay lays moss on a Dock tile (`ART_SIDE_PER_TILE_WIDTH`). The marketing site's logo is the same document: this repo is the source, and the site ports `src/icon/` from it (ADR-0019).

A vitest file snapshot fails `pnpm test` when the master is stale. After a change to the icon or to the moss generator:

```sh
pnpm test src/icon -u    # rewrite app-icon.svg; the filter goes before -u
# re-pin APP_ICON_SHA256 in src/icon/app-icon.test.ts (the failing test prints the new value)
pnpm tauri icon src-tauri/icons/source/app-icon.svg
# then delete the non-macOS outputs (android/, ios/, icon.ico, Square*Logo.png, StoreLogo.png, 64x64.png)
```

- **Put the filter before `-u`.** Vitest's `-u` takes an optional value (`--update [type]`), so in `pnpm test -u src/icon` vitest reads `src/icon` as that value and updates snapshots across the whole suite.
- **Re-pin the SHA-256 by hand.** `-u` never rewrites `APP_ICON_SHA256`, so the SHA test stays red until you re-pin it. A changed icon is therefore always a deliberate edit.
- **Re-port the site.** The site then re-ports `src/icon/` and pins the same SHA-256 in its own test.
- **Rerun `pnpm tauri icon`.** No test fails if you skip it, so run it every time the master changes.

Two tests parse `appIconSvg()` and `appIconMaster()` with fast-xml-parser's `XMLValidator`. It rejects a missing or mismatched closing tag, a duplicate attribute and an unquoted attribute. It accepts some documents that XML forbids:
- `--` inside a comment, which the master test asserts against separately;
- a comment that ends in `-` (`--->`);
- `&` or `<` inside an attribute value;
- an empty second root element.

`xmllint` rejects all of these. After a change that touches the header comment or the document's structure, also run:

```sh
xmllint --noout src-tauri/icons/source/app-icon.svg
```

### Tray glyph

```sh
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
