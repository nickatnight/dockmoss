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
  - **Launch lift:** fixed. The Dock posts nothing for a lift, so with the pointer away from the Dock the lift was followed at 2 Hz: 1 or 2 frames in its ~690 ms bounce, the first a median 186 ms after it began. A regular app's launch notification now holds the 60 Hz rate until 1 s after the Dock stops moving, at most 2 s after the latest launch (so a second app launched during the first one's hold is followed too), so the first frame comes a median 39 ms into the lift (the notification's own delay) and the bounce gets about 30 frames (job 019). After a click on the icon the pointer is near, so the lift was already followed within a frame (job 011).
- **Auto-hidden Dock:** icons sit offscreen, so draw nothing.
- **Switching Spaces:** every window DockMoss can own slides with the Space animation; only the Dock stays still. So when the Dock comes back from off screen (a switch from a full-screen Space) with the pointer more than 150 pt above the bottom edge, the overlay stays out until the switch ends: the Dock's hover notice or the Space notice, both at the end of the animation (about 940 to 960 ms after the key), or 1.2 s. The moss then appears on its icons with no slide, a median 985 ms after the key (980 to 1028 ms, 10 runs), against a slide that began a median 400 ms after the key, up to 536 pt off, and landed at about 844 ms before (job 021, ADR-0020). With the pointer within 150 pt of the bottom edge the Dock's return counts as an auto-hide reveal, and the moss comes in at once and follows the Dock up; a Space switch made with the pointer that low still slides. Desktop to desktop is unchanged (the Dock never leaves the screen; not measured). A trackpad swipe was not measured: a swipe held longer than the 1.2 s cap shows the slide for its rest. Known limits of telling a switch from a reveal by the pointer's height:
  - A Dock that comes back from off screen for another reason, with the pointer away from the bottom edge, gets no end notice and is held until the cap. Measured with an app that auto-hides the Dock (`NSApplicationPresentationAutoHideDock`) and quits: the moss came in place 1.33 s after the app quit, against 0.62 s on `main`, where it followed the Dock up at 2 Hz; the Dock started back about 0.09 s after the quit on both (job 021, review round 1). Turning auto-hide off while the Dock is hidden should do the same (not measured).
  - A side Dock with auto-hide is revealed with the pointer at a side edge, so its reveal is held until a hover notice or the cap. The reach is measured from the bottom edge only.
  - A switch reversed before its animation ends (back within about 940 ms) was not measured. A notice from the first switch that arrives while the second is held would end that hold early, and the moss would slide for the rest of the second animation, as before this change.
  - A notice that comes before the read that first sees the Dock back is dropped, so if that read is ever late by more than about 400 ms, that one return waits for the cap. It is not seen in practice: the Dock is readable 25 ms into a switch and the notices come at about 940 ms.
- **Slow start:** with the launch log starting empty, real moss only appears after weeks of use. The Spotlight seed covers part of it at the first run, and the fast-forward setting previews growth for testing and screenshots.
- **Apps launched outside our watch:** apps already running at first start, or launched while DockMoss was quit, won't be logged. Treat "app is running" as in use, and accept the gap while the app is closed.
- **Single-instance lock in shared `/tmp`:** `tauri-plugin-single-instance` keys "one DockMoss at a time" on a fixed socket, `/tmp/app_dockmoss_DockMoss_si.sock` (it follows the bundle identifier; it was `/tmp/com_dockmoss_app_si.sock` before job 014). Another local account can take that path first, so DockMoss exits at every launch or runs without the lock, and two launches within milliseconds can both run. Fixing it needs an upstream plugin change (job 013).
- **Data left in the old app folder:** job 014 changed the identifier from `com.dockmoss.app` to `app.dockmoss.DockMoss` and moves the launch log and settings once, at startup. A file whose move fails stays in the old folder, and that session runs without it (no usage tracking for the launch log; default settings, not saved, for the settings) until a later start moves it. On a volume without hard links (exFAT, some network home folders) the move copies each file into the new folder instead, only into a name no file holds, and then removes the old one. A copy cut short by a crash is redone at the next start, because the short file is a byte-for-byte start of the old one. While a file is left behind, the menu-bar menu says so. Nothing is overwritten, and no data is lost: a file stays in the old folder until its whole copy is on disk.
- **Moss that fits the icon shape:** icons are irregular, so moss grows from anchors laid along the icon's edges rather than reading the artwork. It starts as a thin strip along the bottom edge, climbs the sides, and reaches the top edge last. At level 1 it is a thick frame around the glyph, whose centre stays mostly clear (the user's look-check decision, job 017). The moss may overhang the icon by half the gap to each neighbouring icon, so it never reaches a neighbour on a bottom Dock; only its contact shadow is clipped to the icon's rounded rect. A side Dock is not handled: the overhang assumes a tile at least as tall as it is wide.
- **Battery:** keep the render loop idle when nothing is animating. Static moss only needs a redraw when the Dock moves. While the Dock is hidden (auto-hide, a full-screen Space) or cannot be read, the overlay window is ordered out, so WindowServer has nothing of ours to composite; it is ordered back in when the Dock shows. Every Dock read also costs the Dock process CPU, so the tracker reads fast only while something moves near the Dock, or for at most 2 s after each app launch (about 100 reads a launch, at about 10% Dock CPU while it lasts). Launches that keep coming less than about 2 s apart, such as login items at login, keep 60 Hz reads running for as long as they come. A launch while the Dock is hidden or cannot be read starts no hold; so a launch from a full-screen Space that switches to a desktop Space is followed at 2 Hz, and only from the end of the switch (a known limit: the Space notice comes only at the end of the switch's animation, about 950 ms in, so it cannot start a launch hold sooner; job 021). A Space switch adds no `Fast` reads: while the overlay is held after it (see "Switching Spaces"), the tracker reads at the rate it had, and the hold ends on the read the end-of-switch notice brings, or on one read at the 1.2 s cap if no notice comes. Five reads in the 2 s after a full-screen to desktop switch, as before (job 021). A continuous animation is the expensive case: a 30 fps `requestAnimationFrame` sway cost about 19% CPU (job 010), so the sway loop was removed in job 017 and sway waits for its own job. The moss is baked: each icon's moss is drawn once per level step (24 steps), at most two bakes a frame, and every later frame samples the cached bitmaps, so a magnification only moves boxes. The launch shrivel is bounded instead: it runs the same 30 fps loop for 600 ms per mossy app opened, then the loop stops and no frame callback runs until something changes again (job 011).
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
