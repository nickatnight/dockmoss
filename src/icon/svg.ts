import { ICON_CORNER_RADIUS, ICON_UNITS, type MossScene, type Paint, type Point, type Shape } from "../render/generator";

const CLIP_ID = "icon";

const tenths = (value: number): string => String(Number(value.toFixed(1)));
const thousandths = (value: number): string => String(Number(value.toFixed(3)));
const point = ([x, y]: Point): string => `${tenths(x)} ${tenths(y)}`;
const opacity = (attribute: string, alpha: number): string =>
  alpha === 1 ? "" : ` ${attribute}="${thousandths(alpha)}"`;

function paintAttributes(paint: Paint): string {
  switch (paint.kind) {
    case "fill":
      return `fill="${paint.color}"${opacity("fill-opacity", paint.alpha)}`;
    case "stroke":
      return `fill="none" stroke="${paint.color}" stroke-width="${thousandths(paint.width)}"${opacity("stroke-opacity", paint.alpha)}`;
    case "fillAndStroke":
      return `fill="${paint.fill}" stroke="${paint.stroke}" stroke-width="${thousandths(paint.width)}"`;
  }
}

function shapeElement(shape: Shape): string {
  switch (shape.kind) {
    case "circle":
      return `<circle cx="${tenths(shape.x)}" cy="${tenths(shape.y)}" r="${tenths(shape.r)}"/>`;
    case "ellipse": {
      const [cx, cy] = [tenths(shape.x), tenths(shape.y)];
      const degrees = tenths((shape.rotation * 180) / Math.PI);
      return `<ellipse cx="${cx}" cy="${cy}" rx="${tenths(shape.rx)}" ry="${tenths(shape.ry)}" transform="rotate(${degrees} ${cx} ${cy})"/>`;
    }
    case "path": {
      const segments = shape.segments
        .map((segment) =>
          segment.kind === "line" ? `L${point(segment.to)}` : `Q${point(segment.control)} ${point(segment.to)}`,
        )
        .join("");
      return `<path d="M${point(shape.from)}${segments}${shape.closed ? "Z" : ""}"/>`;
    }
  }
}

export function shapesSvg(shapes: readonly Shape[]): string {
  let out = "";
  let open: string | null = null;
  for (const shape of shapes) {
    const attributes = paintAttributes(shape.paint);
    if (attributes !== open) {
      if (open !== null) out += "</g>";
      out += `<g ${attributes}>`;
      open = attributes;
    }
    out += shapeElement(shape);
  }
  return open === null ? out : `${out}</g>`;
}

export function sceneSvg(scene: MossScene): string {
  const occlusion =
    scene.occlusion.length === 0
      ? ""
      : `<clipPath id="${CLIP_ID}"><rect width="${ICON_UNITS}" height="${ICON_UNITS}" rx="${tenths(ICON_CORNER_RADIUS)}"/></clipPath>` +
        `<g clip-path="url(#${CLIP_ID})">${shapesSvg(scene.occlusion)}</g>`;
  const shapes = occlusion + shapesSvg(scene.moss) + shapesSvg(scene.props);
  return shapes === "" ? "" : `<g stroke-linecap="round" stroke-miterlimit="10">${shapes}</g>`;
}
