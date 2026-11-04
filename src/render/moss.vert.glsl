#version 300 es

layout(location = 0) in vec4 a_box;
layout(location = 1) in vec4 a_moss;
layout(location = 2) in vec4 a_sprite0;
layout(location = 3) in vec4 a_sprite1;
layout(location = 4) in vec4 a_sprite2;

uniform vec2 u_resolution;

flat out vec4 v_box;
flat out vec2 v_seed;
flat out float v_overgrowth;
flat out float v_hue;
flat out vec4 v_sprites[3];

const float MARGIN_PX = 1.0;

void main() {
  vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
  vec2 pixel = a_box.xy - MARGIN_PX + corner * (a_box.zw + 2.0 * MARGIN_PX);
  gl_Position = vec4(pixel / u_resolution * 2.0 - 1.0, 0.0, 1.0);
  v_box = a_box;
  v_seed = a_moss.xy;
  v_overgrowth = a_moss.z;
  v_hue = a_moss.w;
  v_sprites[0] = a_sprite0;
  v_sprites[1] = a_sprite1;
  v_sprites[2] = a_sprite2;
}
