// Composite: paper, minus black ink, plus white ink, atmosphere fade; or a debug view.
// common.wgsl is prepended.

@group(1) @binding(0) var fill_tex: texture_2d<f32>;
@group(1) @binding(1) var ink_tex: texture_2d<f32>;
@group(1) @binding(2) var depth_tex: texture_depth_2d;
@group(1) @binding(3) var id_tex: texture_2d<u32>;
@group(1) @binding(4) var normal_tex: texture_2d<f32>;
@group(1) @binding(5) var tone_tex: texture_2d<f32>;

// Screen-anchored paper texture in 0..1: two octaves of value noise at fibre scale
// (S.wash3.z pixels at 720 lines). Screen-anchored on purpose: it is the paper, not the
// drawing. Only used when S.wash3.y > 0.
fn paper_noise(px: vec2<f32>) -> f32 {
    let sc = max(S.wash3.z * G.misc.x, 0.5);
    let a = vnoise3(0x68bc21ebu, vec3<f32>(px / sc, 0.5));
    let b = vnoise3(0x02e5be93u, vec3<f32>(px / (sc * 0.37), 0.5));
    let c = vnoise3(0x967a889bu, vec3<f32>(vec2<f32>(px.x / (sc * 4.0), px.y / sc), 0.5));
    return saturate(0.45 * a + 0.3 * b + 0.25 * c);
}

fn to_srgb_if_needed(c: vec3<f32>) -> vec3<f32> {
    // Shader values are display values; an sRGB target would re-encode them, so undo that.
    if (G.flags.y == 1u) {
        return pow(c, vec3<f32>(2.2));
    }
    return c;
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(pos.xy));
    let d = textureLoad(depth_tex, p, 0);
    let sky = d <= 0.0;
    let ray = pixel_ray(pos.xy);
    // Euclidean distance, so the fade does not change as the view turns.
    let dist = select(linear_depth(d) / max(dot(ray, G.camera_fwd.xyz), 1e-3), 1e7, sky);
    let fill = textureLoad(fill_tex, p, 0);
    // R hatching and solid black, G white ink, B lines (strokes and contours), A the major
    // ones among them. Major lines that fall inside solid black turn white by S.blacks.w, so
    // structure stays legible there; minor ones (panel seams) vanish into the black.
    let ink4 = textureLoad(ink_tex, p, 0);
    // Wash (continuous tone models): grey ink coverage under the lines. Paper texture makes
    // the paper uneven and the wash granulate, strongest in mid greys.
    var wash = select(textureLoad(tone_tex, p, 0).x, 0.0, sky);
    var paper = S.paper.rgb;
    if (S.wash3.y > 0.0) {
        let pn = paper_noise(pos.xy) - 0.5;
        paper = paper * (1.0 + S.wash3.y * pn);
        wash = saturate(wash + S.wash3.y * 6.0 * pn * wash * (1.0 - wash));
    }
    let inv = fill.w * S.blacks.w;
    let ink = vec2<f32>(max(ink4.x, ink4.z * (1.0 - inv)), max(ink4.y, ink4.w * inv));
    let view = G.flags.x;

    var c: vec3<f32>;
    switch view {
        case VIEW_ID: {
            let id = textureLoad(id_tex, p, 0).x;
            c = vec3<f32>(select(0.15 + 0.85 * u01(pcg(id)), 1.0, id == 0u));
        }
        case VIEW_NORMALS: {
            c = select(oct_decode(textureLoad(normal_tex, p, 0).xy) * 0.5 + 0.5, vec3<f32>(0.0), sky);
        }
        case VIEW_DEPTH: {
            // log2 distance, 1 m white to 1024 m black.
            c = vec3<f32>(1.0 - saturate(log2(max(dist, 1.0)) / 10.0));
        }
        case VIEW_BANDS: {
            c = select(vec3<f32>(1.0 - fill.y), vec3<f32>(1.0, 0.8, 0.8), sky);
        }
        case VIEW_HATCHING, VIEW_STROKES, VIEW_CONTOURS: {
            // Ink on paper with no atmosphere, to see distance behaviour unmasked.
            c = mix(mix(paper, S.ink.rgb, wash), S.ink.rgb, ink.x);
            c = mix(c, paper, ink.y);
        }
        case VIEW_SHADOW: {
            // Cascade tint (0 white, 1 pale blue, 2 pale green, 3 pale red, beyond grey)
            // under the shadow amount in ink.
            let k = fill.y * 4.0;
            var tint = vec3<f32>(1.0);
            if (k >= 0.5 && k < 1.5) { tint = vec3<f32>(0.8, 0.88, 1.0); }
            if (k >= 1.5 && k < 2.5) { tint = vec3<f32>(0.82, 1.0, 0.82); }
            if (k >= 2.5 && k < 3.5) { tint = vec3<f32>(1.0, 0.84, 0.84); }
            if (k >= 3.5) { tint = vec3<f32>(0.7); }
            c = select(mix(tint, vec3<f32>(0.1), ink4.x), vec3<f32>(1.0, 0.95, 0.7), sky);
        }
        default: {
            // Final and lines: atmosphere pushes tone to the fog colour and attenuates ink by
            // the same factor.
            let f = select(saturate(1.0 - exp(-max(dist - S.fog.x, 0.0) * S.fog.y)) * S.fog.z, S.fog.z, sky);
            // Fog takes the light level of the surface behind it (sky counts as open).
            let ao = select(fill.z, 1.0, sky);
            let fog = mix(S.fog_colour.rgb, S.ink.rgb, saturate(max(S.fog.w * (1.0 - ao), S.fog2.x * fill.w)));
            var base = mix(mix(paper, S.ink.rgb, wash), fog, f);
            if (sky && S.tone.w >= 0.0) {
                // A flat sky tone instead of the fogged paper.
                base = mix(S.paper.rgb, S.ink.rgb, S.tone.w);
            }
            c = mix(base, S.ink.rgb, ink.x * (1.0 - f));
            c = mix(c, paper, ink.y * (1.0 - f));
            // Screen-anchored paper grain.
            let grain = u01(hash2(bitcast<u32>(p.x), bitcast<u32>(p.y))) - 0.5;
            c = c * (1.0 + grain * S.paper.a);
        }
    }
    return vec4<f32>(to_srgb_if_needed(saturate(c)), 1.0);
}
