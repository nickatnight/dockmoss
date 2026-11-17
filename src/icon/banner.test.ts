import { XMLValidator } from "fast-xml-parser";
import { describe, expect, it } from "vitest";

import { LEVEL_STEPS, mossScene } from "../render/generator";
import { fnv1a32 } from "../render/model";
import { STONE_TILE } from "./app-icon";
import { BANNER_TILES, bannerSvg } from "./banner";
import { sceneSvg } from "./svg";

describe("the README banner", () => {
  const svg = bannerSvg();

  it("is a 1600 by 900 SVG document", () => {
    expect(svg).toMatch(
      /^<svg xmlns="http:\/\/www\.w3\.org\/2000\/svg" width="1600" height="900" viewBox="0 0 1600 900"/,
    );
  });

  it("is well-formed XML", () => {
    expect(XMLValidator.validate(svg)).toBe(true);
  });

  it("is the same document on every call", () => {
    expect(bannerSvg()).toBe(svg);
  });

  it("holds nothing GitHub's image proxy strips: no script, no foreignObject, no event handler", () => {
    expect(svg).not.toMatch(/<script/i);
    expect(svg).not.toMatch(/<foreignObject/i);
    expect(svg).not.toMatch(/\son[a-z]+=/i);
  });

  it("refers only to its own elements: every href and url() points at an id in the document", () => {
    const ids = new Set([...svg.matchAll(/\sid="([^"]+)"/g)].map(([, id]) => id));
    const references = [
      ...[...svg.matchAll(/\shref="([^"]*)"/g)].map(([, target]) => target),
      ...[...svg.matchAll(/url\(([^)]*)\)/g)].map(([, target]) => target),
    ];
    expect(references.length).toBeGreaterThan(0);
    for (const target of references) {
      expect(target, `reference ${target}`).toMatch(/^#/);
      expect(ids.has(target.slice(1)), `reference ${target}`).toBe(true);
    }
  });

  it("gives every id to one element only", () => {
    const ids = [...svg.matchAll(/\sid="([^"]+)"/g)].map(([, id]) => id);
    expect(ids.length).toBeGreaterThan(0);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("puts the app icon's stone tile in the Dock", () => {
    expect(svg).toContain(STONE_TILE);
  });

  it("draws each tile's moss with the app's generator, seeded per tile, at the stage it ends at", () => {
    const mossy = BANNER_TILES.filter(({ stages }) => stages.length > 0);
    expect(mossy.length).toBeGreaterThanOrEqual(4);
    for (const [index, { stages }] of BANNER_TILES.entries()) {
      for (const { step } of stages.slice(-1)) {
        const moss = sceneSvg(mossScene({ seed: fnv1a32(`banner#${index}`), step, overgrown: step === LEVEL_STEPS }), "shared");
        expect(svg, `tile ${index} at step ${step}`).toContain(moss);
      }
    }
  });

  it("grows at least one tile all the way to Overgrown, with its mushroom and flower", () => {
    expect(BANNER_TILES.some(({ stages }) => stages.some(({ step }) => step === LEVEL_STEPS))).toBe(true);
  });

  it("leaves at least one tile clean, as an app in daily use", () => {
    expect(BANNER_TILES.some(({ stages }) => stages.length === 0)).toBe(true);
  });
});
