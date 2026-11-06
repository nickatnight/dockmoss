import {
  ICON_CORNER_RADIUS,
  ICON_UNITS,
  OVERHANG_UNITS,
  TILE_UNITS,
  type MossScene,
  type Paint,
  type Shape,
} from "./generator";

export type BakeContext = Pick<
  OffscreenCanvasRenderingContext2D,
  | "setTransform"
  | "save"
  | "restore"
  | "beginPath"
  | "roundRect"
  | "clip"
  | "arc"
  | "ellipse"
  | "moveTo"
  | "lineTo"
  | "quadraticCurveTo"
  | "closePath"
  | "fill"
  | "stroke"
  | "fillStyle"
  | "strokeStyle"
  | "lineWidth"
  | "lineCap"
  | "globalAlpha"
>;

const FULL_TURN = 2 * Math.PI;

function isEmpty({ occlusion, moss, props }: MossScene): boolean {
  return occlusion.length === 0 && moss.length === 0 && props.length === 0;
}

function trace(context: BakeContext, shape: Shape): void {
  context.beginPath();
  switch (shape.kind) {
    case "circle":
      context.arc(shape.x, shape.y, shape.r, 0, FULL_TURN);
      return;
    case "ellipse":
      context.ellipse(shape.x, shape.y, shape.rx, shape.ry, shape.rotation, 0, FULL_TURN);
      return;
    case "path":
      context.moveTo(...shape.from);
      for (const segment of shape.segments) {
        if (segment.kind === "line") {
          context.lineTo(...segment.to);
        } else {
          context.quadraticCurveTo(...segment.control, ...segment.to);
        }
      }
      if (shape.closed) {
        context.closePath();
      }
  }
}

function paintShape(context: BakeContext, paint: Paint): void {
  switch (paint.kind) {
    case "fill":
      context.globalAlpha = paint.alpha;
      context.fillStyle = paint.color;
      context.fill();
      return;
    case "stroke":
      context.globalAlpha = paint.alpha;
      context.strokeStyle = paint.color;
      context.lineWidth = paint.width;
      context.stroke();
      return;
    case "fillAndStroke":
      context.globalAlpha = 1;
      context.fillStyle = paint.fill;
      context.fill();
      context.strokeStyle = paint.stroke;
      context.lineWidth = paint.width;
      context.stroke();
  }
}

function draw(context: BakeContext, shapes: readonly Shape[]): void {
  for (const shape of shapes) {
    trace(context, shape);
    paintShape(context, shape.paint);
  }
}

export function drawScene(context: BakeContext, scene: MossScene, side: number): void {
  if (isEmpty(scene)) {
    return;
  }
  const scale = side / TILE_UNITS;
  context.setTransform(scale, 0, 0, scale, OVERHANG_UNITS * scale, OVERHANG_UNITS * scale);
  context.lineCap = "round";
  if (scene.occlusion.length > 0) {
    context.save();
    context.beginPath();
    context.roundRect(0, 0, ICON_UNITS, ICON_UNITS, ICON_CORNER_RADIUS);
    context.clip();
    draw(context, scene.occlusion);
    context.restore();
  }
  draw(context, scene.moss);
  draw(context, scene.props);
}

export type Bake = { kind: "empty" } | { kind: "baked"; canvas: OffscreenCanvas } | { kind: "noContext" };

export function bake(scene: MossScene, side: number): Bake {
  if (isEmpty(scene)) {
    return { kind: "empty" };
  }
  const canvas = new OffscreenCanvas(side, side);
  // Software-backed: an accelerated 2D canvas draws and keeps its surface in the
  // GPU process, which put about 30 MB on the startup peak (job 017).
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (context === null) {
    return { kind: "noContext" };
  }
  drawScene(context, scene, side);
  return { kind: "baked", canvas };
}
