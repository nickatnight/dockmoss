# DockMoss

A novelty macOS app that grows moss on Dock icons for apps you haven't opened in a while. The longer an app sits unused, the more overgrown it gets. Launch the app and the moss retreats.

## Concept

- A transparent, click-through overlay sits above the Dock.
- Each Dock app icon gets a moss layer whose coverage scales with days since the app was last used.
- Stages (tunable): clean, light fuzz (5 days), patchy moss (10 days), fully overgrown with a tiny mushroom and flower (30+ days).
- Opening the app clears its moss, with a small "shrivel" animation. A settings change that clears a mossy icon (an exclusion, intensity 0, higher thresholds) plays the same shrivel.

## How it works

1. **Find icons:** read Dock item geometry and app paths through the Accessibility API (`AXUIElement`, Dock process; app dock items expose position, size and URL).
2. **Track usage:** record launches ourselves (see "Last-used data" below) and compute an overgrowth value from 0 to 1 per app.
3. **Draw:** render moss per icon in the overlay, repositioned each frame to follow Dock magnification.
4. **Refresh:** update on app launch/quit notifications, and recompute growth every few minutes.

## Last-used data

Spotlight metadata turned out to be unreliable. In a real test on one Mac, 12 of 24 apps returned `(null)` for `kMDItemLastUsedDate`, and nearly every `kMDItemDateAdded` landed in the same two seconds, which points to an OS update or migration re-stamping the data. So Spotlight is a hint, not the source of truth.

**Rules**

1. **Own launch log is the source of truth.** Listen for `NSWorkspace` launch, activation and quit notifications and store a per-app timestamp (keyed by bundle ID) in our own app data. Quitting an app counts as using it, so "last used" is the last time the app was running or frontmost, and moss starts growing from when it quit. Apps already running when DockMoss starts count as used at that moment. DockMoss never records its own launches as use: a second copy starting up is ignored, like any copy of DockMoss. The log survives OS updates and can't be silently rewritten.
2. **Effective last-used** is the latest of: our launch log, the app's Spotlight seed, and its first-seen time (rules 3 to 5). An app has a Spotlight seed or a first-seen time, never both.
3. **First run.** The first time DockMoss reads a Dock that lists apps, and its log has no `baseline_at`, each Dock app gets one of two things. An app with a sane `kMDItemLastUsedDate` (rule 4) keeps that date as its Spotlight seed, so an app unused for weeks shows moss on day one. Every other Dock app with no launch history is first seen at that moment and grows from there, so the Dock isn't green on day one. DockMoss then sets `baseline_at` to that moment. It only marks that the first run is done and is not a last-used time. A missing log, a log set aside as corrupt, and a log from before slice 8 all lead to a first run. Without the Accessibility grant, or while the Dock stays hidden, the first run waits.
4. **Spotlight as a seed only.** `kMDItemLastUsedDate` is read once per Dock app, at the first run only. An app pinned twice is one app and is read once. A date later than now is ignored. If the remaining dates of 5 or more apps fall within 5 seconds of each other (a migration signature), every Spotlight date is thrown out and every Dock app is first seen at the first run. `kMDItemDateAdded` is never read.
5. **Unknown is gentle.** An app that first appears in the Dock after the first run, with no launch, seed or first-seen time, is first seen when DockMoss first lists it. It starts clean and grows on the normal curve, and Spotlight is not read for it: a single date can't be checked against a migration batch.
6. **Jitter.** Add a per-app offset of up to ±1 day to the growth curve so apps with similar baselines don't all change stage on the same day. It comes from a fixed hash of the bundle ID, so an app's offset is the same on every run.
7. **Check periodically.** Quick test: `mdls -name kMDItemLastUsedDate /Applications/Safari.app`.

**Growth curve**

- **Clean hold.** An app used within the Fuzz threshold (5 days by default, shifted by its jitter) has no moss.
- **Linear growth.** From Fuzz, overgrowth rises linearly to 0.5 at Patchy (10 days) and to 1.0 at Overgrown (30 days), then stays at 1.0. It never jumps at a stage boundary.
- **Settings apply at once.** The Fuzz, Patchy and Overgrown thresholds and the fast-forward speed come from Settings, and a saved change reaches the overlay without a restart.
- **Excluded apps** get no moss: overgrowth 0 and stage clean, whatever their last use. They keep their place in the growth list.
- **Intensity** (0 to 100%) scales the overgrowth that is drawn. It does not change the stage, so an Overgrown app keeps its mushrooms or flowers at any intensity above 0. At 0 no icon has moss.
- **Fast-forward** (1x, 10x, 100x, 1000x) multiplies the time since last use. In debug builds the `DOCKMOSS_FAST_FORWARD` variable, when set to one of those values, replaces the saved speed; it never changes the setting. The jitter is added afterwards and never scaled. Scaled, it would be worth ±1,000 days at 1000x.
- **Threshold limits.** Thresholds are whole days with 2 ≤ Fuzz < Patchy < Overgrown ≤ 3650. Fuzz is at least one day more than the largest jitter, so jitter alone never puts moss on an app that was just used.
- **Running or just launched** means clean, whatever the jitter. An app with no last-used time at all shows no moss. After the first run every Dock app has a time (rules 3 and 5), so that happens only before the first run, for example while the Dock has never been visible.
- **Recompute.** Growth is recomputed after every launch, activation, quit and Dock change, and on a timer: every 5 minutes at 1x, and 5 minutes divided by the fast-forward factor otherwise, never more often than once a second.

## Stack (leading option)

- **Tauri 2 + Rust**, with `objc2` for the window level, click-through, workspace notifications, and Accessibility calls.
- Effects drawn inside the webview: procedural moss generated as shapes, baked once per level step on a Canvas2D canvas, and drawn from a WebGL2 texture atlas (ADR-0018). No sway yet.
- Menu-bar item for settings: thresholds, intensity, per-app exclusions, launch at login.
- Electron is the fallback if Tauri's transparent overlay proves fiddly, with a small native helper for Dock positions.

## Permissions

- **Accessibility** (required, for Dock geometry).
- No Full Disk Access expected, since `/Applications` metadata is readable. Verify early.
- Must be signed with a stable Developer ID, or macOS forgets the Accessibility grant on every rebuild.

## Known risks / open questions

- **Dock magnification lag:**
  - **Steady magnification:** once the tracker reads at 60 Hz, the moss trails the icons by about one frame (about 11 ms read to screen), the WebKit pipeline's floor. Predicting the motion in the renderer gained only 2 to 3 ms and was reverted (job 016).
  - **Entry:** the visible trail was the start of a magnification, while the tracker still polled at 2 Hz. It is fixed: the Dock's hover-change notification (`AXSelectedChildrenChanged`) wakes the tracker at once, and entry went from a median of 117 to 304 ms down to 17 ms (job 016, ADR-0017). The Dock posts no move or resize notification for its items, so reads stay polled.
  - **Launch lift:** fixed. The Dock posts nothing for a lift, so with the pointer away from the Dock the lift was followed at 2 Hz: 1 or 2 frames in its ~690 ms bounce, the first a median 186 ms after it began. A regular app's launch notification now holds the 60 Hz rate until 1 s after the Dock stops moving, at most 2 s, so the first frame comes a median 39 ms into the lift (the notification's own delay) and the bounce gets about 30 frames (job 019). After a click on the icon the pointer is near, so the lift was already followed within a frame (job 011).
- **Auto-hidden Dock:** icons sit offscreen, so draw nothing.
- **Slow start:** with the launch log starting empty, real moss only appears after weeks of use. The Spotlight seed covers part of it at the first run, and the fast-forward setting previews growth for testing and screenshots.
- **Apps launched outside our watch:** apps already running at first start, or launched while DockMoss was quit, won't be logged. Treat "app is running" as in use, and accept the gap while the app is closed.
- **Single-instance lock in shared `/tmp`:** `tauri-plugin-single-instance` keys "one DockMoss at a time" on a fixed socket, `/tmp/app_dockmoss_DockMoss_si.sock` (it follows the bundle identifier; it was `/tmp/com_dockmoss_app_si.sock` before job 014). Another local account can take that path first, so DockMoss exits at every launch or runs without the lock, and two launches within milliseconds can both run. Fixing it needs an upstream plugin change (job 013).
- **Data left in the old app folder:** job 014 changed the identifier from `com.dockmoss.app` to `app.dockmoss.DockMoss` and moves the launch log and settings once, at startup. A file whose move fails stays in the old folder, and that session runs without it (no usage tracking for the launch log; default settings, not saved, for the settings) until a later start moves it. On a volume without hard links (exFAT, some network home folders) the move copies each file into the new folder instead, only into a name no file holds, and then removes the old one. While a file is left behind, the menu-bar menu says so, for example "Usage history is still in the old folder". Nothing is lost or overwritten.
- **Moss that fits the icon shape:** icons are irregular, so moss grows from anchors laid along the icon's edges rather than reading the artwork. It starts as a thin strip along the bottom edge, climbs the sides, and reaches the top edge last. At level 1 it is a thick frame around the glyph, whose centre stays mostly clear (the user's look-check decision, job 017). The moss may overhang the icon by half the gap to each neighbouring icon, so it never reaches a neighbour on a bottom Dock; only its contact shadow is clipped to the icon's rounded rect. A side Dock is not handled: the overhang assumes a tile at least as tall as it is wide.
- **Battery:** keep the render loop idle when nothing is animating. Static moss only needs a redraw when the Dock moves. While the Dock is hidden (auto-hide, a full-screen Space) or cannot be read, the overlay window is ordered out, so WindowServer has nothing of ours to composite; it is ordered back in when the Dock shows. Every Dock read also costs the Dock process CPU, so the tracker reads fast only while something moves near the Dock, or for at most 2 s after an app launches (about 100 reads a launch). A continuous animation is the expensive case: a 30 fps `requestAnimationFrame` sway cost about 19% CPU (job 010), so the sway loop was removed in job 017 and sway waits for its own job. The moss is baked: each icon's moss is drawn once per level step (24 steps), at most two bakes a frame, and every later frame samples the cached bitmaps, so a magnification only moves boxes. The launch shrivel is bounded instead: it runs the same 30 fps loop for 600 ms per mossy app opened, then the loop stops and no frame callback runs until something changes again (job 011).
- **Many icons:** use one shared shader or instanced draw rather than a layer per icon. Since slice 10, every mossy icon is one instance of a single draw call; since job 017, each samples its own layer of one texture array of baked moss.

## Possible milestones

1. Overlay window above the Dock, click-through, all Spaces.
2. Track Dock icon rects and app paths via Accessibility.
3. Launch log via `NSWorkspace`, with Spotlight seeding and migration detection.
4. Map effective last-used to a 0-1 overgrowth value (with jitter).
5. Draw a first moss effect on one icon, then all icons.
6. Launch-clears-moss animation and menu-bar settings (including fast-forward).
7. Signing, notarization, and polish.

## Later ideas

- Seasonal variants (autumn leaves, snow, cobwebs).
- Mushrooms or tiny critters on the worst offenders.
- A "garden" view listing your most neglected apps, with a one-click uninstall suggestion.
