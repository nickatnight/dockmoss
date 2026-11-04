import { MAX_SPRITES, type MossInstance, type Sprite, glslConstants, seedOffset } from "./model";
import fragmentSource from "./moss.frag.glsl?raw";
import vertexSource from "./moss.vert.glsl?raw";

const CONSTANTS_LINE = "#pragma moss_constants";

// Per instance: the device box (4 floats), the seed offset and the
// overgrowth (3), then MAX_SPRITES sprites of x, y, size and kind (4 each;
// kind 0 is no sprite). The locations match the `layout` qualifiers in
// moss.vert.glsl.
const BOX_LOCATION = 0;
const MOSS_LOCATION = 1;
const FIRST_SPRITE_LOCATION = 2;
const FLOATS_PER_SPRITE = 4;
const FLOATS_PER_INSTANCE = 7 + MAX_SPRITES * FLOATS_PER_SPRITE;
const BYTES_PER_FLOAT = 4;
const SPRITE_KINDS: Record<Sprite["kind"], number> = { mushroom: 1, flower: 2 };

export interface MossGl {
  /** Sets the backing store size in device pixels. Clears the canvas if it changes. */
  resize(width: number, height: number): void;
  /** Clears the canvas, then draws every instance in one call. */
  draw(instances: readonly MossInstance[]): void;
  clear(): void;
}

export function createMossGl(canvas: HTMLCanvasElement): MossGl | null {
  const gl = canvas.getContext("webgl2", {
    alpha: true,
    premultipliedAlpha: true,
    antialias: false,
    preserveDrawingBuffer: false,
    powerPreference: "low-power",
  });
  if (gl === null) {
    console.error("moss renderer: WebGL2 is not available");
    return null;
  }
  const program = linkProgram(gl);
  if (program === null) {
    return null;
  }
  gl.useProgram(program);
  gl.disable(gl.BLEND);
  gl.clearColor(0, 0, 0, 0);
  const resolution = gl.getUniformLocation(program, "u_resolution");
  gl.uniform2f(resolution, canvas.width, canvas.height);

  const buffer = gl.createBuffer();
  gl.bindVertexArray(gl.createVertexArray());
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  const stride = FLOATS_PER_INSTANCE * BYTES_PER_FLOAT;
  gl.enableVertexAttribArray(BOX_LOCATION);
  gl.vertexAttribPointer(BOX_LOCATION, 4, gl.FLOAT, false, stride, 0);
  gl.vertexAttribDivisor(BOX_LOCATION, 1);
  gl.enableVertexAttribArray(MOSS_LOCATION);
  gl.vertexAttribPointer(MOSS_LOCATION, 3, gl.FLOAT, false, stride, 4 * BYTES_PER_FLOAT);
  gl.vertexAttribDivisor(MOSS_LOCATION, 1);
  for (let index = 0; index < MAX_SPRITES; index++) {
    const location = FIRST_SPRITE_LOCATION + index;
    const offset = (7 + index * FLOATS_PER_SPRITE) * BYTES_PER_FLOAT;
    gl.enableVertexAttribArray(location);
    gl.vertexAttribPointer(location, FLOATS_PER_SPRITE, gl.FLOAT, false, stride, offset);
    gl.vertexAttribDivisor(location, 1);
  }
  let data = new Float32Array(0);

  const clear = (): void => {
    gl.clear(gl.COLOR_BUFFER_BIT);
  };

  return {
    resize(width, height) {
      // Assigning the size reallocates the buffer and clears it, even unchanged.
      if (canvas.width === width && canvas.height === height) {
        return;
      }
      canvas.width = width;
      canvas.height = height;
      gl.viewport(0, 0, width, height);
      gl.uniform2f(resolution, width, height);
    },
    draw(instances) {
      clear();
      const length = instances.length * FLOATS_PER_INSTANCE;
      if (data.length < length) {
        data = new Float32Array(length);
      }
      data.fill(0, 0, length);
      instances.forEach(({ box, seed, overgrowth, sprites }, index) => {
        const start = index * FLOATS_PER_INSTANCE;
        data.set([box.x, box.y, box.width, box.height, ...seedOffset(seed), overgrowth], start);
        sprites.slice(0, MAX_SPRITES).forEach(({ x, y, size, kind }, slot) => {
          data.set([x, y, size, SPRITE_KINDS[kind]], start + 7 + slot * FLOATS_PER_SPRITE);
        });
      });
      gl.bufferData(gl.ARRAY_BUFFER, data.subarray(0, length), gl.DYNAMIC_DRAW);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, instances.length);
    },
    clear,
  };
}

function linkProgram(gl: WebGL2RenderingContext): WebGLProgram | null {
  const vertex = compile(gl, gl.VERTEX_SHADER, vertexSource);
  if (vertex === null) {
    return null;
  }
  const fragment = compile(gl, gl.FRAGMENT_SHADER, fragmentSource.replace(CONSTANTS_LINE, glslConstants()));
  if (fragment === null) {
    gl.deleteShader(vertex);
    return null;
  }
  const program = gl.createProgram();
  gl.attachShader(program, vertex);
  gl.attachShader(program, fragment);
  gl.linkProgram(program);
  gl.deleteShader(vertex);
  gl.deleteShader(fragment);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    if (!gl.isContextLost()) {
      console.error("moss renderer: shader link failed", gl.getProgramInfoLog(program));
    }
    gl.deleteProgram(program);
    return null;
  }
  return program;
}

/** Logs and deletes a shader that fails to compile. */
function compile(gl: WebGL2RenderingContext, type: GLenum, source: string): WebGLShader | null {
  const shader = gl.createShader(type);
  if (shader === null) {
    return null;
  }
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    if (!gl.isContextLost()) {
      const stage = type === gl.VERTEX_SHADER ? "vertex" : "fragment";
      console.error(`moss renderer: ${stage} shader failed to compile`, gl.getShaderInfoLog(shader));
    }
    gl.deleteShader(shader);
    return null;
  }
  return shader;
}
