import { describe, expect, it } from "vitest";

import { EMPTY_SCENE, type Paint, type Shape } from "../render/generator";
import { sceneSvg, shapesSvg } from "./svg";

const fill = (color: string, alpha = 1): Paint => ({ kind: "fill", color, alpha });
const dot = (x: number, paint: Paint): Shape => ({ kind: "circle", x, y: 2, r: 3, paint });

describe("shapesSvg", () => {
  it("draws a filled circle with its alpha as fill-opacity", () => {
    expect(shapesSvg([dot(1.5, fill("#123456", 0.5))])).toBe(
      '<g fill="#123456" fill-opacity="0.5"><circle cx="1.5" cy="2" r="3"/></g>',
    );
  });

  it("leaves out an opacity of 1", () => {
    expect(shapesSvg([dot(1, fill("#123456"))])).toBe('<g fill="#123456"><circle cx="1" cy="2" r="3"/></g>');
  });

  it("rounds coordinates to tenths", () => {
    expect(shapesSvg([{ kind: "circle", x: 1.23456, y: -0.04, r: 2.05001, paint: fill("#000") }])).toBe(
      '<g fill="#000"><circle cx="1.2" cy="0" r="2.1"/></g>',
    );
  });

  it("keeps opacities and stroke widths to thousandths", () => {
    expect(shapesSvg([dot(1, { kind: "stroke", color: "#000", width: 0.9450001, alpha: 0.07 })])).toBe(
      '<g fill="none" stroke="#000" stroke-width="0.945" stroke-opacity="0.07"><circle cx="1" cy="2" r="3"/></g>',
    );
  });

  it("shares one group between neighbours with the same paint, in draw order", () => {
    const a = fill("#aaa");
    const b = fill("#bbb");
    expect(shapesSvg([dot(1, a), dot(2, a), dot(3, b), dot(4, a)])).toBe(
      '<g fill="#aaa"><circle cx="1" cy="2" r="3"/><circle cx="2" cy="2" r="3"/></g>' +
        '<g fill="#bbb"><circle cx="3" cy="2" r="3"/></g>' +
        '<g fill="#aaa"><circle cx="4" cy="2" r="3"/></g>',
    );
  });

  it("draws nothing for no shapes", () => {
    expect(shapesSvg([])).toBe("");
  });

  it("traces a path through its lines and quadratic curves", () => {
    expect(
      shapesSvg([
        {
          kind: "path",
          from: [0, 0],
          segments: [
            { kind: "line", to: [1, 1] },
            { kind: "quad", control: [2, 2], to: [3, 3] },
          ],
          closed: true,
          paint: fill("#000"),
        },
      ]),
    ).toBe('<g fill="#000"><path d="M0 0L1 1Q2 2 3 3Z"/></g>');
  });

  it("leaves an open path open", () => {
    expect(
      shapesSvg([
        { kind: "path", from: [0, 0], segments: [{ kind: "line", to: [1, 1] }], closed: false, paint: fill("#000") },
      ]),
    ).toBe('<g fill="#000"><path d="M0 0L1 1"/></g>');
  });

  it("turns an ellipse by its rotation in radians", () => {
    expect(
      shapesSvg([{ kind: "ellipse", x: 5, y: 6, rx: 1, ry: 2, rotation: Math.PI / 2, paint: fill("#000") }]),
    ).toBe('<g fill="#000"><ellipse cx="5" cy="6" rx="1" ry="2" transform="rotate(90 5 6)"/></g>');
  });

  it("strokes without a fill", () => {
    expect(shapesSvg([dot(1, { kind: "stroke", color: "#3F7A32", width: 1.5, alpha: 0.8 })])).toBe(
      '<g fill="none" stroke="#3F7A32" stroke-width="1.5" stroke-opacity="0.8"><circle cx="1" cy="2" r="3"/></g>',
    );
  });

  it("strokes at full opacity without a stroke-opacity", () => {
    expect(shapesSvg([dot(1, { kind: "stroke", color: "#000", width: 1, alpha: 1 })])).toBe(
      '<g fill="none" stroke="#000" stroke-width="1"><circle cx="1" cy="2" r="3"/></g>',
    );
  });

  it("fills and strokes at full opacity", () => {
    expect(shapesSvg([dot(1, { kind: "fillAndStroke", fill: "#F7D54A", stroke: "#D9A92B", width: 0.4 })])).toBe(
      '<g fill="#F7D54A" stroke="#D9A92B" stroke-width="0.4"><circle cx="1" cy="2" r="3"/></g>',
    );
  });
});

describe("sceneSvg", () => {
  it("draws nothing for an empty scene", () => {
    expect(sceneSvg(EMPTY_SCENE)).toBe("");
  });

  it("clips the occlusion to the 112-unit icon with its 0.225 corner radius, then draws the moss and the props over it, with the bake's round line caps and miter limit", () => {
    const svg = sceneSvg({
      occlusion: [dot(1, fill("#000", 0.2))],
      moss: [dot(2, fill("#3E7E2F"))],
      props: [dot(3, fill("#C8553D"))],
    });
    expect(svg).toBe(
      '<g stroke-linecap="round" stroke-miterlimit="10">' +
        '<clipPath id="icon"><rect width="112" height="112" rx="25.2"/></clipPath>' +
        '<g clip-path="url(#icon)"><g fill="#000" fill-opacity="0.2"><circle cx="1" cy="2" r="3"/></g></g>' +
        '<g fill="#3E7E2F"><circle cx="2" cy="2" r="3"/></g>' +
        '<g fill="#C8553D"><circle cx="3" cy="2" r="3"/></g>' +
        "</g>",
    );
  });

  it("leaves out the clip when nothing is occluded", () => {
    expect(sceneSvg({ occlusion: [], moss: [dot(2, fill("#3E7E2F"))], props: [] })).toBe(
      '<g stroke-linecap="round" stroke-miterlimit="10"><g fill="#3E7E2F"><circle cx="2" cy="2" r="3"/></g></g>',
    );
  });
});
