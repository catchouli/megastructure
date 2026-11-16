// Forward geometry pass. Writes ids, octahedral normal, fill (palette, band, ao, flags) and
// black ink from ruled hatching and solid black. common.wgsl is prepended.

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) hatch_axis: vec3<f32>,
    @location(3) id_fine: u32,
    @location(4) id_coarse: u32,
    @location(5) ao: f32,
    // Baked one-bounce light: x sun, y sky (vertex buffer slot 1).
    @location(6) bounce: vec2<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(flat) normal: vec3<f32>,
    @location(2) @interpolate(flat) axis: vec3<f32>,
    @location(3) @interpolate(flat) ids: vec2<u32>,
    @location(4) ao: f32,
    @location(5) bounce: vec2<f32>,
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
    out.bounce = in.bounce;
    return out;
}

struct FsOut {
    @location(0) id: vec2<u32>,
    @location(1) normal: vec2<f32>,
    @location(2) fill: vec4<f32>,
    // R black ink, G white ink, B line ink (written by the line passes), A unused.
    @location(3) ink: vec4<f32>,
    // x wash coverage (continuous tone models), y surface paper relief 0.5 + 2 h.
    @location(4) tone: vec2<f32>,
};

// Everything one hatch layer needs about the fragment. u is the spacing coordinate (metres,
// across the lines), v the coordinate along the lines; g and gv their screen derivatives in
// metres per pixel.
struct LayerIn {
    u: f32,
    v: f32,
    g: f32,
    gv: f32,
    // Rotation-invariant footprints of u and v in metres per pixel (see fs_main). lod
    // chooses the octave; lodv sets stroke shapes along the line. Coverage uses g and gv.
    lod: f32,
    lodv: f32,
    // Light gradient along the lines, per metre (for feathering tone boundaries).
    lgrad: f32,
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

// Patch runs, nested across line levels so that a line's strokes depend only on its own
// identity, never on the octave being drawn (E020). Lines are grouped into columns: a line of
// level l belongs to column floor(n / (L 2^l)), L = S.strokes.y, so at any octave a column
// holds about L drawn lines, and the columns of level l + 1 are unions of two of level l.
// Along the lines, runs start from a world grid of length R0 2^11 (R0 = hatch_break_len *
// hatch_patch_len) with a phase per top column; going down one level, each run is split once,
// at a random point between 25% and 75%, chosen per run and per column of that level. A line
// of level l uses the runs reached after splitting down to l. Runs of level l are therefore
// unions of runs of level l - 1: lines in one column start together at the run's start (a
// common start edge), and their ends are jittered per line (a ragged trailing end).
// x metres since the stroke started, y metres until it ends, z a random value per run,
// w the run's length.
fn patch_run(salt: u32, key: u32, n: i32, lvl: f32, v: f32, gap: f32) -> vec4<f32> {
    let lines = max(S.strokes.y, 1.0);
    let top = 11;
    let ln = i32(min(lvl, f32(top)));
    let r_top = S.hatch2.x * S.strokes.z * exp2(f32(top));
    let fnn = f32(n);
    var id = hash2(salt, bitcast<u32>(i32(floor(fnn / (lines * exp2(f32(top)))))));
    let ph = u01(id);
    let jt = floor(v / r_top + ph);
    var a = (jt - ph) * r_top;
    var b = a + r_top;
    id = hash2(id, bitcast<u32>(i32(jt)));
    for (var j = top - 1; j >= ln; j--) {
        let cj = i32(floor(fnn / (lines * exp2(f32(j)))));
        let h = hash2(id, bitcast<u32>(cj));
        let m = a + (b - a) * (0.25 + 0.5 * u01(h));
        let right = v >= m;
        a = select(a, m, right);
        b = select(m, b, right);
        id = hash2(h, select(0x2545f491u, 0x9e3779b9u, right));
    }
    let len = b - a;
    let kl = pcg(key ^ id);
    // Ragged at both ends, by S.feather.y of the run: the start skewed (most strokes start
    // near the run's start, a few later), the end uniform.
    let js = u01(kl);
    let start = a + len * S.feather.y * js * js;
    let end = b - len * (gap * (0.6 + 0.8 * u01(id)) + S.feather.y * u01(pcg(kl ^ 9u)));
    return vec4<f32>(v - start, end - v, u01(pcg(id ^ 5u)), len);
}

// Random offset added to the light value along line n, so band edges are made of line ends.
// With patches on, it is mostly one value per run, so a band edge is a group of strokes
// stopping together rather than a sawtooth.
fn band_jit(c: LayerIn, n: i32) -> f32 {
    // Per line only. Until E026 it was shared by the lines of a patch run, which made a
    // whole run (a rectangle of lines) switch band at once: flat-topped blocks of hatching,
    // and of solid black where the light sat just above band_black.
    let key = line_key(c.salt, n) ^ 0x51ed270bu;
    let len = S.light.w * exp2(min(line_level(n), 10.0));
    let filt = 1.0 - saturate(c.lodv / len * 2.0);
    return (vnoise(key, c.v / len + 31.7) - 0.5) * S.light.z * filt;
}

// Width multiplier for a stroke end d pixels away, over taper length tp.
fn end_taper(d: f32, tp: f32) -> f32 {
    return select(1.0, smoothstep(0.0, tp, d), tp > 0.0);
}

// Coverage of a stroke of full width w (pixels) narrowed to w * k, with one edge kept in place
// (side +-1), at signed distance sd (pixels) from the stroke's centre line. A stroke that
// thins keeps one straight edge and loses the other, like a pen lifting off at an angle,
// rather than closing symmetrically into a point.
fn stroke_cov(sd: f32, w: f32, k: f32, side: f32) -> f32 {
    let kk = saturate(k);
    return tent_cov(abs(sd - side * 0.5 * w * (1.0 - kk)), w * kk);
}

// Coverage of line n, whose centre is at u = pos. fade (0..1) is how much of the line the
// octave crossfade keeps. Distances along the line are taken in true pixels (c.gv) for
// anti-aliasing and in rotation-invariant pixels (c.lodv) for the shape of the stroke, so a
// stroke keeps its shape as the camera turns. Every way a stroke fades (octave crossfade,
// band edge, break, shadow edge) narrows it rather than greying it: a visible mark is
// full-strength ink (E026).
fn line_cov(c: LayerIn, n: i32, pos: f32, fade: f32) -> f32 {
    if (fade <= 0.0) {
        return 0.0;
    }
    let key = line_key(c.salt, n);
    let lvl = line_level(n);
    let dark = saturate((c.thresh - c.light) / max(c.thresh - c.next, 1e-3));
    // Width in rotation-invariant pixels (E020), capped (E026) so near walls are not drawn
    // as bars: up close, spacing carries tone, not width. Converted to true pixels for
    // coverage.
    let wi = min(S.hatch.z * G.misc.x * c.width_scale * (1.0 + (u01(pcg(key)) - 0.5) * S.hatch.w) * (1.0 + S.hatch2.w * dark),
        S.feather2.z * G.misc.x);
    let w = wi * (c.lod / c.g);
    let gap = c.gap;
    let sd = (c.u - pos) / c.g;
    let inv = c.gv / max(c.lodv, 1e-9);
    // Per stroke: which edge stays when it thins, and how long its tapers are: a multiple of
    // its width (S.feather2.y), at least S.strokes.w pixels, varied by +-40%.
    let side = select(-1.0, 1.0, (pcg(key ^ 0x2f6b9e01u) & 1u) == 1u);
    let tp = max(S.strokes.w * G.misc.x, S.feather2.y * wi) * (0.6 + 0.8 * u01(pcg(key ^ 0x6c8e9cf5u)));
    // Tone: present where the jittered light is below the band threshold. Distance in
    // pixels to where it stops, from the light's change per pixel (c.edge).
    // Feathering: this stroke's end at the band edge moves along the line by its own amount
    // in metres, scaled by the spacing of its level, so coarser (longer-lived) lines run on
    // further and the boundary thins out stroke by stroke.
    var fshift = 0.0;
    if (S.feather.x > 0.0) {
        let delta = S.feather.x * S.hatch.x * exp2(min(lvl, 10.0)) * (u01(pcg(key ^ 0x3c6ef372u)) - 0.35);
        fshift = clamp(c.lgrad * delta, -0.2, 0.2);
    }
    let d_tone = (c.thresh - c.light + fshift - band_jit(c, n)) / max(c.edge, 1e-4);
    let tone = saturate(d_tone + 0.5);
    var k = fade * end_taper(d_tone * inv, tp);
    if (SH_GATE != 0u) {
        k *= line_shadow(c.salt, n);
    }
    if (S.strokes.x > 0.0) {
        let r = patch_run(c.salt ^ 0x9e3779b9u, key, n, lvl, c.v, gap);
        let minified = saturate(c.lodv * 4.0 / r.w - 0.5);
        // Short taper where the stroke starts, long where it trails off.
        let kr = k * end_taper(r.x / c.lodv, tp * 0.3) * end_taper(r.y / c.lodv, tp);
        let sharp = stroke_cov(sd, w, kr, side) * saturate(r.x / c.gv + 0.5) * saturate(r.y / c.gv + 0.5);
        return mix(sharp, stroke_cov(sd, w, k, side) * (1.0 - gap), minified) * tone;
    }
    // Per-line breakup: noise along v with a segment length that grows with the line's
    // level, so lines that survive to coarse octaves break up at a similar screen scale.
    let bl = S.hatch2.x * exp2(min(lvl, 10.0));
    let nl = vnoise_d(key ^ 0x9e3779b9u, c.v / bl + u01(pcg(key ^ 7u)) * 64.0);
    // Signed distance in pixels along the line to the nearest break end (first order).
    let df = nl.y / bl;
    let d_break = (nl.x - gap) / max(abs(df) * c.gv, 1e-6);
    let minified = saturate(c.lodv / bl * 2.0);
    let present = mix(saturate(d_break + 0.5), 1.0 - gap, minified);
    let kb = k * mix(end_taper(d_break * inv, tp * select(1.0, 0.3, df > 0.0)), 1.0, minified);
    return stroke_cov(sd, w, kb, side) * present * tone;
}

// Fragment context for drawing inside a cast shadow (set in fs_main).
var<private> SH_GATE: u32;
var<private> FP: vec3<f32>;
var<private> FN: vec3<f32>;
var<private> FS: vec3<f32>;
var<private> FDIR: vec3<f32>;
var<private> FAMT: f32;
var<private> FDPX: vec3<f32>;
var<private> FDPY: vec3<f32>;

// How much a shadow stroke is drawn at this fragment: the cast-shadow amount, either of the
// fragment itself (crisp edge) or, with S.feather.z > 0, tested at a point moved along the
// stroke by the stroke's own random amount, so each stroke ends at its own place near the
// shadow edge (feathered edge).
fn line_shadow(salt: u32, n: i32) -> f32 {
    if (S.feather.z <= 0.0) {
        return FAMT;
    }
    let d = S.feather.z * (2.0 * u01(pcg(line_key(salt, n) ^ 0x1b56c4e9u)) - 1.0);
    return 1.0 - smoothstep(0.35, 0.65, sun_vis_cheap(FP + FDIR * d));
}

// One layer of ruled lines. The octave comes from the footprint of u alone, so a grazing
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
    // The odd lines fade out over the octave by narrowing, not by opacity (E026).
    let ce = line_cov(c, n_even, je * s1, 1.0);
    let co = line_cov(c, n_odd, (2.0 * jo + 1.0) * s0, 1.0 - frac);
    var cov = max(ce, co);

    // Past the last octave the pitch keeps shrinking: fade to the mean tone.
    let over = saturate(raw - kmax);
    let pitch = s0 / c.lod;
    let mean = saturate(S.hatch.z * G.misc.x * c.width_scale / pitch) * (1.0 - c.gap)
        * (1.0 - smoothstep(c.thresh - c.edge, c.thresh + c.edge, c.light));
    out.cov = mix(cov, mean, over);

    // Jitter of the strip this fragment belongs to: the nearest octave-k0 line, blended
    // towards the nearest octave-(k0+1) line as frac goes to 1, so it never jumps.
    let jn = round(c.u / s0);
    let n_near = i32(jn) * step0;
    out.jit = mix(band_jit(c, n_near), band_jit(c, n_even), frac);
    return out;
}


// ---------------------------------------------------------------------------------------
// Continuous ruled tone (tone_model 1, E027 onwards).
//
// The tone signal c (0 paper .. 1 solid ink) sets the coverage of the ruling directly:
// every line's width is a fraction of the local pitch, width = c * pitch, so marks belong to
// the surface (back away and lines get thinner and closer together in proportion) and tone
// is conserved across distance. Through an octave change the odd lines narrow to nothing
// while the even lines gain exactly that width (even c P (1 + f), odd c P (1 - f)), so mean
// coverage is constant across the whole octave. Past about half coverage the white between
// lines is a sliver; a merge term near c = 1 widens lines further so slivers thin, break
// up and close into solid black: hatching and black are one quantity.

struct RuleIn {
    u: f32,
    v: f32,
    g: f32,
    gv: f32,
    lod: f32,
    lodv: f32,
    // dc/dv, signed, per metre: which way along the line the tone darkens.
    cgrad: f32,
    salt: u32,
    c: f32,
};

// Private: solid black coverage for the white specks (E066).
var<private> SPECK_BLACK: f32 = 0.0;
// Private: the near-shadow-edge factor for shadow strokes (shadow_hatch_near).
var<private> FNEAR: f32 = 1.0;

// Weight profile along a stroke, t = 0 at its dark end, 1 at its light end: heavy and blunt
// where the pen goes down, thinning steadily to a hairline that gives out. Normalised to
// mean 1 over the stroke so it does not change the tone (heavy end 1.84, light end 0.07:
// a hairline).
fn rule_profile(t: f32) -> f32 {
    return (1.0 - 0.96 * pow(saturate(t), 1.1)) / 0.5429;
}

// Coverage of ruled line n, centred at u = pos, with base width wb (rotation-invariant
// pixels, before modulation). merge (0..1) fades the modulations out as the tone approaches
// solid black, so the merge closes evenly.
// share is the part of the pitch this line stands for (its width at full coverage).
fn rule_line(r: RuleIn, n: i32, pos: f32, wb: f32, merge: f32, share: f32) -> f32 {
    if (wb <= 0.0) {
        return 0.0;
    }
    let key = line_key(r.salt, n);
    let lvl = line_level(n);
    let modamt = 1.0 - merge * merge;
    // Weight per stroke (mean 1).
    let vary = 1.0 + S.rule3.x * modamt * (u01(pcg(key ^ 0xa511e9b3u)) * 2.0 - 1.0);
    // Breaks: fewer as the tone darkens. A patch run loses `gap` of its length on average
    // plus the ragged start and end (S.feather.y: about 0.83 of it, see patch_run); that
    // coverage is put back into width so the tone stays what c asks for.
    let gap0 = S.hatch2.y * (1.0 - r.c);
    let ragged = 0.83 * S.feather.y;
    let kept0 = max(1.0 - gap0 - ragged, 0.05);
    var w0 = wb * vary / kept0;
    // Below the minimum width a stroke does not thin further: it breaks into dashes, so
    // the very lightest tone is a few fine marks, not grey haze. The dashes keep pres of
    // the usual length, so coverage is still conserved.
    let wmin = S.rule2.y;
    let pres = saturate(w0 / wmin);
    w0 = max(w0, wmin);
    let gap = 1.0 - ragged - pres * kept0;
    if (gap >= 0.999) {
        return 0.0;
    }
    var k = 1.0;
    if (SH_GATE != 0u) {
        k = line_shadow(r.salt, n) * FNEAR;
    }
    let rr = patch_run(r.salt ^ 0x9e3779b9u, key, n, lvl, r.v, gap);
    let len = max(rr.x + rr.y, 1e-6);
    let t = saturate(rr.x / len);
    // Which end is dark: from the tone gradient along the line where there is one, else
    // a random direction per stroke.
    let srand = select(0.0, 1.0, (pcg(key ^ 0x1f83d9abu) & 1u) == 1u);
    let gl = r.cgrad * len;
    let sdir = saturate(mix(srand, 0.5 + 4.0 * gl, saturate(abs(gl) * 8.0)));
    // The thinning runs over at least 24 stroke widths, so a short stroke stays near its
    // heavy end and stops bluntly instead of being squeezed into a wedge (a dagger).
    let kl = len / max(len, 24.0 * w0 * r.lodv);
    let prof = mix(rule_profile(t * kl), rule_profile((1.0 - t) * kl), sdir);
    // Pressure: low-frequency width variation along the stroke, per stroke.
    let lam = S.hatch2.x * exp2(min(lvl, 10.0)) * 1.3;
    let press = 1.0 + S.rule2.w * modamt * (2.0 * vnoise(key ^ 0x5be0cd19u, r.v / lam + u01(pcg(key)) * 17.0) - 1.0);
    let weight = mix(1.0, prof, S.rule2.z * modamt) * press * k;
    // Never wider than most of the pitch unless the tone asks for the merge into black, so
    // neighbouring strokes do not fuse by accident.
    let wi = min(w0 * weight, share * (0.85 + 1.5 * merge));
    let w = wi * (r.lod / r.g);
    let sd = abs(r.u - pos) / r.g;
    // A short blunt ramp at both ends (1.5 px): the light end is already a hairline from the
    // profile, so nothing ends in a point.
    let tpS = 1.5 * G.misc.x;
    let ends = end_taper(rr.x / r.lodv, tpS) * end_taper(rr.y / r.lodv, tpS);
    let sharp = tent_cov(sd, w * ends) * saturate(rr.x / r.gv + 0.5) * saturate(rr.y / r.gv + 0.5);
    let minified = saturate(r.lodv * 4.0 / rr.w - 0.5);
    return mix(sharp, tent_cov(sd, w0 * k * (r.lod / r.g)) * (1.0 - gap), minified);
}

// Target ruling pitch in pixels at this output size (rule_target_px * px_scale^rule_res_exp).
fn rule_target() -> f32 {
    return S.rule.w * pow(G.misc.x, S.dark.w);
}

// Ruled coverage for tone c. With S.rule.y 0 the octave is chosen from the rotation-invariant
// footprint (pitch between T/2 and T pixels, denser as c rises); with 1 there is one world
// pitch and no line is ever dropped. Either way, where the pitch falls below the handover
// (S.rule3.y pixels) the ruling dissolves into its mean coverage, c, over a factor of two.
fn rule_layer(r: RuleIn) -> f32 {
    let c = r.c;
    if (c <= 0.002) {
        return 0.0;
    }
    // Inside a cast shadow, flat parts (solid black, the dissolved far ruling) follow the
    // fragment's own shadow amount; strokes are gated in rule_line.
    let gate = select(1.0, FAMT * FNEAR, SH_GATE != 0u);
    if (c >= 0.985) {
        return gate;
    }
    let base = S.rule.z;
    var kf = 0.0;
    // Diagnostics (--hatch-lod): fixed keeps octave 0 (rigid by construction, the stability
    // baseline); screen chooses the octave from the screen derivative.
    if (S.rule.y < 0.5 && G.flags.z != 2u) {
        let tgt = rule_target() / (1.0 + S.rule2.x * c);
        let fp = select(r.lod, r.g, G.flags.z == 3u);
        kf = clamp(log2(max(tgt * fp / base, 1e-6)), 0.0, 14.0);
    }
    let k0 = floor(kf);
    let f = kf - k0;
    let step0 = 1 << u32(k0);
    let s0 = base * f32(step0);
    let s1 = 2.0 * s0;
    let pitch = s0 / r.lod;
    // Handover to flat tone: with nested octaves only past the last octave (as the band
    // model does), with one world pitch where the pitch falls below the handover.
    let hpx = S.rule3.y * G.misc.x;
    var hand = smoothstep(hpx, 2.0 * hpx, pitch);
    if (S.rule.y < 0.5) {
        hand = 1.0 - saturate(log2(max(rule_target() / (1.0 + S.rule2.x * c) * r.lod / base, 1e-6)) - 14.0);
    }
    if (hand <= 0.0) {
        return c * gate;
    }
    let merge = smoothstep(0.55, 1.0, c);
    let cw = c + 0.4 * merge * merge;
    let we = cw * pitch * (1.0 + f);
    let wo = cw * pitch * (1.0 - f);
    let je = round(r.u / s1);
    let sp = i32(step0);
    let ne = i32(je) * sp * 2;
    var sum = rule_line(r, ne, je * s1, we, merge, pitch * (1.0 + f));
    sum += rule_line(r, ne - 2 * sp, (je - 1.0) * s1, we, merge, pitch * (1.0 + f));
    sum += rule_line(r, ne + 2 * sp, (je + 1.0) * s1, we, merge, pitch * (1.0 + f));
    sum += rule_line(r, ne - sp, (2.0 * je - 1.0) * s0, wo, merge, pitch * (1.0 - f));
    sum += rule_line(r, ne + sp, (2.0 * je + 1.0) * s0, wo, merge, pitch * (1.0 - f));
    return mix(c * gate, min(sum, 1.0), hand);
}

// Tone curve of the ruling: 0 at and above fall_white, 1 at and below fall_black.
fn fall_curve(l: f32) -> f32 {
    let x = saturate((S.fall.x - l) / max(S.fall.x - S.fall.y, 1e-3));
    return pow(x, max(S.fall.z, 1e-3));
}

// Pale wash at the light end (E062): S.paper13.x at wash_white, thinning to 0 at light_white
// (S.paper13.y) along (1 - t)^gamma (S.paper13.z: above 1 a quick fall then a long pale
// shoulder); below wash_white it fades out over 0.25 of light, leaving the darker tones as
// they were.
fn light_end(l: f32) -> f32 {
    let ww = S.wash.y;
    let lw = max(S.paper13.y, ww + 0.01);
    if (l >= ww) {
        let t = saturate((l - ww) / (lw - ww));
        return S.paper13.x * pow(1.0 - t, max(S.paper13.z, 0.1));
    }
    return S.paper13.x * smoothstep(ww - 0.25, ww, l);
}

// Wash curve, already scaled to ink coverage.
fn wash_curve(l: f32) -> f32 {
    let x = saturate((S.wash.y - l) / max(S.wash.y - S.wash.z, 1e-3));
    return pow(x, max(S.wash.w, 1e-3)) * S.wash2.x * S.wash.x;
}

// Signed derivative along v of a quantity with screen derivatives (lx, ly).
fn grad_along_signed(ux: f32, uy: f32, vx: f32, vy: f32, lx: f32, ly: f32) -> f32 {
    let det = ux * vy - vx * uy;
    if (abs(det) < 1e-14) {
        return 0.0;
    }
    return (ux * ly - uy * lx) / det;
}

// ---------------------------------------------------------------------------------------
// Screentone: a threshold map T in 0..1; the wash w prints as ink where w > T.

const TAU_: f32 = 6.2831853;

fn dot_screen(x: vec2<f32>) -> f32 {
    return 0.5 - 0.25 * (cos(TAU_ * x.x) + cos(TAU_ * x.y));
}

fn grain_screen(n: f32) -> f32 {
    return saturate(0.5 + (n - 0.5) * 2.6);
}

fn screen_kind(dots: f32, grain: f32) -> f32 {
    let k = S.wash2.z;
    if (k < 0.5) {
        return dots;
    }
    if (k < 1.5) {
        return grain;
    }
    return 0.5 * (dots + grain);
}


// Surface screentone with one octave per surface axis (screentone_aniso, E036). The cell
// size along each surface axis is chosen from that axis's own rotation-invariant footprint,
// so on a grazing floor the cells are short along the grazed direction and the grain stays
// grain-sized on screen instead of stretching into streaks. Each octave pair (ku, kv) has
// its own seed, a function of the octave numbers only, so the field drawn at the end of one
// octave's crossfade is exactly the one drawn at the start of the next: no seam where the
// octave changes (the E032 grain used one seed for "this octave" and another for "the next",
// so the pattern swapped at every integer octave, along lines of equal footprint). The four
// fields are blended bilinearly and the blend is renormalised by its standard deviation, so
// the grain's contrast does not dip halfway through a crossfade.
fn screentone_aniso(uv: vec2<f32>, gg: vec2<f32>, pn: f32, px: f32, base: f32) -> f32 {
    let ku = max(log2(max(px * gg.x / base, 1e-6)), 0.0);
    let kv = max(log2(max(px * gg.y / base, 1e-6)), 0.0);
    let ku0 = floor(ku);
    let kv0 = floor(kv);
    let fu = ku - ku0;
    let fv = kv - kv0;
    // The plane's offset decorrelates parallel faces.
    let plane = pcg(bitcast<u32>(i32(floor(pn * 4.0))));
    var sumg = 0.0;
    var sumd = 0.0;
    var ww = 0.0;
    for (var j = 0; j < 2; j++) {
        for (var i = 0; i < 2; i++) {
            let w = select(1.0 - fu, fu, i == 1) * select(1.0 - fv, fv, j == 1);
            let cu = base * exp2(ku0 + f32(i));
            let cv = base * exp2(kv0 + f32(j));
            let q = vec2<f32>(uv.x / cu, uv.y / cv);
            let key = hash2(hash2(0x3243f6a8u ^ plane, u32(ku0) + u32(i)), u32(kv0) + u32(j));
            // Two lattices per field: a main one at 0.8 of the cell (resolved, about 1.6 to 3
            // pixels) and a fine one at 0.4 (sub-pixel, the E032 grain), weighted by
            // screentone_detail. The sum clumps a little, like paper fibre, and the
            // coarse lattice is what keeps the grain still under sub-pixel motion.
            let det = S.grain.y;
            let g2 = (1.0 - det) * (vnoise2(key ^ 0x6a09e667u, q / 0.8) - 0.5) + det * (vnoise2(key, q / 0.4) - 0.5);
            sumg += w * g2 * inverseSqrt(max((1.0 - det) * (1.0 - det) + det * det, 0.25));
            sumd += w * (dot_screen(q) - 0.5);
            ww += w * w;
        }
    }
    let norm = inverseSqrt(max(ww, 0.25));
    return screen_kind(saturate(0.5 + sumd * norm), grain_screen(0.5 + sumg * norm));
}

// Threshold at this fragment for the chosen anchor. frag: pixel position; dv: unit view ray;
// uv, gg: surface coordinates and their rotation-invariant footprints.
fn screentone_t(frag: vec2<f32>, dv: vec3<f32>, p: vec3<f32>, uv: vec2<f32>, gg: vec2<f32>, pn: f32) -> f32 {
    let px = max(S.wash3.x * G.misc.x, 1.0);
    let a = S.wash2.w;
    if (a < 0.5) {
        // Screen: a 45 degree dot screen in pixels (print-true, shower-doors).
        let q = frag / px;
        let x = vec2<f32>(q.x + q.y, q.y - q.x) * 0.70710678;
        let n = vnoise3(0x3243f6a8u, vec3<f32>(frag / (px * 0.4), 0.5));
        return screen_kind(dot_screen(x), grain_screen(n));
    }
    if (a < 1.5) {
        // View sphere: an equi-angular cube map of the view direction, whose dot lattice is
        // aligned so that it mirrors seamlessly across every cube edge. Depends only on the
        // direction in the world, so it is fixed under rotation; it slides under translation.
        let pr0 = px * G.misc.y;
        let pr = 1.5707963 / max(round(1.5707963 / pr0), 1.0);
        let ad = abs(dv);
        var ab: vec2<f32>;
        var m: f32;
        if (ad.x >= ad.y && ad.x >= ad.z) {
            ab = dv.yz;
            m = ad.x;
        } else if (ad.y >= ad.z) {
            ab = dv.xz;
            m = ad.y;
        } else {
            ab = dv.xy;
            m = ad.z;
        }
        let ang = vec2<f32>(atan(ab.x / m), atan(ab.y / m)) + 0.78539816;
        let n = vnoise3(0x3243f6a8u, dv / (pr * 0.4));
        return screen_kind(dot_screen(ang / pr), grain_screen(n));
    }
    // Surface: dots in the surface's own frame, octaves chosen from the rotation-invariant
    // footprint and crossfaded; the octave-(k+1) screen's dots are every other dot of
    // octave k, so the crossfade grows those and shrinks the rest.
    let base = 0.004;
    if (S.bounce.z > 0.5) {
        return screentone_aniso(uv, gg, pn, px, base);
    }
    let kf = max(log2(max(px * max(gg.x, gg.y) / base, 1e-6)), 0.0);
    let k0 = floor(kf);
    let fr = kf - k0;
    let c0 = base * exp2(k0);
    let d = mix(dot_screen(uv / c0), dot_screen(uv / (2.0 * c0)), fr);
    let n = mix(vnoise3(0x3243f6a8u, p / (c0 * 0.4)), vnoise3(0x85a308d3u, p / (c0 * 0.8)), fr);
    return screen_kind(d, grain_screen(n));
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
@group(1) @binding(3) var paper_tex: texture_2d_array<f32>;
@group(1) @binding(4) var paper_smp: sampler;
@group(1) @binding(5) var shade_tex: texture_2d_array<f32>;

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

// Cheaper sun visibility for the feathered shadow edge: the smooth kernel in the first
// cascade that contains q, no blending between cascades and no receiver-plane bias.
fn sun_vis_cheap(q: vec3<f32>) -> f32 {
    if (SH.params.x < 0.5) {
        return 1.0;
    }
    let margin = 3.0 / shadow_size();
    for (var i = 0u; i < 4u; i++) {
        let l = cascade_lookup(i, q, FN, FS);
        if (l.w > margin && l.z > 0.0) {
            // The receiver's plane is the fragment's own face, so its depth gradient applies
            // to the moved point too (without it, grazed faces shadowed themselves in specks).
            return shadow_pcf(i, l.xy, l.z, cascade_plane(i, FDPX, FDPY, cascade_w(i, q)));
        }
    }
    return 1.0;
}

// ---------------------------------------------------------------------------------------
// Halftone (E040): a soft-edged, rotated dot screen fixed to the surface, carrying a small
// part of the mid-to-dark wash. It never swaps scale abruptly: there are eight fixed world
// pitches (S.halftone.y times 1, 2, 4 ... 128), and each is faded in and out by amplitude
// according to its pitch on screen (rotation-invariant footprint, in pixels at 720 lines),
// present only between about 3.5 and 11 pixels. Far away every layer has faded and the wash is smooth, well
// before the dots could reach pixel scale.

fn halftone_cov(w: f32, uv: vec2<f32>, pitch: f32) -> f32 {
    let a = S.halftone.w;
    let r = vec2<f32>(cos(a) * uv.x - sin(a) * uv.y, sin(a) * uv.x + cos(a) * uv.y) / pitch;
    // Slight irregularity: each dot a little larger or smaller (a threshold offset per cell).
    // The lattice itself stays exact, so the layers stay nested (the dots of pitch 2P are
    // every other dot of pitch P) and a crossfade between them does not beat; a wobble of
    // the lattice, tried first, differed between layers and showed as bands (E040).
    let cell = vec2<i32>(floor(r + 0.5));
    let jit = u01(hash2(hash2(0x1d872b41u, bitcast<u32>(cell.x)), bitcast<u32>(cell.y))) - 0.5;
    // Round dots, area-true (threshold pi d^2 for the distance d to the dot's centre), so
    // mid tones stay round dots like printed screentone instead of turning into the
    // checkerboard of the cosine screen.
    let d = length(r - floor(r + 0.5));
    let t = saturate(3.14159265 * d * d) + 0.08 * jit;
    return saturate((w - t) / max(S.halftone.z, 0.02) + 0.5);
}

fn halftone(w: f32, uv: vec2<f32>, gmax: f32) -> f32 {
    // Only mid to dark tone; paper and near-solid tone stay smooth.
    let band = smoothstep(0.1, 0.35, w) * (1.0 - smoothstep(0.85, 0.98, w));
    if (band <= 0.0) {
        return w;
    }
    var sum = 0.0;
    var wsum = 0.0;
    // Only the pitches that can fall in the 3.5 to 11 px window (at most three).
    let kmin = max(floor(log2(3.5 * gmax * G.misc.x / S.halftone.y)), 0.0);
    for (var j = 0; j < 3; j++) {
        let k = kmin + f32(j);
        if (k > 7.0) {
            break;
        }
        let pitch = S.halftone.y * exp2(k);
        let px = pitch / max(gmax, 1e-7) / G.misc.x;
        // Capped in world units (S.paper4.z, E044): far walls get no large dots (the owner's
        // "snakeskin"); the largest pitch fades out over the last octave below the cap.
        let cap = 1.0 - smoothstep(0.5 * S.paper4.z, S.paper4.z, pitch);
        let win = smoothstep(3.5, 5.0, px) * (1.0 - smoothstep(8.0, 11.0, px)) * cap;
        if (win > 0.0) {
            sum += win * (halftone_cov(w, uv, pitch) - w);
            wsum += win;
        }
    }
    return saturate(w + S.halftone.x * band * sum / max(wsum, 1.0));
}

// ---------------------------------------------------------------------------------------
// Surface-anchored paper (paper_anchor 1). Since E044 the tooth is a baked fibre tile
// (paper.rs: short fibres at random orientations, loose clumps, fine pits, no cloud)
// sampled at fixed world scales: layer k has texels of S.paper4.x * 2^k metres. A layer is
// present while its texel is between S.paper2.y and 4 S.paper2.y pixels at 720 lines on
// screen (rotation-invariant footprint), faded by amplitude, so at any distance about two
// layers are drawn, each band-limited by the tile's mips, and walking closer refines rather
// than swaps. Layers whose texel exceeds S.paper5.x metres are not drawn, so beyond a
// distance the paper fades to smooth tone. Each layer is rotated and offset so the tile's
// repeat and orientation never line up. Since E049 the relief is one layer of a texture
// array chosen by S.paper5.y (the generated fibre tile or a scanned sheet); a scan is
// stretched back to its own aspect (PAPER_ASPECT), so its texels stay square in the world.
// The E040 mottle was removed in E049. Returns the relief h, already weighted.
fn paper_surface(uv: vec2<f32>, pn: f32, gu: f32, gv: f32, duvx: vec2<f32>, duvy: vec2<f32>, r: f32) -> f32 {
    let kind = i32(clamp(round(S.paper5.y), 0.0, 4.0));
    let tn = paper_layers(kind, -1.0, uv, pn, gu, gv, duvx, duvy, S.paper2.y, 1.0, S.paper5.x);
    // Calmer up close (E051): the amplitude rises from paper_near_amp at the eye to 1 at
    // paper_near_dist metres, smoothly, so walking up to a wall the paper quietens instead
    // of turning to embossed stucco.
    let env = mix(S.paper6.y, 1.0, smoothstep(0.0, max(S.paper6.x, 1e-3), r));
    return S.paper2.x * tn * 0.35 * env;
}

// Far pencil strokes (E057, approach B): a procedural stroke field in the surface frame,
// tonal-art-map style. Lines of strokes run along direction `ang`; along each line, strokes of
// 6 to 14 line-pitches start at random offsets, each with its own rank, pressure and slight
// sideways offset, tapering at both ends (short at the start, long at the end). A stroke
// is drawn once the tone `tm` passes its rank, so strokes appear one by one as it darkens,
// and its pressure rises with tone: grey graphite first, near ink only at the end. The
// line pitch is held at about S.paper9.z * 4 pixels (720 lines) by octaves, blended over
// the octave; the octave-(k+1) lines are every other line of octave k, so the coarser
// octave's strokes are a subset and the blend thins the field rather than swapping it.
// Returns ink coverage 0..1.
fn far_stroke_level(a: f32, b: f32, pitch: f32, gb_px: f32, tm: f32, seed: u32, k: u32) -> f32 {
    let jl = floor(b / pitch + 0.5);
    var cov = 0.0;
    // The nearest line and its neighbours (strokes wander a little across).
    for (var dj = -1; dj <= 1; dj++) {
        let j = jl + f32(dj);
        // Line identity in octave-0 units, so octaves share strokes.
        let jid = i32(j) << k;
        let lk = hash2(seed, bitcast<u32>(jid));
        let len = pitch * 10.0;
        let off = u01(lk) * len;
        let si = floor((a + off) / len);
        for (var ds = 0; ds <= 1; ds++) {
            let sidx = si - f32(ds);
            let h = hash2(lk, bitcast<u32>(i32(sidx)));
            let rank = u01(h);
            if (rank > tm * 1.15) {
                continue;
            }
            let slen = len * (0.6 + 0.8 * u01(pcg(h ^ 3u)));
            let s0 = sidx * len - off + len * 0.3 * u01(pcg(h ^ 5u));
            let t = (a - s0) / slen;
            if (t < 0.0 || t > 1.0) {
                continue;
            }
            let centre = j * pitch + pitch * 0.35 * (u01(pcg(h ^ 7u)) - 0.5) + pitch * 0.15 * (t - 0.5) * (u01(pcg(h ^ 11u)) - 0.5);
            let taper = smoothstep(0.0, 0.12, t) * (1.0 - smoothstep(0.55, 1.0, t));
            let w_px = max(1.1 * G.misc.x * (0.7 + 0.6 * u01(pcg(h ^ 13u))) * taper, 0.0);
            let d_px = abs(b - centre) / max(gb_px, 1e-9);
            let c = saturate(0.5 * w_px - d_px + 0.5) * saturate(w_px);
            // Pressure: rises with how far the tone is past this stroke's rank, and overall.
            let press = mix(0.2, 1.0, pow(saturate(tm), 1.4)) * mix(0.5, 1.0, saturate((tm * 1.15 - rank) * 4.0)) * (0.75 + 0.25 * u01(pcg(h ^ 17u)));
            cov = max(cov, c * press);
        }
    }
    return cov;
}

fn far_strokes(uv: vec2<f32>, ang: f32, lod_u: f32, lod_v: f32, duvx: vec2<f32>, duvy: vec2<f32>, tm: f32, seed: u32) -> f32 {
    let ca = cos(ang);
    let sa = sin(ang);
    // a along the strokes, b across them.
    let a = ca * uv.x + sa * uv.y;
    let b = -sa * uv.x + ca * uv.y;
    // Rotation-invariant footprint of b (for the octave), true screen derivative (for AA).
    let foot_b = length(vec2<f32>(sa * lod_u, ca * lod_v));
    let gb = max(length(vec2<f32>(-sa * duvx.x + ca * duvx.y, -sa * duvy.x + ca * duvy.y)), 1e-7);
    let base = 0.01;
    let target_px = max(S.paper9.z, 0.5) * 4.0 * G.misc.x;
    let kf = max(log2(max(target_px * foot_b / base, 1e-6)), 0.0);
    let k0 = floor(kf);
    let fr = kf - k0;
    let p0 = base * exp2(k0);
    let c0 = far_stroke_level(a, b, p0, gb, tm, seed, u32(k0));
    let c1 = far_stroke_level(a, b, 2.0 * p0, gb, tm, seed, u32(k0) + 1u);
    return mix(c0, c1, fr);
}

// Pencil shading coverage (E059) at level L (0 none .. 4 darkest) from the hand-shaded tiles
// (paper.rs: clusters of strokes with drifting direction, uneven spacing, overlapping
// passes; level k holds every stroke of level k - 1). Fixed world scales, each present while
// its texel is S.paper11.y to 4 S.paper11.y pixels at 720 lines on screen, with triangular
// weights that sum to 1 (coverage is a mean, not zero-mean), each at its own offset and a
// rotation drifting +-20 degrees about the pencil direction. Between levels the coverage is
// mixed, which only fades strokes in, never swaps them.
fn shade_cov(L: f32, uv: vec2<f32>, pn: f32, gu: f32, gv: f32, duvx: vec2<f32>, duvy: vec2<f32>) -> f32 {
    let gmax = max(gu, gv);
    let aniso = smoothstep(0.08, 0.2, min(gu, gv) / max(gmax, 1e-9));
    if (aniso <= 0.0 || L <= 0.0) {
        return 0.0;
    }
    let plane = pcg(bitcast<u32>(i32(floor(pn * 4.0))) ^ 0xb5297a4du);
    let lo = max(S.paper11.y, 0.1);
    let foot = min(gu, gv) * G.misc.x;
    let base = 0.0005;
    let k0 = max(floor(log2(lo * foot / base)), 0.0);
    let fl = min(floor(L), 3.0);
    let fr = saturate(L - fl);
    var cov = 0.0;
    for (var j = 0; j < 3; j++) {
        let k = k0 + f32(j);
        let c = base * exp2(k);
        let t = log2(max(c / max(foot, 1e-9), 1e-9) / lo);
        let w = max(1.0 - abs(t - 1.0), 0.0);
        if (w > 0.0) {
            let h = hash2(0x51633e2du ^ plane, u32(k));
            let a = S.paper7.w + (u01(h) - 0.5) * 0.7;
            let rot = mat2x2<f32>(vec2<f32>(cos(a), sin(a)), vec2<f32>(-sin(a), cos(a)));
            let q = vec2<f32>(cos(a) * uv.x - sin(a) * uv.y, sin(a) * uv.x + cos(a) * uv.y) / c;
            let st = q / 512.0 + vec2<f32>(u01(pcg(h ^ 1u)), u01(pcg(h ^ 2u)));
            let gx = (rot * duvx / c) / 512.0;
            let gy = (rot * duvy / c) / 512.0;
            let hi = textureSampleGrad(shade_tex, paper_smp, st, i32(fl), gx, gy).x;
            var lo_c = 0.0;
            if (fl >= 1.0) {
                lo_c = textureSampleGrad(shade_tex, paper_smp, st, i32(fl) - 1, gx, gy).x;
            }
            // L in (fl, fl + 1]: from level fl (layer fl - 1) to level fl + 1 (layer fl).
            cov += w * mix(lo_c, hi, fr);
        }
    }
    return cov * aniso;
}

// The relief of paper kind `kind` in the surface layers, about unit standard deviation where
// all layers are present. angle < 0: each layer at its own random rotation (no direction);
// angle >= 0: every layer at that angle in the surface frame (one direction, for the pencil
// layer). Layer weights are an energy-preserving window in log2 of the on-screen texel size
// (E051): w = sin(pi/2 t) over the first octave of the window and cos(pi/2 (t - 1)) over
// the second, t = log2(px / lo), so the two layers present at any distance always have
// w_a^2 + w_b^2 = 1 and contrast is the same at every distance (the E044 window left
// octave-periodic dips to about 0.7, which showed as blotchy zones on a wall seen at an
// angle). Faded by the world cap S.paper5.x over two octaves and on grazing surfaces.
fn paper_layers(kind: i32, angle: f32, uv: vec2<f32>, pn: f32, gu: f32, gv: f32, duvx: vec2<f32>, duvy: vec2<f32>, lo_px: f32, stretch: f32, cap: f32) -> f32 {
    let gmax = max(gu, gv);
    let aniso = smoothstep(0.08, 0.2, min(gu, gv) / max(gmax, 1e-9));
    if (aniso <= 0.0) {
        return 0.0;
    }
    let plane = pcg(bitcast<u32>(i32(floor(pn * 4.0))) ^ u32(kind) * 0x9e3779b9u);
    let lo = max(lo_px, 0.1);
    let foot = min(gu, gv) * G.misc.x;
    // Width over height of each relief (mega_render::paper::PAPER_ASPECT).
    var aspects = array<f32, 5>(1.0, 1.7066667, 0.7753906, 1.0, 1.0);
    let aspect = aspects[kind];
    let k0 = max(floor(log2(lo * foot / S.paper4.x)), 0.0);
    var tooth = 0.0;
    for (var j = 0; j < 3; j++) {
        let k = k0 + f32(j);
        let c = S.paper4.x * exp2(k);
        let t = log2(max(c / max(foot, 1e-9), 1e-9) / lo);
        var w = 0.0;
        if (t > 0.0 && t < 1.0) {
            w = sin(1.5707963 * t);
        } else if (t >= 1.0 && t < 2.0) {
            w = cos(1.5707963 * (t - 1.0));
        }
        w *= 1.0 - smoothstep(0.25 * cap, cap, c);
        if (w > 0.0) {
            let h = hash2(0x5851f42du ^ plane, u32(k));
            let a = select(u01(h) * 6.2831853, angle, angle >= 0.0);
            let q = vec2<f32>(cos(a) * uv.x - sin(a) * uv.y, sin(a) * uv.x + cos(a) * uv.y) / c;
            // stretch > 1 lengthens the relief along the layer's x axis (E054: far pencil
            // strokes from Paper004's laid lines).
            let sa = vec2<f32>(1.0 / (512.0 * aspect * max(stretch, 1.0)), 1.0 / 512.0);
            let st = q * sa + vec2<f32>(u01(pcg(h ^ 1u)), u01(pcg(h ^ 2u)));
            let rot = mat2x2<f32>(vec2<f32>(cos(a), sin(a)), vec2<f32>(-sin(a), cos(a)));
            let gx = (rot * duvx / c) * sa;
            let gy = (rot * duvy / c) * sa;
            let g = (textureSampleGrad(paper_tex, paper_smp, st, kind, gx, gy).x - 0.5) * 4.0;
            tooth += w * g;
        }
    }
    return tooth * aniso;
}

// ---------------------------------------------------------------------------------------
// Soft cast-shadow edge (E041). A penumbra whose width on the receiver grows with the
// distance from the occluder, as from a sun of finite size, exaggerated: width =
// S.paper3.y * (receiver to blocker distance), capped at S.paper3.z metres. The blocker
// distance comes from a search in the scene-wide cascade; the penumbra is then filtered in
// the finest cascade where its radius is at most four texels, so a wide penumbra is a smooth
// gradient rather than sparse taps. Contact stays tight down to S.paper4.y metres, the same
// in every cascade, so a shadow does not change character where it crosses a cascade.
fn soft_sun(p: vec3<f32>, n: vec3<f32>, s: vec3<f32>, hard: f32, hard_cascade: f32, dpx: vec3<f32>, dpy: vec3<f32>) -> f32 {
    let l3 = cascade_lookup(3u, p, n, s);
    if (l3.w <= 0.0 || l3.z <= 0.0 || l3.z >= 1.0) {
        return hard;
    }
    let size = shadow_size();
    let t3 = cascade_texel(3u, p);
    let wmax = S.paper3.z;
    let rs = max(0.5 * wmax / t3, 1.5);
    let plane3 = cascade_plane(3u, dpx, dpy, cascade_w(3u, p));
    // 3x3 taps, each read as the four texels around it with bilinear weights, so the
    // estimate moves continuously with the receiver instead of stepping at texel edges
    // (stepping showed as faint staircase lines inside the penumbra).
    var bsum = 0.0;
    var bn = 0.0;
    let slack = 2.0 * abs(dot(plane3.xy, vec2<f32>(1.0) / size)) + 1e-5;
    let maxc = vec2<i32>(i32(size) - 1);
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let o = vec2<f32>(f32(i), f32(j)) * rs;
            let zr = l3.z + dot(plane3.xy, o / size);
            let st = l3.xy * size + o - 0.5;
            let b0 = floor(st);
            let f = st - b0;
            for (var k = 0; k < 4; k++) {
                let d = vec2<i32>(k & 1, k >> 1);
                let wt = select(1.0 - f.x, f.x, d.x == 1) * select(1.0 - f.y, f.y, d.y == 1);
                let z = textureLoad(shadow_tex, clamp(vec2<i32>(b0) + d, vec2<i32>(0), maxc), 3, 0);
                if (z < zr - slack) {
                    bsum += wt * (zr - z);
                    bn += wt;
                }
            }
        }
    }
    if (bn < 0.05) {
        return hard;
    }
    let dz_per_m = abs((SH.mats[3] * vec4<f32>(s, 0.0)).z);
    let dist = (bsum / bn) / max(dz_per_m, 1e-9);
    // At least shadow_soft_min metres wide (E044): without a floor, contact penumbrae
    // narrower than a texel were drawn soft in a fine cascade and hard in a coarse one, so
    // one shadow switched from soft to hard and serrated at a cascade boundary.
    let w = clamp(S.paper3.y * dist, S.paper4.y, wmax);
    // The finest cascade containing p in which the penumbra's radius is at most 4 texels,
    // blended into the next coarser one as the radius approaches that limit, so the switch
    // of cascade (which follows the penumbra width, and so the noisy blocker estimate) is
    // never a seam.
    let margin = 3.0 / size;
    var ci = 3u;
    for (var i = 0u; i < 3u; i++) {
        let l = cascade_lookup(i, p, n, s);
        if (l.w > margin && l.z > 0.0 && 0.5 * w / cascade_texel(i, p) <= 4.0) {
            ci = i;
            break;
        }
    }
    let r = 0.5 * w / cascade_texel(ci, p);
    var v = soft_pcf(ci, p, n, s, r, dpx, dpy);
    let toward = select(0.0, smoothstep(2.0, 4.0, r), ci < 3u);
    if (toward > 0.0) {
        let r2 = 0.5 * w / cascade_texel(ci + 1u, p);
        v = mix(v, soft_pcf(ci + 1u, p, n, s, r2, dpx, dpy), toward);
    }
    return v;
}

// Tent-weighted 5x5 comparison taps spread over a radius of r texels in cascade ci, each
// against the receiver's own plane.
fn soft_pcf(ci: u32, p: vec3<f32>, n: vec3<f32>, s: vec3<f32>, r: f32, dpx: vec3<f32>, dpy: vec3<f32>) -> f32 {
    let size = shadow_size();
    let l = cascade_lookup(ci, p, n, s);
    let plane = cascade_plane(ci, dpx, dpy, cascade_w(ci, p));
    var v = 0.0;
    var ws = 0.0;
    for (var j = -2; j <= 2; j++) {
        for (var i = -2; i <= 2; i++) {
            let o = vec2<f32>(f32(i), f32(j)) * 0.5 * max(r, 0.5) / size;
            let wt = f32(3 - abs(i)) * f32(3 - abs(j));
            v += wt * textureSampleCompareLevel(shadow_tex, shadow_cmp, l.xy + o, i32(ci), l.z + dot(plane.xy, o));
            ws += wt;
        }
    }
    return v / ws;
}

// Derivative along v of a quantity with screen derivatives (lx, ly), given the screen
// derivatives of the coordinates u and v: solves [ux vx; uy vy] [Lu Lv] = [lx ly].
fn grad_along(ux: f32, uy: f32, vx: f32, vy: f32, lx: f32, ly: f32) -> f32 {
    let det = ux * vy - vx * uy;
    if (abs(det) < 1e-14) {
        return 0.0;
    }
    return abs((ux * ly - uy * lx) / det);
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
    lodv1: f32,
    lgrad1: f32,
    // Crosshatch layer, at its own angle.
    u3: f32,
    v3: f32,
    g3: f32,
    gv3: f32,
    lod3: f32,
    lodv3: f32,
    lgrad3: f32,
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
    // Shadow strokes (drawn with SH_GATE set) have their own salt and gap fraction.
    let gap1 = select(S.hatch2.y, S.feather2.x, SH_GATE != 0u);
    let salt1 = select(0x68e31da4u, 0x7c3a91e5u, SH_GATE != 0u);
    let h1 = hatch_layer(LayerIn(f.u1, f.u2, f.g1, f.g2, f.lod1, f.lodv1, f.lgrad1, salt1, light, S.bands.x, S.bands.z, f.edge, 1.0, 1.0, gap1));
    var c2 = 0.0;
    if (light <= S.bands.y + margin) {
        c2 = hatch_layer(LayerIn(f.u3, f.v3, f.g3, f.gv3, f.lod3, f.lodv3, f.lgrad3, 0xb5297a4du, light, S.bands.y, S.bands.z, f.edge, S.cross.x, S.cross.y, S.white.w)).cov;
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
    // Rotation-invariant footprints, so a speck stays the same speck as the camera turns.
    let gg = vec2<f32>(f.lod1, f.lodv1);
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
    let duv_x = vec2<f32>(dpdx(u1), dpdx(u2));
    let duv_y = vec2<f32>(dpdy(u1), dpdy(u2));
    let g2 = max(length(vec2<f32>(dpdx(u2), dpdy(u2))), 1e-7);
    let g3 = max(length(vec2<f32>(dpdx(u3), dpdy(u3))), 1e-7);
    let gv3 = max(length(vec2<f32>(dpdx(v3), dpdy(v3))), 1e-7);
    // Rotation-invariant footprints (E020). Everything that decides what is drawn (octave,
    // stroke shapes, specks) uses these; only anti-aliasing uses the screen derivatives. The
    // screen derivative of a coordinate changes as the camera turns, because a rectilinear
    // image magnifies towards its edges (about 1.4 octaves at the edge of a 70 degree view),
    // so choosing the octave from it made the pattern on a wall change as the view turned.
    // Instead: the pixel is a fixed angle a (that of the image centre), and a displacement on
    // the surface is seen from the eye at distance r along the ray d. The rate at which
    // coordinate dot(P, e) changes per pixel is then r * a * |e - n (e.d) / (n.d)|, the
    // derivative a pixel at the image centre would see. It depends only on the eye position
    // and the surface.
    let to_p = p - G.camera_pos.xyz;
    let rr = length(to_p);
    let dv = to_p / max(rr, 1e-6);
    let nd_raw = dot(n, dv);
    let nd = select(-1.0, 1.0, nd_raw >= 0.0) * max(abs(nd_raw), 0.02);
    let ra = rr * G.misc.y;
    let lod_u1 = max(ra * length(perp - n * (dot(perp, dv) / nd)), 1e-7);
    let lod_v1 = max(ra * length(ax - n * (dot(ax, dv) / nd)), 1e-7);
    let lod_u3 = max(ra * length(e2 - n * (dot(e2, dv) / nd)), 1e-7);
    let lod_v3 = max(ra * length(d2 - n * (dot(d2, dv) / nd)), 1e-7);
    // Diagnostic alternatives for the octave experiments (E002, E020). Coverage still uses
    // the true derivative; only the octave choice changes.
    let iso = length(fwidth(p)) * 0.7071;
    var lod1 = lod_u1;
    var lod3 = lod_u3;
    if (G.flags.z == 1u) {
        lod1 = iso;
        lod3 = iso;
    } else if (G.flags.z == 2u) {
        lod1 = 0.0;
        lod3 = 0.0;
    } else if (G.flags.z == 3u) {
        lod1 = g1;
        lod3 = g3;
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
    // One bounce of sun and sky light (baked per vertex, docs/features/bounced-light.md). It
    // reaches shaded and cast-shadowed surfaces alike, so it is added after the shadow
    // clamp; 0 with both strengths 0, which leaves the light exactly as before.
    let bounced = S.bounce.x * S.light.x * G.sun_params.y * in.bounce.x + S.bounce.y * S.light.y * in.bounce.y;
    var lit = (S.light.x * G.sun_params.y * max(ndl, 0.0) * sun_vis + ambient + noise) * shade + bounced;
    let dark = clamp((ambient * S.shadow.x + noise) * shade, S.shadow.y, S.shadow.z) + bounced;
    // Smooth part of the light only: the shadow edge has its own anti-aliasing below.
    let edge = max(fwidth(lit), 0.004);

    // Sun shadow: the filtered visibility is turned into a hard edge at its 0.5 iso-line,
    // anti-aliased over one pixel from its own screen gradient.
    let sv = sun_visibility(p, n, s, dpdx(p), dpdy(p));
    let gsv = length(vec2<f32>(dpdx(sv.x), dpdy(sv.x)));
    var vis = select(step(0.5, sv.x), saturate((sv.x - 0.5) / gsv + 0.5), gsv > 1e-5);
    let dpx_p = dpdx(p);
    let dpy_p = dpdy(p);
    if (S.paper3.y > 0.0 && SH.params.x > 0.5 && ndl > 0.05) {
        vis = soft_sun(p, n, s, vis, sv.y, dpx_p, dpy_p);
    }
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

    // Light gradient along each layer's lines (per metre), for feathering band edges:
    // solve the screen derivatives of the light for its derivative along v.
    let lx = dpdx(lit);
    let ly = dpdy(lit);
    let lgrad1 = grad_along(dpdx(u1), dpdy(u1), dpdx(u2), dpdy(u2), lx, ly);
    let lgrad3 = grad_along(dpdx(u3), dpdy(u3), dpdx(v3), dpdy(v3), lx, ly);
    let f = Frag(u1, u2, g1, g2, lod1, lod_v1, lgrad1, u3, v3, g3, gv3, lod3, lod_v3, lgrad3, edge);

    // Inside a cast shadow the hatching may run in its own direction: along the sun's
    // direction projected onto the surface (S.feather.w), so a shadow on a hatched wall
    // still separates from it. The direction is constant per face for a remote sun.
    var sax = ax;
    let sp = s - n * ndl;
    if (S.feather.w > 0.5 && length(sp) > 0.2) {
        sax = normalize(sp);
    }
    let sperp = normalize(cross(n, sax));
    let su1 = dot(p, sperp);
    let su2 = dot(p, sax);
    let sg1 = max(length(vec2<f32>(dpdx(su1), dpdy(su1))), 1e-7);
    let sg2 = max(length(vec2<f32>(dpdx(su2), dpdy(su2))), 1e-7);
    var slod1 = max(ra * length(sperp - n * (dot(sperp, dv) / nd)), 1e-7);
    let slodv1 = max(ra * length(sax - n * (dot(sax, dv) / nd)), 1e-7);
    if (G.flags.z == 3u) {
        slod1 = sg1;
    }
    let fsh = Frag(su1, su2, sg1, sg2, slod1, slodv1, 0.0, u3, v3, g3, gv3, lod3, lod_v3, 0.0, edge);

    // The lit side is drawn where the fragment is lit; the shadow side's strokes are each
    // gated by line_shadow, at the fragment (crisp edge) or at a point moved along the
    // stroke (feathered edge), so with feathering shadow strokes run over the edge into the
    // lit side or stop short of it.
    FP = p;
    FN = n;
    FS = s;
    FDIR = sax;
    FAMT = amount;
    FDPX = dpdx(p);
    FDPY = dpdy(p);
    var near_shadow = amount > 0.0;
    if (!near_shadow && S.feather.z > 0.0 && S.fog2.y > 0.0 && facing > 0.5) {
        near_shadow = sun_vis_cheap(p + sax * S.feather.z) < 0.99 || sun_vis_cheap(p - sax * S.feather.z) < 0.99;
    }
    // Continuous tone models: the light, darkened with depth into the structure (ao) and
    // with distance from the eye, then the ruled and wash tone curves.
    let ruled = S.rule.x > 0.5;
    var darkf = 1.0;
    if (S.fall.w > 0.0) {
        darkf *= pow(max(ao, 1e-3), S.fall.w);
    }
    if (S.dark.y > 0.0) {
        darkf *= exp(-max(rr - S.dark.x, 0.0) / S.dark.y);
    }
    if (S.wash3.w > 0.0) {
        darkf *= exp(-max(G.camera_pos.y - p.y, 0.0) / S.wash3.w);
    }
    let lit_t = lit * darkf;
    let dark_t = dark_l * darkf;
    // Ruling as a distance effect (S.far, E035): the ruling reads the light darkened further
    // with effective distance, and its coverage is gated off near the eye, so near surfaces,
    // even in deep shade or cast shadow, are carried by the wash alone. Effective distance
    // counts depth below the eye extra, so looking down a shaft gathers ruling sooner than
    // looking across it. Functions of the eye position only: rigid under rotation.
    var rgate = 1.0;
    var rdark = 1.0;
    if (S.far.y > 0.0 || S.far.w > 0.0) {
        let de = rr + S.far.z * max(G.camera_pos.y - p.y, 0.0);
        if (S.far.y > 0.0) {
            rgate = smoothstep(S.far.x, S.far.x + S.far.y, de);
        }
        if (S.far.w > 0.0) {
            rdark = exp(-max(de - S.far.x, 0.0) / S.far.w);
        }
    }
    let c_lit = fall_curve(lit_t * rdark) * rgate;
    let c_dark = fall_curve(dark_t * rdark) * rgate;
    let cg_lit = grad_along_signed(dpdx(u1), dpdy(u1), dpdx(u2), dpdy(u2), dpdx(c_lit), dpdy(c_lit));
    let cg_dark = grad_along_signed(dpdx(su1), dpdy(su1), dpdx(su2), dpdy(su2), dpdx(c_dark), dpdy(c_dark));

    // Distance shading mode (E054): 0 ruling, 1 the tone that drove the ruling goes into
    // the wash instead, with pencil strokes gathering on it.
    let far_pencil = S.paper9.x > 0.5;
    var t: Tone;
    if (ruled) {
        if (S.rule3.w > 0.5) {
            FNEAR = saturate(shadow_nearby(p, n, s) * 3.0);
        }
        // The ruled widths need a real footprint, so the octave diagnostics are applied in
        // rule_layer rather than by zeroing it.
        let rl = RuleIn(u1, u2, g1, g2, lod_u1, lod_v1, cg_lit, 0x68e31da4u, c_lit);
        var cov = 0.0;
        if (!far_pencil) {
            cov = rule_layer(rl) * (1.0 - amount);
        }
        var cblack = smoothstep(0.85, 0.985, c_lit) * (1.0 - amount);
        if (far_pencil) {
            // Distance shading as wash and pencil (E054): no ruled strokes; the black of the
            // merge follows the far tone, so specks and white lines still appear in it.
            cblack = smoothstep(0.85, 0.985, mix(c_lit, c_dark, amount));
        } else if (near_shadow) {
            SH_GATE = 1u;
            // Shadow strokes are heavy where the occluder is (towards the sun along the
            // surface) and trail off away from it, as a shadow hangs from what casts it.
            let toward = dot(sax, sp);
            let cg_sh = cg_dark + select(0.0, -sign(toward) * 10.0, abs(toward) > 0.05);
            let rd = RuleIn(su1, su2, sg1, sg2, max(ra * length(sperp - n * (dot(sperp, dv) / nd)), 1e-7), slodv1, cg_sh, 0x7c3a91e5u, c_dark);
            cov = max(cov, rule_layer(rd));
            SH_GATE = 0u;
            cblack = max(cblack, smoothstep(0.85, 0.985, c_dark) * amount * FNEAR);
        }
        t.cov = cov;
        t.black = cblack;
    } else if (amount >= 1.0 && S.feather.z <= 0.0) {
        t = tone_ink(fsh, dark_l);
    } else {
        t = tone_ink(f, lit);
        t.cov *= 1.0 - amount;
        t.black *= 1.0 - amount;
        if (near_shadow) {
            SH_GATE = 1u;
            let b = tone_ink(fsh, dark_l);
            SH_GATE = 0u;
            // Solid black inside the shadow (if its light is that low) follows the fragment.
            let bb = b.black * amount;
            t.cov = max(t.cov, max(b.cov * select(1.0, amount, b.black >= 1.0), bb));
            t.black = max(t.black, bb);
        }
    }
    let light = mix(lit, dark_l, amount);

    // An ink line along the cast shadow's edge (its anti-aliased 0.5 iso-line), rigid
    // because the edge is.
    if (S.rule3.z > 0.0 && gsv > 1e-5 && facing > 0.5 && S.fog2.y > 0.0) {
        let dpx = (sv.x - 0.5) / gsv;
        t.cov = max(t.cov, tent_cov(abs(dpx), S.rule3.z * G.misc.x));
    }

    var ph = 0.0;
    if (S.paper3.w > 0.5 && S.paper2.x > 0.0) {
        ph = paper_surface(vec2<f32>(u1, u2), dot(p, n), lod_u1, lod_v1, duv_x, duv_y, rr);
    }
    // Wash, printed partly as screentone.
    var wash = 0.0;
    if (S.wash.x > 0.0) {
        var w0 = mix(wash_curve(lit_t), wash_curve(dark_t), amount);
        // The light end (E062): above wash_white the light no longer clips to paper but
        // prints a pale wash that thins along a shoulder to paper only at light_white (the
        // sun square on a surface), so lit and sky-lit walls keep their modelling.
        // light_wash is the wash at wash_white; it fades out below wash_white, so the dark
        // end is untouched. light_far adds a faint pale tint with distance on top.
        if (S.paper13.x > 0.0 || S.paper14.x > 0.0) {
            // Direct sun counts S.paper13.w times more at the light end than in the mid
            // tones (it is far brighter than the sky in reality), so sunlit faces still reach
            // paper and the shadows on them keep their contrast; sky-lit faces go pale.
            let sun_direct = S.light.x * G.sun_params.y * max(ndl, 0.0) * sun_vis;
            let pale = mix(light_end(lit_t + S.paper13.w * sun_direct), light_end(dark_t), amount);
            let far = S.paper14.x * smoothstep(S.paper14.y, S.paper14.z, rr);
            w0 = 1.0 - (1.0 - w0) * (1.0 - pale) * (1.0 - far);
        }
        // Far tone into the wash (E054): the light darkened with distance and depth below the
        // eye, gated near the eye (rule_near, rule_fade_in, rule_down_gain, rule_dist_len),
        // deepens the wash towards solid ink at the bottom of the shaft.
        let cf = select(0.0, mix(c_lit, c_dark, amount), far_pencil && ruled);
        if (cf > 0.0) {
            w0 = 1.0 - (1.0 - w0) * (1.0 - cf * S.paper10.y);
        }
        let tt = screentone_t(in.clip.xy, dv, p, vec2<f32>(u1, u2), vec2<f32>(lod1, lod_v1), dot(p, n));
        // The threshold's ramp: one pixel of the threshold map (hard, two-level grain), or
        // with screentone_soft a ramp that wide in threshold units, so the grain becomes a
        // continuous modulation of the grey with no hard edges (E036).
        let ht = saturate((w0 - tt) / max(max(fwidth(tt), 0.05), S.grain.x) + 0.5);
        wash = mix(w0, ht, S.wash2.y);
        // Blacks (E066): where the total light is very low (below S.paper15.x plus a range
        // S.paper15.y that varies by patch), the wash closes to solid ink and the fragment
        // counts as solid black, so contours and strokes turn white over it and specks
        // appear. The open shaft's light never gets this low (its cast-shadow light is
        // clamped well above it), so its shadows stay grey.
        if (S.paper15.y > 0.0) {
            let lb = mix(lit_t, dark_t, amount);
            let tb = 0.65 * vnoise3(0x27d4eb2fu, p / 3.0) + 0.35 * vnoise3(0x165667b1u, p / 1.3);
            let edge = S.paper15.x + S.paper15.y * mix(1.0, 0.4 + 1.2 * tb, S.paper15.z);
            let bk = 1.0 - smoothstep(S.paper15.x, max(edge, S.paper15.x + 1e-3), lb);
            wash = 1.0 - (1.0 - wash) * (1.0 - bk);
            // Lines turn white over solid black and over wash dark enough that black lines would
            // vanish into it (the near-black of sealed corridors), so the walls, floor edges and
            // doors stay drawn.
            SPECK_BLACK = max(t.black, smoothstep(0.6, 0.95, bk));
            t.black = max(t.black, max(smoothstep(0.35, 0.8, bk), smoothstep(0.72, 0.85, wash)));
        }
        if (S.halftone.x > 0.0) {
            // Faded on foreshortened surfaces, where dots would be squeezed into lines.
            let fs = saturate(2.0 * min(lod_u1, lod_v1) / max(max(lod_u1, lod_v1), 1e-9));
            wash = mix(wash, halftone(wash, vec2<f32>(u1, u2), max(lod_u1, lod_v1)), fs);
        }
        // Pencil shading in shade (E052): a second paper relief (S.paper6.w, Paper004's laid
        // lines by default) in one direction on the surface, darkening and lightening the wash
        // along its lines, coming in with the darkness of the wash from S.paper7.x to
        // S.paper7.y; S.paper7.z adds the same relief at right angles in the darkest tone
        // (soft crosshatching). Zero mean, so tone is kept; 0 strength leaves the wash as is.
        if (S.paper6.z > 0.0) {
            var band = smoothstep(S.paper7.x, S.paper7.y, w0);
            // Patches (E053, S.paper8.x > 0): the pencil arrives patch by patch rather than as
            // one front along the shadow's shape. A low-frequency world noise T (two octaves,
            // S.paper8.y metres) sets where in the tone range each spot starts; a spot is
            // drawn as the tone passes T, over a wide feathered ramp, so early patches appear in
            // light shade, more join as it darkens, and all are present in the deepest shade.
            // Fixed in the world, faded only by amplitude: still under walking.
            if (S.paper8.x > 0.0) {
                let ps = max(S.paper8.y, 0.1);
                let tn = 0.65 * vnoise3(0x7f4a7c15u, p / ps) + 0.35 * vnoise3(0x94d049bbu, p / (0.43 * ps));
                let thr = saturate((tn - 0.25) * 2.0);
                let pmask = smoothstep(thr - 0.25, thr + 0.25, band * 1.25);
                band = mix(band, band * pmask, S.paper8.x);
            }
            if (band > 0.0) {
                let pk = i32(clamp(round(S.paper6.w), 0.0, 4.0));
                let env = mix(S.paper6.y, 1.0, smoothstep(0.0, max(S.paper6.x, 1e-3), rr));
                var pr = paper_layers(pk, S.paper7.w, vec2<f32>(u1, u2), dot(p, n) + 17.0, lod_u1, lod_v1, duv_x, duv_y, S.paper2.y, 1.0, S.paper5.x);
                if (S.paper7.z > 0.0) {
                    var cb = smoothstep(S.paper7.y, 1.0, w0) * S.paper7.z;
                    if (S.paper8.x > 0.0) {
                        // The second direction only in some patches.
                        let t2 = vnoise3(0x2c1b3c6du, p / max(S.paper8.y, 0.1));
                        cb *= mix(1.0, smoothstep(0.45, 0.65, t2), S.paper8.x);
                    }
                    if (cb > 0.0) {
                        pr += cb * paper_layers(pk, S.paper7.w + 1.5707963, vec2<f32>(u1, u2), dot(p, n) + 31.0, lod_u1, lod_v1, duv_x, duv_y, S.paper2.y, 1.0, S.paper5.x);
                    }
                }
                wash = saturate(wash + S.paper6.z * band * env * 0.25 * pr);
            }
        }
        // Far pencil (E054, built up like pencil since E057). The marks follow their own tone,
        // tm, which starts much earlier than the wash's far darkening: the same light darkened
        // with distance and depth (rdark, gated by rgate) through a curve from far_white
        // (S.paper10.z) to fall_black. Three passes of the stretched laid-line relief, at
        // slightly different angles and sizes, enter at tm 0, 0.3 and 0.55. In each pass the
        // threshold on the relief slides down as its tone rises, so at first only the highest
        // ridges pass (a few thin strokes), later most of them; a stroke's darkness is how far
        // its ridge clears the threshold, capped low early (light grey graphite) and rising
        // to near ink only at the end. Passes combine like graphite layers, 1 - prod(1 - d),
        // so overlaps darken. Patches enter progressively over a wide range of tone.
        if (far_pencil && ruled && S.paper9.y > 0.0) {
            let lf = mix(lit_t, dark_t, amount) * rdark;
            let fw = max(S.paper10.z, S.fall.y + 0.01);
            let tm = rgate * pow(saturate((fw - lf) / (fw - S.fall.y)), 1.2);
            if (tm > 0.0) {
                let ps = max(S.paper8.y, 0.1) * 2.0;
                let tn = 0.65 * vnoise3(0x1b873593u, p / ps) + 0.35 * vnoise3(0xcc9e2d51u, p / (0.43 * ps));
                let pm = smoothstep(tn - 0.35, tn + 0.35, tm * 1.6);
                // Two passes: the main direction, and a second 20 degrees round entering at
                // tm 0.5; they combine like graphite layers.
                let tmp = tm * mix(pm, 1.0, tm * tm);
                var keep = 1.0 - saturate(S.paper9.y * far_strokes(vec2<f32>(u1, u2), S.paper7.w, lod_u1, lod_v1, duv_x, duv_y, tmp, 0x6c078965u));
                let t2 = saturate((tmp - 0.5) / 0.5);
                if (t2 > 0.0) {
                    keep *= 1.0 - saturate(S.paper9.y * far_strokes(vec2<f32>(u1, u2), S.paper7.w + 0.35, lod_u1, lod_v1, duv_x, duv_y, t2, 0x9908b0dfu));
                }
                wash = 1.0 - (1.0 - wash) * keep;
                // Occasional light gaps in the black, from the relief's troughs.
                if (cf > 0.8) {
                    let g = paper_layers(3, S.paper7.w, vec2<f32>(u1, u2), dot(p, n) + 91.0, lod_u1, lod_v1, duv_x, duv_y, S.paper9.z, S.paper9.w, 1e6);
                    wash = saturate(wash - 0.35 * smoothstep(0.8, 1.0, cf) * smoothstep(1.4, 2.2, -g));
                }
            }
        }
    }

    // Pencil shading from the hand-shaded tile (E059), driven by darkness from every source
    // (the wash, which carries light, shadow, enclosure and the far fall, plus enclosure
    // weighted by S.paper12.x), laid in patches that arrive at different darknesses, grey on
    // grey, and catching the paper's tooth (S.paper12.y).
    if (S.paper11.x > 0.0 && S.wash.x > 0.0) {
        let dk = saturate(wash + S.paper12.x * (1.0 - ao));
        var band = smoothstep(S.paper11.z, S.paper11.w, dk);
        if (band > 0.0) {
            let ps = max(S.paper8.y, 0.1);
            let tn = 0.65 * vnoise3(0x7f4a7c15u, p / ps) + 0.35 * vnoise3(0x94d049bbu, p / (0.43 * ps));
            let thr = saturate((tn - 0.25) * 2.0);
            band *= mix(1.0, smoothstep(thr - 0.3, thr + 0.3, band * 1.3), S.paper8.x);
            let cov = shade_cov(4.0 * band, vec2<f32>(u1, u2), dot(p, n) + 7.0, lod_u1, lod_v1, duv_x, duv_y);
            let tooth = clamp(1.0 + S.paper12.y * ph / max(S.paper2.x * 0.35, 1e-4) * 0.5, 0.0, 2.0);
            wash = 1.0 - (1.0 - wash) * (1.0 - saturate(S.paper11.x * cov * tooth));
        }
    }

    let view = G.flags.x;
    let hatch_on = view == VIEW_FINAL || view == VIEW_HATCHING;
    var white = speckle(f, in.ids.x);
    if (S.white.x > 0.0 && t.black > 0.0) {
        // Sparse white rules in the black, along the first hatch direction.
        let wl = hatch_layer(LayerIn(u1, u2, g1, g2, lod1, lod_v1, 0.0, 0x3c6ef372u, 0.0, 1.0, 0.0, edge, S.white.y, 0.7, S.hatch2.y));
        white = max(white, wl.cov * S.white.x);
    }
    // White specks only inside solid black (E066: not over dark grey).
    var speck = white * select(t.black, SPECK_BLACK, S.paper15.y > 0.0);
    if (S.dark.z > 0.0 && S.wash.x > 0.0) {
        // White specks in dark wash.
        speck = max(speck, speckle(f, in.ids.x ^ 0x2545f491u) * smoothstep(0.3, 0.65, wash) * S.dark.z);
    }

    var out: FsOut;
    out.id = in.ids;
    out.normal = oct_encode(n);
    // Palette index 0 (the only one in black and white), band, ao, solid black coverage
    // (read by the composite to turn lines white inside black).
    out.fill = vec4<f32>(0.0, band_of(light) / 3.0, ao, select(0.0, t.black, hatch_on));
    out.ink = vec4<f32>(select(0.0, t.cov, hatch_on), select(0.0, speck, hatch_on), 0.0, 0.0);
    out.tone = vec2<f32>(select(0.0, wash, hatch_on), 0.5 + 2.0 * ph);
    if (view == VIEW_SHADOW) {
        out.fill = vec4<f32>(0.0, sv.y / 4.0, ao, 0.0);
        out.ink = vec4<f32>(amount, 0.0, 0.0, 0.0);
        out.tone = vec2<f32>(0.0, 0.5);
    }
    return out;
}
