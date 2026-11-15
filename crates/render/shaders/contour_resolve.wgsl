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
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec2<f32> {
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
    let e = mix(centre, smoothed, coherence);
    return vec2<f32>(e, 0.0);
}
