#version 300 es
precision highp float;

flat in vec4 v_box;
flat in vec2 v_seed;
flat in float v_overgrowth;
flat in vec4 v_sprites[3];

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
  float reach = MAX_REACH * pow(v_overgrowth, REACH_EXPONENT);
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

const float FLOWER = 2.0;

float ellipseDistance(vec2 p, vec2 radii) {
  return (length(p / radii) - 1.0) * min(radii.x, radii.y);
}

float boxDistance(vec2 p, vec2 halfSize) {
  vec2 q = abs(p) - halfSize;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0);
}

// A sprite's colour (premultiplied) at q, 0 to 1 across its box and 0 at its
// foot. `pixel` is one device pixel in the same units.
vec4 mushroom(vec2 q, float pixel) {
  float stem = boxDistance(q - vec2(0.5, 0.28), vec2(0.075, 0.26));
  float cap = max(ellipseDistance(q - vec2(0.5, 0.5), vec2(0.44, 0.36)), 0.47 - q.y);
  vec3 stemColour = mix(vec3(0.90, 0.85, 0.72), vec3(0.62, 0.55, 0.42), smoothstep(0.45, 0.6, q.x));
  vec3 capColour = mix(vec3(0.52, 0.14, 0.07), vec3(0.86, 0.36, 0.18), smoothstep(0.5, 0.85, q.y));
  float capCover = clamp(0.5 - cap / pixel, 0.0, 1.0);
  float stemCover = clamp(0.5 - stem / pixel, 0.0, 1.0);
  vec3 colour = mix(stemColour * stemCover, capColour, capCover);
  return vec4(colour, max(capCover, stemCover));
}

vec4 flower(vec2 q, float pixel, float tint) {
  float stem = boxDistance(q - vec2(0.5, 0.3), vec2(0.04, 0.3));
  vec2 head = q - vec2(0.5, 0.68);
  float angle = atan(head.y, head.x);
  float petals = length(head) - (0.21 + 0.09 * cos(5.0 * angle));
  float centre = length(head) - 0.09;
  vec3 petalColour = mix(vec3(0.97, 0.95, 0.90), vec3(0.86, 0.72, 0.94), tint);
  float stemCover = clamp(0.5 - stem / pixel, 0.0, 1.0);
  float petalCover = clamp(0.5 - petals / pixel, 0.0, 1.0);
  float centreCover = clamp(0.5 - centre / pixel, 0.0, 1.0);
  vec3 colour = vec3(0.20, 0.42, 0.12) * stemCover;
  colour = mix(colour, petalColour, petalCover);
  colour = mix(colour, vec3(0.98, 0.76, 0.20), centreCover);
  return vec4(colour, max(stemCover, petalCover));
}

// One sprite (x, y, size, kind) composited at uv, or nothing outside its box.
vec4 spriteColour(vec4 sprite, vec2 uv, float pixel, float grain) {
  vec2 q = (uv - sprite.xy) / sprite.z + 0.5;
  if (sprite.w < 0.5 || any(lessThan(q, vec2(0.0))) || any(greaterThan(q, vec2(1.0)))) {
    return vec4(0.0);
  }
  float spritePixel = pixel / sprite.z;
  vec4 colour = sprite.w < FLOWER - 0.5 ? mushroom(q, spritePixel) : flower(q, spritePixel, step(0.5, fract(sprite.x * 37.0)));
  // The foot sinks into the moss, unevenly.
  return colour * smoothstep(0.02, 0.2, q.y + (grain - 0.5) * 0.2);
}

void main() {
  vec2 local = gl_FragCoord.xy - v_box.xy;
  vec2 size = v_box.zw;
  float side = min(size.x, size.y);

  float edge = roundedRectDistance(local - size * 0.5, size * 0.5, CORNER_RADIUS_PER_SIDE * side);
  float clip = clamp(0.5 - edge, 0.0, 1.0);
  if (clip <= 0.0 || v_overgrowth <= 0.0) {
    fragColor = vec4(0.0);
    return;
  }

  vec2 uv = local / size;
  vec2 p = uv + v_seed;
  float front = mossFront(uv, p);
  float grain = noise(p * 95.0);
  float cover = mossCover(front, grain);
  vec3 colour = mossColour(p, grain);
  float shadow = contactShadow(front, cover);

  vec4 moss = vec4(colour * cover, cover + shadow);
  if (v_sprites[0].w > 0.5) {
    float pixel = 1.0 / side;
    for (int index = 0; index < 3; index++) {
      vec4 drawn = spriteColour(v_sprites[index], uv, pixel, grain);
      moss = drawn + moss * (1.0 - drawn.a);
    }
  }
  fragColor = moss * clip;
}
