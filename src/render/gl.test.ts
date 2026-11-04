import { describe, expect, it } from "vitest";
import { INSTANCE_ATTRIBUTES } from "./gl";
import { type MossInstance, quantize } from "./model";
import vertexSource from "./moss.vert.glsl?raw";

const GLSL_SIZES: Record<string, number> = { float: 1, vec2: 2, vec3: 3, vec4: 4 };

const instance: MossInstance = {
  box: { x: 10, y: 20, width: 74, height: 74 },
  seed: 0x12345678,
  overgrowth: quantize(0.6),
  hue: -0.5,
  sprites: [{ kind: "flower", x: 0.4, y: 0.2, size: 0.15 }],
};

describe("INSTANCE_ATTRIBUTES", () => {
  it("lists every input of moss.vert.glsl with its size, in the shader's order", () => {
    const inputs = [...vertexSource.matchAll(/^in (\w+) (\w+);$/gm)].map(([, type, name]) => ({
      name,
      size: GLSL_SIZES[type],
    }));

    expect(inputs.length).toBeGreaterThan(0);
    expect(INSTANCE_ATTRIBUTES.map(({ name, size }) => ({ name, size }))).toEqual(inputs);
  });

  it("writes exactly `size` values per attribute, zeros for a missing sprite", () => {
    const values = INSTANCE_ATTRIBUTES.map((attribute) => attribute.values(instance));

    expect(values.map((written) => written.length)).toEqual(INSTANCE_ATTRIBUTES.map(({ size }) => size));
    expect(values.flat()).toEqual([
      10, 20, 74, 74,
      ((0x5678 / 0x10000) * 64), ((0x1234 / 0x10000) * 64),
      quantize(0.6), -0.5,
      0.4, 0.2, 0.15, 2,
      0, 0, 0, 0,
      0, 0, 0, 0,
    ]);
  });
});
