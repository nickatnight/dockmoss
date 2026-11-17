import type { LogicalRect } from "../ipc";
import { ICON_UNITS, LEVEL_STEPS, type LevelStep, mossScene, OVERHANG_UNITS, TILE_UNITS } from "../render/generator";
import {
  artBox,
  CORNER_RADIUS_PER_SIDE,
  fnv1a32,
  SHRIVEL_FRONT_WIDTH,
  SHRIVEL_MS,
  SHRIVEL_WITHER_END,
  smoothstep,
  WITHER_TINT,
} from "../render/model";
import fragmentSource from "../render/moss.frag.glsl?raw";
import { STONE_TILE } from "./app-icon";
import { SHARED_ICON_CLIP } from "./svg";

const WIDTH = 1600;
const HEIGHT = 900;

const n = (value: number): string => String(Number(value.toFixed(2)));

export const BANNER_SECONDS = 12;
const POSTER = 7;
const JUMP = 0.02;

type Value = number | readonly number[];
type Frame = readonly [at: number, value: Value];

const formatValue = (value: Value): string => (typeof value === "number" ? n(value) : value.map(n).join(" "));

function valueAt(frames: readonly Frame[], at: number): Value {
  const after = frames.findIndex(([time]) => time > at);
  if (after <= 0) {
    return frames[after === 0 ? 0 : frames.length - 1][1];
  }
  const [[t0, v0], [t1, v1]] = [frames[after - 1], frames[after]];
  const k = (at - t0) / (t1 - t0);
  const mix = (a: number, b: number): number => a + (b - a) * k;
  if (typeof v0 === "number" && typeof v1 === "number") {
    return mix(v0, v1);
  }
  const [a, b] = [[v0].flat(), [v1].flat()];
  return a.map((value, index) => mix(value, b[index] ?? value));
}

function animated(
  attribute: "opacity" | "y" | "transform",
  frames: readonly Frame[],
  transform?: "translate" | "scale",
): { attribute: string; animation: string } {
  const [first, last] = [frames[0], frames[frames.length - 1]];
  const all: Frame[] = [
    ...(first[0] > 0 ? [[0, first[1]] as const] : []),
    ...frames,
    ...(last[0] < BANNER_SECONDS ? [[BANNER_SECONDS, last[1]] as const] : []),
  ];
  const times = all.map(([at]) => Number((at / BANNER_SECONDS).toFixed(4)));
  times.forEach((time, index) => {
    if (index > 0 && !(time > times[index - 1])) {
      throw new Error(`key frames for ${attribute} must rise: ${all.map(([at]) => at).join(", ")}`);
    }
  });
  const poster = formatValue(valueAt(all, POSTER));
  const timing = `dur="${BANNER_SECONDS}s" repeatCount="indefinite" keyTimes="${times.join(";")}" values="${all.map(([, value]) => formatValue(value)).join(";")}"`;
  return transform === undefined
    ? { attribute: `${attribute}="${poster}"`, animation: `<animate attributeName="${attribute}" ${timing}/>` }
    : {
        attribute: `transform="${transform}(${poster})"`,
        animation: `<animateTransform attributeName="transform" type="${transform}" ${timing}/>`,
      };
}

function eased(from: number, to: number, start: Value, end: Value, samples = 8): Frame[] {
  return Array.from({ length: samples + 1 }, (_, index): Frame => {
    const k = smoothstep(0, 1, index / samples);
    const lerp = (a: number, b: number): number => a + (b - a) * k;
    const value: Value =
      typeof start === "number" && typeof end === "number"
        ? lerp(start, end)
        : [start].flat().map((a, i) => lerp(a, [end].flat()[i] ?? a));
    return [from + ((to - from) * index) / samples, value];
  });
}

export interface Stage {
  at: number;
  step: LevelStep;
}

/** The app icon's stone, or a tile drawn in a 100-unit square. */
export type TileFace = { kind: "stone" } | { kind: "drawn"; svg: string };

export interface BannerTile {
  face: TileFace;
  stages: readonly Stage[];
  running: boolean;
  launchesAt?: number;
}

const CALENDAR_PAGES: readonly (readonly [at: number, day: number])[] = [
  [0, 1],
  [1, 5],
  [1.8, 8],
  [2.6, 12],
  [3.4, 15],
  [4.2, 19],
  [5, 23],
  [5.8, 26],
  [6.6, 30],
];

/** A stage reached as the calendar turns to May `day`. */
function stage(day: number, step: number): Stage {
  const page = CALENDAR_PAGES.find(([, pageDay]) => pageDay === day);
  if (page === undefined) {
    throw new Error(`the calendar never shows May ${day}`);
  }
  return { at: page[0], step: step as LevelStep };
}

const TILE_RADIUS = 100 * CORNER_RADIUS_PER_SIDE;

const tileBase = (top: string, bottom: string, id: string): string =>
  `<linearGradient id="${id}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${top}"/><stop offset="1" stop-color="${bottom}"/></linearGradient>`;

const tileRect = (id: string): string =>
  `<rect width="100" height="100" rx="${n(TILE_RADIUS)}" fill="url(#${id})" filter="url(#${STONE_TILE.shadow})"/>`;

const drawn = (id: string, top: string, bottom: string, art: string): TileFace => ({
  kind: "drawn",
  svg: `${tileBase(top, bottom, id)}${tileRect(id)}${art}`,
});

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

export const BANNER_TILES: readonly BannerTile[] = [
  { face: drawn("tile-0", "#5B9BE0", "#2F6FB8", FILES), stages: [], running: true },
  {
    face: drawn("tile-1", "#FFFFFF", "#E6E9ED", CALENDAR),
    stages: [stage(8, 6), stage(15, 12)],
    running: false,
  },
  {
    face: drawn("tile-2", "#FFE07A", "#F6C443", NOTES),
    stages: [stage(5, 6), stage(12, 12), stage(23, LEVEL_STEPS)],
    running: false,
    launchesAt: 7.9,
  },
  {
    face: drawn("tile-3", "#FF7A86", "#E3465A", MUSIC),
    stages: [stage(12, 6), stage(19, 12)],
    running: false,
  },
  {
    face: drawn("tile-4", "#3B5C92", "#22375C", PHOTOS),
    stages: [stage(5, 6)],
    running: false,
  },
  { face: { kind: "stone" }, stages: [stage(5, 6), stage(15, 12), stage(26, 16)], running: false },
];

const SCREEN = { x: 300, y: 106, width: 1000, height: 625 } as const;
const TILE = 140;
const DOCK_PADDING = 12;
const DOCK_WIDTH = BANNER_TILES.length * TILE + 2 * DOCK_PADDING;
const DOCK_X = SCREEN.x + (SCREEN.width - DOCK_WIDTH) / 2;
const DOCK_Y = SCREEN.y + SCREEN.height - TILE - 8;

function tileRectAt(index: number): LogicalRect {
  return { x: DOCK_X + DOCK_PADDING + index * TILE, y: DOCK_Y + 1, width: TILE, height: TILE - 2 };
}

const GROW = 0.4;
const RESET = 10.9;
const RESET_END = 11.5;

const LAUNCH_INDEX = BANNER_TILES.findIndex(({ launchesAt }) => launchesAt !== undefined);
const LAUNCH = BANNER_TILES[LAUNCH_INDEX]?.launchesAt ?? 0;
const SHRIVEL_START = LAUNCH + 0.1;
const SHRIVEL_SECONDS = SHRIVEL_MS / 1000;
const LAUNCH_MOSS = "launch-moss";

function mossAt(index: number, step: LevelStep): string {
  const scene = mossScene({ seed: fnv1a32(`banner#${index}`), step, overgrown: step === LEVEL_STEPS });
  return SHARED_ICON_CLIP.sceneSvg(scene);
}

function stageFrames(stages: readonly Stage[], index: number): Frame[] {
  const { at } = stages[index];
  const next = stages[index + 1];
  const end: Frame[] =
    next === undefined
      ? [
          [RESET, 1],
          [RESET_END, 0],
        ]
      : [
          [next.at + GROW, 1],
          [next.at + GROW + JUMP, 0],
        ];
  return [[at, 0], [at + GROW, 1], ...end];
}

const SHRIVEL_SAMPLES = Array.from({ length: 13 }, (_, index) => index / 12);

const BAKE_TOP = -OVERHANG_UNITS;
const BAKE_BOTTOM = ICON_UNITS + OVERHANG_UNITS;
const FRONT_DEPTH = SHRIVEL_FRONT_WIDTH * TILE_UNITS;
const MASK_HEIGHT = TILE_UNITS + 2 * FRONT_DEPTH;

function frontTop(progress: number): number {
  const remaining = 1 - smoothstep(0, 1, progress);
  return BAKE_BOTTOM - remaining * (1 + SHRIVEL_FRONT_WIDTH) * TILE_UNITS;
}

const SHRIVEL_FRONT = animated("y", [
  ...SHRIVEL_SAMPLES.map((progress): Frame => [SHRIVEL_START + progress * SHRIVEL_SECONDS, frontTop(progress)]),
  [RESET_END + JUMP, frontTop(1)],
  [RESET_END + 2 * JUMP, frontTop(0)],
]);

const WITHERING = animated("opacity", [
  ...SHRIVEL_SAMPLES.map((progress): Frame => [
    SHRIVEL_START + progress * SHRIVEL_SECONDS,
    smoothstep(0, SHRIVEL_WITHER_END, progress),
  ]),
  [RESET_END + JUMP, 1],
  [RESET_END + 2 * JUMP, 0],
]);

const LUMA = (() => {
  const weights = /const vec3 REC709_LUMA = vec3\(([^)]+)\);/.exec(fragmentSource)?.[1];
  if (weights === undefined) {
    throw new Error("moss.frag.glsl no longer declares REC709_LUMA");
  }
  return weights.split(",").map(Number);
})();

const WITHER_MATRIX = [
  ...WITHER_TINT.flatMap((tint) => [...LUMA.map((weight) => tint * weight), 0, 0]),
  ...[0, 0, 0, 1, 0],
]
  .map((value) => String(Number(value.toFixed(5))))
  .join(" ");

const SHRIVEL_DEFS = `<linearGradient id="shrivel-edge" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#000000"/><stop offset="${n(FRONT_DEPTH / MASK_HEIGHT)}" stop-color="#FFFFFF"/></linearGradient>
<mask id="shrivel" maskUnits="userSpaceOnUse" x="${n(BAKE_TOP - FRONT_DEPTH)}" y="${n(BAKE_TOP - FRONT_DEPTH)}" width="${n(TILE_UNITS + 2 * FRONT_DEPTH)}" height="${n(TILE_UNITS + 2 * FRONT_DEPTH)}"><rect x="${n(BAKE_TOP - FRONT_DEPTH)}" ${SHRIVEL_FRONT.attribute} width="${n(TILE_UNITS + 2 * FRONT_DEPTH)}" height="${n(MASK_HEIGHT)}" fill="url(#shrivel-edge)">${SHRIVEL_FRONT.animation}</rect></mask>
<filter id="wither" color-interpolation-filters="sRGB"><feColorMatrix type="matrix" values="${WITHER_MATRIX}"/></filter>`;

function bounceFrames(at: number): Frame[] {
  const hop = (start: number, seconds: number, height: number, from: number): Frame[] =>
    Array.from({ length: 9 - from }, (_, index): Frame => {
      const k = (index + from) / 8;
      return [start + seconds * k, [0, -height * 4 * k * (1 - k)]];
    });
  return [...hop(at, 0.5, TILE * 0.4, 0), ...hop(at + 0.5, 0.4, TILE * 0.22, 1)];
}

function tileSvg(tile: BannerTile, index: number): string {
  const rect = tileRectAt(index);
  const art = artBox(rect);
  const face =
    tile.face.kind === "stone"
      ? `<g transform="translate(${n(rect.x)} ${n(art.y - (TILE - art.width) / 2)}) scale(${n(TILE / STONE_TILE.side)})">${STONE_TILE.svg}</g>`
      : `<g transform="translate(${n(art.x)} ${n(art.y)}) scale(${n(art.width / 100)})">${tile.face.svg}</g>`;
  const stages = tile.stages.map(({ step }, stageIndex) => {
    const shown = animated("opacity", stageFrames(tile.stages, stageIndex));
    const moss = mossAt(index, step);
    const launchStage = tile.launchesAt !== undefined && stageIndex === tile.stages.length - 1;
    const body = launchStage
      ? `<g mask="url(#shrivel)"><g id="${LAUNCH_MOSS}">${moss}</g><use href="#${LAUNCH_MOSS}" filter="url(#wither)" ${WITHERING.attribute}>${WITHERING.animation}</use></g>`
      : moss;
    return `<g ${shown.attribute}>${shown.animation}${body}</g>`;
  });
  const mossLayer =
    stages.length === 0
      ? ""
      : `<g transform="translate(${n(art.x)} ${n(art.y)}) scale(${n(art.width / ICON_UNITS)})">${stages.join("\n")}</g>`;
  const dotAt = `cx="${n(rect.x + TILE / 2)}" cy="${n(art.y + art.height + 6)}" r="3" fill="#1D2430" fill-opacity="0.7"`;
  let dot = "";
  if (tile.running) {
    dot = `<circle ${dotAt}/>`;
  } else if (tile.launchesAt !== undefined) {
    const lit = animated("opacity", [
      [tile.launchesAt + 0.15, 0],
      [tile.launchesAt + 0.35, 1],
      [RESET, 1],
      [RESET + 0.3, 0],
    ]);
    dot = `<circle ${dotAt} ${lit.attribute}>${lit.animation}</circle>`;
  }
  const lifted = tile.launchesAt === undefined ? undefined : animated("transform", bounceFrames(tile.launchesAt), "translate");
  return lifted === undefined
    ? `<g>${face}${mossLayer}</g>${dot}`
    : `<g ${lifted.attribute}>${lifted.animation}${face}${mossLayer}</g>${dot}`;
}

const LAUNCH_ART = artBox(tileRectAt(LAUNCH_INDEX));
const LAUNCH_CENTRE: readonly number[] = [LAUNCH_ART.x + LAUNCH_ART.width / 2, LAUNCH_ART.y + LAUNCH_ART.height / 2];

const POINTER_REST: readonly number[] = [1130, 430];
const POINTER_AWAY: readonly number[] = [1170, 540];
const POINTER_PATH = animated(
  "transform",
  [
    ...eased(LAUNCH - 1, LAUNCH - 0.2, POINTER_REST, LAUNCH_CENTRE),
    ...eased(LAUNCH + 1.3, LAUNCH + 2, LAUNCH_CENTRE, POINTER_AWAY),
    ...eased(RESET, RESET_END, POINTER_AWAY, POINTER_REST),
  ],
  "translate",
);
const POINTER_PRESS = animated(
  "transform",
  [
    [LAUNCH - 0.12, 1],
    [LAUNCH - 0.06, 0.86],
    [LAUNCH + 0.02, 1],
  ],
  "scale",
);
const POINTER = `<g ${POINTER_PATH.attribute}>${POINTER_PATH.animation}<g ${POINTER_PRESS.attribute}>${POINTER_PRESS.animation}<path d="M0 0V34L8.5 26L14 38.5L20 36L14.6 23.6H26.5Z" fill="#111111" stroke="#FFFFFF" stroke-width="2.5" stroke-linejoin="round"/></g></g>`;

const WINDOW_CENTRE: readonly number[] = [800, 360];
const WINDOW_SIZE = { width: 580, height: 320 } as const;
const OPEN = LAUNCH + 0.7;
const WINDOW_MOVE = animated(
  "transform",
  [...eased(OPEN, OPEN + 0.45, LAUNCH_CENTRE, WINDOW_CENTRE), ...eased(RESET, RESET + 0.4, WINDOW_CENTRE, LAUNCH_CENTRE)],
  "translate",
);
const WINDOW_SCALE = animated(
  "transform",
  [...eased(OPEN, OPEN + 0.45, 0.1, 1), ...eased(RESET, RESET + 0.4, 1, 0.1)],
  "scale",
);
const WINDOW_SHOWN = animated("opacity", [
  [OPEN, 0],
  [OPEN + 0.1, 1],
  [RESET + 0.3, 1],
  [RESET + 0.4, 0],
]);
const [WW, WH] = [WINDOW_SIZE.width, WINDOW_SIZE.height];
const NOTE_LINES = [0.82, 0.64, 0.74, 0.5]
  .map((share, line) => `<rect x="${-WW / 2 + 36}" y="${-WH / 2 + 86 + line * 44}" width="${n((WW - 72) * share)}" height="12" rx="6"/>`)
  .join("");
const APP_WINDOW = `<g ${WINDOW_SHOWN.attribute}>${WINDOW_SHOWN.animation}<g ${WINDOW_MOVE.attribute}>${WINDOW_MOVE.animation}<g ${WINDOW_SCALE.attribute}>${WINDOW_SCALE.animation}
<rect x="${-WW / 2}" y="${-WH / 2}" width="${WW}" height="${WH}" rx="14" fill="#FFFDF6"/>
<path d="M${-WW / 2} ${-WH / 2 + 14}Q${-WW / 2} ${-WH / 2} ${-WW / 2 + 14} ${-WH / 2}H${WW / 2 - 14}Q${WW / 2} ${-WH / 2} ${WW / 2} ${-WH / 2 + 14}V${-WH / 2 + 46}H${-WW / 2}Z" fill="#F6C443"/>
<g fill="#FFFFFF" fill-opacity="0.75"><circle cx="${-WW / 2 + 24}" cy="${-WH / 2 + 23}" r="6"/><circle cx="${-WW / 2 + 44}" cy="${-WH / 2 + 23}" r="6"/><circle cx="${-WW / 2 + 64}" cy="${-WH / 2 + 23}" r="6"/></g>
<g fill="#5C5446" fill-opacity="0.28">${NOTE_LINES}</g></g></g></g>`;

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
  [122, 664, 30, 17, -20, "#3D7030"],
  [212, 662, 30, 17, 22, "#3D7030"],
  [146, 676, 26, 15, -8, "#467C35"],
  [188, 676, 26, 15, 10, "#467C35"],
  [100, 618, 28, 16, -38, "#467C35"],
  [232, 614, 28, 16, 40, "#467C35"],
  [136, 626, 27, 16, -24, "#4E8A3A"],
  [198, 624, 27, 16, 26, "#4E8A3A"],
  [114, 570, 25, 15, -48, "#4E8A3A"],
  [222, 566, 25, 15, 50, "#4E8A3A"],
  [148, 586, 24, 14, -30, "#5C9A43"],
  [186, 584, 24, 14, 32, "#5C9A43"],
  [130, 528, 22, 13, -56, "#5C9A43"],
  [206, 524, 22, 13, 58, "#5C9A43"],
  [154, 544, 21, 13, -38, "#67A34A"],
  [180, 542, 21, 13, 40, "#67A34A"],
  [150, 498, 18, 11, -62, "#77B055"],
  [184, 496, 18, 11, 64, "#77B055"],
  [167, 474, 15, 10, -90, "#86BD5E"],
];

const HERB = `<ellipse cx="166" cy="808" rx="74" ry="8" fill="#000000" fill-opacity="0.12"/>
<g fill="none" stroke="#3D7030" stroke-width="4" stroke-linecap="round"><path d="M166 700Q162 600 167 480"/><path d="M164 690Q132 630 116 570"/><path d="M168 688Q200 628 220 566"/></g>
${LEAVES.map(([x, y, rx, ry, turn, color]) => `<ellipse cx="${x}" cy="${y}" rx="${rx}" ry="${ry}" transform="rotate(${turn} ${x} ${y})" fill="${color}"/><path d="M${x - rx * 0.75} ${y}H${x + rx * 0.75}" transform="rotate(${turn} ${x} ${y})" stroke="#2F5A26" stroke-opacity="0.35" stroke-width="2"/>`).join("")}
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

const DAYS = CALENDAR_PAGES.map(([at, day], page) => {
  const next = CALENDAR_PAGES[page + 1]?.[0] ?? RESET_END;
  const frames: Frame[] =
    page === 0
      ? [
          [next, 1],
          [next + JUMP, 0],
          [RESET_END, 0],
          [RESET_END + JUMP, 1],
        ]
      : [
          [at, 0],
          [at + JUMP, 1],
          [next, 1],
          [next + JUMP, 0],
        ];
  const shown = animated("opacity", frames);
  return `<text x="1448" y="290" ${shown.attribute}>${shown.animation}${day}</text>`;
}).join("");

const CALENDAR_DAYS_TEXT = `<g font-family="'Helvetica Neue', Helvetica, Arial, sans-serif" font-size="96" font-weight="700" text-anchor="middle" fill="#2B2F36">${DAYS}</g>`;

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
${STONE_TILE.defs}
${SHARED_ICON_CLIP.defs}
${SHRIVEL_DEFS}
<clipPath id="${SCREEN_CLIP}"><rect x="${SCREEN.x}" y="${SCREEN.y}" width="${SCREEN.width}" height="${SCREEN.height}" rx="6"/></clipPath>
<pattern id="subway" width="160" height="80" patternUnits="userSpaceOnUse" y="560"><rect width="160" height="80" fill="#E2DCD0"/><g fill="#F6F3ED"><rect x="2" y="2" width="76" height="36" rx="3"/><rect x="82" y="2" width="76" height="36" rx="3"/><rect x="-38" y="42" width="76" height="36" rx="3"/><rect x="42" y="42" width="76" height="36" rx="3"/><rect x="122" y="42" width="76" height="36" rx="3"/></g></pattern>
</defs>`;

export function bannerSvg(): string {
  const screen = `<g clip-path="url(#${SCREEN_CLIP})">${WALLPAPER}
${MENU_BAR}
${APP_WINDOW}
${DOCK_PANEL}
${BANNER_TILES.map(tileSvg).join("\n")}
${POINTER}</g>`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${WIDTH}" height="${HEIGHT}" viewBox="0 0 ${WIDTH} ${HEIGHT}">
${DEFS}
${BACKDROP}
${CALENDAR_CARD}
${CALENDAR_DAYS_TEXT}
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

const MASTER_HEADER = `<!-- DockMoss README banner. Generated by src/icon/banner.ts (bannerMaster); do not edit it by hand.
     After a change to the banner or the moss generator, rewrite it with: pnpm test src/icon -u
     Then re-pin the SHA-256 in src/icon/banner.test.ts (src-tauri/icons/README.md, "Banner"). -->
`;

export function bannerMaster(): string {
  return MASTER_HEADER + bannerSvg();
}
