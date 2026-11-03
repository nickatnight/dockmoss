# DockMoss

A novelty macOS app that grows moss on Dock icons for apps you haven't opened in a while. The longer an app sits unused, the more overgrown it gets. Launch the app and the moss retreats.

## Concept

- A transparent, click-through overlay sits above the Dock.
- Each Dock app icon gets a moss layer whose coverage scales with days since the app was last used.
- Stages (tunable): clean, light fuzz (5 days), patchy moss (10 days), fully overgrown with tiny mushrooms or flowers (30+ days).
- Opening the app clears its moss, with a small "shrivel" animation.

## How it works

1. **Find icons:** read Dock item geometry and app paths through the Accessibility API (`AXUIElement`, Dock process; app dock items expose position, size and URL).
2. **Track usage:** record launches ourselves (see "Last-used data" below) and compute an overgrowth value from 0 to 1 per app.
3. **Draw:** render moss per icon in the overlay, repositioned each frame to follow Dock magnification.
4. **Refresh:** update on app launch/quit notifications, and recompute growth every few minutes.

## Last-used data

Spotlight metadata turned out to be unreliable. In a real test on one Mac, 12 of 24 apps returned `(null)` for `kMDItemLastUsedDate`, and nearly every `kMDItemDateAdded` landed in the same two seconds, which points to an OS update or migration re-stamping the data. So Spotlight is a hint, not the source of truth.

**Rules**

1. **Own launch log is the source of truth.** Listen for `NSWorkspace` launch, activation and quit notifications and store a per-app timestamp (keyed by bundle ID) in our own app data. Quitting an app counts as using it, so "last used" is the last time the app was running or frontmost, and moss starts growing from when it quit. Apps already running when DockMoss starts count as used at that moment. The log survives OS updates and can't be silently rewritten.
2. **Effective last-used** is the latest of: our launch log, `kMDItemLastUsedDate`, and the first-run baseline below.
3. **First-run grace period.** On first run, treat every app with no launch history as "last seen today" and let moss grow from there, so the Dock isn't green on day one.
4. **Spotlight as a seed only.** Use `kMDItemLastUsedDate` to seed the initial state when it looks sane. Ignore `kMDItemDateAdded` and Spotlight dates entirely when many apps share an identical timestamp (a migration signature, e.g. 5+ apps within a few seconds).
5. **Unknown is gentle.** Apps with no data never jump straight to overgrown; they start young and grow on the normal curve.
6. **Jitter.** Add a per-app offset of up to ±1 day to the growth curve so apps with similar baselines don't all change stage on the same day. It comes from a fixed hash of the bundle ID, so an app's offset is the same on every run.
7. **Check periodically.** Quick test: `mdls -name kMDItemLastUsedDate /Applications/Safari.app`.

**Growth curve**

- **Clean hold.** An app used within the Fuzz threshold (5 days by default, shifted by its jitter) has no moss.
- **Linear growth.** From Fuzz, overgrowth rises linearly to 0.5 at Patchy (10 days) and to 1.0 at Overgrown (30 days), then stays at 1.0. It never jumps at a stage boundary.
- **Fast-forward** (1x, 10x, 100x, 1000x) multiplies the time since last use. The jitter is added afterwards and never scaled. Scaled, it would be worth ±1,000 days at 1000x.
- **Threshold limits.** Thresholds are whole days with 2 ≤ Fuzz < Patchy < Overgrown ≤ 3650. Fuzz is at least one day more than the largest jitter, so jitter alone never puts moss on an app that was just used.
- **Running or just launched** means clean, whatever the jitter. An app with no last-used time at all shows no moss. Rule 3's baseline gives every app present at first run a time. Slice 8 is to give apps that first appear later a first-seen time, so they grow as rule 5 says.

## Stack (leading option)

- **Tauri 2 + Rust**, with `objc2` for the window level, click-through, workspace notifications, and Accessibility calls.
- Effects drawn in canvas/WebGL inside the webview (procedural moss: noise-driven patches, slight sway).
- Menu-bar item for settings: thresholds, intensity, per-app exclusions, launch at login.
- Electron is the fallback if Tauri's transparent overlay proves fiddly, with a small native helper for Dock positions.

## Permissions

- **Accessibility** (required, for Dock geometry).
- No Full Disk Access expected, since `/Applications` metadata is readable. Verify early.
- Must be signed with a stable Developer ID, or macOS forgets the Accessibility grant on every rebuild.

## Known risks / open questions

- **Dock magnification lag:** polling icon positions can trail the animation by a frame. Mask or accept it; consider an `AXObserver` later.
- **Auto-hidden Dock:** icons sit offscreen, so draw nothing.
- **Slow start:** with the launch log starting empty, real moss only appears after weeks of use. Consider a "demo/fast-forward" setting for testing and screenshots.
- **Apps launched outside our watch:** apps already running at first start, or launched while DockMoss was quit, won't be logged. Treat "app is running" as in use, and accept the gap while the app is closed.
- **Moss that fits the icon shape:** icons are irregular, so grow moss from the bottom edge and corners, clipped to the icon's rounded rect, rather than reading the artwork.
- **Battery:** keep the render loop idle when nothing is animating. Static moss only needs a redraw when the Dock moves. While the Dock is hidden (auto-hide, a full-screen Space) or cannot be read, the overlay window is ordered out, so WindowServer has nothing of ours to composite; it is ordered back in when the Dock shows.
- **Many icons:** use one shared shader or instanced draw rather than a layer per icon.

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
