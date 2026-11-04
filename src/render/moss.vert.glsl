#version 300 es

// One instance per mossy icon: its art box in canvas device pixels (origin
// bottom-left), then its seed offset and overgrowth.
layout(location = 0) in vec4 a_box;
layout(location = 1) in vec3 a_moss;

uniform vec2 u_resolution;

flat out vec4 v_box;
flat out vec2 v_seed;
flat out float v_overgrowth;

// Keeps the clip's antialiased edge inside the quad.
const float MARGIN_PX = 1.0;

void main() {
  vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
  vec2 pixel = a_box.xy - MARGIN_PX + corner * (a_box.zw + 2.0 * MARGIN_PX);
  gl_Position = vec4(pixel / u_resolution * 2.0 - 1.0, 0.0, 1.0);
  v_box = a_box;
  v_seed = a_moss.xy;
  v_overgrowth = a_moss.z;
}
