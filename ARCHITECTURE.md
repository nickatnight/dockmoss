# Architecture

DockMoss is a Tauri 2 app with a Rust backend and two webviews: a transparent overlay that draws the moss, and a Settings window. Product rules (what counts as "used", the growth curve) are in [SUMMARY.md](SUMMARY.md). Coding conventions are in [CLAUDE.md](CLAUDE.md).

## The big picture

```
          Dock process                          NSWorkspace
    (Accessibility API)                    (launch / activate / quit)
          │       │                                   │
   reads  │       │ hover changed                     │ app used
          ▼       ▼                                   ▼
   ┌──────────────────┐  app launching      ┌──────────────────┐
   │   dock-tracker   │◄────────────────────│    main thread   │
   │  (+dock-observer)│                     │ AppKit, tray,    │
   └────────┬─────────┘                     │ windows          │
            │ Dock apps                     └────────┬─────────┘
            ▼                                        │ used / settings
   ┌──────────────────┐◄─────────────────────────────┘
   │    launch-log    │  owns launch_log.json, computes growth
   └────────┬─────────┘
            │                    watch_dock channel (icon rects)
            │ watch_growth channel ─────────────┐   │
            ▼                                   ▼   ▼
                                       ┌──────────────────┐
                                       │ overlay webview  │  WebGL2 moss
                                       └──────────────────┘
```

Rust owns all state. The webviews only render (overlay) or edit through commands (Settings).

## Threads

| Thread | Job |
|---|---|
| main | AppKit, the tray menu, both windows, `NSWorkspace` notifications. Never blocks on the Dock. |
| `dock-tracker` | Reads the Dock's items over Accessibility, converts their rects to overlay coordinates, and streams a frame to the overlay when something moved 0.5 pt or more. |
| `dock-observer` | Optional. Listens for the Dock's "hovered item changed" notice on its own run loop and wakes the tracker early. Polling is the fallback. |
| `launch-log` | Sole owner of the launch log and the usage state. Records use, saves, recomputes growth, and streams it to the overlay. |
| `spotlight-seed` | Short-lived. Runs `mdls` once per Dock app at the first run. |
| `ax-trust` | Re-checks the Accessibility grant every 5 s and updates the tray menu. |

Threads talk over channels. Nothing else touches the launch-log thread's state; others send it `WriterMessage`s.

## Reading the Dock

Every Accessibility read costs the Dock process CPU, so the tracker reads fast only while something on screen can move (`tracker.rs`, `Activity`):

- **60 Hz** while the pointer is near the Dock and moving, within 250 ms of the last frame sent, while the pointer waits at a hidden Dock's edge (for 1 s), or during an app launch (about 2 s, so the bouncing icon is followed).
- **10 Hz pointer-only checks**, with no Dock read, while the pointer rests near the Dock.
- **2 Hz** otherwise.

Two notices end a slow wait early: the Dock's hover change (via `dock-observer`) and an app launch. Neither can make reads closer than 1/60 s apart.

The Dock reports rects in global top-left coordinates. `geometry::to_overlay_local` is the one place that converts them to overlay pixels.

## The overlay

A transparent, click-through window covers the main display one level above the Dock, on every Space. It is ordered out while the Dock is hidden or can't be read.

The renderer (`src/render/`) has no frame loop while nothing changes. It redraws only when a new Dock or growth frame arrives:

1. `generator.ts` builds the moss shapes for one icon at one of 24 growth steps, seeded from its bundle ID so the pattern is stable across runs.
2. `bake.ts` draws those shapes once onto a Canvas2D canvas.
3. `atlas.ts` keeps bakes in a WebGL2 texture array, at most two new bakes per frame.
4. `gl.ts` draws every mossy icon in one instanced call, so magnification only moves quads.

When an icon that was mossy comes back clean, the webview plays a 600 ms shrivel (a 30 fps loop that stops when it's done). Rust sends no event for it; the webview infers it from the growth stream.

## Usage and growth

The launch-log thread keeps three times per app in `launch_log.json`:

- `last_launched`: the last launch, activation or quit, from `NSWorkspace`.
- `spotlight_seed`: Spotlight's last-used date, read once at the first run and thrown out if it looks like an OS migration batch.
- `first_seen`: when DockMoss first listed an app that had neither.

Growth is the time since the latest of these, shifted by a stable per-app jitter (±1 day, FNV-1a of the bundle ID), run through the curve in `growth/` and then the user's settings (thresholds, speed, exclusions, intensity). A running app is always clean. Growth is recomputed after every use, Dock change and settings change, and every 5 minutes.

`growth/` and `usage/` are pure Rust with time passed in, so the rules are table-tested with fixed clocks.

## Settings

`settings.json` is owned by Rust. The Settings window reads it with `get_settings` and changes it only through `save_settings`, which validates every field, writes the file, then applies it. A save is all or nothing. Launch at login isn't in the file; it's `SMAppService` state that macOS keeps.

## Data files

| File | Written | On a bad read |
|---|---|---|
| `launch_log.json` | 500 ms after a change, and at quit | Bad entries are dropped (original copied to `.dropped`). An unreadable file is renamed to `.corrupt` and DockMoss starts empty. |
| `settings.json` | Only on a user change | Missing or bad fields fall back to defaults. The file is left alone until the next save. |

Both are written atomically: a 0600 temp file beside the target, synced, then renamed over it. Both are capped on read (1 MiB and 256 KiB).

Launch log format (version 1, RFC 3339 UTC times, maps sorted by bundle ID):

```json
{
  "version": 1,
  "baseline_at": "2026-11-03T08:00:57Z",
  "last_launched": { "com.apple.Safari": "2026-11-03T09:14:03Z" },
  "first_seen":    { "com.apple.Chess": "2026-11-03T08:00:57Z" },
  "spotlight_seed": { "com.hnc.Discord": "2026-10-18T17:49:11Z" }
}
```

`baseline_at` marks that the first run happened; it is not a last-used time.

### The one-time data move

Builds before job 014 stored data under `com.dockmoss.app`. At startup, before reading anything, `usage::migrate` moves the four data files into the new folder. It hard-links then removes, falls back to a careful copy on volumes without hard links, never overwrites a file, and redoes a copy cut short by a crash. A file it can't move stays behind, and the tray menu says so: usage tracking is off for the session if the log was left, and saving is off if the settings were left. The next start retries.

## Source layout

```
src-tauri/src/
  lib.rs           builder, plugins, setup(), thread glue
  commands.rs      Tauri commands (thin; delegate to modules)
  tracker.rs       poll rate and per-read decisions             pure
  geometry.rs      AX rects → overlay pixels                    pure
  dock.rs          raw Dock children → DockItems                pure
  settings.rs      settings, validation, load and save          pure + std I/O
  focus.rs         where focus goes when Settings closes        pure
  login_item.rs    launch-at-login outcome rules                pure
  navigation.rs    which URLs a window may load                 pure
  bounded.rs       a read with a deadline on its own thread
  tray.rs, trust.rs  menu-bar menu, Accessibility trust state
  growth/          last-used sources, jitter, curve             pure
  usage/           launch log, storage, schedule, Spotlight,
                   UsageState, the data move                    pure + std I/O
  macos/           every objc2 / AppKit / AX call and every unsafe block
src-tauri/capabilities/  one permission file per window
src/
  overlay.ts, ipc.ts     overlay entry, every invoke call
  render/                the moss renderer (above)
  settings.ts, settings/ the Settings form and its pure model
  debug.ts               dev-only outlines and labels
  icon/                  dev-only app icon generator (src-tauri/icons/README.md)
```

The boundary that matters is `macos/`. Everything outside it is plain Rust with no `tauri` or `objc2`, so it can be unit-tested. `macos/` returns plain Rust types and holds every `unsafe` block.

## Security

- Each window has its own capability file. The overlay may only subscribe to its two channels; Settings may only call the settings commands. Neither can call the other's.
- Both windows refuse navigation outside the bundled origin.
- Logs name files, never paths, and never log `argv`. The log folder is 0700 and data files are 0600.
