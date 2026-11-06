#version 300 es

in vec4 a_box;
in float a_layer;
in float a_wither;
in float a_remaining;

uniform vec2 u_resolution;

flat out vec4 v_box;
flat out float v_layer;
flat out float v_wither;
flat out float v_remaining;

void main() {
  vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
  vec2 pixel = a_box.xy + corner * a_box.zw;
  gl_Position = vec4(pixel / u_resolution * 2.0 - 1.0, 0.0, 1.0);
  v_box = a_box;
  v_layer = a_layer;
  v_wither = a_wither;
  v_remaining = a_remaining;
}
