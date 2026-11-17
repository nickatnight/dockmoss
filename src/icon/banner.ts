import { ICON_UNITS, LEVEL_STEPS, type LevelStep, mossScene } from "../render/generator";
import type { LogicalRect } from "../ipc";
import { artBox, CORNER_RADIUS_PER_SIDE, fnv1a32 } from "../render/model";
import { SIDE as STONE_TILE_SIDE, STONE_DEFS, STONE_TILE } from "./app-icon";
import { ICON_CLIP_PATH, sceneSvg } from "./svg";

const WIDTH = 1600;
const HEIGHT = 900;

const n = (value: number): string => String(Number(value.toFixed(2)));

/** A moss stage a tile reaches, and when in the loop, in seconds. */
export interface Stage {
  at: number;
  step: LevelStep;
}

export interface BannerTile {
  /** The tile's art in a 100-unit square, or the app icon's stone. */
  glyph: string | "stone";
  /** Its moss stages in order; none for an app in daily use. */
  stages: readonly Stage[];
  running: boolean;
}

const stage = (at: number, step: number): Stage => ({ at, step: step as LevelStep });

const TILE_RADIUS = 100 * CORNER_RADIUS_PER_SIDE;

const tileBase = (top: string, bottom: string, id: string): string =>
  `<linearGradient id="${id}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${top}"/><stop offset="1" stop-color="${bottom}"/></linearGradient>`;

const tileRect = (id: string): string =>
  `<rect width="100" height="100" rx="${n(TILE_RADIUS)}" fill="url(#${id})" filter="url(#drop)"/>`;

const headerBand = (height: number, color: string): string =>
  `<path d="M0 ${n(TILE_RADIUS)}A${n(TILE_RADIUS)} ${n(TILE_RADIUS)} 0 0 1 ${n(TILE_RADIUS)} 0H${n(100 - TILE_RADIUS)}A${n(TILE_RADIUS)} ${n(TILE_RADIUS)} 0 0 1 100 ${n(TILE_RADIUS)}V${height}H0Z" fill="${color}"/>`;

const FILES = `<path d="M20 30H40L46 36H78Q82 36 82 40V72Q82 76 78 76H22Q18 76 18 72V34Q18 30 22 30Z" fill="#BFD9F6"/>
<rect x="18" y="44" width="64" height="32" rx="4" fill="#F4F8FD"/>`;

const CALENDAR_DAYS = Array.from({ length: 12 }, (_, day) => {
  const [column, row] = [day % 4, Math.floor(day / 4)];
  const today = day === 6;
  return `<rect x="${21 + column * 15}" y="${41 + row * 14}" width="10" height="9" rx="2" fill="${today ? "#E5533D" : "#C9CED6"}"/>`;
}).join("");

const CALENDAR = `${headerBand(28, "#E5533D")}${CALENDAR_DAYS}`;

const NOTES = `${headerBand(24, "#F0B32E")}
<g fill="#A97A12" fill-opacity="0.45"><rect x="20" y="38" width="60" height="5" rx="2.5"/><rect x="20" y="51" width="60" height="5" rx="2.5"/><rect x="20" y="64" width="60" height="5" rx="2.5"/><rect x="20" y="77" width="38" height="5" rx="2.5"/></g>`;

const MUSIC = `<g fill="#FFFFFF"><ellipse cx="36" cy="70" rx="10" ry="7.5" transform="rotate(-18 36 70)"/><ellipse cx="68" cy="63" rx="10" ry="7.5" transform="rotate(-18 68 63)"/>
<rect x="42" y="31" width="5" height="40"/><rect x="74" y="24" width="5" height="40"/><path d="M42 31L79 23V33L42 41Z"/></g>`;

const PHOTOS = `<circle cx="67" cy="34" r="10" fill="#F6C445"/>
<path d="M14 80L38 46L52 64L64 52L88 80Z" fill="#9FB6D6"/><path d="M14 80L38 46L48 60L36 80Z" fill="#DDE7F3"/>`;

/**
 * The Dock, left to right. Files is in daily use and stays clean. The rest go
 * unused at staggered times, so they reach their stages out of step; Notes
 * grows all the way to Overgrown, and the stone stops at the app icon's step 16.
 */
export const BANNER_TILES: readonly BannerTile[] = [
  { glyph: `${tileBase("#5B9BE0", "#2F6FB8", "tile-0")}${tileRect("tile-0")}${FILES}`, stages: [], running: true },
  {
    glyph: `${tileBase("#FFFFFF", "#E6E9ED", "tile-1")}${tileRect("tile-1")}${CALENDAR}`,
    stages: [stage(1.8, 6), stage(3.4, 12)],
    running: false,
  },
  {
    glyph: `${tileBase("#FFE07A", "#F6C443", "tile-2")}${tileRect("tile-2")}${NOTES}`,
    stages: [stage(1, 6), stage(2.6, 12), stage(5, LEVEL_STEPS)],
    running: false,
  },
  {
    glyph: `${tileBase("#FF7A86", "#E3465A", "tile-3")}${tileRect("tile-3")}${MUSIC}`,
    stages: [stage(2.6, 6), stage(4.2, 12)],
    running: false,
  },
  {
    glyph: `${tileBase("#3B5C92", "#22375C", "tile-4")}${tileRect("tile-4")}${PHOTOS}`,
    stages: [stage(1, 6)],
    running: false,
  },
  { glyph: "stone", stages: [stage(1, 6), stage(3.4, 12), stage(5.8, 16)], running: false },
];

/** The laptop's screen, and the Dock on it: one tile per item, side by side, as AX reports them. */
const SCREEN = { x: 300, y: 106, width: 1000, height: 625 } as const;
const TILE = 140;
const DOCK_PADDING = 12;
const DOCK_WIDTH = BANNER_TILES.length * TILE + 2 * DOCK_PADDING;
const DOCK_X = SCREEN.x + (SCREEN.width - DOCK_WIDTH) / 2;
const DOCK_Y = SCREEN.y + SCREEN.height - TILE - 8;

function tileRectAt(index: number): LogicalRect {
  return { x: DOCK_X + DOCK_PADDING + index * TILE, y: DOCK_Y + 1, width: TILE, height: TILE - 2 };
}

function mossAt(index: number, step: LevelStep): string {
  const scene = mossScene({ seed: fnv1a32(`banner#${index}`), step, overgrown: step === LEVEL_STEPS });
  return sceneSvg(scene, "shared");
}

function tileSvg(tile: BannerTile, index: number): string {
  const rect = tileRectAt(index);
  const art = artBox(rect);
  const face =
    tile.glyph === "stone"
      ? `<g transform="translate(${n(rect.x)} ${n(art.y - (TILE - art.width) / 2)}) scale(${n(TILE / STONE_TILE_SIDE)})">${STONE_TILE}</g>`
      : `<g transform="translate(${n(art.x)} ${n(art.y)}) scale(${n(art.width / 100)})">${tile.glyph}</g>`;
  const last = tile.stages.length === 0 ? undefined : tile.stages[tile.stages.length - 1];
  const moss =
    last === undefined
      ? ""
      : `<g transform="translate(${n(art.x)} ${n(art.y)}) scale(${n(art.width / ICON_UNITS)})">${mossAt(index, last.step)}</g>`;
  const dot = tile.running
    ? `<circle cx="${n(rect.x + TILE / 2)}" cy="${n(art.y + art.height + 6)}" r="3" fill="#1D2430" fill-opacity="0.7"/>`
    : "";
  return `<g>${face}${moss}</g>${dot}`;
}

const WALL = "#EDE7DC";

const BACKDROP = `<rect width="${WIDTH}" height="${HEIGHT}" fill="${WALL}"/>
<path d="M268 120L560 120L700 560L268 560Z" fill="#F6F1E7" fill-opacity="0.55"/>
<rect x="44" y="92" width="228" height="348" rx="6" fill="#F7F4EE"/>
<rect x="58" y="106" width="200" height="320" fill="#CFE2EC"/>
<path d="M58 352Q120 318 180 336T258 326V426H58Z" fill="#B5CFBE"/>
<path d="M58 382Q130 360 200 376T258 370V426H58Z" fill="#9CBFA7"/>
<rect x="153" y="106" width="10" height="320" fill="#F7F4EE"/>
<rect x="58" y="252" width="200" height="10" fill="#F7F4EE"/>
<rect x="34" y="436" width="248" height="14" rx="3" fill="#E4DDD0"/>
<rect y="560" width="${WIDTH}" height="160" fill="url(#subway)"/>`;

const COUNTER = `<rect y="716" width="${WIDTH}" height="110" fill="#D4A66F"/>
<rect y="716" width="${WIDTH}" height="10" fill="#C29360"/>
<rect y="826" width="${WIDTH}" height="6" fill="#E3BC88"/>
<rect y="832" width="${WIDTH}" height="24" fill="#AE7C4A"/>
<rect y="856" width="${WIDTH}" height="44" fill="#3E5B4C"/>
<g fill="#334D40"><rect x="397" y="856" width="6" height="44"/><rect x="797" y="856" width="6" height="44"/><rect x="1197" y="856" width="6" height="44"/></g>
<g fill="#C9A24A"><rect x="330" y="868" width="44" height="7" rx="3.5"/><rect x="426" y="868" width="44" height="7" rx="3.5"/><rect x="1130" y="868" width="44" height="7" rx="3.5"/><rect x="1226" y="868" width="44" height="7" rx="3.5"/></g>`;

const LEAVES: readonly (readonly [x: number, y: number, rx: number, ry: number, turn: number, color: string])[] = [
  [128, 640, 30, 14, -35, "#3D7030"],
  [204, 636, 30, 14, 32, "#3D7030"],
  [112, 598, 28, 13, -52, "#4E8A3A"],
  [220, 594, 28, 13, 50, "#4E8A3A"],
  [150, 600, 26, 13, -70, "#67A34A"],
  [186, 598, 26, 13, 68, "#67A34A"],
  [138, 552, 24, 12, -60, "#4E8A3A"],
  [196, 550, 24, 12, 58, "#4E8A3A"],
  [166, 566, 24, 12, -90, "#86BD5E"],
  [154, 516, 20, 10, -75, "#67A34A"],
  [180, 514, 20, 10, 72, "#67A34A"],
  [166, 484, 16, 9, -90, "#86BD5E"],
  [146, 668, 26, 12, -18, "#4E8A3A"],
  [188, 666, 26, 12, 16, "#67A34A"],
];

const HERB = `<ellipse cx="166" cy="808" rx="74" ry="8" fill="#000000" fill-opacity="0.12"/>
<g fill="none" stroke="#3D7030" stroke-width="4" stroke-linecap="round"><path d="M166 700Q160 620 166 490"/><path d="M164 660Q140 610 128 560"/><path d="M168 650Q192 600 200 552"/></g>
${LEAVES.map(([x, y, rx, ry, turn, color]) => `<ellipse cx="${x}" cy="${y}" rx="${rx}" ry="${ry}" transform="rotate(${turn} ${x} ${y})" fill="${color}"/>`).join("")}
<path d="M100 700H232L220 806Q219 812 213 812H119Q113 812 112 806Z" fill="#EFE9DD"/>
<rect x="104" y="736" width="124" height="14" fill="#7FA7B5"/>
<rect x="92" y="690" width="148" height="20" rx="6" fill="#E3DACA"/>`;

const BOARD = `<g transform="rotate(-7 1470 810)"><rect x="1404" y="500" width="132" height="310" rx="58" fill="#D9B07A"/><rect x="1418" y="512" width="104" height="286" rx="48" fill="#E0BA86"/><circle cx="1470" cy="548" r="11" fill="${WALL}"/></g>`;

const MUG = `<ellipse cx="1430" cy="812" rx="62" ry="7" fill="#000000" fill-opacity="0.14"/>
<path d="M1476 744Q1512 744 1512 772Q1512 800 1474 800" fill="none" stroke="#2F5D8C" stroke-width="12"/>
<rect x="1384" y="726" width="96" height="88" rx="12" fill="#2F5D8C"/>
<rect x="1384" y="726" width="96" height="12" rx="6" fill="#4C7BAA"/>`;

const CALENDAR_CARD = `<circle cx="1448" cy="116" r="5" fill="#8A8F96"/>
<rect x="1376" y="132" width="152" height="200" rx="8" fill="#E3DED4"/>
<rect x="1372" y="126" width="152" height="200" rx="8" fill="#FBFAF7"/>
<path d="M1372 134Q1372 126 1380 126H1516Q1524 126 1524 134V172H1372Z" fill="#D8573F"/>
<text x="1448" y="160" font-family="'Helvetica Neue', Helvetica, Arial, sans-serif" font-size="24" font-weight="700" letter-spacing="2" text-anchor="middle" fill="#FFFFFF">MAY</text>`;

const DAY_TEXT = (day: number): string =>
  `<text x="1448" y="290" font-family="'Helvetica Neue', Helvetica, Arial, sans-serif" font-size="96" font-weight="700" text-anchor="middle" fill="#2B2F36">${day}</text>`;

const SCREEN_CLIP = "screen";

const WALLPAPER = `<rect x="${SCREEN.x}" y="${SCREEN.y}" width="${SCREEN.width}" height="${SCREEN.height}" fill="#BCD0E3"/>
<circle cx="1080" cy="300" r="54" fill="#F3E3C3"/>
<path d="M300 470Q420 400 560 440T840 420T1120 400T1300 430V731H300Z" fill="#93AECB"/>
<path d="M300 540Q460 470 640 520T960 500T1300 520V731H300Z" fill="#6D8DB4"/>
<path d="M300 610Q520 550 760 600T1300 590V731H300Z" fill="#4A6C98"/>
<path d="M300 680Q600 640 900 670T1300 660V731H300Z" fill="#34527C"/>`;

const MENU_BAR = `<rect x="${SCREEN.x}" y="${SCREEN.y}" width="${SCREEN.width}" height="26" fill="#FFFFFF" fill-opacity="0.6"/>
<g fill="#1D2430" fill-opacity="0.7"><rect x="318" y="115" width="10" height="9" rx="2"/><rect x="342" y="115" width="44" height="8" rx="4"/><rect x="398" y="115" width="34" height="8" rx="4"/><rect x="444" y="115" width="40" height="8" rx="4"/><rect x="496" y="115" width="30" height="8" rx="4"/>
<rect x="1178" y="113" width="14" height="13" rx="3"/><rect x="1204" y="115" width="22" height="8" rx="4"/><rect x="1238" y="115" width="46" height="8" rx="4"/></g>`;

const DOCK_PANEL = `<rect x="${n(DOCK_X)}" y="${n(DOCK_Y)}" width="${n(DOCK_WIDTH)}" height="${TILE}" rx="26" fill="#FFFFFF" fill-opacity="0.3" stroke="#FFFFFF" stroke-opacity="0.45" stroke-width="1.5"/>`;

const LAPTOP_SHADOW = `<ellipse cx="800" cy="812" rx="610" ry="12" fill="#000000" fill-opacity="0.14"/>`;

const LAPTOP_BASE = `<path d="M270 744H1330L1374 800H226Z" fill="#CDD2D7"/>
<path d="M322 750H1278L1300 778H300Z" fill="#B3B9C0"/>
<path d="M700 782H900L906 796H694Z" fill="#DCE0E4"/>
<rect x="226" y="798" width="1148" height="14" rx="6" fill="#AEB4BA"/>
<rect x="720" y="798" width="160" height="5" rx="2.5" fill="#9BA2A9"/>`;

const LAPTOP_LID = `<rect x="282" y="88" width="1036" height="660" rx="26" fill="#C4C9CE"/>
<rect x="286" y="92" width="1028" height="652" rx="23" fill="#121416"/>`;

const DEFS = `<defs>
${STONE_DEFS}
${ICON_CLIP_PATH}
<clipPath id="${SCREEN_CLIP}"><rect x="${SCREEN.x}" y="${SCREEN.y}" width="${SCREEN.width}" height="${SCREEN.height}" rx="6"/></clipPath>
<pattern id="subway" width="160" height="80" patternUnits="userSpaceOnUse" y="560"><rect width="160" height="80" fill="#E2DCD0"/><g fill="#F6F3ED"><rect x="2" y="2" width="76" height="36" rx="3"/><rect x="82" y="2" width="76" height="36" rx="3"/><rect x="-38" y="42" width="76" height="36" rx="3"/><rect x="42" y="42" width="76" height="36" rx="3"/><rect x="122" y="42" width="76" height="36" rx="3"/></g></pattern>
</defs>`;

export function bannerSvg(): string {
  const days = DAY_TEXT(30);
  const screen = `<g clip-path="url(#${SCREEN_CLIP})">${WALLPAPER}
${MENU_BAR}
${DOCK_PANEL}
${BANNER_TILES.map(tileSvg).join("\n")}</g>`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${WIDTH}" height="${HEIGHT}" viewBox="0 0 ${WIDTH} ${HEIGHT}">
${DEFS}
${BACKDROP}
${CALENDAR_CARD}
${days}
${COUNTER}
${BOARD}
${HERB}
${MUG}
${LAPTOP_SHADOW}
${LAPTOP_BASE}
${LAPTOP_LID}
${screen}
</svg>
`;
}
