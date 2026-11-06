#version 300 es
precision highp float;
precision highp sampler2DArray;

#pragma moss_constants

uniform sampler2DArray u_atlas;

flat in vec4 v_box;
flat in float v_layer;
flat in float v_wither;
flat in float v_remaining;

out vec4 fragColor;

const vec3 REC709_LUMA = vec3(0.2126, 0.7152, 0.0722);

void main() {
  vec2 uv = (gl_FragCoord.xy - v_box.xy) / v_box.zw;
  // A bake's rows run down from its top, GL's up from the bottom.
  vec4 moss = texture(u_atlas, vec3(uv.x, 1.0 - uv.y, v_layer), MIP_LOD_BIAS);

  float front = v_remaining * (1.0 + SHRIVEL_FRONT_WIDTH);
  moss *= 1.0 - smoothstep(front - SHRIVEL_FRONT_WIDTH, front, uv.y);
  moss.rgb = mix(moss.rgb, WITHER_TINT * dot(moss.rgb, REC709_LUMA), v_wither);
  fragColor = moss;
}
