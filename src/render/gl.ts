// The WebGL2 side of the renderer: one program, one quad, three uniforms.

import type { DeviceBox } from "./model";
import fragmentSource from "./moss.frag.glsl?raw";
import vertexSource from "./moss.vert.glsl?raw";

export interface MossGl {
  /** Sets the backing store size in device pixels. Clears the canvas. */
  resize(width: number, height: number): void;
  setBox(box: DeviceBox): void;
  setMoss(seed: number, overgrowth: number): void;
  /** Draws the moss on a cleared canvas. */
  draw(): void;
  clear(): void;
}

/** Null when WebGL2 is unavailable or the shaders fail; the reason is logged once. */
export function createMossGl(canvas: HTMLCanvasElement): MossGl | null {
  const gl = canvas.getContext("webgl2", {
    alpha: true,
    premultipliedAlpha: true,
    antialias: false,
    preserveDrawingBuffer: false,
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
  const uniforms = {
    box: gl.getUniformLocation(program, "u_box"),
    seed: gl.getUniformLocation(program, "u_seed"),
    overgrowth: gl.getUniformLocation(program, "u_overgrowth"),
  };
  let scissor: [number, number, number, number] = [0, 0, 0, 0];
  let overgrowth = 0;

  const clear = (): void => {
    gl.disable(gl.SCISSOR_TEST);
    gl.clear(gl.COLOR_BUFFER_BIT);
  };

  return {
    resize(width, height) {
      canvas.width = width;
      canvas.height = height;
      gl.viewport(0, 0, width, height);
    },
    setBox(box) {
      gl.uniform4f(uniforms.box, box.x, box.y, box.width, box.height);
      // One pixel of margin keeps the antialiased edge inside the scissor.
      const x = Math.floor(box.x) - 1;
      const y = Math.floor(box.y) - 1;
      scissor = [x, y, Math.ceil(box.x + box.width) + 1 - x, Math.ceil(box.y + box.height) + 1 - y];
    },
    setMoss(seed, value) {
      // Two offsets from the seed's halves move each item to its own patch of noise.
      gl.uniform2f(uniforms.seed, ((seed & 0xffff) / 0x10000) * 64, ((seed >>> 16) / 0x10000) * 64);
      gl.uniform1f(uniforms.overgrowth, value);
      overgrowth = value;
    },
    draw() {
      clear();
      if (overgrowth <= 0) {
        return;
      }
      gl.enable(gl.SCISSOR_TEST);
      gl.scissor(...scissor);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    },
    clear,
  };
}

function linkProgram(gl: WebGL2RenderingContext): WebGLProgram | null {
  const vertex = compile(gl, gl.VERTEX_SHADER, vertexSource);
  const fragment = compile(gl, gl.FRAGMENT_SHADER, fragmentSource);
  const program = gl.createProgram();
  if (vertex === null || fragment === null) {
    return null;
  }
  gl.attachShader(program, vertex);
  gl.attachShader(program, fragment);
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    if (!gl.isContextLost()) {
      console.error("moss renderer: shader link failed", gl.getProgramInfoLog(program));
    }
    return null;
  }
  return program;
}

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
    return null;
  }
  return shader;
}
