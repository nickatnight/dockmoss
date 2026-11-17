# DockMoss

A novelty macOS app that grows moss on Dock icons for apps you haven't opened in a while. The longer an app sits unused, the more overgrown it gets. Open the app and the moss shrivels away.

This file holds the product rules: what counts as "used", how moss grows, and the known limits. How it is built is in [ARCHITECTURE.md](ARCHITECTURE.md).

## Concept

- A transparent, click-through overlay sits above the Dock.
- Each app icon in the Dock gets moss that grows with the days since the app was last used.
- Stages: clean, Fuzz (5 days), Patchy (10 days), Overgrown (30 days or more, with a tiny mushroom or flower). The thresholds are settings.
- Opening an app clears its moss with a short shrivel. A settings change that clears a mossy icon (excluding the app, intensity 0, higher thresholds) plays the same shrivel.

## When an app was last used

Spotlight's last-used date is unreliable. On one test Mac, half the apps had none, and nearly every "date added" fell in the same two seconds, a sign that an OS update had rewritten them. So DockMoss keeps its own log, and Spotlight is only a hint at the first run.

1. **The launch log is the source of truth.** DockMoss stores a time per bundle ID whenever an app launches, becomes active or quits. Quitting counts as use, so moss starts growing when the app quits. Apps already running when DockMoss starts count as used at that moment. DockMoss never records itself, including a second copy starting up.
2. **Last used** is the latest of three times: the launch log, the app's Spotlight seed, and its first-seen time. An app has a Spotlight seed or a first-seen time, never both.
3. **First run.** The first time DockMoss reads a Dock that lists apps, and the log has no `baseline_at`, each Dock app with no launch history gets one of:
   - its Spotlight last-used date as a seed, if the date is sane (rule 4), so an app unused for weeks is mossy on day one;
   - otherwise a first-seen time of now, so the Dock isn't green on day one.

   DockMoss then sets `baseline_at` to now. It only marks that the first run is done; it is not a last-used time. A missing log, a log set aside as corrupt, and a log written before `baseline_at` existed all lead to a first run. Without the Accessibility grant, or while the Dock stays hidden, the first run waits.
4. **Spotlight is a seed only.** `kMDItemLastUsedDate` is read once per Dock app, at the first run only. An app pinned twice is read once. A date in the future is ignored. If 5 or more of the remaining dates fall within 5 seconds of each other (the mark of an OS migration), every Spotlight date is thrown out and every Dock app is first seen at the first run. `kMDItemDateAdded` is never read.
5. **New apps start clean.** An app that first appears in the Dock after the first run, with no other time, is first seen when DockMoss first lists it. Spotlight is not read for it, because a single date can't be checked against a migration batch.
6. **Jitter.** Each app's growth curve is shifted by up to ±1 day, so apps used on the same day don't all change stage together. The shift is a fixed hash of the bundle ID, so it is the same on every run.

To see what Spotlight has for an app: `mdls -name kMDItemLastUsedDate /Applications/Safari.app`.

## Growth curve

- **Clean at first.** An app used within the Fuzz threshold (shifted by its jitter) has no moss.
- **Straight-line growth.** From 0 at Fuzz, overgrowth rises to 0.5 at Patchy and 1.0 at Overgrown, then stays at 1.0. It never jumps at a stage boundary.
- **Running means clean.** A running or just-launched app has no moss, whatever its jitter.
- **No time, no moss.** An app with no last-used time has no moss. After the first run every Dock app has one, so this happens only before it, for example while the Dock has never been visible.
- **Thresholds** are whole days, with 2 ≤ Fuzz < Patchy < Overgrown ≤ 3650. Fuzz's minimum is one day more than the largest jitter, so jitter alone never puts moss on an app that was just used.
- **Excluded apps** get no moss (overgrowth 0, stage clean) but keep their place in the growth list.
- **Intensity** (0 to 100%) scales how much moss is drawn, not the stage, so an Overgrown app keeps its mushroom or flower at any intensity above 0. At 0, no icon has moss.
- **Fast-forward** (1×, 10×, 100×, 1000×) multiplies the time since last use. Jitter is added afterwards and never scaled; at 1000× it would be worth ±1,000 days. In debug builds, `DOCKMOSS_FAST_FORWARD` set to one of those values replaces the saved speed without changing the setting.
- **Settings apply at once.** A saved change reaches the overlay without a restart.
- **Recompute** after every launch, activation, quit and Dock change, and on a timer: every 5 minutes divided by the fast-forward factor, never more than once a second.

## How the moss looks

Icons are irregular, so moss grows from anchors along the icon's edges instead of reading its artwork. It starts as a thin strip along the bottom edge, climbs the sides, and reaches the top edge last. Fully grown it is a thick frame around the glyph, whose centre stays mostly clear. Moss may overhang the icon by half the gap to each neighbour, so it never reaches a neighbour on a bottom Dock; only its contact shadow is clipped to the icon's rounded rect.

## Stack

- **Tauri 2 and Rust**, with `objc2` for the overlay window, workspace notifications and Accessibility calls.
- **Moss in the webview:** generated as shapes, baked once per growth step on a Canvas2D canvas, and drawn from a WebGL2 texture array (ADR-0018). It doesn't sway yet.
- **Menu-bar item** for Settings: thresholds, intensity, fast-forward, excluded apps, launch at login.

## Permissions

- **Accessibility** is required, to read the Dock.
- **No Full Disk Access:** `/Applications` metadata is readable.
- **A stable Developer ID signature** is needed, or macOS forgets the Accessibility grant on every rebuild.

## Battery and CPU

- **Nothing runs while nothing changes.** The renderer redraws only when the Dock moves or growth changes. Moss is baked once per growth step (24 steps, at most two bakes a frame), so a magnification only moves boxes.
- **No continuous animation.** A 30 fps sway cost about 19% CPU, so it was removed until it can be done cheaply. The shrivel runs a 30 fps loop for 600 ms per mossy app opened, then stops.
- **The overlay leaves the screen** while the Dock is hidden (auto-hide, a full-screen Space) or can't be read, so WindowServer has nothing of DockMoss's to draw.
- **Every Dock read costs the Dock CPU.** The tracker reads at 2 Hz unless something near the Dock moves. It reads at 60 Hz only:
  - while the pointer moves near the Dock, or the Dock itself is moving;
  - for at most 2 s after each app launch (about 100 reads, at about 10% Dock CPU while it lasts). Launches that keep coming less than 2 s apart, such as login items, keep it running as long as they come;
  - for at most 1 s after the pointer leaves a Dock that auto-hides, until the Dock is off screen (11 to 16 reads a hide).
- **A Space switch adds no fast reads.**
- **Many icons cost one draw.** Every mossy icon is one instance of a single draw call, sampling its own layer of one texture array.

## Known limits

### Following the Dock

- **Magnification.** Moss trails the icons by about one frame (about 11 ms), the floor of WebKit's pipeline. Predicting the motion saved only 2 to 3 ms, so it was dropped.
- **Hover.** The Dock's hover notice (`AXSelectedChildrenChanged`) wakes the tracker at once, so moss keeps up from the start of a magnification (ADR-0017). The Dock posts nothing when its items move or resize, so every read after that is polled.
- **Launch bounce.** The Dock posts nothing for a bouncing icon either, so a launch holds 60 Hz reads until 1 s after the Dock stops moving, at most 2 s after the latest launch. The first frame comes about 40 ms into the bounce, which is the launch notice's own delay. A launch while the Dock is hidden starts no hold, so a launch from a full-screen Space that switches to a desktop is followed at 2 Hz once the switch ends.
- **Auto-hidden Dock.** When the pointer leaves a Dock that auto-hides, DockMoss reads at 60 Hz until the Dock is off screen, so the moss slides away with it and is gone about 33 ms after the Dock (it used to linger about 300 ms). It reads the auto-hide setting from the Dock's preferences only on that exit (ADR-0021). Reading faster wouldn't halve the 33 ms: much of it is moss drawn a few points above the icon, and the measurement's own frame. Two hides are still followed at only 2 Hz:
  - after a Dock menu or a drag, because the 1 s hold runs out while the menu holds the Dock open, and the pointer never comes back to start another;
  - the end of a slide slowed past 1 s with `autohide-time-modifier`.
- **Switching Spaces.** Every DockMoss window slides with the Space animation, but the Dock stays still. So when the Dock comes back from a full-screen Space with the pointer more than 150 pt above the bottom edge, the overlay stays out until the switch ends (the Dock's hover notice or the Space notice, about 950 ms after the key, or 1.2 s at most). The moss then appears in place, with no slide (ADR-0020). With the pointer within 150 pt of the bottom edge, the return counts as an auto-hide reveal, and the moss follows the Dock up at once. Desktop-to-desktop switches are unchanged. Telling a switch from a reveal by the pointer's height has limits:
  - Any other return from off screen with the pointer high, such as when an app that hid the Dock quits, waits for the 1.2 s cap.
  - A side Dock with auto-hide is revealed at a side edge, so it waits for a hover notice or the cap.
  - A notice that arrives before the first read that sees the Dock back is dropped, and that return waits for the cap. It needs a read about 400 ms late, which hasn't been seen.
  - Not measured: trackpad swipes (a swipe longer than 1.2 s shows the slide for the rest of it) and a switch reversed within about 940 ms (a leftover notice could end the second hold early, and the moss would slide as it did before).
- **Side Docks** aren't handled: the moss's overhang assumes a tile at least as tall as it is wide.

### Data

- **Slow start.** With an empty log, real moss takes weeks. Spotlight seeds cover part of that at the first run, and fast-forward previews growth.
- **Apps used while DockMoss is quit** aren't logged. A running app counts as in use; the gap while it is closed is accepted.
- **Data in the old app folder.** Older builds stored data under `com.dockmoss.app`. DockMoss moves it once at startup, into `app.dockmoss.DockMoss`, and never overwrites a file: it hard-links then removes, or copies where hard links aren't supported (exFAT, some network home folders). A copy cut short by a crash is redone at the next start. A file that fails to move stays where it was, the menu-bar menu says so, and that session runs without it: no usage tracking if the launch log was left, or unsaved default settings if the settings were. The next start tries again. No data is lost.
- **Single-instance lock in shared `/tmp`.** The single-instance plugin uses a fixed socket, `/tmp/app_dockmoss_DockMoss_si.sock`. Another local account can take that path first, so DockMoss exits at every launch or runs without the lock, and two launches within milliseconds can both run. Fixing it needs a change upstream in the plugin.

## Milestones

1. Overlay window above the Dock, click-through, on all Spaces. Done.
2. Dock icon rects and app paths via Accessibility. Done.
3. Launch log via `NSWorkspace`, with Spotlight seeding and migration detection. Done.
4. Effective last-used mapped to a 0 to 1 overgrowth, with jitter. Done.
5. Moss on one icon, then all icons. Done.
6. The launch shrivel and menu-bar settings, including fast-forward. Done.
7. Signing, notarization and polish. Next.

## Later ideas

- Seasonal variants (autumn leaves, snow, cobwebs).
- Mushrooms or tiny critters on the worst offenders.
- A "garden" view listing your most neglected apps, with a one-click uninstall suggestion.
