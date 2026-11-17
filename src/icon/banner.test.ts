import { XMLValidator } from "fast-xml-parser";
import { describe, expect, it } from "vitest";

import { LEVEL_STEPS, mossScene } from "../render/generator";
import { fnv1a32 } from "../render/model";
import { STONE_TILE } from "./app-icon";
import { BANNER_SECONDS, BANNER_TILES, bannerMaster, bannerSvg } from "./banner";
import { SHARED_ICON_CLIP } from "./svg";

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
    expect(svg).toContain(STONE_TILE.svg);
    expect(svg).toContain(STONE_TILE.defs);
  });

  it("draws each tile's moss with the app's generator, seeded per tile, at every stage it reaches", () => {
    const mossy = BANNER_TILES.filter(({ stages }) => stages.length > 0);
    expect(mossy.length).toBeGreaterThanOrEqual(4);
    for (const [index, { stages }] of BANNER_TILES.entries()) {
      for (const { step } of stages) {
        const moss = SHARED_ICON_CLIP.sceneSvg(mossScene({ seed: fnv1a32(`banner#${index}`), step, overgrown: step === LEVEL_STEPS }));
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

  it("grows each tile's stages in order, all within the loop", () => {
    for (const { stages } of BANNER_TILES) {
      for (const [index, { at, step }] of stages.entries()) {
        expect(at).toBeGreaterThan(0);
        expect(at).toBeLessThan(BANNER_SECONDS);
        if (index > 0) {
          expect(at).toBeGreaterThan(stages[index - 1].at);
          expect(step).toBeGreaterThan(stages[index - 1].step);
        }
      }
    }
  });

  it("launches exactly one tile, and that tile is Overgrown when it launches", () => {
    const launching = BANNER_TILES.filter(({ launchesAt }) => launchesAt !== undefined);
    expect(launching).toHaveLength(1);
    const [{ stages, launchesAt = 0 }] = launching;
    const last = stages[stages.length - 1];
    expect(last.step).toBe(LEVEL_STEPS);
    expect(last.at).toBeLessThan(launchesAt);
  });
});

describe("the banner's animation", () => {
  const svg = bannerSvg();
  const animations = [...svg.matchAll(/<animate(?:Transform)?\s[^>]*\/>/g)].map(([element]) => element);
  const attribute = (element: string, name: string): string | undefined =>
    new RegExp(`\\s${name}="([^"]*)"`).exec(element)?.[1];

  it("is SMIL, not script", () => {
    expect(animations.length).toBeGreaterThan(10);
  });

  it("runs every animation on one shared clock that loops forever", () => {
    for (const element of animations) {
      expect(attribute(element, "dur"), element).toBe(`${BANNER_SECONDS}s`);
      expect(attribute(element, "repeatCount"), element).toBe("indefinite");
    }
  });

  it("loops for 10 to 12 seconds", () => {
    expect(BANNER_SECONDS).toBeGreaterThanOrEqual(10);
    expect(BANNER_SECONDS).toBeLessThanOrEqual(12);
  });

  it("gives every animation one key time per value, rising from 0 to 1", () => {
    for (const element of animations) {
      const times = (attribute(element, "keyTimes") ?? "").split(";").map(Number);
      const values = (attribute(element, "values") ?? "").split(";");
      expect(times, element).toHaveLength(values.length);
      expect(times[0], element).toBe(0);
      expect(times[times.length - 1], element).toBe(1);
      for (const [index, time] of times.entries()) {
        if (index > 0) expect(time, element).toBeGreaterThan(times[index - 1]);
      }
    }
  });

  it("ends every animation on the value it starts with, so the loop has no seam", () => {
    for (const element of animations) {
      const values = (attribute(element, "values") ?? "").split(";");
      expect(values[values.length - 1], element).toBe(values[0]);
    }
  });
});

describe("the README banner master", () => {
  it("is docs/readme/banner.svg, which `pnpm test src/icon -u` rewrites", async () => {
    await expect(bannerMaster()).toMatchFileSnapshot("../../docs/readme/banner.svg");
  });

  it("pins bannerSvg()'s SHA-256, so a change to the art or to the moss generator is deliberate", async () => {
    const BANNER_SHA256 = "9e41a0a192172e79bc3056281c4a04686b6b1d20d1f6b97480d031fb80e397c2";
    const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(bannerSvg()));
    const hex = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
    expect(hex).toBe(BANNER_SHA256);
  });

  it("stays under 1 MB", () => {
    expect(new TextEncoder().encode(bannerMaster()).length).toBeLessThan(1_000_000);
  });

  it("is the banner after a comment that says it is generated and names the update command", () => {
    const master = bannerMaster();
    const header = master.slice(0, master.length - bannerSvg().length);
    expect(master.endsWith(bannerSvg())).toBe(true);
    expect(header).toMatch(/^<!--[\s\S]*-->\n$/);
    expect(header).toContain("Generated by src/icon/banner.ts");
    expect(header).toContain("pnpm test src/icon -u");
    expect(header).toContain("SHA-256");
  });

  it("is well-formed XML, header comment included", () => {
    const master = bannerMaster();
    expect(XMLValidator.validate(master)).toBe(true);
    // XML forbids "--" inside a comment, and XMLValidator does not check it.
    expect(master.slice("<!--".length, master.indexOf("-->"))).not.toContain("--");
  });
});
