// Contour resolve: anti-aliases the binary contour mask along the local line direction and
// max-blends it into ink R. The orientation comes from the structure tensor of the mask's
// gradient over a 3x3 block; the mask is then averaged along the tangent, which spreads a
// stair-stepped line over the two rows it alternates between (the idea behind MLAA, without
// the run-length search). Where there is no single direction (corners, junctions) the
// original mask is kept. common.wgsl is prepended.

@group(1) @binding(0) var mask_tex: texture_2d<f32>;

fn mask_at(p: vec2<i32>) -> f32 {
    let dims = vec2<i32>(textureDimensions(mask_tex));
    return textureLoad(mask_tex, clamp(p, vec2<i32>(0), dims - 1), 0).r;
}

fn mask_bilinear(x: vec2<f32>) -> f32 {
    let f = x - 0.5;
    let i = vec2<i32>(floor(f));
    let t = f - floor(f);
    let a = mix(mask_at(i), mask_at(i + vec2<i32>(1, 0)), t.x);
    let b = mix(mask_at(i + vec2<i32>(0, 1)), mask_at(i + vec2<i32>(1, 1)), t.x);
    return mix(a, b, t.y);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(pos.xy));
    let centre = mask_at(p);
    var jxx = 0.0;
    var jxy = 0.0;
    var jyy = 0.0;
    var any = centre;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let q = p + vec2<i32>(dx, dy);
            let gx = mask_at(q + vec2<i32>(1, 0)) - mask_at(q - vec2<i32>(1, 0));
            let gy = mask_at(q + vec2<i32>(0, 1)) - mask_at(q - vec2<i32>(0, 1));
            jxx += gx * gx;
            jxy += gx * gy;
            jyy += gy * gy;
            any = max(any, mask_at(q));
        }
    }
    if (any <= 0.0) {
        discard;
    }
    // Gradient orientation; the line runs perpendicular to it.
    let theta = 0.5 * atan2(2.0 * jxy, jxx - jyy);
    let tangent = vec2<f32>(-sin(theta), cos(theta));
    let tr = jxx + jyy;
    let det_term = sqrt(max((jxx - jyy) * (jxx - jyy) + 4.0 * jxy * jxy, 0.0));
    let coherence = select(0.0, det_term / tr, tr > 1e-6);

    var sum = 0.0;
    for (var k = -3; k <= 3; k++) {
        sum += mask_bilinear(pos.xy + tangent * f32(k));
    }
    let smoothed = sum / 7.0;
    var e = mix(centre, smoothed, coherence);
    // Near-axis lines (E045): a line within about 10 degrees of vertical or horizontal is a
    // staircase with jogs many pixels apart, which the 3x3 tensor reads as exactly axial, so
    // the average above never spreads the jog, and the jogs crawl along the line as the
    // camera moves. Fit the line instead: its sub-pixel centre in each of the 17 rows (or
    // columns) around the pixel, a least-squares line through them, and redraw it with
    // box-filtered coverage at its mean width. Used only where every row has the line and the
    // fit is good, so ends, junctions and curves keep the result above.
    // Both axes are tried (the tensor's orientation is wrong exactly at a jog, where it
    // sees a corner); the better fit wins.
    let fv = axis_fit(p, true);
    let fh = axis_fit(p, false);
    let fit = select(fh, fv, fv.w >= fh.w);
    if (fit.w > 0.0) {
        e = mix(e, fit.x, fit.w);
    }
    return vec4<f32>(0.0, 0.0, e, e);
}

// Line fit along rows (vertical lines) or columns (horizontal ones). x: coverage at p from
// the fitted line, w: confidence 0..1 (0 when the fit does not apply).
fn axis_fit(p: vec2<i32>, vertical: bool) -> vec4<f32> {
    let along = select(vec2<i32>(1, 0), vec2<i32>(0, 1), vertical);
    let across = select(vec2<i32>(0, 1), vec2<i32>(1, 0), vertical);
    var sw = 0.0;
    var st = 0.0;
    var stt = 0.0;
    var sc = 0.0;
    var stc = 0.0;
    var width = 0.0;
    var cs: array<f32, 17>;
    for (var t = -8; t <= 8; t++) {
        var m = 0.0;
        var mc = 0.0;
        for (var c = -3; c <= 3; c++) {
            let v = mask_at(p + along * t + across * c);
            m += v;
            mc += v * f32(c);
        }
        if (m < 0.3 || m > 3.5) {
            return vec4<f32>(0.0);
        }
        let ctr = mc / m;
        cs[t + 8] = ctr;
        let ft = f32(t);
        sw += 1.0;
        st += ft;
        stt += ft * ft;
        sc += ctr;
        stc += ft * ctr;
        width += m;
    }
    let slope = (sw * stc - st * sc) / max(sw * stt - st * st, 1e-6);
    let c0 = (sc - slope * st) / sw;
    // Only near-axis lines (slope under 0.2 px per px, about 11 degrees), and only if the centres lie on the
    // line within 0.8 px (straight, single line).
    if (abs(slope) > 0.2) {
        return vec4<f32>(0.0);
    }
    var worst = 0.0;
    for (var t = -8; t <= 8; t++) {
        worst = max(worst, abs(cs[t + 8] - (c0 + slope * f32(t))));
    }
    if (worst > 0.8) {
        return vec4<f32>(0.0);
    }
    let w = width / sw;
    // The line's centre is at c0 across the pixel row; its strength is the mask's peak.
    let cov = box_cov(abs(c0), w) * cos(atan(slope));
    let conf = saturate((0.8 - worst) / 0.3) * (1.0 - smoothstep(0.14, 0.2, abs(slope)));
    return vec4<f32>(saturate(cov * max_strength(p, across)), 0.0, 0.0, conf);
}

fn max_strength(p: vec2<i32>, across: vec2<i32>) -> f32 {
    var m = 0.0;
    for (var c = -2; c <= 2; c++) {
        m = max(m, mask_at(p + across * c));
    }
    return max(m, 1e-3);
}
