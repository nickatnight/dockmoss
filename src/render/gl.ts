import { INSTANCE_ATTRIBUTES, type MossInstance, glslConstants } from "./model";
import fragmentSource from "./moss.frag.glsl?raw";
import vertexSource from "./moss.vert.glsl?raw";

const CONSTANTS_LINE = "#pragma moss_constants";

const FLOATS_PER_INSTANCE = INSTANCE_ATTRIBUTES.reduce((sum, { size }) => sum + size, 0);
const BYTES_PER_FLOAT = 4;

export interface MossGl<Image> {
  /** Sets the backing store size in device pixels. Clears the canvas if it changes. */
  resize(width: number, height: number): void;
  /** Makes the atlas again: `layers` cells of `side` px square, all blank. */
  allocate(layers: number, side: number): void;
  /** Puts each image in its atlas layer, then remakes the atlas's mipmaps. */
  upload(cells: readonly { layer: number; image: Image }[]): void;
  /** Clears the canvas, then draws every instance in one call. */
  draw(instances: readonly MossInstance[]): void;
  clear(): void;
}

export function createMossGl(canvas: HTMLCanvasElement): MossGl<OffscreenCanvas> | null {
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

  // The atlas: one texture array, a bake per layer, so no mip level mixes two bakes.
  gl.activeTexture(gl.TEXTURE0);
  let atlas: WebGLTexture | null = null;
  let atlasSide = 0;

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
    allocate(layers, side) {
      gl.deleteTexture(atlas);
      atlas = gl.createTexture();
      atlasSide = side;
      gl.bindTexture(gl.TEXTURE_2D_ARRAY, atlas);
      gl.texStorage3D(gl.TEXTURE_2D_ARRAY, Math.floor(Math.log2(side)) + 1, gl.RGBA8, side, side, layers);
      gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
      gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    },
    upload(cells) {
      if (cells.length === 0) {
        return;
      }
      // Premultiplied, as the canvas composites, so mipmaps do not darken the moss's edges.
      // WebGL applies this to a canvas source; it would ignore it for an ImageBitmap.
      gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true);
      for (const { layer, image } of cells) {
        gl.texSubImage3D(
          gl.TEXTURE_2D_ARRAY,
          0,
          0,
          0,
          layer,
          atlasSide,
          atlasSide,
          1,
          gl.RGBA,
          gl.UNSIGNED_BYTE,
          image,
        );
      }
      gl.generateMipmap(gl.TEXTURE_2D_ARRAY);
    },
    draw(instances) {
      clear();
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
