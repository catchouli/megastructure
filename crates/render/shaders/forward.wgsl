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
    @location(3) ink: vec2<f32>,
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

// Random offset added to the light value along line n, so band edges are made of line ends.
fn band_jit(c: LayerIn, n: i32) -> f32 {
    let key = line_key(c.salt, n) ^ 0x51ed270bu;
    let len = S.light.w * exp2(min(line_level(n), 10.0));
    let filt = 1.0 - saturate(c.gv / len * 2.0);
    return (vnoise(key, c.v / len + 31.7) - 0.5) * S.light.z * filt;
}

// Coverage of line n, whose centre is at u = pos.
fn line_cov(c: LayerIn, n: i32, pos: f32) -> f32 {
    let key = line_key(c.salt, n);
    let lvl = line_level(n);
    let dark = saturate((c.thresh - c.light) / max(c.thresh - c.next, 1e-3));
    let w = S.hatch.z * G.misc.x * c.width_scale * (1.0 + (u01(pcg(key)) - 0.5) * S.hatch.w) * (1.0 + S.hatch2.w * dark);
    // Breakup into scratchy segments. The segment length grows with the line's level, so
    // lines that survive to coarse octaves (seen far away) break up at a similar screen
    // scale. Under minification along the line the threshold is replaced by its mean.
    let bl = S.hatch2.x * exp2(min(lvl, 10.0));
    let nb = vnoise(key ^ 0x9e3779b9u, c.v / bl + u01(pcg(key ^ 7u)) * 64.0);
    let gap = S.hatch2.y;
    let sharp = smoothstep(gap - 0.03, gap + 0.03, nb);
    let minified = saturate(c.gv / bl * 2.0);
    let present = mix(sharp, 1.0 - gap, minified);
    // No taper towards gaps: tapering in noise units turned short segments into spindles
    // (see E007).
    let shape = tent_cov(abs(c.u - pos) / c.g, w);
    // Tone: present where the jittered light is below the band threshold.
    let tone = 1.0 - smoothstep(c.thresh - c.edge, c.thresh + c.edge, c.light + band_jit(c, n));
    return shape * present * tone;
}

// One layer of ruled lines. The octave comes from the derivative of u alone, so a grazing
// floor picks the octave its lines actually need. Octave k has lines every spacing*2^k; the
// lines of octave k+1 are the even lines of octave k, so crossfading only fades the odd
// lines and nothing jumps when k crosses an integer.
fn hatch_layer(c: LayerIn) -> LayerOut {
    var out: LayerOut;
    let base = S.hatch.x;
    let tgt = S.hatch.y * G.misc.x * c.pitch_scale;
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
    let ce = line_cov(c, n_even, je * s1);
    let co = line_cov(c, n_odd, (2.0 * jo + 1.0) * s0) * (1.0 - frac);
    var cov = max(ce, co);

    // Past the last octave the pitch keeps shrinking: fade to the mean tone.
    let over = saturate(raw - kmax);
    let pitch = s0 / c.g;
    let mean = saturate(S.hatch.z * G.misc.x * c.width_scale / pitch) * (1.0 - S.hatch2.y)
        * (1.0 - smoothstep(c.thresh - c.edge, c.thresh + c.edge, c.light));
    out.cov = mix(cov, mean, over);

    // Jitter of the strip this fragment belongs to: the nearest octave-k0 line, blended
    // towards the nearest octave-(k0+1) line as frac goes to 1, so it never jumps.
    let jn = round(c.u / s0);
    let n_near = i32(jn) * step0;
    out.jit = mix(band_jit(c, n_near), band_jit(c, n_even), frac);
    return out;
}

fn sun_dir(p: vec3<f32>) -> vec3<f32> {
    if (G.sun.w > 0.5) {
        return normalize(G.sun.xyz - p);
    }
    return G.sun.xyz;
}

@fragment
fn fs_main(in: VsOut) -> FsOut {
    let n = normalize(in.normal);
    let ax = normalize(in.axis);
    let perp = normalize(cross(n, ax));
    let p = in.world;

    // Layer 1: lines run along the hatch axis, spaced along perp. Layer 2 crosses it.
    let u1 = dot(p, perp);
    let u2 = dot(p, ax);
    var g1 = max(length(vec2<f32>(dpdx(u1), dpdy(u1))), 1e-7);
    var g2 = max(length(vec2<f32>(dpdx(u2), dpdy(u2))), 1e-7);
    // Diagnostic alternatives for the octave experiment. Coverage still uses the true
    // derivative; only the octave choice changes.
    let iso = length(fwidth(p)) * 0.7071;
    var lod1 = g1;
    var lod2 = g2;
    if (G.flags.z == 1u) {
        lod1 = iso;
        lod2 = iso;
    } else if (G.flags.z == 2u) {
        lod1 = 0.0;
        lod2 = 0.0;
    }

    let light = S.light.x * G.sun_params.y * max(dot(n, sun_dir(p)), 0.0) + S.light.y * in.ao;
    let edge = max(fwidth(light), 0.004);

    let h1 = hatch_layer(LayerIn(u1, u2, g1, g2, lod1, 0x68e31da4u, light, S.bands.x, S.bands.z, edge, 1.0, 1.0));
    let h2 = hatch_layer(LayerIn(u2, u1, g2, g1, lod2, 0xb5297a4du, light, S.bands.y, S.bands.z, edge, S.cross.x, S.cross.y));
    let black = 1.0 - smoothstep(S.bands.z - edge, S.bands.z + edge, light + h1.jit);

    var band = 0.0;
    if (light < S.bands.x) { band = 1.0; }
    if (light < S.bands.y) { band = 2.0; }
    if (light < S.bands.z) { band = 3.0; }

    let view = G.flags.x;
    let hatch_on = view == VIEW_FINAL || view == VIEW_HATCHING;

    var out: FsOut;
    out.id = in.ids;
    out.normal = oct_encode(n);
    // Palette index 0 (the only one in black and white), band, ao, flags.
    out.fill = vec4<f32>(0.0, band / 3.0, in.ao, 0.0);
    out.ink = vec2<f32>(select(0.0, max(max(h1.cov, h2.cov), black), hatch_on), 0.0);
    return out;
}
