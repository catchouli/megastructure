// Composite: paper, minus black ink, plus white ink, atmosphere fade; or a debug view.
// common.wgsl is prepended.

@group(1) @binding(0) var fill_tex: texture_2d<f32>;
@group(1) @binding(1) var ink_tex: texture_2d<f32>;
@group(1) @binding(2) var depth_tex: texture_depth_2d;
@group(1) @binding(3) var id_tex: texture_2d<u32>;
@group(1) @binding(4) var normal_tex: texture_2d<f32>;

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
    let ink = textureLoad(ink_tex, p, 0).xy;
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
            c = mix(S.paper.rgb, S.ink.rgb, ink.x);
            c = mix(c, S.paper.rgb, ink.y);
        }
        default: {
            // Final and lines: atmosphere pushes tone to the fog colour and attenuates ink by
            // the same factor.
            let f = select(saturate(1.0 - exp(-max(dist - S.fog.x, 0.0) * S.fog.y)) * S.fog.z, S.fog.z, sky);
            let base = mix(S.paper.rgb, S.fog_colour.rgb, f);
            c = mix(base, S.ink.rgb, ink.x * (1.0 - f));
            c = mix(c, S.paper.rgb, ink.y * (1.0 - f));
            // Screen-anchored paper grain.
            let grain = u01(hash2(bitcast<u32>(p.x), bitcast<u32>(p.y))) - 0.5;
            c = c * (1.0 + grain * S.paper.a);
        }
    }
    return vec4<f32>(to_srgb_if_needed(saturate(c)), 1.0);
}
