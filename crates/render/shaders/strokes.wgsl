// Stroke pass: one screen-space quad per visible stroke, pulled from storage buffers.
// Visibility is tested against the scene depth in the fragment shader. Output is max-blended
// into the ink target. common.wgsl is prepended.

// Mirrors mega_gen::Stroke: 17 scalar fields, 68-byte stride.
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

@group(1) @binding(0) var<storage, read> strokes: array<Stroke>;
// Indices into `strokes` of the strokes to draw this frame (CPU-culled for now; a compute
// cull can write the same list plus indirect arguments later).
@group(1) @binding(1) var<storage, read> visible: array<u32>;
@group(1) @binding(2) var depth_tex: texture_depth_2d;

const STROKE_WHITE: u32 = 4u;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    // Screen-space endpoints in pixels after near-plane clipping.
    @location(0) @interpolate(flat) p0: vec2<f32>,
    @location(1) @interpolate(flat) p1: vec2<f32>,
    // Clip w (view depth) at p0 and p1.
    @location(2) @interpolate(flat) w01: vec2<f32>,
    // Stroke parameter (0 at a, 1 at b) at p0 and p1.
    @location(3) @interpolate(flat) t01: vec2<f32>,
    // x world length, y weight, z alpha from projected length, w real-end bits.
    @location(4) @interpolate(flat) info: vec4<f32>,
    // x seed, y rank, z kind.
    @location(5) @interpolate(flat) ids: vec4<u32>,
};

fn to_px(c: vec4<f32>) -> vec2<f32> {
    let ndc = c.xy / c.w;
    return vec2<f32>((ndc.x * 0.5 + 0.5) * G.screen.x, (0.5 - 0.5 * ndc.y) * G.screen.y);
}

// Stroke width in pixels before taper and noise.
fn width_px(weight: f32, z: f32) -> f32 {
    let falloff = min(pow(S.stroke.z / max(z, 1e-3), S.stroke.w), S.stroke3.z);
    return weight * S.stroke.x * G.misc.x * falloff;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VOut {
    var o: VOut;
    o.clip = vec4<f32>(2.0, 2.0, 0.0, 1.0);
    let s = strokes[visible[ii]];
    let a = vec3<f32>(s.a[0], s.a[1], s.a[2]);
    let b = vec3<f32>(s.b[0], s.b[1], s.b[2]);
    var ca = G.view_proj * vec4<f32>(a, 1.0);
    var cb = G.view_proj * vec4<f32>(b, 1.0);
    let wn = G.camera_pos.w * 1.01;
    if (ca.w < wn && cb.w < wn) {
        return o;
    }
    // Full projected length decides the length fade, before clipping.
    var len_full = 1e6;
    if (ca.w >= wn && cb.w >= wn) {
        len_full = length(to_px(cb) - to_px(ca));
    }
    var t0 = 0.0;
    var t1 = 1.0;
    var real = 3u;
    if (ca.w < wn) {
        let t = (wn - ca.w) / (cb.w - ca.w);
        ca = mix(ca, cb, t);
        t0 = t;
        real = 2u;
    } else if (cb.w < wn) {
        let t = (wn - ca.w) / (cb.w - ca.w);
        cb = mix(ca, cb, t);
        t1 = t;
        real = 1u;
    }
    let p0 = to_px(ca);
    let p1 = to_px(cb);
    let d = p1 - p0;
    let len = length(d);
    let dir = select(vec2<f32>(1.0, 0.0), d / len, len > 1e-4);
    let nrm = vec2<f32>(-dir.y, dir.x);
    let wmax = max(width_px(s.weight, ca.w), width_px(s.weight, cb.w)) * (1.0 + S.stroke2.z);
    let half_w = 0.5 * max(wmax, S.stroke.y) + 1.0;
    let ext = S.stroke3.x * G.misc.x + 1.0;

    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let corner = corners[vi];
    let base = select(p0 - dir * ext, p1 + dir * ext, corner.x > 0.5);
    let q = base + nrm * half_w * corner.y;
    o.clip = vec4<f32>(q.x * G.screen.z * 2.0 - 1.0, 1.0 - q.y * G.screen.w * 2.0, 0.5, 1.0);

    let min_len = S.rank_min_len[min(s.rank, 3u)] * G.misc.x;
    o.p0 = p0;
    o.p1 = p1;
    o.w01 = vec2<f32>(ca.w, cb.w);
    o.t01 = vec2<f32>(t0, t1);
    o.info = vec4<f32>(length(b - a), s.weight, smoothstep(0.5 * min_len, min_len, len_full), f32(real));
    o.ids = vec4<u32>(s.seed, min(s.rank, 3u), s.kind, 0u);
    return o;
}

@fragment
fn fs_main(o: VOut) -> @location(0) vec2<f32> {
    let px = o.clip.xy;
    let d = o.p1 - o.p0;
    let len = length(d);
    let dir = select(vec2<f32>(1.0, 0.0), d / len, len > 1e-4);
    let rel = px - o.p0;
    let s_px = dot(rel, dir);
    let perp = abs(dot(rel, vec2<f32>(-dir.y, dir.x)));

    // Perspective-correct depth and stroke parameter at the nearest centreline point.
    let sf = clamp(s_px / max(len, 1e-4), 0.0, 1.0);
    let z = 1.0 / mix(1.0 / o.w01.x, 1.0 / o.w01.y, sf);
    let t = mix(o.t01.x / o.w01.x, o.t01.y / o.w01.y, sf) * z;

    let real = u32(o.info.w);
    let world_len = o.info.x;
    let rank = o.ids.y;

    // Width: weight, depth falloff, taper towards real ends (in pixels), seeded noise.
    let e0 = select(1e6, s_px, (real & 1u) != 0u);
    let e1 = select(1e6, len - s_px, (real & 2u) != 0u);
    let taper = mix(S.stroke2.y, 1.0, smoothstep(0.0, S.stroke2.x * G.misc.x, min(e0, e1)));
    let noise = 1.0 + S.stroke2.z * (vnoise(o.ids.x, t * world_len / S.stroke2.w) * 2.0 - 1.0);
    var w = width_px(o.info.y, z) * taper * noise;
    // Below the minimum width keep the width and lose coverage instead.
    let alpha_w = saturate(w / S.stroke.y);
    w = max(w, S.stroke.y);

    // Analytic coverage across the line and along it (ends run on by the overshoot).
    let ov = S.stroke3.x * G.misc.x;
    let lo = select(0.0, -ov, (real & 1u) != 0u);
    let hi = len + select(0.0, ov, (real & 2u) != 0u);
    let along = saturate(min(s_px + 0.5, hi) - max(s_px - 0.5, lo));
    let cov = tent_cov(perp, w) * along;
    if (cov <= 0.0) {
        discard;
    }

    // Visibility: compare with the farthest scene depth in a 3x3 block around the centreline
    // pixel, with a tolerance proportional to depth. Using the centreline (not this
    // fragment's own pixel) makes the whole width of the line agree.
    // G.flags.w selects a diagnostic variant (experiment E002): 1 = centreline pixel only,
    // 2 = this fragment's own pixel only.
    let c = select(o.p0 + dir * clamp(s_px, 0.0, len), px, G.flags.w == 2u);
    let dims = vec2<i32>(textureDimensions(depth_tex));
    let ic = vec2<i32>(floor(c));
    let r = select(1, 0, G.flags.w != 0u);
    var dfar = 1.0;
    for (var dy = -r; dy <= r; dy++) {
        for (var dx = -r; dx <= r; dx++) {
            let q = clamp(ic + vec2<i32>(dx, dy), vec2<i32>(0), dims - 1);
            dfar = min(dfar, textureLoad(depth_tex, q, 0));
        }
    }
    let zs = linear_depth(dfar);
    let tol = S.stroke3.y;
    let vis = 1.0 - smoothstep(zs * (1.0 + tol), zs * (1.0 + 2.0 * tol), z);

    // Rank fade with distance.
    let fade = 1.0 - smoothstep(S.rank_fade_start[rank], S.rank_fade_end[rank], z);

    let a = cov * alpha_w * vis * fade * o.info.z;
    if (o.ids.z == STROKE_WHITE) {
        return vec2<f32>(0.0, a);
    }
    return vec2<f32>(a, 0.0);
}
