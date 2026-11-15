// Forward pass (placeholder for the ink renderer): white surfaces shaded by baked AO and a
// simple N.L term.

struct Globals {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    // xyz: direction towards the light, normalised.
    light_dir: vec4<f32>,
    // x: 1.0 if the colour target is sRGB (shader values are display values, so convert).
    params: vec4<f32>,
};

// Mirrors mega_gen::Stroke: 17 scalar fields, 68 byte stride. Not drawn yet.
struct Stroke {
    a: array<f32, 3>,
    b: array<f32, 3>,
    n0: array<f32, 3>,
    n1: array<f32, 3>,
    kind: u32,
    rank: u32,
    weight: f32,
    seed: u32,
    instance: u32,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> strokes: array<Stroke>;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) hatch_axis: vec3<f32>,
    @location(3) id_fine: u32,
    @location(4) id_coarse: u32,
    @location(5) ao: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) ao: f32,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = globals.view_proj * vec4<f32>(in.position, 1.0);
    out.normal = in.normal;
    out.ao = in.ao;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let ndl = max(dot(n, globals.light_dir.xyz), 0.0);
    var tone = in.ao * (0.30 + 0.70 * ndl);
    if (globals.params.x > 0.5) {
        tone = pow(tone, 2.2);
    }
    return vec4<f32>(vec3<f32>(tone), 1.0);
}
