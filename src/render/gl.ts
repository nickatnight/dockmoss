import { INSTANCE_ATTRIBUTES, type MossInstance, glslConstants } from "./model";
import fragmentSource from "./moss.frag.glsl?raw";
import vertexSource from "./moss.vert.glsl?raw";

const CONSTANTS_LINE = "#pragma moss_constants";

const FLOATS_PER_INSTANCE = INSTANCE_ATTRIBUTES.reduce((sum, { size }) => sum + size, 0);
const BYTES_PER_FLOAT = 4;

export interface MossGl {
  /** Sets the backing store size in device pixels. Clears the canvas if it changes. */
  resize(width: number, height: number): void;
  /** Clears the canvas, then draws every instance in one call, swayed to `swaySeconds`. */
  draw(instances: readonly MossInstance[], swaySeconds: number): void;
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
  const swaySecondsUniform = gl.getUniformLocation(program, "u_sway_seconds");
  gl.uniform2f(resolution, canvas.width, canvas.height);

  const buffer = gl.createBuffer();
  gl.bindVertexArray(gl.createVertexArray());
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  const stride = FLOATS_PER_INSTANCE * BYTES_PER_FLOAT;
  let offset = 0;
  for (const { name, size } of INSTANCE_ATTRIBUTES) {
    const location = gl.getAttribLocation(program, name);
    if (location >= 0) {
      gl.enableVertexAttribArray(location);
      gl.vertexAttribPointer(location, size, gl.FLOAT, false, stride, offset * BYTES_PER_FLOAT);
      gl.vertexAttribDivisor(location, 1);
    }
    offset += size;
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
    draw(instances, swaySeconds) {
      clear();
      gl.uniform1f(swaySecondsUniform, swaySeconds);
      const length = instances.length * FLOATS_PER_INSTANCE;
      if (data.length < length) {
        data = new Float32Array(length);
      }
      let offset = 0;
      for (const instance of instances) {
        for (const { size, values } of INSTANCE_ATTRIBUTES) {
          data.set(values(instance), offset);
          offset += size;
        }
      }
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
