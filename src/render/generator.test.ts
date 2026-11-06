import { describe, expect, it } from "vitest";

import type { LogicalRect } from "../ipc";
import {
  ICON_CORNER_RADIUS,
  ICON_UNITS,
  LEVEL_STEPS,
  OVERHANG_UNITS,
  TILE_UNITS,
  type Anchor,
  type LevelStep,
  type Point,
  type Shape,
  iconAnchors,
  levelStep,
  mossScene,
  mossShapes,
} from "./generator";
import { artBox, bakeBox, fnv1a32, quantize } from "./model";

// The seed the prototype's page starts with; moss-rendering.md counts its shapes.
const PROTOTYPE_SEED = 4271;
const SEEDS = [PROTOTYPE_SEED, 0, 0xffffffff, fnv1a32("com.hnc.Discord#0"), fnv1a32("com.apple.Safari#1")];
const STEPS = Array.from({ length: LEVEL_STEPS + 1 }, (_, step) => step as LevelStep);

function step(level: number): LevelStep {
  return levelStep(quantize(level));
}

function circles(shapes: readonly Shape[]): Extract<Shape, { kind: "circle" }>[] {
  return shapes.flatMap((shape) => (shape.kind === "circle" ? [shape] : []));
}

function strokes(shapes: readonly Shape[], color: string): Shape[] {
  return shapes.filter(({ paint }) => paint.kind === "stroke" && paint.color === color);
}

/** A one-segment path as the prototype writes it: its start, then the offsets to its points, to one decimal. */
function prototypePath(shape: Shape): string {
  if (shape.kind !== "path" || shape.segments.length !== 1) {
    return `not a one-segment path: ${shape.kind}`;
  }
  const [x, y] = shape.from;
  const [segment] = shape.segments;
  const offset = ([px, py]: Point): string => `${(px - x).toFixed(1)} ${(py - y).toFixed(1)}`;
  const rest =
    segment.kind === "line" ? `l${offset(segment.to)}` : `q${offset(segment.control)} ${offset(segment.to)}`;
  return `M${x.toFixed(1)} ${y.toFixed(1)}${rest}`;
}

function bounds(shape: Shape): { left: number; top: number; right: number; bottom: number } {
  const pad = shape.paint.kind === "fill" ? 0 : shape.paint.width / 2;
  if (shape.kind === "circle") {
    return {
      left: shape.x - shape.r - pad,
      top: shape.y - shape.r - pad,
      right: shape.x + shape.r + pad,
      bottom: shape.y + shape.r + pad,
    };
  }
  if (shape.kind === "ellipse") {
    const cos = Math.cos(shape.rotation);
    const sin = Math.sin(shape.rotation);
    const halfWidth = Math.hypot(shape.rx * cos, shape.ry * sin) + pad;
    const halfHeight = Math.hypot(shape.rx * sin, shape.ry * cos) + pad;
    return {
      left: shape.x - halfWidth,
      top: shape.y - halfHeight,
      right: shape.x + halfWidth,
      bottom: shape.y + halfHeight,
    };
  }
  // A quadratic curve stays inside the hull of its points.
  const points = [
    shape.from,
    ...shape.segments.flatMap((segment) =>
      segment.kind === "quad" ? [segment.control, segment.to] : [segment.to],
    ),
  ];
  const xs = points.map(([x]) => x);
  const ys = points.map(([, y]) => y);
  return {
    left: Math.min(...xs) - pad,
    top: Math.min(...ys) - pad,
    right: Math.max(...xs) + pad,
    bottom: Math.max(...ys) + pad,
  };
}

function insideIcon(x: number, y: number): boolean {
  const nearest = (value: number): number =>
    Math.min(Math.max(value, ICON_CORNER_RADIUS), ICON_UNITS - ICON_CORNER_RADIUS);
  return Math.hypot(x - nearest(x), y - nearest(y)) <= ICON_CORNER_RADIUS;
}

/** The icon's unit cells, row by row: 1 where a cell's centre is inside the rounded rect and under a moss circle. */
function coverGrid(shapes: readonly Shape[]): Uint8Array {
  const covered = new Uint8Array(ICON_UNITS * ICON_UNITS);
  for (const { x, y, r } of circles(shapes)) {
    const fromColumn = Math.max(0, Math.floor(x - r));
    const toColumn = Math.min(ICON_UNITS - 1, Math.ceil(x + r));
    const fromRow = Math.max(0, Math.floor(y - r));
    const toRow = Math.min(ICON_UNITS - 1, Math.ceil(y + r));
    for (let row = fromRow; row <= toRow; row++) {
      for (let column = fromColumn; column <= toColumn; column++) {
        if (Math.hypot(column + 0.5 - x, row + 0.5 - y) <= r && insideIcon(column + 0.5, row + 0.5)) {
          covered[row * ICON_UNITS + column] = 1;
        }
      }
    }
  }
  return covered;
}

/** How many of the icon's unit cells in its top `rows` have moss. */
function coveredCells(shapes: readonly Shape[], rows = ICON_UNITS): number {
  return coverGrid(shapes)
    .subarray(0, rows * ICON_UNITS)
    .reduce((sum, cell) => sum + cell, 0);
}

/** The bottom edge: the icon's last 12 rows. */
const EDGE_ROWS = 12;

/** The share of the icon's columns along its bottom edge that have moss somewhere in that edge. */
function bottomEdgeShare(shapes: readonly Shape[]): number {
  const grid = coverGrid(shapes);
  let columns = 0;
  let mossy = 0;
  for (let column = 0; column < ICON_UNITS; column++) {
    const rows = Array.from({ length: EDGE_ROWS }, (_, index) => ICON_UNITS - 1 - index);
    if (rows.some((row) => insideIcon(column + 0.5, row + 0.5))) {
      columns++;
      if (rows.some((row) => grid[row * ICON_UNITS + column] === 1)) {
        mossy++;
      }
    }
  }
  return mossy / columns;
}

/** The highest row with moss, or ICON_UNITS for none. */
function highestMossRow(shapes: readonly Shape[]): number {
  const index = coverGrid(shapes).indexOf(1);
  return index === -1 ? ICON_UNITS : Math.floor(index / ICON_UNITS);
}

describe("levelStep", () => {
  it.each([
    [0, 0],
    [1 / 256, 1],
    [0.25, 6],
    [0.26, 7],
    [0.5, 12],
    [0.75, 18],
    [1, 24],
  ])("puts overgrowth %d on step %d of 24, any moss on step 1 or above", (overgrowth, expected) => {
    expect(levelStep(quantize(overgrowth))).toBe(expected);
  });

  it("does not round a float32 one third up past its step", () => {
    expect(levelStep(quantize(Math.fround(1 / 3)))).toBe(8);
  });
});

// Expected values come from running moss-prototype.js itself at the same seed and level.
describe("iconAnchors, against the prototype", () => {
  // Below level 1 the bottom row grows earlier than the prototype's (look check round 1).
  it("shows the prototype's 151 anchors at level 1", () => {
    expect(iconAnchors(PROTOTYPE_SEED, 1)).toHaveLength(151);
  });

  it("places and sizes them as the prototype does at level 1", () => {
    const full = iconAnchors(PROTOTYPE_SEED, 1);

    expect(full[0]).toEqual({ id: 0, x: 0.25908533413894475, y: 109.71585649810731, r: 6.840293253771961 });
    expect(full[full.length - 1]).toEqual({
      id: 160,
      x: 25.259098537266254,
      y: 79.72674252092838,
      r: 5.920865388121456,
    });
  });
});

describe("mossScene, against the prototype", () => {
  it("draws the prototype's shapes at level 1: 2 shadow circles, 22 moss circles and 4 fronds an anchor", () => {
    const scene = mossScene({ seed: PROTOTYPE_SEED, step: step(1), overgrown: false });

    expect(circles(scene.occlusion)).toHaveLength(302);
    expect(circles(scene.moss)).toHaveLength(3322);
    expect(strokes(scene.moss, "#A6DA6C").length + strokes(scene.moss, "#7DBE52").length).toBe(536);
    expect(strokes(scene.moss, "#8A5A2B")).toHaveLength(47);
  });

  it("starts with the prototype's shadow, cushion, frond and stalk at level 1", () => {
    const full = mossScene({ seed: PROTOTYPE_SEED, step: step(1), overgrown: false });

    expect(full.occlusion[0]).toEqual({
      kind: "circle",
      x: 1.2590853341389447,
      y: 111.21585649810731,
      r: 9.234395892592147,
      paint: { kind: "fill", color: "#000", alpha: 0.07 },
    });
    // The prototype writes these to one decimal.
    const [cushion] = circles(full.moss);
    expect(cushion.paint).toEqual({ kind: "fill", color: "#17381B", alpha: 1 });
    expect([cushion.x, cushion.y, cushion.r].map((value) => value.toFixed(1))).toEqual([
      "3.0",
      "107.4",
      "5.2",
    ]);

    const [frond] = strokes(full.moss, "#A6DA6C");
    expect(prototypePath(frond)).toBe("M2.9 104.2l2.0 -4.2");

    const [stalk] = strokes(full.moss, "#8A5A2B");
    expect(prototypePath(stalk)).toBe("M6.6 108.3q-1.4 -3.8 -2.1 -7.5");
    const capsule = full.moss[full.moss.indexOf(stalk) + 1];
    expect(capsule).toMatchObject({ kind: "ellipse", rx: 1.1, ry: 1.9, paint: { color: "#C98F45" } });
  });
});

// The prototype's bottom row left one tuft at level 0.25, over 27 to 37% of the
// bottom edge's columns; its own level 0.5, which reads as a strip, covered 84 to
// 92%. Look check round 1 asked for a thin strip at 0.25: most of the edge, at
// least 80% of it, so a tuft fails by far, and none of it above the bottom quarter.
const FUZZ_EDGE_SHARE = 0.8;
const FUZZ_TOP_ROW = ICON_UNITS * 0.75;

describe("Fuzz", () => {
  it.each(SEEDS)("draws a strip along most of the bottom edge at level 0.25 (seed %d)", (seed) => {
    const { moss } = mossScene({ seed, step: step(0.25), overgrown: false });

    expect(bottomEdgeShare(moss)).toBeGreaterThanOrEqual(FUZZ_EDGE_SHARE);
  });

  it.each(SEEDS)(
    "keeps that strip thin: nothing above the bottom quarter at level 0.25 (seed %d)",
    (seed) => {
      const { moss } = mossScene({ seed, step: step(0.25), overgrown: false });

      expect(highestMossRow(moss)).toBeGreaterThanOrEqual(FUZZ_TOP_ROW);
    },
  );
});

describe("growth across the 24 steps", () => {
  it.each(SEEDS)("never moves an anchor or shrinks it as the level rises (seed %d)", (seed) => {
    for (let index = 0; index < LEVEL_STEPS; index++) {
      const now = new Map(iconAnchors(seed, index / LEVEL_STEPS).map((anchor) => [anchor.id, anchor]));
      const next = new Map(iconAnchors(seed, (index + 1) / LEVEL_STEPS).map((anchor) => [anchor.id, anchor]));
      for (const [id, anchor] of now) {
        const later = next.get(id);
        expect(later).toBeDefined();
        expect([later?.x, later?.y]).toEqual([anchor.x, anchor.y]);
        expect(later?.r).toBeGreaterThanOrEqual(anchor.r);
      }
    }
  });

  it("keeps an anchor's cushions in their colours and places as it grows, scaled about the anchor", () => {
    const small: Anchor = { id: 7, x: 40, y: 100, r: 3 };
    const grown: Anchor = { ...small, r: 6 };

    const before = circles(mossShapes([small], PROTOTYPE_SEED, 0).moss);
    const after = circles(mossShapes([grown], PROTOTYPE_SEED, 0).moss);

    expect(after.map(({ paint }) => paint)).toEqual(before.map(({ paint }) => paint));
    expect(after.map(({ x, y, r }) => [(x - 40) / 6, (y - 100) / 6, r / 6])).toEqual(
      before.map(({ x, y, r }) => [
        expect.closeTo((x - 40) / 3, 12),
        expect.closeTo((y - 100) / 3, 12),
        expect.closeTo(r / 3, 12),
      ]),
    );
  });

  it("gives an anchor the same shapes whatever other anchors are drawn with it", () => {
    const anchor: Anchor = { id: 12, x: 60, y: 104, r: 7 };
    const others: Anchor[] = [
      { id: 3, x: 10, y: 108, r: 8 },
      { id: 40, x: 100, y: 60, r: 6 },
    ];

    const alone = mossShapes([anchor], PROTOTYPE_SEED, 1);
    const together = mossShapes([others[0], anchor, others[1]], PROTOTYPE_SEED, 1);

    // Drawn with others, its shapes are still all there, in the same order.
    let found = 0;
    for (const shape of [...together.occlusion, ...together.moss]) {
      const wanted = [...alone.occlusion, ...alone.moss][found];
      if (wanted !== undefined && JSON.stringify(shape) === JSON.stringify(wanted)) {
        found++;
      }
    }
    expect(found).toBe(alone.occlusion.length + alone.moss.length);
  });

  it.each(SEEDS)("never uncovers part of the icon as the level rises (seed %d)", (seed) => {
    const coverage = STEPS.map((at) => coveredCells(mossScene({ seed, step: at, overgrown: true }).moss));

    for (let index = 1; index < coverage.length; index++) {
      expect(coverage[index]).toBeGreaterThanOrEqual(coverage[index - 1]);
    }
    expect(coverage[LEVEL_STEPS]).toBeGreaterThan(coverage[1]);
  });

  it.each(SEEDS)(
    "reaches the top edge last: none of it at level 0.5, most of it at level 1 (seed %d)",
    (seed) => {
      const topBand = (shapes: readonly Shape[]): number => coveredCells(shapes, 8);

      expect(topBand(mossScene({ seed, step: step(0.5), overgrown: false }).moss)).toBe(0);
      expect(topBand(mossScene({ seed, step: step(1), overgrown: false }).moss)).toBeGreaterThan(
        (ICON_UNITS * 8) / 2,
      );
    },
  );

  it("draws the same shapes for the same seed and level, every time", () => {
    const input = { seed: fnv1a32("com.hnc.Discord#0"), step: step(0.75), overgrown: true };

    expect(mossScene(input)).toEqual(mossScene(input));
  });

  it.each([false, true])("draws nothing at level 0 (overgrown: %s)", (overgrown) => {
    expect(mossScene({ seed: PROTOTYPE_SEED, step: step(0), overgrown })).toEqual({
      occlusion: [],
      moss: [],
      props: [],
    });
  });
});

describe("props", () => {
  it.each(SEEDS)("never grow on an item that is not overgrown (seed %d)", (seed) => {
    for (const at of STEPS) {
      expect(mossScene({ seed, step: at, overgrown: false }).props).toEqual([]);
    }
  });

  it.each(SEEDS)("grow on an overgrown item at every level above 0, inside the icon (seed %d)", (seed) => {
    for (const at of STEPS.slice(1)) {
      const { props } = mossScene({ seed, step: at, overgrown: true });

      expect(props.length).toBeGreaterThan(0);
      for (const shape of props) {
        const box = bounds(shape);
        expect(box.left).toBeGreaterThanOrEqual(0);
        expect(box.top).toBeGreaterThanOrEqual(0);
        expect(box.right).toBeLessThanOrEqual(ICON_UNITS);
        expect(box.bottom).toBeLessThanOrEqual(ICON_UNITS);
      }
    }
  });

  it("are a mushroom and a flower, at the lowest level 70% of their size at level 1, in the same place", () => {
    const seed = fnv1a32("com.hnc.Discord#0");
    const low = mossScene({ seed, step: 1 as LevelStep, overgrown: true }).props;
    const full = mossScene({ seed, step: step(1), overgrown: true }).props;
    const width = (shapes: readonly Shape[], color: string): number => {
      const box = bounds(shapes.find(({ paint }) => paint.kind === "fill" && paint.color === color) as Shape);
      return box.right - box.left;
    };
    const foot = (shapes: readonly Shape[], color: string): [number, number] => {
      const shape = shapes.find(({ paint }) => paint.kind === "fill" && paint.color === color);
      return shape?.kind === "ellipse" ? [shape.x, shape.y] : [Number.NaN, Number.NaN];
    };

    // The mushroom's cap and the flower's centre.
    // Look check round 1: big enough to read at any intensity above 0.
    for (const color of ["#C8553D", "#E8833A"]) {
      expect(width(low, color) / width(full, color)).toBeCloseTo(0.7, 6);
    }
    // The mushroom's shadow sits at its foot.
    expect(foot(low, "#17381B")).toEqual(foot(full, "#17381B"));
  });
});

describe("bakeBox", () => {
  // Job 009's AX tile rects in points, normal and magnified over Discord.
  it.each([
    [
      "normal",
      [
        [476, 46],
        [523, 47],
        [570, 47],
        [617, 47],
        [664, 47],
        [711, 47],
      ],
    ],
    [
      "magnified",
      [
        [435, 46],
        [482, 46],
        [529, 48],
        [578, 57],
        [635, 65],
        [701, 67],
        [769, 65],
      ],
    ],
  ])("of adjacent %s tiles stay inside their own tiles, so moss never reaches a neighbour", (_, tiles) => {
    const rects: LogicalRect[] = tiles.map(([x, width]) => ({ x, y: 918, width, height: width + 12 }));
    const boxes = rects.map(bakeBox);

    rects.forEach((tile, index) => {
      const box = boxes[index];
      expect(box.x).toBeGreaterThanOrEqual(tile.x - 1e-9);
      expect(box.y).toBeGreaterThanOrEqual(tile.y - 1e-9);
      expect(box.x + box.width).toBeLessThanOrEqual(tile.x + tile.width + 1e-9);
      expect(box.y + box.height).toBeLessThanOrEqual(tile.y + tile.height + 1e-9);
    });
    for (let index = 1; index < boxes.length; index++) {
      expect(boxes[index].x).toBeGreaterThanOrEqual(boxes[index - 1].x + boxes[index - 1].width - 1e-9);
    }
  });

  it("puts the icon's 112 units exactly on the art box, with the overhang around it", () => {
    const tile = { x: 711, y: 918, width: 47, height: 59 };
    const box = bakeBox(tile);
    const art = artBox(tile);
    const pointsPerUnit = art.width / ICON_UNITS;

    expect(box.height).toBe(box.width);
    expect(box.width).toBeCloseTo(TILE_UNITS * pointsPerUnit, 9);
    expect(art.x - box.x).toBeCloseTo(OVERHANG_UNITS * pointsPerUnit, 9);
    expect(art.y - box.y).toBeCloseTo(OVERHANG_UNITS * pointsPerUnit, 9);
  });
});
