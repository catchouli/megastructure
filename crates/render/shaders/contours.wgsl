// Contour pass, fullscreen fragment: Roberts cross on the id target plus a depth-relative
// Sobel on linear depth with an N.V-scaled threshold. Writes a one-channel contour mask;
// contour_resolve.wgsl anti-aliases it and max-blends it into ink R.
// common.wgsl is prepended.

@group(1) @binding(0) var id_tex: texture_2d<u32>;
@group(1) @binding(1) var depth_tex: texture_depth_2d;
@group(1) @binding(2) var normal_tex: texture_2d<f32>;

fn load_id(p: vec2<i32>, dims: vec2<i32>) -> vec2<u32> {
    return textureLoad(id_tex, clamp(p, vec2<i32>(0), dims - 1), 0).xy;
}

fn load_z(p: vec2<i32>, dims: vec2<i32>) -> f32 {
    let d = textureLoad(depth_tex, clamp(p, vec2<i32>(0), dims - 1), 0);
    return select(linear_depth(d), 1e7, d <= 0.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) f32 {
    let dims = vec2<i32>(textureDimensions(depth_tex));
    let p = vec2<i32>(floor(pos.xy));

    // 3x3 linear depth, row-major from top-left.
    var z: array<f32, 9>;
    for (var j = 0; j < 3; j++) {
        for (var i = 0; i < 3; i++) {
            z[j * 3 + i] = load_z(p + vec2<i32>(i - 1, j - 1), dims);
        }
    }
    let zc = z[4];

    // Roberts cross on ids, between this pixel and its +x, +y, +xy neighbours.
    let i00 = load_id(p, dims);
    let i11 = load_id(p + vec2<i32>(1, 1), dims);
    let i10 = load_id(p + vec2<i32>(1, 0), dims);
    let i01 = load_id(p + vec2<i32>(0, 1), dims);
    let zn = min(min(zc, z[8]), min(z[5], z[7]));
    let fine_w = 1.0 - smoothstep(S.contour.x, S.contour.y, zn);
    let e_coarse = i00.y != i11.y || i10.y != i01.y;
    let e_fine = i00.x != i11.x || i10.x != i01.x;
    let e_coarse_v = select(0.0, S.contour2.x, e_coarse);
    let e_fine_v = select(0.0, fine_w * S.contour2.w, e_fine);

    // Depth-relative Sobel. A plane seen at incidence N.V produces a relative depth step per
    // pixel of about (radians per pixel) / |N.V|, so the threshold scales with that and
    // grazing floors do not light up.
    let gx = (z[2] + 2.0 * z[5] + z[8]) - (z[0] + 2.0 * z[3] + z[6]);
    let gy = (z[6] + 2.0 * z[7] + z[8]) - (z[0] + 2.0 * z[1] + z[2]);
    let rel = length(vec2<f32>(gx, gy)) / (8.0 * zc);
    let n = oct_decode(textureLoad(normal_tex, p, 0).xy);
    let v = -pixel_ray(pos.xy);
    let nv = max(abs(dot(n, v)), S.contour.w);
    let thr = S.contour.z * G.misc.y / nv;
    // Only mark the nearer side of a depth step, so silhouettes hug the occluder.
    var zmin = zc;
    var zmax = zc;
    for (var k = 0; k < 9; k++) {
        zmin = min(zmin, z[k]);
        zmax = max(zmax, z[k]);
    }
    let near_side = select(0.0, 1.0, zmax - zc > zc - zmin);
    let e_depth = smoothstep(thr, 2.0 * thr, rel) * near_side;

    let fade = 1.0 - smoothstep(S.contour2.y, S.contour2.z, zn);
    let e = max(max(e_coarse_v, e_fine_v), e_depth * S.contour2.x) * fade;
    return e;
}
