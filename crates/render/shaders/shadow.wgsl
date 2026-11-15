// Sun shadow map: depth only, one cascade per pass. Standalone (common.wgsl is not
// prepended). Both sides of every face are drawn, because most of the generated geometry
// is single-sided (the inner faces of walls) and must still block the sun.

@group(0) @binding(0) var<uniform> light_view_proj: mat4x4<f32>;

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return light_view_proj * vec4<f32>(position, 1.0);
}
