# Ink renderer

## Status

In progress, 2026-11-15. Milestone M0 frame working on the test scene: forward MRT pass with ruled hatching and tone bands, stroke pass, contour pass with its own anti-aliasing resolve, composite with atmosphere, nine debug views. Black-and-white presets `nihei` (test scene) and `nihei_vast` (the One Shaft, M1; see `one-shaft.md`). Not yet built: shadow maps, sky, light shafts, screentone, the compute cull prepass, world-keyed contour wobble, colour presets.

## What it is

The frame that turns the generator's triangles and ink strokes into a page that should read as hand-inked: ruled hatching lying on surfaces, blacks pooling in recesses, generator strokes and screen-space contours merged into one black-ink channel, and distance that fades to a paper-coloured haze instead of turning to noise. All tunables live in one `StylePreset`.

## Design

- **Everything that is drawn lives on a surface or a stroke, not on the screen.** Hatch lines are functions of world position, strokes are world segments, so ink moves with geometry. The only screen-anchored element is the optional paper grain.
- **Tone is quantised, line weight is not.** A light value is cut into four bands (white, hatch, crosshatch, black). Inside a band, line weight grows towards the next darker band, so hatching thickens into solid black instead of stopping at a clean contour. Band edges are jittered per hatch line, so a band boundary is made of ragged line ends.
- **Line density is held in screen space by octaves.** Hatch spacing doubles per octave; the octave is chosen from the screen derivative of the hatch coordinate itself, so a grazing floor gets the octave its lines actually need. Octave `k+1`'s lines are the even lines of octave `k`, so crossfading fades only the odd lines and nothing jumps.
- **One black-ink channel.** Hatching and solid black (forward pass), strokes and contours are all max-blended into ink R, so a contour that coincides with a stroke merges with it instead of darkening it.
- **Analytic coverage everywhere, plus one targeted filter.** Hatch lines and strokes compute their own filtered coverage. Contours come from a binary id/depth test, so they get a small directional resolve filter of their own instead of a post-process over the whole image. No TAA, no SMAA or FXAA on the composite (not needed so far; see E005).
- **Distance simplifies.** Fine-id contours fade out between 25 and 60 m, strokes fade by rank with distance and with projected length, hatching keeps its pixel pitch and fades to its mean tone past the last octave, and the atmosphere fades tone and ink together.
- **Diagnosis is built in.** Debug views isolate each contribution, and `mega_capture --metrics` measures temporal stability by reprojection (see `capture-tool.md` and E001).

## Technical details

### The frame

| # | Pass | Kind | Reads | Writes |
|---|---|---|---|---|
| 0 | CPU stroke cull | CPU | strokes, camera, preset | `visible` index list (storage buffer) |
| 1 | Forward | raster, MRT, depth test | vertices | depth, id, normal, fill, ink |
| 2 | Contours | fullscreen fragment | id, depth, normal | contour mask |
| 3 | Ink lines | two draws in one pass, max blend | `visible`, strokes, depth; contour mask | ink |
| 3a | Strokes | instanced quads, 6 vertices per stroke, vertex pulling | | ink R (G for white strokes) |
| 3b | Contour resolve | fullscreen fragment | contour mask | ink R |
| 4 | Composite | fullscreen fragment | fill, ink, depth (id, normal for debug) | caller's colour target |

The contour pass is a fullscreen fragment pass, not compute. It writes a fragment per pixel, its only neighbourhood is 3x3, and a fragment pass can write the R8 mask and later max-blend into the `Rg8Unorm` ink target through fixed-function blending; compute would need a storage-texture write path and a separate merge. The split into detection (2) and resolve (3b) exists so the binary mask can be anti-aliased before it is merged (E005).

### Targets

| Target | Format | Contents |
|---|---|---|
| depth | `Depth32Float` | reversed-Z, infinite far plane; cleared to 0, test `Greater` |
| id | `Rg32Uint` | R `id_fine`, G `id_coarse`; 0 is background |
| normal | `Rg16Float` | octahedral world normal |
| fill | `Rgba8Unorm` | R palette index / 255 (always 0 in black and white), G tone band / 3, B ao, A flags / 255 (unused) |
| ink | `Rg8Unorm` | R black ink, G white ink |
| contour mask | `R8Unorm` | contour strength before the resolve |

All are recreated on `resize`. Only the composite writes to the caller's target, which may be any colour format; if it is sRGB the composite pre-linearises so the stored values stay display values.

### Light and bands

```
L = sun_strength * intensity * max(N . S, 0) * mix(1, ao, sun_occlusion) + ambient * ao
S = normalize(sun_position - P)     (positioned sun)
S = sun_direction                   (beyond 1e5 m from the camera)
band = 0 if L >= band_hatch, 1 if >= band_cross, 2 if >= band_black, else 3
```

No shadow term yet. `sun_occlusion` (0 in `nihei`, 1 in `nihei_vast`) scales the sun term by the baked ao, the only thing that keeps sunlight off the bottom of a 600 m shaft without shadows (E010). Packed in `bands.w`. The sun uniform already carries position, radius and intensity (`Sun` in `renderer.rs`, set with `Renderer::set_sun`), with the switch to a pure direction done on the CPU when the sun is farther than `Sun::DIRECTIONAL_DISTANCE`. The default sun is remote, about 32 degrees up, direction `(0.6, 0.53, -0.6)`.

### Ruled hatching

Per fragment, with face normal `N`, hatch axis `A` (from the vertex) and `B = normalize(N x A)`:

- Layer 1: lines run along `A`. Spacing coordinate `u = dot(P, B)`, along-line coordinate `v = dot(P, A)`. Present when the jittered light is below `band_hatch`.
- Layer 2 (crosshatch): lines run along `B`. `u = dot(P, A)`, `v = dot(P, B)`. Present below `band_cross`. Its target pitch is multiplied by `cross_pitch_scale` and its width by `cross_width_scale`.

Octave selection for one layer (`s = hatch_spacing`, `T = hatch_target_px * px_scale * pitch_scale`):

```
g    = length(vec2(dpdx(u), dpdy(u)))            metres per pixel across the lines
raw  = log2(T * g / s)
kf   = clamp(raw, 0, hatch_max_octave);  k0 = floor(kf);  f = kf - k0
s0   = s * 2^k0                                   octave-k0 spacing, pitch between T/2 and T pixels
even lines: u = j * 2 * s0   (also the lines of octave k0 + 1), weight 1
odd  lines: u = (2j + 1) * s0,                                   weight 1 - f
cov  = max(cov(nearest even), (1 - f) * cov(nearest odd))
```

With linear fading the mean ink density varies by at most 6% over an octave cycle (maximum of `(2 - f) 2^f / 2` at `f = 2 - 1/ln 2`), so no compensation is applied. Past `hatch_max_octave` the coverage is blended towards its mean `width / pitch * (1 - gap)` by `saturate(raw - kmax)`.

Each line has a stable identity: its index `n` in units of the finest spacing, which is the same whichever octave it is drawn in. Its level is `ctz(n)` (how many octaves it survives). Per line, from `hash(n)`:

- width `= hatch_line_px * px_scale * width_scale * (1 + (h - 0.5) * hatch_width_jitter) * (1 + hatch_width_gain * d)`, where `d = saturate((threshold - L) / (threshold - band_black))` is how far into the band the fragment is;
- breakup: 1D value noise along `v` with segment length `hatch_break_len * 2^level`; the line is absent where the noise is below `hatch_break_gap` (a short soft step, no width taper; see E007). Because the segment length scales with the line's level, lines that survive to coarse octaves (seen far away) break up at a similar screen scale. When the breakup is minified (`dv/dpixel * 2 > segment length`) the threshold is replaced by its mean;
- band jitter: `(noise(v / (band_jitter_len * 2^level)) - 0.5) * band_jitter` is added to `L` before the band test, so lines end raggedly near a band edge.

Coverage across a line is the overlap of the line with a tent filter of radius one pixel at distance `|u - u_line| / g` pixels (`tent_cov` in `common.wgsl`). This is analytic in the derivative and smoother than a box, which matters for thin diagonal lines (E006).

Solid black uses the same ragged edge: black where `L + jitter < band_black`, with the jitter of the strip the fragment lies in. That strip is the nearest octave-`k0` line blended towards the nearest even line by `f`, so it is continuous across octave changes.

Debug: `--hatch-lod isotropic|fixed` swaps the octave choice for `length(fwidth(P))` or octave 0 (`Renderer::set_hatch_lod`); coverage keeps using the true derivative.

### Strokes

`Stroke` records (68 bytes, declared with scalars and `array<f32, 3>` in WGSL) stay in one storage buffer. Each frame the CPU writes the indices of strokes that pass the cull into a second storage buffer, and the stroke pass draws `6 x visible` vertices with no vertex buffers. A compute cull can later write the same list plus indirect arguments without changing the shader.

CPU cull: reject when both endpoints are outside the same side plane (with a 2% margin) or behind the near plane, or when the closest point of the segment is farther than `rank_fade_end[rank]`.

Vertex shader: project both ends; if one is behind `1.01 * near`, clip the segment in clip space at that w and mark that end as not real (no taper or overshoot there). Expand to a screen-space rectangle around the clipped segment, half-width `0.5 * max(width at either end * (1 + noise_amp), min_px) + 1`, extended along the segment by `overshoot + 1` pixels. The quad carries the screen endpoints, their clip w, the stroke parameter at each, and flags.

Fragment shader, for pixel centre `x`:

```
s       = dot(x - p0, dir)                   along the segment, pixels
d       = |cross(x - p0, dir)|               across, pixels
sf      = clamp(s / len, 0, 1)
z       = 1 / mix(1/w0, 1/w1, sf)            perspective-correct view depth at the centreline point
t       = mix(t0/w0, t1/w1, sf) * z          stroke parameter
width   = weight * stroke_width_px * px_scale * min((depth_ref / z)^depth_exp, falloff_max)
        * mix(taper_min, 1, smoothstep(0, taper_px * px_scale, distance to the nearer real end))
        * (1 + noise_amp * (2 * noise(seed, t * length / noise_len) - 1))
alpha_w = saturate(width / stroke_min_px);  width = max(width, stroke_min_px)
cov     = tent_cov(d, width) * box overlap of the pixel with [-overshoot, len + overshoot] along s
```

Visibility: take the farthest scene depth (smallest reversed-Z value) in the 3x3 pixel block around the centreline point `p0 + dir * clamp(s, 0, len)`, linearise it to `zs`, and fade the fragment out between `zs * (1 + tol)` and `zs * (1 + 2 tol)`. Sampling at the centreline makes the whole width of a line agree; the 3x3 maximum absorbs the depth slope of grazing faces next to a crease. The cost is that a stroke behind an occluder can show for one pixel at the occluder's silhouette, where the contour is drawn anyway (E003). There is no hardware depth test and no bias, so there is no z-fighting.

Distance and length: `alpha *= 1 - smoothstep(rank_fade_start[rank], rank_fade_end[rank], z)` per fragment (so a long receding stroke fades along its length), and `alpha *= smoothstep(min_len / 2, min_len, projected length)` with `min_len = rank_min_len_px[rank] * px_scale`.

White strokes (`StrokeKind::White`) write ink G instead of R. Debug: `--stroke-vis centreline|pixel` (`Renderer::set_stroke_visibility`).

### Contours

Contour pass, per pixel `p`:

- **Ids.** Roberts cross between `p`, `p+(1,1)` and `p+(1,0)`, `p+(0,1)`. A coarse-id difference is an edge of strength `contour_strength`. A fine-id difference has strength `contour_fine_strength * (1 - smoothstep(contour_fine_near, contour_fine_far, z))`, using the nearest depth of the four pixels; beyond `contour_fine_far` only coarse ids are compared. Fine edges are weaker because inside one part they mostly coincide with seam strokes.
- **Depth.** Sobel on linear depth over 3x3, divided by `8 * z_centre` (relative step per pixel). A plane at incidence `N.V` produces a relative step of about `radians_per_pixel / |N.V|`, so the threshold is `contour_depth_k * radians_per_pixel / max(|N.V|, contour_nv_min)` and the edge is `smoothstep(thr, 2 thr, rel)`. Grazing floors therefore raise their own threshold instead of lighting up. Only the nearer side of a step is marked, so silhouettes sit on the occluder.
- **Distance.** The result is multiplied by `1 - smoothstep(contour_fade_start, contour_fade_end, z)`.

Contour resolve: the 3x3 structure tensor of the mask gradient gives the line orientation and a coherence `(l1 - l2) / (l1 + l2)`. The mask is averaged with bilinear taps at -3..3 pixels along the tangent, and the result is `mix(centre, average, coherence)`. A stair-stepped line spreads over the two rows it alternates between, like MLAA without the run-length search; corners and junctions keep the raw mask. The result is max-blended into ink R.

### Composite

```
dist  = euclidean distance from the camera (from depth and the pixel ray, so it does not change as the view turns)
f     = fog_max * (1 - exp(-max(dist - fog_start, 0) * fog_density))      (fog_max for background)
fog   = mix(fog_colour, ink, saturate(fog_ao * (1 - ao)))                (ao = 1 for background)
base  = mix(paper, fog, f)
c     = mix(base, ink, ink.R * (1 - f))
c     = mix(c, paper, ink.G * (1 - f))
c    *= 1 + (hash(pixel) - 0.5) * paper_grain
```

The fog takes the light level of the surface behind it: `fog_ao` (0 in `nihei`, 1.5 in `nihei_vast`, packed in `fog.w`) darkens it where the baked ao is low, so an abyss fades to dark grey rather than to paper (E008). The fill target's palette index and band are written but not yet used by the final view; the band is shown by the `bands` debug view. A colour preset would look the palette index up here.

### Pixel scale

Pixel-sized preset values are for a 720-pixel-high image and are multiplied by `px_scale = height / 720`, so a 1440p capture is the same drawing with a finer pen. `stroke_min_px` is the exception: it is a floor in real pixels.

### StylePreset fields (nihei values)

| Field | nihei | Meaning |
|---|---|---|
| `paper` | 0.94, 0.925, 0.89 | Off-white paper |
| `ink` | 0.07, 0.065, 0.06 | Off-black ink |
| `paper_grain` | 0.035 | Screen-anchored grain strength |
| `fog_colour` | 0.86, 0.85, 0.82 | Tone distance fades to |
| `fog_start` | 25 m | Fade starts |
| `fog_density` | 0.006 /m | Exponential density past the start |
| `fog_max` | 0.85 | Largest fade |
| `fog_ao` | 0 | Fog darkening towards ink by `1 - ao` |
| `sun_strength` | 0.5 | Weight of the sun term |
| `ambient` | 0.5 | Weight of `ao` |
| `sun_occlusion` | 0 | Sun term scaled by `mix(1, ao, x)` |
| `band_hatch` | 0.62 | Light below this is hatched |
| `band_cross` | 0.45 | Below this, crosshatched |
| `band_black` | 0.30 | Below this, solid black |
| `band_jitter` | 0.09 | Per-line light offset amplitude |
| `band_jitter_len` | 0.6 m | Its wavelength along the line at level 0 |
| `hatch_spacing` | 0.03 m | Octave-0 line spacing |
| `hatch_target_px` | 4.5 px | Pitch the octave choice aims for |
| `hatch_line_px` | 1.1 px | Hatch line width |
| `hatch_width_jitter` | 0.5 | Relative width variation per line |
| `hatch_break_len` | 0.35 m | Breakup segment length at level 0 |
| `hatch_break_gap` | 0.22 | Fraction of a line lost to gaps |
| `hatch_max_octave` | 12 | Last octave before fading to mean tone |
| `hatch_width_gain` | 0.8 | Width growth towards the next darker band |
| `cross_pitch_scale` | 1.6 | Crosshatch pitch relative to layer 1 |
| `cross_width_scale` | 0.8 | Crosshatch width relative to layer 1 |
| `stroke_width_px` | 1.6 px | Weight-1 stroke width at `stroke_depth_ref` |
| `stroke_min_px` | 1.25 px | Minimum width (real pixels); coverage fades below it |
| `stroke_depth_ref` | 6 m | Distance of falloff 1 |
| `stroke_depth_exp` | 0.6 | Falloff exponent |
| `stroke_falloff_max` | 1.6 | Cap on near widening |
| `stroke_taper_px` | 18 px | Taper length at real ends |
| `stroke_taper_min` | 0.3 | Width at the very end |
| `stroke_noise_amp` | 0.25 | Seeded width noise |
| `stroke_noise_len` | 1.5 m | Its wavelength |
| `stroke_overshoot_px` | 1.5 px | Run-on past real ends |
| `stroke_depth_tol` | 0.005 | Relative visibility tolerance (E003) |
| `rank_fade_start` | 400, 120, 35, 15 m | Per rank 0..3 |
| `rank_fade_end` | 800, 220, 70, 30 m | Per rank; also the CPU cull distance |
| `rank_min_len_px` | 2, 4, 8, 12 px | Per rank projected-length fade |
| `contour_fine_near` | 25 m | Fine-id edges at full strength nearer than this |
| `contour_fine_far` | 60 m | Fine-id edges gone beyond this |
| `contour_depth_k` | 6 | Depth edge threshold in planar-step units |
| `contour_nv_min` | 0.08 | Floor on |N.V| in that threshold |
| `contour_strength` | 0.9 | Coarse-id and depth edge strength |
| `contour_fine_strength` | 0.45 | Fine-id edge strength |
| `contour_fade_start` | 150 m | Contours start fading |
| `contour_fade_end` | 400 m | Contours gone |

### The `nihei_vast` preset

`StylePreset::nihei_vast()` is `nihei` with these changes, for spaces hundreds of metres deep. `StylePreset::for_scene("shaft")` returns it; `by_name` and `NAMES` list both presets.

| Field | nihei_vast | Why |
|---|---|---|
| `fog_start`, `fog_density`, `fog_max` | 40 m, 0.0026 /m, 0.8 | Structure 300 to 600 m away must stay legible (about 60% faded at 600 m) |
| `fog_ao` | 1.5 | The abyss fades to dark, not to paper |
| `ambient` | 0.75 | Walls near the rim can reach white; tone comes from baked openness |
| `sun_strength` | 0.4 | |
| `sun_occlusion` | 1 | No sunlit floors at the bottom of the shaft |
| `band_cross` | 0.36 | Crosshatch only in a narrow band above black: on walls seen at grazing angles up or down the shaft it read as a grid. Darkness comes from vertical rules thickening into black, as in the references |
| `hatch_width_gain` | 1.0 | Idem |
| `rank_fade_start` | 500, 140, 40, 16 m | |
| `rank_fade_end` | 900, 260, 80, 32 m | Rank 0 (massing, bridge decks, shaft corners) draws the whole 600 m; panel seams are gone by 80 m |
| `rank_min_len_px` | 3, 5, 8, 12 px | |
| `contour_fade_start`, `contour_fade_end` | 220 m, 650 m | Coarse-id contours outline elements all the way down the shaft |

See E008 for the comparison with `nihei` on the shaft.

### Debug views

Selected with `Renderer::set_view(DebugView)` or `mega_capture --view <name>`.

| Name | Shows |
|---|---|
| `final` | The frame |
| `id` | `id_fine` hashed to grey; background white |
| `normals` | World normal as colour |
| `depth` | log2 distance, 1 m white to 1 km black |
| `bands` | Tone band: white, light grey, dark grey, black (no jitter) |
| `hatching` | Forward-pass ink only (hatching and solid black), no atmosphere |
| `strokes` | Stroke pass only, no atmosphere |
| `contours` | Contours only (after the resolve), no atmosphere |
| `lines` | Final without hatching: strokes and contours with atmosphere and grain |

### Measured (Apple M3 Max, Metal)

Frame time from `mega_capture --bench 200` (wall clock over back-to-back submits, so it includes the CPU cull and submission; an upper bound on GPU time): 0.47 to 0.51 ms at 1280x720 and 1.56 to 1.66 ms at 2560x1440 over `corridor_walk`, `still_hall` and `hall_orbit`, with 575 to 771 of 1568 strokes drawn.

Temporal stability (`--metrics`, 40 frames over a slow stretch of each path; blurred reprojected MAD per unit of mean ink, lower is steadier; see E001 for the method):

| Path, stretch | final | hatching | strokes | contours | lines |
|---|---|---|---|---|---|
| `corridor_walk` t 0.05..0.062 | 0.020 | 0.019 | 0.005 | 0.006 | 0.011 |
| `corridor_strafe` t 0.4..0.6 | 0.012 | 0.011 | 0.003 | 0.006 | 0.008 |
| `hall_orbit` t 0..0.012 | 0.022 | 0.020 | 0.010 | 0.024 | 0.024 |

### Files

- `crates/render/src/style.rs`: `StylePreset`, `nihei()`, `nihei_vast()`, `for_scene`, `by_name`, GPU packing.
- `crates/render/src/renderer.rs`: targets, pipelines, CPU stroke cull, `DebugView`, `Sun`, `HatchLod`, `StrokeVisibility`.
- `crates/render/shaders/common.wgsl`: uniforms, hashes, noise, filtered coverage, octahedral encoding, fullscreen triangle. Prepended to every pass.
- `crates/render/shaders/forward.wgsl`, `strokes.wgsl`, `contours.wgsl`, `contour_resolve.wgsl`, `composite.wgsl`: the passes.
- `crates/capture/src/main.rs`: `--view`, `--metrics`, `--bench` and the diagnostic switches.

## Open questions

- Tone band edges followed per-vertex AO interpolated over 3 to 4 m panels. For generated scenes the kit now tessellates faces to 1.5 to 2 m and bakes a smooth AO (E009, `kit-of-parts.md`); the test scene is unchanged. Parts narrower than the tessellation still cannot occlude cleanly.
- Without shadows, `sun_occlusion` and `fog_ao` are stand-ins that read the baked ao. A shadow pass would replace the first and could feed the second.
- Whether the hatch octave crossfade should be eased (it is linear) after watching video at 60 fps; the metrics do not show octave banding.
- Hatching on large lit-from-nowhere interiors reads as graph paper when the floor is lit and panelled; this is a scene and shadow question more than a renderer one, to revisit with shadows.
- Contour depth edges at silhouettes are one pixel and binary before the resolve; in orbiting views they are the least stable contribution (0.75% of compared pixels off by more than 64 levels). A sub-pixel silhouette estimate from the depth planes would help.
- World-keyed contour wobble (planned) is not implemented.
- The compute cull prepass should write the `visible` list and indirect arguments; nothing in the stroke shader needs to change.

### Review notes, 2026-11-15 (Cat, from the windowed test scene)

- Liked: the crosshatching, especially where it gathers in corners as ambient occlusion would.
- Disliked: the image is plain overall, and has the look that comics converted to 3D tend to have.
- Working diagnosis of that look, to be tested as experiments with side-by-side sheets in `compare/`:
  1. Every surface gets equal treatment; a draughtsman draws detail in patches and leaves the rest blank.
  2. Lines are too correct: straight, meeting exactly at corners, uniform in weight.
  3. Tone boundaries sit where a renderer puts them; in the references blacks are placed as shapes.
  4. The crosshatch is an exact 90 degree mesh.
  5. Lines glide with perfect coherence in motion.
- Seen by accident and liked: from outside the test scene, back-face culling removes the walls but seam strokes still draw, giving a coarse grid you can see through. A candidate for a deliberate cutaway in the exterior view.

## Related experiments

E001 (temporal metric), E002 (hatch octave selection), E003 (stroke depth tolerance), E004 (distance behaviour of lines), E005 (contour anti-aliasing), E006 (tent versus box coverage), E007 (tone and lighting for the test scene), E008 (distance on the 600 m shaft), E010 (tone and light for the shaft).
