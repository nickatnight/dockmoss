#version 300 es
precision highp float;

uniform vec4 u_box;
uniform vec2 u_seed;
uniform float u_overgrowth;

out vec4 fragColor;

// CORNER_RADIUS_PER_SIDE, the growth shape and the front's caps: model.ts
// generates them (glslConstants), and gl.ts puts them on this line.
#pragma moss_constants

// Integer hash (no sin), so the pattern is the same on every GPU.
float hash(vec2 p) {
  uvec2 q = uvec2(ivec2(floor(p)) + ivec2(32768));
  uint h = q.x * 1597334677u ^ q.y * 3812015801u;
  h = (h ^ (h >> 16u)) * 2246822519u;
  return float(h >> 8u) / 16777216.0;
}

float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  vec2 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  float a = hash(i);
  float b = hash(i + vec2(1.0, 0.0));
  float c = hash(i + vec2(0.0, 1.0));
  float d = hash(i + vec2(1.0, 1.0));
  return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

float fbm(vec2 p) {
  float sum = 0.0;
  float amplitude = 0.5;
  for (int octave = 0; octave < FBM_OCTAVES; octave++) {
    sum += amplitude * noise(p);
    p = mat2(1.6, 1.2, -1.2, 1.6) * p;
    amplitude *= 0.5;
  }
  return sum;
}

float roundedRectDistance(vec2 p, vec2 halfSize, float radius) {
  vec2 q = abs(p) - halfSize + radius;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}

float growthDistance(vec2 uv) {
  float fromBottom = uv.y * BOTTOM_DISTANCE_SCALE;
  float fromCorner = CORNER_DISTANCE_SCALE * min(length(uv - LEFT_CORNER), length(uv - RIGHT_CORNER));
  return min(fromBottom, fromCorner);
}

// Positive inside the moss, negative beyond its front. The front's noise is
// scaled down at low reach, so the first step of overgrowth is a thin fringe.
float mossFront(vec2 uv, vec2 p) {
  float reach = MAX_REACH * pow(u_overgrowth, REACH_EXPONENT);
  float ragged = fbm(p * 5.0) - 0.5;
  float tufts = noise(p * 26.0) - 0.5;
  return reach - growthDistance(uv) + ragged * min(RAGGED_CAP, reach * RAGGED_PER_REACH) +
    tufts * min(TUFT_CAP, reach * TUFT_PER_REACH);
}

float mossCover(float front, float grain) {
  float cover = smoothstep(0.0, 0.03, front);
  float nearFront = 1.0 - smoothstep(0.0, FRONT_FUZZ_WIDTH, abs(front));
  return clamp(cover + nearFront * (grain - 0.55) * 0.9, 0.0, 1.0);
}

// Cushions with dark crevices between them, lit from above.
vec3 mossColour(vec2 p, float grain) {
  float clump = fbm(p * 13.0 + 7.3);
  float cushion = smoothstep(0.3, 0.62, clump);
  float lift = clamp((fbm(p * 13.0 + 7.3 - vec2(0.0, 0.025)) - clump) * -14.0, -1.0, 1.0);

  vec3 crevice = vec3(0.06, 0.09, 0.03);
  vec3 body = mix(vec3(0.25, 0.42, 0.09), vec3(0.18, 0.38, 0.14), fbm(p * 2.0));
  vec3 lit = vec3(0.64, 0.76, 0.24);
  vec3 colour = mix(crevice, body, cushion);
  colour = mix(colour, lit, clamp(lift * 1.4 + cushion * 0.25 - 0.1, 0.0, 1.0) * cushion);
  colour *= mix(0.78, 1.12, grain);
  return colour;
}

float contactShadow(float front, float cover) {
  return 0.4 * smoothstep(-0.06, 0.0, front) * (1.0 - cover);
}

void main() {
  vec2 local = gl_FragCoord.xy - u_box.xy;
  vec2 size = u_box.zw;
  float side = min(size.x, size.y);

  float edge = roundedRectDistance(local - size * 0.5, size * 0.5, CORNER_RADIUS_PER_SIDE * side);
  float clip = clamp(0.5 - edge, 0.0, 1.0);
  if (clip <= 0.0 || u_overgrowth <= 0.0) {
    fragColor = vec4(0.0);
    return;
  }

  vec2 uv = local / size;
  vec2 p = uv + u_seed;
  float front = mossFront(uv, p);
  float grain = noise(p * 95.0);
  float cover = mossCover(front, grain);
  vec3 colour = mossColour(p, grain);
  float shadow = contactShadow(front, cover);

  float alpha = (cover + shadow) * clip;
  fragColor = vec4(colour * cover * clip, alpha);
}
