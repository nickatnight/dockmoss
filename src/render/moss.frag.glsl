#version 300 es
precision highp float;

// The icon's box in canvas device pixels: x, y from the bottom-left, width, height.
uniform vec4 u_box;
// Per-item noise offset, from the item's FNV-1a seed.
uniform vec2 u_seed;
// 0 to 1, already quantized.
uniform float u_overgrowth;

out vec4 fragColor;

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
  for (int octave = 0; octave < 5; octave++) {
    sum += amplitude * noise(p);
    p = mat2(1.6, 1.2, -1.2, 1.6) * p;
    amplitude *= 0.5;
  }
  return sum;
}

// Signed distance to a rounded rect centred on the origin.
float roundedRect(vec2 p, vec2 halfSize, float radius) {
  vec2 q = abs(p) - halfSize + radius;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}

void main() {
  vec2 local = gl_FragCoord.xy - u_box.xy;
  vec2 size = u_box.zw;
  float side = min(size.x, size.y);

  // Clip to the icon's rounded rect, with a one-device-pixel antialiased edge.
  float edge = roundedRect(local - size * 0.5, size * 0.5, 0.225 * side);
  float clip = clamp(0.5 - edge, 0.0, 1.0);
  if (clip <= 0.0 || u_overgrowth <= 0.0) {
    fragColor = vec4(0.0);
    return;
  }

  // Icon space: 0 to 1 across, 0 at the bottom edge.
  vec2 uv = local / size;
  vec2 p = uv + u_seed;

  // Growth starts at the bottom edge and at the two bottom corners (where the
  // corner arcs sit), and spreads upward and inward as overgrowth rises.
  float fromBottom = uv.y * 1.45;
  float fromCorner = min(length(uv - vec2(0.066)), length(uv - vec2(0.934, 0.066)));
  float grown = min(fromBottom, fromCorner);
  float reach = 0.7 * u_overgrowth;

  float ragged = fbm(p * 5.0) - 0.5;
  float front = reach - grown + ragged * min(0.24, reach * 1.2);
  float cover = smoothstep(0.0, 0.05, front);

  // Clumps with darker gaps between them, and fine fuzz near the front.
  float clump = fbm(p * 16.0 + 7.3);
  float fine = noise(p * 70.0);
  float gap = smoothstep(0.32, 0.5, clump);
  float nearFront = 1.0 - smoothstep(0.0, 0.05, abs(front));
  float fuzz = nearFront * (fine - 0.5);
  cover = clamp(cover * mix(0.7, 1.0, gap) + fuzz * 0.6, 0.0, 1.0);

  // Light from above: the upper side of each clump catches it.
  float lift = fbm(p * 16.0 + 7.3 + vec2(0.0, 0.02)) - clump;
  vec3 deep = vec3(0.07, 0.14, 0.04);
  vec3 mid = vec3(0.22, 0.38, 0.09);
  vec3 light = vec3(0.50, 0.64, 0.18);
  vec3 colour = mix(deep, mid, gap);
  colour = mix(colour, light, clamp(clump * 1.2 - 0.35 + lift * 6.0, 0.0, 1.0) * gap);
  colour *= mix(0.85, 1.1, fine);

  // A soft contact shadow on the icon just beyond the moss.
  float shadow = 0.28 * smoothstep(-0.07, 0.0, front) * (1.0 - cover);

  float alpha = (cover + shadow) * clip;
  fragColor = vec4(colour * cover * clip, alpha);
}
