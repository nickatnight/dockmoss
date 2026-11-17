# Icons

The bundle icons and the tray glyph are generated from the SVG masters in `src-tauri/icons/source/`.

## App icon

`src-tauri/icons/source/app-icon.svg` is generated, so don't edit it by hand. `appIconMaster()` in `src/icon/app-icon.ts` writes it: the stone tile with the moss generator's output (`src/render/generator.ts`) drawn over it, grown at level step 16 of 24 from the seed `fnv1a32("dockmoss#0")`, without props, and laid out as the overlay lays moss on a Dock tile (`ART_SIDE_PER_TILE_WIDTH`). The marketing site's logo is the same document: this repo is the source, and the site ports `src/icon/` from it (ADR-0019).

A vitest file snapshot fails `pnpm test` when the master is stale. After a change to the icon or to the moss generator:

```sh
pnpm test src/icon -u    # rewrite app-icon.svg; the filter goes before -u
# re-pin APP_ICON_SHA256 in src/icon/app-icon.test.ts (the failing test prints the new value)
pnpm tauri icon src-tauri/icons/source/app-icon.svg
# then delete the non-macOS outputs (android/, ios/, icon.ico, Square*Logo.png, StoreLogo.png, 64x64.png)
```

- **Put the filter before `-u`.** Vitest's `-u` takes an optional value (`--update [type]`), so in `pnpm test -u src/icon` vitest reads `src/icon` as that value and updates snapshots across the whole suite.
- **Re-pin the SHA-256 by hand.** `-u` never rewrites `APP_ICON_SHA256`, so the SHA test stays red until you re-pin it. A changed icon is always a deliberate edit.
- **Re-port the site.** The site then re-ports `src/icon/` and pins the same SHA-256 in its own test.
- **Rerun `pnpm tauri icon`.** No test fails if you skip it.

Two tests parse `appIconSvg()` and `appIconMaster()` with fast-xml-parser's `XMLValidator`. It catches missing or mismatched closing tags, duplicate attributes and unquoted attributes, but accepts some things XML forbids: `--` inside a comment (the master test checks this separately), a comment ending in `--->`, `&` or `<` inside an attribute value, and an empty second root element. After a change to the header comment or the document's structure, also run:

```sh
xmllint --noout src-tauri/icons/source/app-icon.svg
```

`pnpm tauri icon` writes the chunks of `icon.icns` in a different order on each run, so a regenerated `icon.icns` differs byte-for-byte even when the artwork is identical. Commit it only when `app-icon.svg` changed.

## Banner

`docs/readme/banner.svg`, the animation at the top of the repo's README, is generated too, so don't edit it by hand. `bannerMaster()` in `src/icon/banner.ts` writes it. Every tile's moss is the generator's output, seeded from `fnv1a32("banner#<i>")` for the tile at Dock position `i`, and the Dock's last tile is the app icon's stone (`STONE_TILE` from `app-icon.ts`). The shrivel uses what the overlay uses, all from `src/render/model.ts`. `shrivelRemaining`, `shrivelWither` and `SHRIVEL_MS` are the curves and length `shrivelInstance` hands the shader. `SHRIVEL_FRONT_WIDTH`, `SHRIVEL_FRONT_REACH`, `WITHER_TINT` and `REC709_LUMA` are the constants `moss.frag.glsl` gets at `#pragma moss_constants`. `shrivelFront` places the front; the shader repeats its product per pixel, and a test in `src/render/model.test.ts` holds the shader's text to it. The front's soft edge is the shader's `smoothstep`, sampled at five gradient stops. The animation is SMIL on one 12 s clock. GitHub plays SMIL inside an `<img>` but strips `<script>`, `<foreignObject>` and external references, and the tests refuse all three.

A vitest file snapshot fails `pnpm test` when the banner is stale, and a pinned SHA-256 fails it on any change to the art or the generator. After a deliberate change:

```sh
pnpm test src/icon -u    # rewrite banner.svg (and app-icon.svg); the filter goes before -u
# re-pin BANNER_SHA256 in src/icon/banner.test.ts (the failing test prints the new value)
xmllint --noout docs/readme/banner.svg
```

A test keeps the file under 1 MB; it is about 610 KB. Look at the result in a browser before you commit it: open `docs/readme/banner.svg` in Safari and in Chrome and watch a whole loop. Each animated attribute's static value is its value as the pointer sets off for Notes (6.9 s), so a viewer that ignores SMIL shows the grown Dock with the pointer at rest, not a bare one. Quick Look does run the SMIL: its thumbnail (`qlmanage -t`) shows the bare first frame, not the grown Dock.

## Tray glyph

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

The current glyph sits exactly on the relief floor. In `tray.svg` the centre tuft tops out at y=24, the outer shoulders at y=25 and the valley floors at y=29, so the shoulder side gives exactly 4 px. Any edit that lowers a shoulder or raises a floor fails the check. If the relief has to grow, raise the tops rather than lowering the floors, so the moss stays in the bottom third. Raising the centre alone doesn't help, because the shoulder side is the shallower one; move the centre to y=23 and the shoulders to y=24 together, which gives 5 px and keeps the centre highest. The tuft arcs are tangent to 3 px valley fillets, so recompute the tangent points instead of nudging single coordinates.
