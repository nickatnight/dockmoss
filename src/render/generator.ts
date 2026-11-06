import {
  ART_SIDE_PER_TILE_WIDTH,
  CORNER_RADIUS_PER_SIDE,
  type QuantizedOvergrowth,
  mulberry32,
  smoothstep,
} from "./model";

export const ICON_UNITS = 112;
export const ICON_CORNER_RADIUS = CORNER_RADIUS_PER_SIDE * ICON_UNITS;
export const TILE_UNITS = ICON_UNITS / ART_SIDE_PER_TILE_WIDTH;
export const OVERHANG_UNITS = (TILE_UNITS - ICON_UNITS) / 2;

export const LEVEL_STEPS = 24;

export type LevelStep = number & { readonly __brand: "LevelStep" };

export function levelStep(overgrowth: QuantizedOvergrowth): LevelStep {
  return Math.ceil(overgrowth * LEVEL_STEPS) as LevelStep;
}

export type Point = readonly [x: number, y: number];

export type Paint =
  | { kind: "fill"; color: string; alpha: number }
  | { kind: "stroke"; color: string; width: number; alpha: number }
  | { kind: "fillAndStroke"; fill: string; stroke: string; width: number };

export type Segment = { kind: "line"; to: Point } | { kind: "quad"; control: Point; to: Point };

export type Shape =
  | { kind: "circle"; x: number; y: number; r: number; paint: Paint }
  | { kind: "ellipse"; x: number; y: number; rx: number; ry: number; rotation: number; paint: Paint }
  | { kind: "path"; from: Point; segments: readonly Segment[]; closed: boolean; paint: Paint };

export interface MossScene {
  occlusion: readonly Shape[];
  moss: readonly Shape[];
  props: readonly Shape[];
}

export const EMPTY_SCENE: MossScene = { occlusion: [], moss: [], props: [] };

export interface Anchor {
  id: number;
  x: number;
  y: number;
  r: number;
}

const ANCHOR_GROWTH_SPAN = 0.2;
const ANCHOR_MIN_GROWTH = 0.04;
const FIRST_ROW_START = 0.04;
const FIRST_ROW_SPREAD = 1 / 1000;

export function iconAnchors(seed: number, level: number): Anchor[] {
  const random = mulberry32(seed);
  const anchors: Anchor[] = [];
  let nextId = 0;
  const add = (x: number, y: number, r: number, threshold: number): void => {
    const growth = smoothstep(0, 1, (level - threshold) / ANCHOR_GROWTH_SPAN);
    const id = nextId++;
    if (growth > ANCHOR_MIN_GROWTH) {
      anchors.push({ id, x, y, r: r * growth });
    }
  };
  for (let row = 0; row < 5; row++) {
    for (let column = 0; column < 20; column++) {
      const x = -2 + column * 6 + random() * 3;
      const y = 111 - row * 7 + random() * 4 - 2;
      const r = 5.5 + random() * 3.5 + (row === 0 ? 1 : 0);
      const threshold =
        row === 0
          ? FIRST_ROW_START + random() * 0.1 + Math.abs(x - 30) * FIRST_ROW_SPREAD
          : 0.08 + row * 0.17 + random() * 0.1 + Math.abs(x - 30) / 220;
      add(x, y, r, threshold);
    }
  }
  for (let y = 96; y > 4; y -= 7) {
    const climb = (111 - y) / 111;
    add(1 + random() * 3, y, 5 + random() * 3, 0.3 + climb * 0.45 + random() * 0.12);
    add(9 + random() * 4, y, 4.5 + random() * 2, 0.45 + climb * 0.45 + random() * 0.12);
  }
  for (let y = 96; y > 4; y -= 7) {
    add(111 - random() * 3, y, 5 + random() * 3, 0.45 + ((111 - y) / 111) * 0.4 + random() * 0.12);
  }
  for (let x = 6; x < 108; x += 8) {
    add(x + random() * 3, 1 + random() * 3, 4.5 + random() * 3, 0.72 + random() * 0.2);
  }
  for (let index = 0; index < 6; index++) {
    add(14 + random() * 20, 76 + random() * 16, 5 + random() * 3, 0.4 + random() * 0.2);
  }
  return anchors;
}

interface Layer {
  color: string;
  size: number;
  /** Up-left toward the light, in eighths of the anchor's radius, as the prototype has them. */
  dxEighths: number;
  dyEighths: number;
  circles: number;
}

const LAYERS: readonly Layer[] = [
  { color: "#17381B", size: 1, dxEighths: 0, dyEighths: 0, circles: 5 },
  { color: "#24521F", size: 0.9, dxEighths: -0.6, dyEighths: -1, circles: 5 },
  { color: "#3E7E2F", size: 0.74, dxEighths: -1.4, dyEighths: -2.2, circles: 5 },
  { color: "#6AAE45", size: 0.52, dxEighths: -2.2, dyEighths: -3.4, circles: 4 },
  { color: "#A6DA6C", size: 0.28, dxEighths: -2.8, dyEighths: -4.2, circles: 3 },
];

const OCCLUSION = {
  dx: 1,
  dy: 1.5,
  layers: [
    { size: 1.35, alpha: 0.07 },
    { size: 1.15, alpha: 0.09 },
  ],
} as const;
const FRONDS = { minRadius: 4, width: 0.6, alpha: 0.85, light: "#A6DA6C", dark: "#7DBE52" } as const;
const STALKS = {
  minRadius: 5,
  width: 0.7,
  color: "#8A5A2B",
  capsule: { rx: 1.1, ry: 1.9, color: "#C98F45" },
  minLevel: 0.55,
  share: 0.35,
} as const;

interface Cushion {
  angle: number;
  distance: number;
  size: number;
}

interface Frond {
  angle: number;
  reach: number;
  length: number;
  light: boolean;
}

interface Stalk {
  chance: number;
  lean: number;
  height: number;
  bend: number;
}

interface AnchorDraws {
  cushions: Cushion[][];
  fronds: Frond[];
  stalk: Stalk;
}

function anchorDraws(seed: number, id: number): AnchorDraws {
  const random = mulberry32(seed * 100003 + id * 7919 + 1);
  const cushions = LAYERS.map((layer) =>
    Array.from({ length: layer.circles }, (): Cushion => ({
      angle: random() * 6.283,
      distance: Math.sqrt(random()),
      size: 0.45 + 0.55 * random(),
    })),
  );
  const fronds = Array.from({ length: 4 }, (): Frond => ({
    angle: -Math.PI * (0.1 + 0.8 * random()),
    reach: 0.6 + 0.3 * random(),
    length: 2 + random() * 3,
    light: random() < 0.5,
  }));
  const stalk: Stalk = {
    chance: random(),
    lean: random() - 0.5,
    height: 6 + random() * 4,
    bend: random() - 0.5,
  };
  return { cushions, fronds, stalk };
}

const black = (alpha: number): Paint => ({ kind: "fill", color: "#000", alpha });
const fill = (color: string, alpha = 1): Paint => ({ kind: "fill", color, alpha });

export function mossShapes(
  anchors: readonly Anchor[],
  seed: number,
  stalkShare: number,
): Pick<MossScene, "occlusion" | "moss"> {
  const draws = anchors.map(({ id }) => anchorDraws(seed, id));
  const occlusion: Shape[] = anchors.flatMap(({ x, y, r }) =>
    OCCLUSION.layers.map(({ size, alpha }): Shape => ({
      kind: "circle",
      x: x + OCCLUSION.dx,
      y: y + OCCLUSION.dy,
      r: r * size,
      paint: black(alpha),
    })),
  );
  const moss: Shape[] = LAYERS.flatMap((layer, layerIndex) =>
    anchors.flatMap(({ x, y, r }, index) =>
      draws[index].cushions[layerIndex].map(({ angle, distance, size }): Shape => {
        const reach = distance * r * 0.55;
        return {
          kind: "circle",
          x: x + Math.cos(angle) * reach + (layer.dxEighths * r) / 8,
          y: y + Math.sin(angle) * reach + (layer.dyEighths * r) / 8,
          r: r * layer.size * size,
          paint: fill(layer.color),
        };
      }),
    ),
  );
  anchors.forEach(({ x, y, r }, index) => {
    if (r < FRONDS.minRadius) {
      return;
    }
    for (const { angle, reach, length, light } of draws[index].fronds) {
      const from: Point = [x + Math.cos(angle) * r * reach, y + Math.sin(angle) * r * reach];
      moss.push({
        kind: "path",
        from,
        segments: [
          { kind: "line", to: [from[0] + Math.cos(angle) * length, from[1] + Math.sin(angle) * length] },
        ],
        closed: false,
        paint: {
          kind: "stroke",
          color: light ? FRONDS.light : FRONDS.dark,
          width: FRONDS.width,
          alpha: FRONDS.alpha,
        },
      });
    }
  });
  if (stalkShare > 0) {
    anchors.forEach(({ x: anchorX, y: anchorY, r }, index) => {
      const { chance, lean, height, bend } = draws[index].stalk;
      if (r < STALKS.minRadius || chance > stalkShare) {
        return;
      }
      const x = anchorX + lean * r;
      const y = anchorY - r * 0.35;
      const sway = bend * 3;
      moss.push(
        {
          kind: "path",
          from: [x, y],
          segments: [{ kind: "quad", control: [x + sway, y - height / 2], to: [x + sway * 1.5, y - height] }],
          closed: false,
          paint: { kind: "stroke", color: STALKS.color, width: STALKS.width, alpha: 1 },
        },
        {
          kind: "ellipse",
          x: x + sway * 1.5,
          y: y - height - 1,
          rx: STALKS.capsule.rx,
          ry: STALKS.capsule.ry,
          rotation: 0,
          paint: fill(STALKS.capsule.color),
        },
      );
    });
  }
  return { occlusion, moss };
}

const degrees = (angle: number): number => (angle * Math.PI) / 180;

export const MUSHROOM_SHADOW = "#17381B";
export const MUSHROOM_CAP = "#C8553D";
export const FLOWER_CENTRE = "#E8833A";

const MUSHROOM: readonly Shape[] = [
  { kind: "ellipse", x: 0, y: 0, rx: 7, ry: 2, rotation: 0, paint: fill(MUSHROOM_SHADOW, 0.55) },
  {
    kind: "path",
    from: [-2.6, 0],
    segments: [
      { kind: "quad", control: [-3, -8], to: [-2, -14] },
      { kind: "line", to: [2, -14] },
      { kind: "quad", control: [3, -8], to: [2.6, 0] },
    ],
    closed: true,
    paint: fill("#EADFC6"),
  },
  {
    kind: "path",
    from: [0.6, 0],
    segments: [
      { kind: "quad", control: [1.4, -8], to: [0.9, -14] },
      { kind: "line", to: [2, -14] },
      { kind: "quad", control: [3, -8], to: [2.6, 0] },
    ],
    closed: true,
    paint: fill("#CFC09C"),
  },
  { kind: "ellipse", x: 0, y: -13.5, rx: 10, ry: 2.2, rotation: 0, paint: fill("#8E3A2B") },
  {
    kind: "path",
    from: [-10.5, -13.5],
    segments: [
      { kind: "quad", control: [-10, -25], to: [0, -25.5] },
      { kind: "quad", control: [10, -25], to: [10.5, -13.5] },
      { kind: "quad", control: [0, -16], to: [-10.5, -13.5] },
    ],
    closed: true,
    paint: fill(MUSHROOM_CAP),
  },
  {
    kind: "path",
    from: [3, -25.3],
    segments: [
      { kind: "quad", control: [10, -24], to: [10.5, -13.5] },
      { kind: "quad", control: [6.5, -14.6], to: [3, -14.9] },
    ],
    closed: true,
    paint: fill("#A8412F"),
  },
  { kind: "ellipse", x: -4.5, y: -21, rx: 3.2, ry: 1.5, rotation: degrees(-25), paint: fill("#E9876B") },
  ...[
    [-6, -17, 1.5],
    [1, -21, 1.7],
    [5.5, -17, 1.2],
    [-1, -16.5, 1],
  ].map(([x, y, r]): Shape => ({ kind: "circle", x, y, r, paint: fill("#FFF3DC") })),
];

const PETAL_CENTRE = 17;
const PETAL_REACH = 3.6;

const FLOWER: readonly Shape[] = [
  {
    kind: "path",
    from: [0, 0],
    segments: [{ kind: "quad", control: [-2, -8], to: [0, -15] }],
    closed: false,
    paint: { kind: "stroke", color: "#3F7A32", width: 1.5, alpha: 1 },
  },
  { kind: "ellipse", x: 3.2, y: -6, rx: 3.4, ry: 1.4, rotation: degrees(-30), paint: fill("#4F8A3A") },
  ...Array.from({ length: 5 }, (_, petal): Shape => {
    const turn = degrees(petal * 72);
    return {
      kind: "ellipse",
      x: PETAL_REACH * Math.sin(turn),
      y: -PETAL_CENTRE - PETAL_REACH * Math.cos(turn),
      rx: 2.3,
      ry: 3.6,
      rotation: turn,
      paint: { kind: "fillAndStroke", fill: "#F7D54A", stroke: "#D9A92B", width: 0.3 },
    };
  }),
  { kind: "circle", x: 0, y: -PETAL_CENTRE, r: 2, paint: fill(FLOWER_CENTRE) },
  { kind: "circle", x: -0.6, y: -PETAL_CENTRE - 0.6, r: 0.7, paint: fill("#F6B26B") },
];

function scaledPaint(paint: Paint, scale: number): Paint {
  return paint.kind === "fill" ? paint : { ...paint, width: paint.width * scale };
}

function placed(shape: Shape, [footX, footY]: Point, scale: number): Shape {
  const at = ([x, y]: Point): Point => [footX + x * scale, footY + y * scale];
  const paint = scaledPaint(shape.paint, scale);
  switch (shape.kind) {
    case "circle": {
      const [x, y] = at([shape.x, shape.y]);
      return { ...shape, x, y, r: shape.r * scale, paint };
    }
    case "ellipse": {
      const [x, y] = at([shape.x, shape.y]);
      return { ...shape, x, y, rx: shape.rx * scale, ry: shape.ry * scale, paint };
    }
    case "path":
      return {
        ...shape,
        from: at(shape.from),
        segments: shape.segments.map((segment) =>
          segment.kind === "line"
            ? { kind: "line", to: at(segment.to) }
            : { kind: "quad", control: at(segment.control), to: at(segment.to) },
        ),
        paint,
      };
  }
}

const PROP_STREAM = 0x50524f50;
const PROP_SCALE = 0.9;
const PROP_SMALLEST_SHARE = 0.7;
const MUSHROOM_RAMP = [0.78, 0.9] as const;
const FLOWER_RAMP = [0.88, 0.98] as const;
const PROP_FOOT_X = [
  [22, 46],
  [66, 90],
] as const;
const PROP_FOOT_Y = [92, 104] as const;

function propScale([from, to]: readonly [number, number], level: number): number {
  return PROP_SCALE * (PROP_SMALLEST_SHARE + (1 - PROP_SMALLEST_SHARE) * smoothstep(from, to, level));
}

function propShapes(seed: number, level: number): Shape[] {
  const random = mulberry32((seed ^ PROP_STREAM) >>> 0);
  const between = ([low, high]: readonly [number, number]): number => low + (high - low) * random();
  const mushroomLeft = random() < 0.5;
  const mushroomFoot: Point = [between(PROP_FOOT_X[mushroomLeft ? 0 : 1]), between(PROP_FOOT_Y)];
  const flowerFoot: Point = [between(PROP_FOOT_X[mushroomLeft ? 1 : 0]), between(PROP_FOOT_Y)];
  const props = [
    { foot: mushroomFoot, shapes: MUSHROOM, scale: propScale(MUSHROOM_RAMP, level) },
    { foot: flowerFoot, shapes: FLOWER, scale: propScale(FLOWER_RAMP, level) },
  ].sort((a, b) => a.foot[1] - b.foot[1]);
  return props.flatMap(({ foot, shapes, scale }) => shapes.map((shape) => placed(shape, foot, scale)));
}

export interface SceneInput {
  seed: number;
  step: LevelStep;
  overgrown: boolean;
}

export function mossScene({ seed, step, overgrown }: SceneInput): MossScene {
  if (step <= 0) {
    return EMPTY_SCENE;
  }
  const level = step / LEVEL_STEPS;
  const { occlusion, moss } = mossShapes(
    iconAnchors(seed, level),
    seed,
    level > STALKS.minLevel ? STALKS.share : 0,
  );
  return { occlusion, moss, props: overgrown ? propShapes(seed, level) : [] };
}
