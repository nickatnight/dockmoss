import { type DeviceBox, seedOffset } from "./model";
import fragmentSource from "./moss.frag.glsl?raw";
import vertexSource from "./moss.vert.glsl?raw";

// Keeps the clip's antialiased edge inside the scissor box.
const SCISSOR_MARGIN_PX = 1;

export interface MossGl {
  /** Sets the backing store size in device pixels. Clears the canvas. */
  resize(width: number, height: number): void;
  setBox(box: DeviceBox): void;
  setMoss(seed: number, overgrowth: number): void;
  draw(): void;
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
      // Assigning the size reallocates the buffer and clears it, even unchanged.
      if (canvas.width === width && canvas.height === height) {
        return;
      }
      canvas.width = width;
      canvas.height = height;
      gl.viewport(0, 0, width, height);
    },
    setBox(box) {
      gl.uniform4f(uniforms.box, box.x, box.y, box.width, box.height);
      const x = Math.floor(box.x) - SCISSOR_MARGIN_PX;
      const y = Math.floor(box.y) - SCISSOR_MARGIN_PX;
      const right = Math.ceil(box.x + box.width) + SCISSOR_MARGIN_PX;
      const top = Math.ceil(box.y + box.height) + SCISSOR_MARGIN_PX;
      scissor = [x, y, right - x, top - y];
    },
    setMoss(seed, value) {
      gl.uniform2f(uniforms.seed, ...seedOffset(seed));
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
  if (vertex === null) {
    return null;
  }
  const fragment = compile(gl, gl.FRAGMENT_SHADER, fragmentSource);
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
