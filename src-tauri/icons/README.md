# Icons

The bundle icons and the tray glyph are generated from the SVG masters in `src-tauri/icons/source/`.

## App icon

`src-tauri/icons/source/app-icon.svg` is generated, so don't edit it by hand. `appIconMaster()` in `src/icon/app-icon.ts` writes it: a stone tile with the real moss generator's output (`src/render/generator.ts`) drawn over it, at growth step 16 of 24, seeded with `fnv1a32("dockmoss#0")`, without props, laid out as the overlay lays moss on a Dock tile. The marketing site's logo is the same file: this repo is the source, and the site copies `src/icon/` from it (ADR-0019).

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

Two tests check the SVGs with fast-xml-parser's `XMLValidator`, which misses some XML errors, such as `&` or `<` inside an attribute value. After changing the header comment or the document's structure, also run:

```sh
xmllint --noout src-tauri/icons/source/app-icon.svg
```

`pnpm tauri icon` writes the chunks of `icon.icns` in a different order on each run, so a regenerated `icon.icns` differs byte-for-byte even when the artwork is identical. Commit it only when `app-icon.svg` changed.

## Banner

`docs/readme/banner.svg`, the animation at the top of the repo's README, is generated too, so don't edit it by hand. `bannerMaster()` in `src/icon/banner.ts` writes it. It is built from the app's own parts, so it looks like the real thing:

- each tile's moss comes from the moss generator, seeded with `fnv1a32("banner#<i>")` for the tile at Dock position `i`;
- the last tile is the app icon's stone (`STONE_TILE`);
- the shrivel uses the overlay's curves, timing and shader constants from `src/render/model.ts`, and a test in `model.test.ts` holds the shader to the same maths.

The animation is SMIL on one 12 s clock. GitHub plays SMIL inside an `<img>` but strips `<script>`, `<foreignObject>` and external references, so the tests refuse all three.

A vitest file snapshot fails `pnpm test` when the banner is stale, and a pinned SHA-256 fails it on any change to the art or the generator. After a deliberate change:

```sh
pnpm test src/icon -u    # rewrite banner.svg (and app-icon.svg); the filter goes before -u
# re-pin BANNER_SHA256 in src/icon/banner.test.ts (the failing test prints the new value)
xmllint --noout docs/readme/banner.svg
```

A test keeps the file under 1 MB (it is about 610 KB). Before you commit, open it in Safari and in Chrome and watch a whole loop. A viewer that ignores SMIL shows each attribute's static value, which is set to the moment the pointer sets off for Notes (6.9 s), so it shows the grown Dock rather than a bare one. Quick Look's thumbnail (`qlmanage -t`) does run the SMIL, so it shows the bare first frame.

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

The current glyph is exactly at the relief limit: the centre tuft tops out at y=24, the outer shoulders at y=25 and the valley floors at y=29, so the shoulder side gives exactly 4 px. To add relief, raise the tops rather than lowering the floors, so the moss stays in the bottom third. Move the centre and the shoulders together (to y=23 and y=24 gives 5 px); raising the centre alone doesn't help. The tuft arcs are tangent to 3 px valley fillets, so recompute the tangent points rather than nudging single coordinates.
