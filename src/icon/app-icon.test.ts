import { describe, expect, it } from "vitest";

import { type LevelStep, mossScene } from "../render/generator";
import { fnv1a32 } from "../render/model";
import { APP_ICON_MOSS, appIconMaster, appIconSvg } from "./app-icon";
import { sceneSvg } from "./svg";

function mapThrough(transform: string): (value: number) => number {
  const steps = [...transform.matchAll(/(translate|scale)\(([-\d.]+)(?: ([-\d.]+))?\)/g)].map(([, kind, a, b]) => {
    expect(b === undefined || b === a, `non-uniform ${kind} in ${transform}`).toBe(true);
    return { kind, by: Number(a) };
  });
  expect(steps.length).toBeGreaterThan(0);
  return (value) =>
    steps.reduceRight((at, { kind, by }) => (kind === "translate" ? at + by : at * by), value);
}

function groupTransforms(svg: string): string[] {
  return [...svg.matchAll(/<g transform="([^"]+)">/g)].map(([, transform]) => transform);
}

function xmlProblems(document: string): string[] {
  const problems: string[] = [];
  const body = document.replace(/<!--([\s\S]*?)-->/g, (_, text: string) => {
    if (text.includes("--")) problems.push("a comment holds --");
    return "";
  });
  const stack: string[] = [];
  let roots = 0;
  for (const [token] of body.matchAll(/<[^<>]*>|[^<>]+|[<>]/g)) {
    if (!token.startsWith("<") || token === "<") {
      if (token.trim() !== "") problems.push(`stray text ${JSON.stringify(token.slice(0, 20))}`);
      continue;
    }
    const close = /^<\/([A-Za-z][\w:-]*)>$/.exec(token);
    if (close) {
      const name = stack.pop();
      if (name !== close[1]) problems.push(`</${close[1]}> closes <${name}>`);
      continue;
    }
    const open = /^<([A-Za-z][\w:-]*)((?:\s+[A-Za-z][\w:.-]*="[^"<&]*")*)\s*(\/?)>$/.exec(token);
    if (!open) {
      problems.push(`malformed tag ${token.slice(0, 40)}`);
      continue;
    }
    const [, name, attributes, selfClosing] = open;
    const names = [...attributes.matchAll(/([A-Za-z][\w:.-]*)=/g)].map(([, attribute]) => attribute);
    if (new Set(names).size !== names.length) problems.push(`<${name}> repeats an attribute`);
    if (stack.length === 0) roots += 1;
    if (selfClosing === "") stack.push(name);
  }
  if (stack.length > 0) problems.push(`unclosed <${stack.join("> <")}>`);
  if (roots !== 1) problems.push(`${roots} root elements`);
  return problems;
}

describe("xmlProblems", () => {
  it("finds a mismatched tag, an unquoted attribute and a second root", () => {
    expect(xmlProblems('<svg><g fill="#000"></svg>')).not.toEqual([]);
    expect(xmlProblems("<svg><rect width=4/></svg>")).not.toEqual([]);
    expect(xmlProblems("<svg/><svg/>")).not.toEqual([]);
    expect(xmlProblems('<!-- a note --><svg><g fill="#000"><rect/></g></svg>\n')).toEqual([]);
  });
});

describe("the app icon", () => {
  const svg = appIconSvg();

  it("grows the moss of the app's first Dock item keyed dockmoss, at level step 16 of 24, without props", () => {
    expect(APP_ICON_MOSS).toEqual({ seed: fnv1a32("dockmoss#0"), step: 16, overgrown: false });
  });

  it("is the same document on every call", () => {
    expect(appIconSvg()).toBe(svg);
  });

  it("is a well-formed 1024 px square SVG document", () => {
    expect(svg).toMatch(
      /^<svg xmlns="http:\/\/www\.w3\.org\/2000\/svg" width="1024" height="1024" viewBox="0 0 1024 1024">/,
    );
    expect(xmlProblems(svg)).toEqual([]);
  });

  it("puts the stone where the overlay puts a Dock icon's art in its tile: 0.79 of the side, centred", () => {
    const [stone] = groupTransforms(svg);
    expect(svg).toContain('<rect x="100" y="100" width="824" height="824" rx="185" fill="url(#stone)"');
    const toCanvas = mapThrough(stone);
    expect(toCanvas(100)).toBeCloseTo(107.52, 1);
    expect(toCanvas(924)).toBeCloseTo(107.52 + 808.96, 1);
  });

  it("lays the moss's 112 icon units over the same box as the stone", () => {
    const [, moss] = groupTransforms(svg);
    const toCanvas = mapThrough(moss);
    expect(toCanvas(0)).toBeCloseTo(107.52, 1);
    expect(toCanvas(112)).toBeCloseTo(107.52 + 808.96, 1);
  });

  it("draws the generator's moss over the stone", () => {
    const moss = sceneSvg(mossScene(APP_ICON_MOSS));
    expect(moss.length).toBeGreaterThan(1000);
    expect(svg).toContain(moss);
    expect(svg.indexOf(moss)).toBeGreaterThan(svg.indexOf("url(#stone)"));
  });

  it("grows other moss for other inputs", () => {
    expect(appIconSvg({ ...APP_ICON_MOSS, seed: 1 })).not.toBe(svg);
    expect(appIconSvg({ ...APP_ICON_MOSS, step: 8 as LevelStep })).not.toBe(svg);
  });
});

describe("the bundle icon master", () => {
  it("is src-tauri/icons/source/app-icon.svg, which `pnpm test src/icon -u` rewrites", async () => {
    await expect(appIconMaster()).toMatchFileSnapshot("../../src-tauri/icons/source/app-icon.svg");
  });

  it("is the app icon after a comment that says it is generated and names the update command", () => {
    const master = appIconMaster();
    const header = master.slice(0, master.length - appIconSvg().length);
    expect(master.endsWith(appIconSvg())).toBe(true);
    expect(header).toMatch(/^<!--[\s\S]*-->\n$/);
    expect(header).toContain("Generated by src/icon/app-icon.ts");
    expect(header).toContain("pnpm test src/icon -u");
    expect(xmlProblems(master)).toEqual([]);
  });
});
