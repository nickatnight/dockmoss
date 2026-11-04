#version 300 es
precision highp float;

uniform vec4 u_box;
uniform vec2 u_seed;
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

float roundedRect(vec2 p, vec2 halfSize, float radius) {
  vec2 q = abs(p) - halfSize + radius;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}

void main() {
  vec2 local = gl_FragCoord.xy - u_box.xy;
  vec2 size = u_box.zw;
  float side = min(size.x, size.y);

  float edge = roundedRect(local - size * 0.5, size * 0.5, 0.225 * side);
  float clip = clamp(0.5 - edge, 0.0, 1.0);
  if (clip <= 0.0 || u_overgrowth <= 0.0) {
    fragColor = vec4(0.0);
    return;
  }

  vec2 uv = local / size;
  vec2 p = uv + u_seed;

  float fromBottom = uv.y * 1.7;
  float fromCorner = 1.25 * min(length(uv - vec2(0.066)), length(uv - vec2(0.934, 0.066)));
  float grown = min(fromBottom, fromCorner);
  float reach = 0.78 * pow(u_overgrowth, 0.8);

  float ragged = fbm(p * 5.0) - 0.5;
  float tufts = noise(p * 26.0) - 0.5;
  float front = reach - grown + ragged * min(0.24, reach * 1.2) + tufts * min(0.05, reach * 0.3);
  float cover = smoothstep(0.0, 0.03, front);

  float clump = fbm(p * 13.0 + 7.3);
  float cushion = smoothstep(0.3, 0.62, clump);
  float lift = clamp((fbm(p * 13.0 + 7.3 - vec2(0.0, 0.025)) - clump) * -14.0, -1.0, 1.0);
  float grain = noise(p * 95.0);

  vec3 crevice = vec3(0.06, 0.09, 0.03);
  vec3 body = mix(vec3(0.25, 0.42, 0.09), vec3(0.18, 0.38, 0.14), fbm(p * 2.0));
  vec3 lit = vec3(0.64, 0.76, 0.24);
  vec3 colour = mix(crevice, body, cushion);
  colour = mix(colour, lit, clamp(lift * 1.4 + cushion * 0.25 - 0.1, 0.0, 1.0) * cushion);
  colour *= mix(0.78, 1.12, grain);

  float nearFront = 1.0 - smoothstep(0.0, 0.04, abs(front));
  cover = clamp(cover + nearFront * (grain - 0.55) * 0.9, 0.0, 1.0);

  float shadow = 0.4 * smoothstep(-0.06, 0.0, front) * (1.0 - cover);

  float alpha = (cover + shadow) * clip;
  fragColor = vec4(colour * cover * clip, alpha);
}
