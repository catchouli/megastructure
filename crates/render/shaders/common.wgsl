// Shared by every ink pass. Prepended to each pass's source by the renderer.

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    // xyz camera position, w near plane distance.
    camera_pos: vec4<f32>,
    // xyz camera forward.
    camera_fwd: vec4<f32>,
    // w = 1: xyz is the sun's world position. w = 0: xyz is the unit direction towards it.
    sun: vec4<f32>,
    // x radius, y intensity (0 = no sun).
    sun_params: vec4<f32>,
    // width, height, 1/width, 1/height.
    screen: vec4<f32>,
    // x pixel scale (height / 720), y radians per pixel at the centre, z shadow filter
    // (diagnostic: 0 smooth, 1 bilinear, 2 hard).
    misc: vec4<f32>,
    // x debug view, y 1 if the output target is sRGB, z hatch LOD mode,
    // w stroke visibility mode (both diagnostic).
    flags: vec4<u32>,
};

// Mirrors StyleGpu in src/style.rs.
struct Style {
    paper: vec4<f32>,           // rgb, a: grain
    ink: vec4<f32>,             // rgb
    fog_colour: vec4<f32>,      // rgb
    fog: vec4<f32>,             // start, density, max, darkening by (1 - ao)
    light: vec4<f32>,           // sun strength, ambient, band jitter, band jitter length
    bands: vec4<f32>,           // hatch below, cross below, black below, sun occlusion by ao
    cross: vec4<f32>,           // crosshatch pitch scale, width scale, angle (rad), angle jitter (rad)
    hatch: vec4<f32>,           // spacing (m), target pitch (px), line width (px), width jitter
    hatch2: vec4<f32>,          // break length (m), gap fraction, max octave, width gain
    stroke: vec4<f32>,          // width px, min px, depth ref, depth exponent
    stroke2: vec4<f32>,         // taper px, taper min, noise amp, noise length
    stroke3: vec4<f32>,         // overshoot px, depth tolerance, falloff max
    rank_fade_start: vec4<f32>,
    rank_fade_end: vec4<f32>,
    rank_min_len: vec4<f32>,
    contour: vec4<f32>,         // fine near, fine far, depth k, N.V min
    contour2: vec4<f32>,        // strength, fade start, fade end, fine-id strength
    shadow: vec4<f32>,          // ambient in shadow, light min, light max, attached
    shadow2: vec4<f32>,         // edge jitter (m), jitter length (m), indication, indication radius (m)
    tone: vec4<f32>,            // tone noise, its length (m), hatch density gain, sky tone (< 0: fog)
    blacks: vec4<f32>,          // pool ao, pool width, underside black, white lines
    speckle: vec4<f32>,         // density, radius px, cell px, crosshatch angle per panel
    fog2: vec4<f32>,            // fog darkening over solid black, sun shadow, white line max rank
    white: vec4<f32>,           // white hatch strength, its pitch scale, panel tone, cross break gap
    strokes: vec4<f32>,         // hatch patch amount, patch width (lines), patch length, taper px
    feather: vec4<f32>,         // hatch feather (spacings), end jitter, shadow feather (m), shadow hatch dir
    feather2: vec4<f32>,        // shadow break gap, taper length in widths, max line width px
    rule: vec4<f32>,            // tone model, rule lod, rule pitch (m), rule target px
    rule2: vec4<f32>,           // rule density, min px, profile, pressure
    rule3: vec4<f32>,           // stroke variation, handover px, shadow edge line px, shadow hatch near edge
    fall: vec4<f32>,            // fall white, fall black, fall gamma, dark ao exponent
    dark: vec4<f32>,            // dark distance start (m), dark distance length (m), wash speckle, rule resolution exponent
    wash: vec4<f32>,            // wash, wash white, wash black, wash gamma
    wash2: vec4<f32>,           // wash max, screentone, screentone kind, screentone anchor
    wash3: vec4<f32>,           // screentone px, paper texture, paper texture px, dark down length (m)
    far: vec4<f32>,             // rule near (m), rule fade-in (m), rule down gain, rule distance darkening (m)
    bounce: vec4<f32>,          // sun bounce strength, sky bounce strength, anisotropic grain
    grain: vec4<f32>,           // screentone softness, fine detail weight
    halftone: vec4<f32>,        // halftone amount, finest pitch (m), dot edge softness, screen angle (rad)
    paper2: vec4<f32>,          // paper tooth, tooth scale px
    paper3: vec4<f32>,          // wash taking to the tooth, shadow softness, max penumbra (m), paper anchor
    paper4: vec4<f32>,          // finest surface paper cell (m), min penumbra (m), max halftone pitch (m)
    paper5: vec4<f32>,          // largest surface paper texel (m), paper kind (texture array layer)
    paper6: vec4<f32>,          // paper near distance (m), paper near amplitude, pencil strength, pencil kind
    paper7: vec4<f32>,          // pencil from (wash), pencil to (wash), pencil cross, pencil angle (rad)
    paper8: vec4<f32>,          // pencil patches amount, pencil patch scale (m)
    paper9: vec4<f32>,          // distance shading mode (0 ruling, 1 wash and pencil), far pencil strength, far pencil px per texel, far stroke stretch
    paper10: vec4<f32>,         // far pencil second direction, far tone into the wash
};

@group(0) @binding(0) var<uniform> G: Globals;
@group(0) @binding(1) var<uniform> S: Style;

const VIEW_FINAL: u32 = 0u;
const VIEW_ID: u32 = 1u;
const VIEW_NORMALS: u32 = 2u;
const VIEW_DEPTH: u32 = 3u;
const VIEW_BANDS: u32 = 4u;
const VIEW_HATCHING: u32 = 5u;
const VIEW_STROKES: u32 = 6u;
const VIEW_CONTOURS: u32 = 7u;
const VIEW_LINES: u32 = 8u;
const VIEW_SHADOW: u32 = 9u;

fn pcg(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

fn hash2(a: u32, b: u32) -> u32 {
    return pcg(a ^ pcg(b));
}

fn u01(h: u32) -> f32 {
    return f32(h >> 8u) * (1.0 / 16777216.0);
}

// 1D value noise in 0..1 along x, one independent curve per key.
fn vnoise(key: u32, x: f32) -> f32 {
    let i = floor(x);
    let f = x - i;
    let ii = bitcast<u32>(i32(i));
    let a = u01(hash2(key, ii));
    let b = u01(hash2(key, ii + 1u));
    return mix(a, b, f * f * (3.0 - 2.0 * f));
}

fn hash3(key: u32, c: vec3<i32>) -> u32 {
    return pcg(key ^ pcg(bitcast<u32>(c.x) ^ pcg(bitcast<u32>(c.y) ^ pcg(bitcast<u32>(c.z)))));
}

// 3D value noise in 0..1, smooth (C1) in x, one independent field per key.
fn vnoise3(key: u32, x: vec3<f32>) -> f32 {
    let i = vec3<i32>(floor(x));
    let f = x - floor(x);
    let w = f * f * (3.0 - 2.0 * f);
    let a = mix(u01(hash3(key, i)), u01(hash3(key, i + vec3<i32>(1, 0, 0))), w.x);
    let b = mix(u01(hash3(key, i + vec3<i32>(0, 1, 0))), u01(hash3(key, i + vec3<i32>(1, 1, 0))), w.x);
    let c = mix(u01(hash3(key, i + vec3<i32>(0, 0, 1))), u01(hash3(key, i + vec3<i32>(1, 0, 1))), w.x);
    let d = mix(u01(hash3(key, i + vec3<i32>(0, 1, 1))), u01(hash3(key, i + vec3<i32>(1, 1, 1))), w.x);
    return mix(mix(a, b, w.y), mix(c, d, w.y), w.z);
}

// 2D value noise in 0..1, one independent field per key.
fn vnoise2(key: u32, x: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(x));
    let f = x - floor(x);
    let w = f * f * (3.0 - 2.0 * f);
    let k0 = hash2(key, bitcast<u32>(i.y));
    let k1 = hash2(key, bitcast<u32>(i.y + 1));
    let a = mix(u01(hash2(k0, bitcast<u32>(i.x))), u01(hash2(k0, bitcast<u32>(i.x + 1))), w.x);
    let b = mix(u01(hash2(k1, bitcast<u32>(i.x))), u01(hash2(k1, bitcast<u32>(i.x + 1))), w.x);
    return mix(a, b, w.y);
}

// Length of the overlap of a one-pixel box centred at distance d from a line's centre with a
// line of width w: exact box-filtered coverage across the line.
fn box_cov(d: f32, w: f32) -> f32 {
    return max(0.0, min(d + 0.5, 0.5 * w) - max(d - 0.5, -0.5 * w));
}

// Integral of a unit-area tent of radius 1 from -infinity to x.
fn tent_cdf(x: f32) -> f32 {
    let c = clamp(x, -1.0, 1.0);
    return select(1.0 - 0.5 * (1.0 - c) * (1.0 - c), 0.5 * (1.0 + c) * (1.0 + c), c < 0.0);
}

// Coverage of a line of width w at distance d, filtered with a tent of radius 1 pixel
// instead of a box. Smoother than box_cov, so thin diagonal lines do not bead as they move.
fn tent_cov(d: f32, w: f32) -> f32 {
    return tent_cdf(d + 0.5 * w) - tent_cdf(d - 0.5 * w);
}

// Reversed-Z, infinite far plane: device depth d = near / z_view.
fn linear_depth(d: f32) -> f32 {
    return G.camera_pos.w / max(d, 1e-30);
}

fn pixel_ray(px: vec2<f32>) -> vec3<f32> {
    let ndc = vec2<f32>(px.x * G.screen.z * 2.0 - 1.0, 1.0 - px.y * G.screen.w * 2.0);
    let p = G.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    return normalize(p.xyz / p.w - G.camera_pos.xyz);
}

fn oct_wrap(v: vec2<f32>) -> vec2<f32> {
    return (1.0 - abs(v.yx)) * select(vec2<f32>(-1.0), vec2<f32>(1.0), v >= vec2<f32>(0.0));
}

fn oct_encode(n: vec3<f32>) -> vec2<f32> {
    let p = n.xy / (abs(n.x) + abs(n.y) + abs(n.z));
    return select(oct_wrap(p), p, n.z >= 0.0);
}

fn oct_decode(e: vec2<f32>) -> vec3<f32> {
    var n = vec3<f32>(e, 1.0 - abs(e.x) - abs(e.y));
    let t = max(-n.z, 0.0);
    n.x += select(t, -t, n.x >= 0.0);
    n.y += select(t, -t, n.y >= 0.0);
    return normalize(n);
}

// Fullscreen triangle.
@vertex
fn vs_fullscreen(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}
