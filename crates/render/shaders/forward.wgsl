// Forward geometry pass. Writes ids, octahedral normal, fill (palette, band, ao, flags) and
// black ink from ruled hatching and solid black. common.wgsl is prepended.

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
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(flat) normal: vec3<f32>,
    @location(2) @interpolate(flat) axis: vec3<f32>,
    @location(3) @interpolate(flat) ids: vec2<u32>,
    @location(4) ao: f32,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = G.view_proj * vec4<f32>(in.position, 1.0);
    out.world = in.position;
    out.normal = in.normal;
    out.axis = in.hatch_axis;
    out.ids = vec2<u32>(in.id_fine, in.id_coarse);
    out.ao = in.ao;
    return out;
}

struct FsOut {
    @location(0) id: vec2<u32>,
    @location(1) normal: vec2<f32>,
    @location(2) fill: vec4<f32>,
    // R black ink, G white ink, B line ink (written by the line passes), A unused.
    @location(3) ink: vec4<f32>,
};

// Everything one hatch layer needs about the fragment. u is the spacing coordinate (metres,
// across the lines), v the coordinate along the lines; g and gv their screen derivatives in
// metres per pixel.
struct LayerIn {
    u: f32,
    v: f32,
    g: f32,
    gv: f32,
    // Derivative used to choose the octave (equal to g except in diagnostic modes).
    lod: f32,
    salt: u32,
    light: f32,
    thresh: f32,
    // Threshold of the next darker band; line weight grows towards it.
    next: f32,
    edge: f32,
    // Pitch and width multipliers for this layer.
    pitch_scale: f32,
    width_scale: f32,
    // Fraction of each line lost to breaks.
    gap: f32,
};

struct LayerOut {
    cov: f32,
    // Band-edge jitter of the nearest line, blended across octaves so it is continuous.
    jit: f32,
};

// Level of a line: how many octaves it survives. Line n (in units of the finest spacing)
// is present in octaves 0..=ctz(n).
fn line_level(n: i32) -> f32 {
    if (n == 0) {
        return 16.0;
    }
    return f32(min(countTrailingZeros(bitcast<u32>(n)), 16u));
}

fn line_key(salt: u32, n: i32) -> u32 {
    return hash2(salt, bitcast<u32>(n));
}

// The octave a layer is drawn at for this fragment: spacing s0 of octave k0, and the
// crossfade towards octave k0 + 1.
struct Oct {
    s0: f32,
    k0: f32,
    frac: f32,
};

// 1D value noise with its derivative: x value, y d/dx.
fn vnoise_d(key: u32, x: f32) -> vec2<f32> {
    let i = floor(x);
    let f = x - i;
    let ii = bitcast<u32>(i32(i));
    let a = u01(hash2(key, ii));
    let b = u01(hash2(key, ii + 1u));
    let w = f * f * (3.0 - 2.0 * f);
    return vec2<f32>(mix(a, b, w), (b - a) * 6.0 * f * (1.0 - f));
}

// Patch cells. Lines are grouped into columns S.strokes.y lines wide (at the layer's
// octave); along the lines each column is cut into runs S.strokes.z breakup lengths long,
// staggered from column to column. Every line of a run starts at the run's start (with a
// small per-line jitter) and stops at a shared end point (with a small per-line jitter),
// leaving a gap of about the layer's gap fraction before the next run: a group of
// parallel strokes laid down together, as a hand hatches. Everything is keyed to world
// positions and line identities, so it stays on the surface.
// x metres since the stroke started, y metres until it ends, z a random value per run.
fn patch_run(salt: u32, key: u32, pos: f32, v: f32, wdt: f32, len: f32, gap: f32) -> vec3<f32> {
    let ci = i32(floor(pos / wdt + 0.5));
    let hc = hash2(salt, bitcast<u32>(ci));
    let vv = v / len + u01(hc);
    let sj = floor(vv);
    let fr = vv - sj;
    let hs = hash2(hc, bitcast<u32>(i32(sj)));
    let kl = pcg(key ^ hs);
    let start = 0.06 * u01(kl);
    let end = 1.0 - gap * (0.4 + 1.2 * u01(hs)) + (u01(pcg(kl ^ 9u)) - 0.5) * 0.12;
    return vec3<f32>((fr - start) * len, (end - fr) * len, u01(pcg(hs ^ 5u)));
}

fn patch_size(o: Oct, k: f32) -> vec2<f32> {
    return vec2<f32>(o.s0 * exp2(k) * S.strokes.y, S.hatch2.x * S.strokes.z * exp2(o.k0 + k));
}

// Random offset added to the light value along line n, so band edges are made of line ends.
// With patches on, it is mostly shared by one run of one column, so a band edge is a group
// of strokes stopping together rather than a sawtooth.
fn band_jit(c: LayerIn, n: i32, pos: f32, o: Oct) -> f32 {
    let key = line_key(c.salt, n) ^ 0x51ed270bu;
    let len = S.light.w * exp2(min(line_level(n), 10.0));
    let filt = 1.0 - saturate(c.gv / len * 2.0);
    var j = vnoise(key, c.v / len + 31.7) - 0.5;
    if (S.strokes.x > 0.0) {
        // Same narrow switch between the octave layouts as the breakup.
        let wk1 = smoothstep(0.35, 0.65, o.frac);
        var r = 0.0;
        if (wk1 < 1.0) {
            let p0 = patch_size(o, 0.0);
            r += (1.0 - wk1) * patch_run(c.salt ^ 0x27d4eb2fu, key, pos, c.v, p0.x, p0.y, 0.0).z;
        }
        if (wk1 > 0.0) {
            let p1 = patch_size(o, 1.0);
            r += wk1 * patch_run(c.salt ^ 0x165667b1u, key, pos, c.v, p1.x, p1.y, 0.0).z;
        }
        j = mix(j, r - 0.5 + 0.3 * j, S.strokes.x);
    }
    return j * S.light.z * filt;
}

// Width multiplier for a stroke end d pixels away, over taper length tp.
fn end_taper(d: f32, tp: f32) -> f32 {
    return select(1.0, smoothstep(0.0, tp, d), tp > 0.0);
}

// Coverage of line n, whose centre is at u = pos.
fn line_cov(c: LayerIn, n: i32, pos: f32, o: Oct) -> f32 {
    let key = line_key(c.salt, n);
    let lvl = line_level(n);
    let dark = saturate((c.thresh - c.light) / max(c.thresh - c.next, 1e-3));
    let w = S.hatch.z * G.misc.x * c.width_scale * (1.0 + (u01(pcg(key)) - 0.5) * S.hatch.w) * (1.0 + S.hatch2.w * dark);
    let gap = c.gap;
    let tp = S.strokes.w * G.misc.x;
    let ad = abs(c.u - pos) / c.g;
    // Tone: present where the jittered light is below the band threshold. Distance in
    // pixels to where it stops, from the light's change per pixel (c.edge). A line ending
    // at a band edge tapers like a lifted pen.
    let d_tone = (c.thresh - c.light - band_jit(c, n, pos, o)) / max(c.edge, 1e-4);
    let tone = saturate(d_tone + 0.5);
    let wt = w * end_taper(d_tone, tp);
    if (S.strokes.x > 0.0) {
        // Patch runs at this octave's size, switching to the next octave's in a narrow
        // band of the octave crossfade (a wide crossfade of two different layouts would
        // leave many strokes half drawn, grey).
        let wk1 = smoothstep(0.35, 0.65, o.frac);
        var cov = 0.0;
        for (var k = 0; k < 2; k++) {
            let wgt = select(wk1, 1.0 - wk1, k == 0);
            if (wgt <= 0.0) {
                continue;
            }
            let ps = patch_size(o, f32(k));
            let r = patch_run(c.salt ^ (0x9e3779b9u * u32(k + 1)), key, pos, c.v, ps.x, ps.y, gap);
            let ds = r.x / c.gv;
            let de = r.y / c.gv;
            let min_ok = c.gv * 2.0 < ps.y;
            var cv: f32;
            if (min_ok) {
                // Blunt start, trailing end.
                let wk = wt * end_taper(ds, tp * 0.25) * end_taper(de, tp);
                cv = tent_cov(ad, wk) * saturate(ds + 0.5) * saturate(de + 0.5);
            } else {
                cv = tent_cov(ad, wt) * (1.0 - gap);
            }
            cov += cv * wgt;
        }
        return cov * tone;
    }
    // Per-line breakup: noise along v with a segment length that grows with the line's
    // level, so lines that survive to coarse octaves break up at a similar screen scale.
    let bl = S.hatch2.x * exp2(min(lvl, 10.0));
    let nl = vnoise_d(key ^ 0x9e3779b9u, c.v / bl + u01(pcg(key ^ 7u)) * 64.0);
    // Signed distance in pixels along the line to the nearest break end (first order).
    let df = nl.y / bl;
    let d_break = (nl.x - gap) / max(abs(df) * c.gv, 1e-6);
    let minified = saturate(c.gv / bl * 2.0);
    let present = mix(saturate(d_break + 0.5), 1.0 - gap, minified);
    let wb = wt * mix(end_taper(d_break, tp * select(1.0, 0.33, df > 0.0)), 1.0, minified);
    return tent_cov(ad, wb) * present * tone;
}

// One layer of ruled lines. The octave comes from the derivative of u alone, so a grazing
// floor picks the octave its lines actually need. Octave k has lines every spacing*2^k; the
// lines of octave k+1 are the even lines of octave k, so crossfading only fades the odd
// lines and nothing jumps when k crosses an integer.
fn hatch_layer(c: LayerIn) -> LayerOut {
    var out: LayerOut;
    let base = S.hatch.x;
    // Darker inside the band: denser as well as heavier (hatch_density_gain).
    let dark = saturate((c.thresh - c.light) / max(c.thresh - c.next, 1e-3));
    let tgt = S.hatch.y * G.misc.x * c.pitch_scale / (1.0 + S.tone.z * dark);
    let raw = log2(max(tgt * c.lod / base, 1e-6));
    let kmax = S.hatch2.z;
    let kf = clamp(raw, 0.0, kmax);
    let k0 = floor(kf);
    let frac = kf - k0;
    let step0 = 1 << u32(k0);
    let s0 = base * f32(step0);
    let s1 = 2.0 * s0;

    // Nearest even line (also a line of octave k0 + 1) and nearest odd line of octave k0.
    let je = round(c.u / s1);
    let n_even = i32(je) * step0 * 2;
    let jo = round((c.u / s0 - 1.0) * 0.5);
    let n_odd = (i32(jo) * 2 + 1) * step0;
    let o = Oct(s0, k0, frac);
    let ce = line_cov(c, n_even, je * s1, o);
    let co = line_cov(c, n_odd, (2.0 * jo + 1.0) * s0, o) * (1.0 - frac);
    var cov = max(ce, co);

    // Past the last octave the pitch keeps shrinking: fade to the mean tone.
    let over = saturate(raw - kmax);
    let pitch = s0 / c.g;
    let mean = saturate(S.hatch.z * G.misc.x * c.width_scale / pitch) * (1.0 - c.gap)
        * (1.0 - smoothstep(c.thresh - c.edge, c.thresh + c.edge, c.light));
    out.cov = mix(cov, mean, over);

    // Jitter of the strip this fragment belongs to: the nearest octave-k0 line, blended
    // towards the nearest octave-(k0+1) line as frac goes to 1, so it never jumps.
    let jn = round(c.u / s0);
    let n_near = i32(jn) * step0;
    out.jit = mix(band_jit(c, n_near, jn * s0, o), band_jit(c, n_even, je * s1, o), frac);
    return out;
}

fn sun_dir(p: vec3<f32>) -> vec3<f32> {
    if (G.sun.w > 0.5) {
        return normalize(G.sun.xyz - p);
    }
    return G.sun.xyz;
}

// ---------------------------------------------------------------------------------------
// Sun shadow.

struct ShadowU {
    mats: array<mat4x4<f32>, 4>,
    // Per cascade: world size of a texel (orthographic) or texel size per metre from the sun
    // (perspective).
    texel: vec4<f32>,
    // x 1 when the maps are valid this frame, y 1 for a perspective (near) sun, z width of
    // the blend zone to the next cascade in NDC units.
    params: vec4<f32>,
};

@group(1) @binding(0) var shadow_tex: texture_depth_2d_array;
@group(1) @binding(1) var shadow_cmp: sampler_comparison;
@group(1) @binding(2) var<uniform> SH: ShadowU;

fn shadow_size() -> f32 {
    return f32(textureDimensions(shadow_tex).x);
}

fn cascade_texel(i: u32, p: vec3<f32>) -> f32 {
    return SH.texel[i] * select(1.0, distance(p, G.sun.xyz), SH.params.y > 0.5);
}

// Shadow-map uv, depth and distance to the cascade's edge (NDC units) of p in cascade i,
// after the normal and light offsets that keep a lit face from shadowing itself.
// cascade_w gives the clip w of the same point (1 for an orthographic cascade).
fn cascade_w(i: u32, p: vec3<f32>) -> f32 {
    return (SH.mats[i] * vec4<f32>(p, 1.0)).w;
}

fn cascade_lookup(i: u32, p: vec3<f32>, n: vec3<f32>, s: vec3<f32>) -> vec4<f32> {
    let t = cascade_texel(i, p);
    let ns = dot(n, s);
    let sin_t = sqrt(saturate(1.0 - ns * ns));
    let q = p + n * t * (1.5 + 2.0 * sin_t) + s * t;
    let c = SH.mats[i] * vec4<f32>(q, 1.0);
    let ndc = c.xyz / c.w;
    let inside = select(-1.0, 1.0 - max(abs(ndc.x), abs(ndc.y)), c.w > 0.0);
    return vec4<f32>(ndc.xy * vec2<f32>(0.5, -0.5) + 0.5, ndc.z, inside);
}

// The receiver's plane in cascade i, from the screen derivatives of the world position:
// xy the depth gradient per unit of shadow-map uv (receiver-plane depth bias: each filter
// tap compares against the receiver's own depth at that tap, so a wide kernel on a face
// grazed by the light does not shadow itself), z how many texels one screen pixel covers
// (the larger of its two axes).
fn cascade_plane(i: u32, dpx: vec3<f32>, dpy: vec3<f32>, w: f32) -> vec3<f32> {
    let iw = 1.0 / max(w, 1e-6);
    let ax = (SH.mats[i] * vec4<f32>(dpx, 0.0)).xyz * iw;
    let ay = (SH.mats[i] * vec4<f32>(dpy, 0.0)).xyz * iw;
    let ux = ax.xy * vec2<f32>(0.5, -0.5);
    let uy = ay.xy * vec2<f32>(0.5, -0.5);
    let det = ux.x * uy.y - uy.x * ux.y;
    var dz = vec2<f32>(0.0);
    if (abs(det) > 1e-14) {
        dz = vec2<f32>(uy.y * ax.z - ux.y * ay.z, ux.x * ay.z - uy.x * ax.z) / det;
        // A plane seen edge-on by the camera has a meaningless gradient; bound it.
        let lim = 64.0;
        dz = clamp(dz, vec2<f32>(-lim), vec2<f32>(lim));
    }
    return vec3<f32>(dz, max(length(ux), length(uy)) * shadow_size());
}

// Filtered visibility: 3x3 bilinear comparison taps with weights 1-2-1, an effective
// 4x4-texel smooth kernel, so the 0.5 iso-line used as the edge is a smooth curve rather
// than texel steps. Where a pixel covers more than one texel (grazing receivers, distant
// surfaces) the taps spread to the pixel's footprint, otherwise the map's own staircase
// would be undersampled and the thresholded edge would flicker pixel by pixel.
fn shadow_pcf(i: u32, uv: vec2<f32>, z: f32, plane: vec3<f32>) -> f32 {
    if (z >= 1.0) {
        return 1.0;
    }
    let ts = 1.0 / shadow_size();
    if (G.misc.z > 1.5) {
        let c = (floor(uv * shadow_size()) + 0.5) * ts;
        return textureSampleCompareLevel(shadow_tex, shadow_cmp, c, i32(i), z);
    } else if (G.misc.z > 0.5) {
        return textureSampleCompareLevel(shadow_tex, shadow_cmp, uv, i32(i), z);
    }
    let spread = ts * clamp(plane.z, 1.0, 2.0);
    var sum = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let w = f32((2 - abs(dx)) * (2 - abs(dy)));
            let o = vec2<f32>(f32(dx), f32(dy)) * spread;
            sum += w * textureSampleCompareLevel(shadow_tex, shadow_cmp, uv + o, i32(i), z + dot(plane.xy, o));
        }
    }
    return sum / 16.0;
}

// The world-anchored offset of the lookup that makes the edge hand-cut (S.shadow2.xy). It
// lies in the receiver's plane, so the shifted point is still on the surface and cannot
// shadow itself.
fn shadow_jitter(p: vec3<f32>, n: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    if (S.shadow2.x <= 0.0) {
        return vec3<f32>(0.0);
    }
    let a = normalize(cross(s, select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(s.y) > 0.9)));
    let b = cross(s, a);
    let f = p / S.shadow2.y;
    let j = vec2<f32>(vnoise3(0x2c1b3c6du, f), vnoise3(0x297a2d39u, f)) - 0.5;
    let o = (a * j.x + b * j.y) * 2.0 * S.shadow2.x;
    return o - n * dot(o, n);
}

// x: filtered sun visibility (1 lit), y: cascade index used (for the debug view).
// dpx, dpy: screen derivatives of the world position.
fn sun_visibility(p: vec3<f32>, n: vec3<f32>, s: vec3<f32>, dpx: vec3<f32>, dpy: vec3<f32>) -> vec2<f32> {
    if (SH.params.x < 0.5) {
        return vec2<f32>(1.0, 0.0);
    }
    let q = p + shadow_jitter(p, n, s);
    let margin = 3.0 / shadow_size();
    for (var i = 0u; i < 4u; i++) {
        let l = cascade_lookup(i, q, n, s);
        if (l.w > margin && l.z > 0.0) {
            var v = shadow_pcf(i, l.xy, l.z, cascade_plane(i, dpx, dpy, cascade_w(i, q)));
            // Near the edge of a cascade, blend into the next one so the edge does not jump.
            let blend = saturate((SH.params.z - l.w) / SH.params.z);
            if (blend > 0.0 && i < 3u) {
                let l2 = cascade_lookup(i + 1u, q, n, s);
                if (l2.w > margin && l2.z > 0.0) {
                    v = mix(v, shadow_pcf(i + 1u, l2.xy, l2.z, cascade_plane(i + 1u, dpx, dpy, cascade_w(i + 1u, q))), blend);
                }
            }
            return vec2<f32>(v, f32(i) + blend);
        }
    }
    return vec2<f32>(1.0, 4.0);
}

// Fraction of a ring of taps at S.shadow2.w metres (and half that) around p, in the light's
// view, that is lit, from the scene-wide cascade. A lit point is near a shadow edge as this
// falls below 1, a shadowed one as it rises above 0 (indication).
fn shadow_nearby(p: vec3<f32>, n: vec3<f32>, s: vec3<f32>) -> f32 {
    if (SH.params.x < 0.5) {
        return 1.0;
    }
    let l = cascade_lookup(3u, p, n, s);
    if (l.w <= 0.0 || l.z <= 0.0) {
        return 1.0;
    }
    let r = S.shadow2.w / cascade_texel(3u, p) / shadow_size();
    var lit = 0.0;
    for (var k = 0; k < 12; k++) {
        let a = f32(k) * 0.5235988;
        let rr = r * select(1.0, 0.5, (k & 1) == 1);
        lit += textureSampleCompareLevel(shadow_tex, shadow_cmp, l.xy + vec2<f32>(cos(a), sin(a)) * rr, 3, l.z);
    }
    return lit / 12.0;
}

// ---------------------------------------------------------------------------------------
// Tone.

// What the tone of one light value needs about the fragment besides the light.
struct Frag {
    u1: f32,
    u2: f32,
    g1: f32,
    g2: f32,
    lod1: f32,
    lod2: f32,
    // Crosshatch layer, at its own angle.
    u3: f32,
    v3: f32,
    g3: f32,
    gv3: f32,
    lod3: f32,
    edge: f32,
};

struct Tone {
    // Hatching and solid black together.
    cov: f32,
    black: f32,
};

fn tone_ink(f: Frag, light: f32) -> Tone {
    var t: Tone;
    // Above the single-hatch band (with the largest jitter) nothing is drawn.
    let margin = S.light.z + f.edge;
    if (light > S.bands.x + margin) {
        t.cov = 0.0;
        t.black = 0.0;
        return t;
    }
    // Below the black threshold (with the largest jitter) it is solid black.
    if (light < S.bands.z - margin) {
        t.cov = 1.0;
        t.black = 1.0;
        return t;
    }
    let h1 = hatch_layer(LayerIn(f.u1, f.u2, f.g1, f.g2, f.lod1, 0x68e31da4u, light, S.bands.x, S.bands.z, f.edge, 1.0, 1.0, S.hatch2.y));
    var c2 = 0.0;
    if (light <= S.bands.y + margin) {
        c2 = hatch_layer(LayerIn(f.u3, f.v3, f.g3, f.gv3, f.lod3, 0xb5297a4du, light, S.bands.y, S.bands.z, f.edge, S.cross.x, S.cross.y, S.white.w)).cov;
    }
    let black = 1.0 - smoothstep(S.bands.z - f.edge, S.bands.z + f.edge, light + h1.jit);
    t.black = black;
    t.cov = max(max(h1.cov, c2), black);
    return t;
}

fn band_of(light: f32) -> f32 {
    var band = 0.0;
    if (light < S.bands.x) { band = 1.0; }
    if (light < S.bands.y) { band = 2.0; }
    if (light < S.bands.z) { band = 3.0; }
    return band;
}

// White specks inside solid black: one candidate dot per world-anchored cell on the face,
// cells kept near S.speckle.z pixels by octaves (crossfaded), dot radius in pixels.
fn speck_octave(uv: vec2<f32>, cell: f32, gg: vec2<f32>, salt: u32) -> f32 {
    let ci = vec2<i32>(floor(uv / cell));
    let h = hash2(hash2(salt, bitcast<u32>(ci.x)), bitcast<u32>(ci.y));
    if (u01(h) > S.speckle.x) {
        return 0.0;
    }
    let centre = (vec2<f32>(ci) + 0.2 + 0.6 * vec2<f32>(u01(pcg(h ^ 1u)), u01(pcg(h ^ 2u)))) * cell;
    let d = length((uv - centre) / gg);
    let r = S.speckle.y * G.misc.x * (0.5 + u01(pcg(h ^ 3u)));
    return saturate(r - d + 0.5);
}

fn speckle(f: Frag, salt: u32) -> f32 {
    if (S.speckle.x <= 0.0) {
        return 0.0;
    }
    let gg = vec2<f32>(f.g1, f.g2);
    let base = 0.02;
    let kf = max(log2(max(S.speckle.z * G.misc.x * max(gg.x, gg.y) / base, 1e-6)), 0.0);
    let k0 = floor(kf);
    let fr = kf - k0;
    let uv = vec2<f32>(f.u1, f.u2);
    let c0 = base * exp2(k0);
    return mix(speck_octave(uv, c0, gg, salt), speck_octave(uv, 2.0 * c0, gg, salt ^ 0x5bd1e995u), fr);
}

@fragment
fn fs_main(in: VsOut) -> FsOut {
    let n = normalize(in.normal);
    let ax = normalize(in.axis);
    let perp = normalize(cross(n, ax));
    let p = in.world;
    let ao = in.ao;

    // Layer 1: lines run along the hatch axis, spaced along perp.
    let u1 = dot(p, perp);
    let u2 = dot(p, ax);
    // Crosshatch: lines at S.cross.z from the first layer (90 degrees: along perp), varied
    // per element or per panel by S.cross.w.
    let key = select(in.ids.y, in.ids.x, S.speckle.w > 0.5);
    let alpha = S.cross.z + (u01(pcg(key ^ 0x7f4a7c15u)) - 0.5) * S.cross.w;
    let d2 = cos(alpha) * ax + sin(alpha) * perp;
    let e2 = sin(alpha) * ax - cos(alpha) * perp;
    let u3 = dot(p, e2);
    let v3 = dot(p, d2);
    let g1 = max(length(vec2<f32>(dpdx(u1), dpdy(u1))), 1e-7);
    let g2 = max(length(vec2<f32>(dpdx(u2), dpdy(u2))), 1e-7);
    let g3 = max(length(vec2<f32>(dpdx(u3), dpdy(u3))), 1e-7);
    let gv3 = max(length(vec2<f32>(dpdx(v3), dpdy(v3))), 1e-7);
    // Diagnostic alternatives for the octave experiment. Coverage still uses the true
    // derivative; only the octave choice changes.
    let iso = length(fwidth(p)) * 0.7071;
    var lod1 = g1;
    var lod3 = g3;
    if (G.flags.z == 1u) {
        lod1 = iso;
        lod3 = iso;
    } else if (G.flags.z == 2u) {
        lod1 = 0.0;
        lod3 = 0.0;
    }

    // Light with the sun unobstructed, and the light inside a shadow.
    let s = sun_dir(p);
    let ndl = dot(n, s);
    let sun_vis = mix(1.0, ao, S.bands.w);
    var shade = 1.0 - S.blacks.z * saturate(-n.y);
    if (S.blacks.x > 0.0) {
        shade *= smoothstep(S.blacks.x - S.blacks.y, S.blacks.x + S.blacks.y, ao);
    }
    var noise = 0.0;
    if (S.tone.x > 0.0) {
        noise = (vnoise3(0x1b873593u, p / S.tone.y) - 0.5) * 2.0 * S.tone.x;
    }
    // Per-panel tone: each panel (id_fine) gets its own light offset.
    noise += (u01(pcg(in.ids.x ^ 0x6a09e667u)) - 0.5) * 2.0 * S.white.z;
    let ambient = S.light.y * ao;
    var lit = (S.light.x * G.sun_params.y * max(ndl, 0.0) * sun_vis + ambient + noise) * shade;
    let dark = clamp((ambient * S.shadow.x + noise) * shade, S.shadow.y, S.shadow.z);
    // Smooth part of the light only: the shadow edge has its own anti-aliasing below.
    let edge = max(fwidth(lit), 0.004);

    // Sun shadow: the filtered visibility is turned into a hard edge at its 0.5 iso-line,
    // anti-aliased over one pixel from its own screen gradient.
    let sv = sun_visibility(p, n, s, dpdx(p), dpdy(p));
    let gsv = length(vec2<f32>(dpdx(sv.x), dpdy(sv.x)));
    let vis = select(step(0.5, sv.x), saturate((sv.x - 0.5) / gsv + 0.5), gsv > 1e-5);
    let cast_sh = (1.0 - vis) * S.fog2.y;
    // Cast shadow only on faces the sun would reach.
    // A hard decision per face (normals are flat), so a grazing face is either in the
    // shadow test or not, never a grey blend of the two.
    // With no sun every face counts as facing it: there is no attached shadow either.
    let facing = select(1.0, step(0.05, ndl), G.sun_params.y > 0.0);
    let amount = cast_sh * facing;
    // Faces turned away from the sun move their light towards the shadow light by
    // S.shadow.w. A light blend, not an ink blend: the decision is per face, so there is
    // no grey mixture of two drawings.
    lit = mix(lit, dark, S.shadow.w * (1.0 - facing));

    // Indication: lift the light half of the single-hatch band to white, except near a
    // shadow edge (on either side of it), where lit surfaces are drawn down into hatching.
    var dark_l = dark;
    if (S.shadow2.z > 0.0) {
        let ring = shadow_nearby(p, n, s);
        let near = saturate((1.0 - ring) * 3.0);
        let near_d = saturate(ring * 3.0);
        let range = S.bands.x - S.bands.y;
        lit += S.shadow2.z * (1.0 - near) * smoothstep(S.bands.y, S.bands.x, lit) * range;
        dark_l += S.shadow2.z * (1.0 - near_d) * smoothstep(S.bands.y, S.bands.x, dark_l) * range;
        // Near the edge, lit surfaces come down into the single-hatch band, so hatching
        // gathers along the shadow on the lit side and thins out away from it.
        lit = mix(lit, min(lit, S.bands.y + 0.5 * range), saturate(near * S.shadow2.z));
    }

    let f = Frag(u1, u2, g1, g2, lod1, g2, u3, v3, g3, gv3, lod3, edge);
    var t: Tone;
    if (amount <= 0.0) {
        t = tone_ink(f, lit);
    } else if (amount >= 1.0) {
        t = tone_ink(f, dark_l);
    } else {
        let a = tone_ink(f, lit);
        let b = tone_ink(f, dark_l);
        t.cov = mix(a.cov, b.cov, amount);
        t.black = mix(a.black, b.black, amount);
    }
    let light = mix(lit, dark_l, amount);

    let view = G.flags.x;
    let hatch_on = view == VIEW_FINAL || view == VIEW_HATCHING;
    var white = speckle(f, in.ids.x);
    if (S.white.x > 0.0 && t.black > 0.0) {
        // Sparse white rules in the black, along the first hatch direction.
        let wl = hatch_layer(LayerIn(u1, u2, g1, g2, lod1, 0x3c6ef372u, 0.0, 1.0, 0.0, edge, S.white.y, 0.7, S.hatch2.y));
        white = max(white, wl.cov * S.white.x);
    }
    let speck = white * t.black;

    var out: FsOut;
    out.id = in.ids;
    out.normal = oct_encode(n);
    // Palette index 0 (the only one in black and white), band, ao, solid black coverage
    // (read by the composite to turn lines white inside black).
    out.fill = vec4<f32>(0.0, band_of(light) / 3.0, ao, select(0.0, t.black, hatch_on));
    out.ink = vec4<f32>(select(0.0, t.cov, hatch_on), select(0.0, speck, hatch_on), 0.0, 0.0);
    if (view == VIEW_SHADOW) {
        out.fill = vec4<f32>(0.0, sv.y / 4.0, ao, 0.0);
        out.ink = vec4<f32>(amount, 0.0, 0.0, 0.0);
    }
    return out;
}
