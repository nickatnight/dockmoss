import { describe, expect, it, vi } from "vitest";

import { type BakeContext, bake, drawScene } from "./bake";
import {
  EMPTY_SCENE,
  ICON_CORNER_RADIUS,
  ICON_UNITS,
  OVERHANG_UNITS,
  TILE_UNITS,
  type LevelStep,
  type MossScene,
  mossScene,
} from "./generator";

type Command = [name: string, ...args: unknown[]];

class RecordingContext implements BakeContext {
  readonly commands: Command[] = [];
  private readonly record =
    (name: string) =>
    (...args: unknown[]) => {
      this.commands.push([name, ...args]);
    };

  setTransform = this.record("setTransform") as BakeContext["setTransform"];
  save = this.record("save");
  restore = this.record("restore");
  beginPath = this.record("beginPath");
  roundRect = this.record("roundRect");
  clip = this.record("clip") as BakeContext["clip"];
  arc = this.record("arc");
  ellipse = this.record("ellipse");
  moveTo = this.record("moveTo");
  lineTo = this.record("lineTo");
  quadraticCurveTo = this.record("quadraticCurveTo");
  closePath = this.record("closePath");
  fill = this.record("fill") as BakeContext["fill"];
  stroke = this.record("stroke") as BakeContext["stroke"];

  set fillStyle(value: BakeContext["fillStyle"]) {
    this.commands.push(["fillStyle", value]);
  }
  set strokeStyle(value: BakeContext["strokeStyle"]) {
    this.commands.push(["strokeStyle", value]);
  }
  set lineWidth(value: number) {
    this.commands.push(["lineWidth", value]);
  }
  set lineCap(value: CanvasLineCap) {
    this.commands.push(["lineCap", value]);
  }
  set globalAlpha(value: number) {
    this.commands.push(["globalAlpha", value]);
  }
}

const TAU = 2 * Math.PI;

describe("drawScene", () => {
  it("maps the tile, overhang included, onto the whole canvas", () => {
    const context = new RecordingContext();
    drawScene(context, mossScene({ seed: 4271, step: 24 as LevelStep, overgrown: false }), 256);

    const [name, scale, , , , dx, dy] = context.commands[0] as [string, ...number[]];
    expect(name).toBe("setTransform");
    expect(scale * -OVERHANG_UNITS + dx).toBeCloseTo(0, 9);
    expect(scale * (ICON_UNITS + OVERHANG_UNITS) + dx).toBeCloseTo(256, 9);
    expect(dy).toBe(dx);
    expect(scale * TILE_UNITS).toBeCloseTo(256, 9);
  });

  it("draws each kind of shape and paint with the matching canvas calls", () => {
    const scene: MossScene = {
      occlusion: [
        { kind: "circle", x: 10, y: 100, r: 5, paint: { kind: "fill", color: "#000", alpha: 0.07 } },
      ],
      moss: [
        {
          kind: "path",
          from: [1, 2],
          segments: [
            { kind: "line", to: [3, 4] },
            { kind: "quad", control: [5, 6], to: [7, 8] },
          ],
          closed: false,
          paint: { kind: "stroke", color: "#8A5A2B", width: 0.7, alpha: 0.85 },
        },
      ],
      props: [
        {
          kind: "ellipse",
          x: 50,
          y: 90,
          rx: 2.3,
          ry: 3.6,
          rotation: 0.5,
          paint: { kind: "fillAndStroke", fill: "#F7D54A", stroke: "#D9A92B", width: 0.3 },
        },
        {
          kind: "path",
          from: [0, 0],
          segments: [{ kind: "line", to: [1, 0] }],
          closed: true,
          paint: { kind: "fill", color: "#C8553D", alpha: 1 },
        },
      ],
    };
    const context = new RecordingContext();
    const side = 90;
    const scale = side / TILE_UNITS;

    drawScene(context, scene, side);

    expect(context.commands).toEqual([
      ["setTransform", scale, 0, 0, scale, OVERHANG_UNITS * scale, OVERHANG_UNITS * scale],
      ["lineCap", "round"],
      ["save"],
      ["beginPath"],
      ["roundRect", 0, 0, ICON_UNITS, ICON_UNITS, ICON_CORNER_RADIUS],
      ["clip"],
      ["beginPath"],
      ["arc", 10, 100, 5, 0, TAU],
      ["globalAlpha", 0.07],
      ["fillStyle", "#000"],
      ["fill"],
      ["restore"],
      ["beginPath"],
      ["moveTo", 1, 2],
      ["lineTo", 3, 4],
      ["quadraticCurveTo", 5, 6, 7, 8],
      ["globalAlpha", 0.85],
      ["strokeStyle", "#8A5A2B"],
      ["lineWidth", 0.7],
      ["stroke"],
      ["beginPath"],
      ["ellipse", 50, 90, 2.3, 3.6, 0.5, 0, TAU],
      ["globalAlpha", 1],
      ["fillStyle", "#F7D54A"],
      ["fill"],
      ["strokeStyle", "#D9A92B"],
      ["lineWidth", 0.3],
      ["stroke"],
      ["beginPath"],
      ["moveTo", 0, 0],
      ["lineTo", 1, 0],
      ["closePath"],
      ["globalAlpha", 1],
      ["fillStyle", "#C8553D"],
      ["fill"],
    ]);
  });

  it("paints the prototype's seed at level 1 as the prototype does: 302 shadows, then 3,322 cushions, 536 fronds and 47 stalks", () => {
    const scene = mossScene({ seed: 4271, step: 24 as LevelStep, overgrown: false });
    const context = new RecordingContext();

    drawScene(context, scene, 256);

    const names = context.commands.map(([name]) => name);
    const clipEnd = names.indexOf("restore");
    const count = (name: string, from: number, to: number): number =>
      names.slice(from, to).filter((other) => other === name).length;
    expect(names.slice(2, 6)).toEqual(["save", "beginPath", "roundRect", "clip"]);
    expect(count("fill", 0, clipEnd)).toBe(302);
    expect(count("fill", clipEnd, names.length)).toBe(3322 + 47);
    expect(count("stroke", clipEnd, names.length)).toBe(536 + 47);
    expect(count("beginPath", clipEnd, names.length)).toBe(3322 + 536 + 47 + 47);
    const colours = context.commands.flatMap(([name, value]) =>
      name === "fillStyle" || name === "strokeStyle" ? [value] : [],
    );
    expect(colours).toEqual(
      [...scene.occlusion, ...scene.moss].map(({ paint }) =>
        paint.kind === "fillAndStroke" ? paint.fill : paint.color,
      ),
    );
  });

  it("issues no canvas call for an empty scene", () => {
    const context = new RecordingContext();

    drawScene(context, EMPTY_SCENE, 256);

    expect(context.commands).toEqual([]);
  });
});

describe("bake", () => {
  it("reports a canvas without a 2D context", () => {
    vi.stubGlobal(
      "OffscreenCanvas",
      class {
        getContext(): null {
          return null;
        }
      },
    );
    try {
      expect(bake(mossScene({ seed: 4271, step: 24 as LevelStep, overgrown: true }), 256)).toEqual({
        kind: "noContext",
      });
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("makes no canvas at level 0", () => {
    expect(bake(mossScene({ seed: 4271, step: 0 as LevelStep, overgrown: true }), 256)).toEqual({
      kind: "empty",
    });
  });
});
