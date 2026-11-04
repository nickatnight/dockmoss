#version 300 es
precision highp float;

#pragma moss_constants

flat in vec4 v_box;
flat in vec2 v_seed;
flat in float v_overgrowth;
flat in float v_hue;
flat in float v_wither;
flat in vec4 v_sprites[MAX_SPRITES];

uniform float u_sway_seconds;

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

// Noise finer than half a lattice cell per device pixel aliases: it speckles at
// rest and shimmers while the Dock magnifies. Each term fades to its mean first.
const float ALIAS_FADE_START = 0.375;
const float ALIAS_FADE_END = 0.5;

float bandLimit(float cellsPerPixel) {
  return 1.0 - smoothstep(ALIAS_FADE_START, ALIAS_FADE_END, cellsPerPixel);
}

float limitedNoise(vec2 p, float cellsPerPixel) {
  return 0.5 + bandLimit(cellsPerPixel) * (noise(p) - 0.5);
}

float fbm(vec2 p, float firstOctaveCellsPerPixel) {
  float cellsPerPixel = firstOctaveCellsPerPixel;
  float sum = 0.0;
  float amplitude = 0.5;
  for (int octave = 0; octave < FBM_OCTAVES; octave++) {
    sum += amplitude * limitedNoise(p, cellsPerPixel);
    p = mat2(1.6, 1.2, -1.2, 1.6) * p;
    amplitude *= 0.5;
    cellsPerPixel *= 2.0;
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

const float TAU = 6.2831853;

// 0 at time 0, so moss that has never swayed is the static moss.
float swayWave(float period, float phase) {
  return 0.5 * (sin(TAU * u_sway_seconds / period + phase) - sin(phase));
}

// Positive inside the moss, negative beyond its front. The front's noise is
// scaled down at low reach, so the first step of overgrowth is a thin fringe.
float mossFront(vec2 uv, vec2 p, float side) {
  float reach = MAX_REACH * pow(v_overgrowth, REACH_EXPONENT);
  float ragged = fbm(p * 5.0, 5.0 / side) - 0.5;
  float tufts = limitedNoise(p * 26.0, 26.0 / side) - 0.5;
  float sway = SWAY_REACH * swayWave(MOSS_SWAY_PERIOD, uv.x * 9.0 + v_seed.x);
  return reach - growthDistance(uv) + ragged * min(RAGGED_CAP, reach * RAGGED_PER_REACH) +
    tufts * min(TUFT_CAP, reach * TUFT_PER_REACH) + sway;
}

float mossCover(float front, float grain) {
  float cover = smoothstep(0.0, 0.03, front);
  float nearFront = 1.0 - smoothstep(0.0, FRONT_FUZZ_WIDTH, abs(front));
  return clamp(cover + nearFront * (grain - 0.55) * 0.9, 0.0, 1.0);
}

const vec3 CREVICE = vec3(0.10, 0.16, 0.05);
const vec3 HUE_SHIFT = vec3(-0.05, 0.0, 0.05);

// Cushions with dark crevices between them, lit from above.
vec3 mossColour(vec2 p, float grain, float side, float hue) {
  float clump = fbm(p * 13.0 + 7.3, 13.0 / side);
  float cushion = smoothstep(0.3, 0.62, clump);
  float lift = clamp((fbm(p * 13.0 + 7.3 - vec2(0.0, 0.025), 13.0 / side) - clump) * -14.0, -1.0, 1.0);

  vec3 body = mix(vec3(0.25, 0.42, 0.09), vec3(0.18, 0.38, 0.14), fbm(p * 2.0, 2.0 / side)) + hue * HUE_SHIFT;
  vec3 lit = vec3(0.64, 0.76, 0.24) + hue * HUE_SHIFT * 1.5;
  vec3 colour = mix(CREVICE, body, cushion);
  colour = mix(colour, lit, clamp(lift * 1.4 + cushion * 0.25 - 0.1, 0.0, 1.0) * cushion);
  colour *= mix(0.78, 1.12, grain);
  return colour;
}

// Rec. 709 weights. Linear in colour, so a premultiplied colour keeps its alpha.
const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

const float CONTACT_SHADOW_STRENGTH = 0.18;

float contactShadow(float front, float cover) {
  return CONTACT_SHADOW_STRENGTH * smoothstep(-CONTACT_SHADOW_WIDTH, 0.0, front) * (1.0 - cover);
}

// Under half a cell per device pixel on tile size 45's 74 px art box, so the
// grain stays whole there and fades on smaller Docks.
const float GRAIN_FREQUENCY = 28.0;

float ellipseDistance(vec2 p, vec2 radii) {
  return (length(p / radii) - 1.0) * min(radii.x, radii.y);
}

float boxDistance(vec2 p, vec2 halfSize) {
  vec2 q = abs(p) - halfSize;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0);
}

vec4 mushroom(vec2 boxUv, float devicePixelInBox) {
  float stem = boxDistance(boxUv - vec2(0.5, 0.28), vec2(0.075, 0.26));
  float cap = max(ellipseDistance(boxUv - vec2(0.5, 0.5), vec2(0.42, 0.36)), 0.47 - boxUv.y);
  vec3 stemColour = mix(vec3(0.90, 0.85, 0.72), vec3(0.62, 0.55, 0.42), smoothstep(0.45, 0.6, boxUv.x));
  vec3 capColour = mix(vec3(0.52, 0.14, 0.07), vec3(0.86, 0.36, 0.18), smoothstep(0.5, 0.85, boxUv.y));
  float capCover = clamp(0.5 - cap / devicePixelInBox, 0.0, 1.0);
  float stemCover = clamp(0.5 - stem / devicePixelInBox, 0.0, 1.0);
  vec3 colour = mix(stemColour * stemCover, capColour, capCover);
  return vec4(colour, max(capCover, stemCover));
}

vec4 flower(vec2 boxUv, float devicePixelInBox, float tint) {
  float stem = boxDistance(boxUv - vec2(0.5, 0.3), vec2(0.04, 0.3));
  vec2 head = boxUv - vec2(0.5, 0.68);
  float angle = atan(head.y, head.x);
  float petals = length(head) - (0.21 + 0.09 * cos(5.0 * angle));
  float centre = length(head) - 0.09;
  vec3 petalColour = mix(vec3(0.97, 0.95, 0.90), vec3(0.86, 0.72, 0.94), tint);
  float stemCover = clamp(0.5 - stem / devicePixelInBox, 0.0, 1.0);
  float petalCover = clamp(0.5 - petals / devicePixelInBox, 0.0, 1.0);
  float centreCover = clamp(0.5 - centre / devicePixelInBox, 0.0, 1.0);
  vec3 colour = vec3(0.20, 0.42, 0.12) * stemCover;
  colour = mix(colour, petalColour, petalCover);
  colour = mix(colour, vec3(0.98, 0.76, 0.20), centreCover);
  return vec4(colour, max(stemCover, petalCover));
}

int spriteKind(vec4 sprite) {
  return int(sprite.w + 0.5);
}

vec4 spriteColour(vec4 sprite, vec2 uv, float devicePixelInIcon, float grain) {
  vec2 boxUv = (uv - sprite.xy) / sprite.z + 0.5;
  boxUv.x -= SPRITE_TIP_LEAN * boxUv.y * boxUv.y * swayWave(SPRITE_SWAY_PERIOD, sprite.x * 41.0);
  if (spriteKind(sprite) == NO_SPRITE || any(lessThan(boxUv, vec2(0.0))) || any(greaterThan(boxUv, vec2(1.0)))) {
    return vec4(0.0);
  }
  float devicePixelInBox = devicePixelInIcon / sprite.z;
  vec4 colour = spriteKind(sprite) == MUSHROOM ? mushroom(boxUv, devicePixelInBox) : flower(boxUv, devicePixelInBox, step(0.5, fract(sprite.x * 37.0)));
  return colour * smoothstep(0.02, 0.2, boxUv.y + (grain - 0.5) * 0.2);
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
  float front = mossFront(uv, p, side);
  float grain = limitedNoise(p * GRAIN_FREQUENCY, GRAIN_FREQUENCY / side);
  float cover = mossCover(front, grain);
  vec3 colour = mossColour(p, grain, side, v_hue);
  float shadow = contactShadow(front, cover);

  vec4 moss = vec4(colour * cover + CREVICE * shadow, cover + shadow);
  if (spriteKind(v_sprites[0]) != NO_SPRITE) {
    float devicePixelInIcon = 1.0 / side;
    for (int index = 0; index < MAX_SPRITES; index++) {
      vec4 drawn = spriteColour(v_sprites[index], uv, devicePixelInIcon, grain);
      moss = drawn + moss * (1.0 - drawn.a);
    }
  }
  // A shrivel browns the moss and its sprites: their own luminance in a dry brown.
  moss.rgb = mix(moss.rgb, WITHER_TINT * dot(moss.rgb, LUMA), v_wither);
  fragColor = moss * clip;
}
