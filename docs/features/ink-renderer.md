# Ink renderer

## Status

In progress, 2026-11-15. Milestone M0 frame working on the test scene: forward MRT pass with ruled hatching and tone bands, stroke pass, contour pass with its own anti-aliasing resolve, composite with atmosphere, ten debug views. Sun shadow pass (cascaded, orthographic or perspective, refitted every frame) feeding the light before band quantisation, and tone composition working on the One Shaft (E011 to E025): cast shadows drawn as dense hatching with a crisp or feathered edge, solid black only in recesses with white ink inside it, rotation-invariant hatching (E020), nested patch hatching with tapered, ragged ends, feathered tone boundaries, no crosshatch by default. Black-and-white presets `nihei` (test scene; shadows off) and `nihei_vast` (the One Shaft, M1; see `one-shaft.md`). Not yet built: the sky pass, light shafts, screentone, the far-field shadow ray march, the compute cull prepass, world-keyed contour wobble, colour presets.

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
| 0a | Cascade fit | CPU | camera, sun, scene bounds | four light matrices (`shadow::fit`) |
| 0b | Sun shadow | depth-only raster, four passes, no culling | vertex positions | shadow map array (four layers) |
| 1 | Forward | raster, MRT, depth test | vertices, shadow maps | depth, id, normal, fill, ink |
| 2 | Contours | fullscreen fragment | id, depth, normal | contour mask |
| 3 | Ink lines | two draws in one pass, max blend | `visible`, strokes, depth; contour mask | ink |
| 3a | Strokes | instanced quads, 6 vertices per stroke, vertex pulling | | ink B and A (G for white strokes) |
| 3b | Contour resolve | fullscreen fragment | contour mask | ink B and A |
| 4 | Composite | fullscreen fragment | fill, ink, depth (id, normal for debug) | caller's colour target |

The contour pass is a fullscreen fragment pass, not compute. It writes a fragment per pixel, its only neighbourhood is 3x3, and a fragment pass can write the R8 mask and later max-blend into the `Rg8Unorm` ink target through fixed-function blending; compute would need a storage-texture write path and a separate merge. The split into detection (2) and resolve (3b) exists so the binary mask can be anti-aliased before it is merged (E005).

### Targets

| Target | Format | Contents |
|---|---|---|
| depth | `Depth32Float` | reversed-Z, infinite far plane; cleared to 0, test `Greater` |
| id | `Rg32Uint` | R `id_fine`, G `id_coarse`; 0 is background |
| normal | `Rg16Float` | octahedral world normal |
| fill | `Rgba8Unorm` | R palette index / 255 (always 0 in black and white), G tone band / 3, B ao, A solid black coverage (read by the composite for white lines and `fog_black`) |
| ink | `Rgba8Unorm` | R black ink from the forward pass (hatching, solid black), G white ink (white strokes, specks, white rules), B line ink (strokes and contours), A the major lines among B (contours and strokes of rank up to `white_line_rank`) |
| sun shadow | `Depth32Float`, 2048 x 2048 x 4 layers | one cascade per layer; not resized with the frame |
| contour mask | `R8Unorm` | contour strength before the resolve |

All but the shadow maps are recreated on `resize`. The line passes write B and A rather than R so the composite can tell lines from hatching: where a line falls inside solid black, its major part (A) is drawn in white ink by `white_lines` and the rest (B) vanishes into the black. Only the composite writes to the caller's target, which may be any colour format; if it is sRGB the composite pre-linearises so the stored values stay display values.

### Light and bands

Two light values are computed per fragment, the light with the sun unobstructed and the light inside a shadow:

```
n       = 1 - underside_black * max(-N.y, 0)          (times smoothstep(pool_ao -+ pool_width, ao) if pool_ao > 0)
noise   = tone_noise * (2 vnoise3(P / tone_noise_len) - 1) + panel_tone * (2 hash(id_fine) - 1)
lit     = (sun_strength * intensity * max(N.S, 0) * mix(1, ao, sun_occlusion) + ambient * ao + noise) * n
dark    = clamp((ambient * ao * shadow_ambient + noise) * n, shadow_light_min, shadow_light_max)
facing  = 1 if N.S > 0.05 (per face; 1 for every face when there is no sun)
lit     = mix(lit, dark, shadow_attached * (1 - facing))        faces turned away: a light blend
amount  = (1 - vis) * sun_shadow * facing                       cast shadow, anti-aliased (below)
ink     = mix(tone(lit), tone(dark), amount)                    two drawings, blended only at the edge
band    = 0 if L >= band_hatch, 1 if >= band_cross, 2 if >= band_black, else 3
S = normalize(sun_position - P)     (positioned sun)
S = sun_direction                   (beyond 1e5 m from the camera)
```

`tone(L)` is the hatching and solid black for one light value (below). Drawing the lit and the shadowed side separately and blending their ink by the anti-aliased shadow mask gives an exact one-pixel edge whatever the two sides are drawn as; blending the light values instead would put thin strips of every band in between, an outline around each shadow. Both are computed only where `0 < amount < 1`, so the cost is at edges. `fwidth` for the band edges is taken from the smooth `lit` only. The sun uniform carries position, radius and intensity (`Sun`, set with `Renderer::set_sun`), switched to a pure direction on the CPU beyond `Sun::DIRECTIONAL_DISTANCE`; a remote sun below the horizon is uploaded with intensity 0. The default sun is remote, about 32 degrees up, direction `(0.6, 0.53, -0.6)`; the shaft's is 74 degrees up at azimuth -110 (E011).

### Shadow pass

`crates/render/src/shadow.rs` fits four cascades on the CPU in every `render`, so the sun may move every frame:

- **Cascades 0 to 2** follow the camera: spheres of radius 8, 30 and 110 m centred on the eye. They do not change as the camera turns (which cascade a point uses depends on its distance only; unit test `cascade_choice_does_not_depend_on_view_direction`), and for a remote sun the centre is snapped to whole texels in light space, so the maps do not shimmer as the camera moves (`camera_cascades_are_texel_snapped`). Texels are about 0.8, 2.9 and 10.7 cm. Until E020 they were spheres around slices of the view frustum, centred ahead of the camera: larger (1.1, 4.2 and 15 cm texels with the 70 degree view) and moving with the view direction.
- **Cascade 3** is fitted to the scene's bounds (from `MeshData::bounds` in `upload_scene`), so every shadow in the 600 m shaft is held at any range: about 12 cm texels for the shaft at the default sun, finer than a 720p pixel beyond about 60 m. With no bounds it falls back to the camera slice 110..1000 m.
- **Remote sun**: orthographic, depth range the scene bounds along the light (everything in the scene can cast). **Near sun** (closer than `Sun::DIRECTIONAL_DISTANCE`): perspective from the sun's position, a square frustum around each cascade's sphere (the last one around the scene bounds' sphere), near plane at the distance from the sun to the scene; half-angle capped at 1.2 rad for a sun inside or very near a cascade. No cube map.
- **Skipped** (no passes, visibility 1) when the intensity is 0, a remote sun is below the horizon, the preset's `sun_shadow` is 0 or there is no scene. `Renderer::shadows_drawn` and `shadow_cascades` report what happened.

Choice: a scene-fitted cascade over more camera cascades because the M1 scenes are a few hundred metres across and seen whole; one 2048 map over the bounds is finer than the pixel grid beyond 60 m, and the camera cascades cover the near field where the scene map is far too coarse (12 cm against a 1.5 cm pixel at 8 m). When the world is kilometres across a scene-fitted map is impossible: the camera cascades would extend to a few hundred metres, and beyond them the plan's ray march through the 32 m occupancy grid towards the sun takes over (not built, queued). Its hand-over needs the same thresholding as below, applied to the ray march's coverage, or the far field will have soft shadows next to hard near ones.

The pass draws the whole mesh into each layer with no face culling: generated walls are single-sided and must block the sun from both sides. Hardware slope-scaled bias 1.5.

Lookup in the forward pass, per fragment:

1. **Cascade**: the first whose light-space square contains the point with a 3-texel margin; within 0.1 NDC of its edge it is blended with the next one so the edge does not jump.
2. **Offsets**: normal offset `t * (1.5 + 2 sin(theta))` and `t` towards the light, `t` the cascade's texel size (times the distance to the sun for a perspective cascade), and with `shadow_jitter > 0` a world-anchored offset in the receiver's plane, `2 * shadow_jitter * (vnoise3(P / shadow_jitter_len) - 0.5)` on two axes across the light, which makes the edge slightly irregular, like a hand-cut edge. Keeping it in the receiver's plane matters: an offset off the surface on a face grazed by the light makes it shadow itself in blobs (E013).
3. **Filter**: 3x3 bilinear comparison taps weighted 1-2-1, an effective smooth 4x4-texel kernel, spread to the pixel's footprint in the map (up to 2 texels) where one pixel covers more than a texel. Each tap compares against the receiver's own depth at that tap (receiver-plane depth bias, from the screen derivatives of the world position), so a wide kernel on a face grazed by the light does not shadow itself.
4. **Edge**: the filtered visibility `v` is turned into a hard edge at its 0.5 iso-line, anti-aliased over one pixel from its own gradient: `vis = saturate((v - 0.5) / |grad v| + 0.5)`. The iso-line of a smooth kernel is a smooth curve, so the edge has no texel steps, and its position moves continuously as the sun moves (E012).

Debug: `--view shadow` shows the cast-shadow amount in black over a tint for the cascade used (white 0, blue 1, green 2, red 3); `--shadow-filter bilinear|hard` (`Renderer::set_shadow_filter`) replaces step 3 with one bilinear tap or one nearest-texel comparison.

Openings: nothing in the pass assumes closed geometry. A hole or break in a wall casts light down to about two texels of the cascade in use (the kernel is 4 texels wide and thresholded at half): about 2 cm near the camera, about 25 cm through a wall seen at 100 m or more. Thinner occluders (railings, under about one texel) cast dotted shadows at long range; that is the one artefact seen at the edges (E012).

### Drawing inside a cast shadow

The lit side is drawn with the surface's own hatching, multiplied by `1 - amount`. The shadow side is drawn as its own hatch layer with the shadow light (`shadow_light_min/max`, 0.33 in `nihei_vast`: dense, heavy single hatching), its own stroke identities and its own gap fraction (`shadow_break_gap`, nearly continuous strokes), and each of its strokes is gated:

- crisp (`shadow_feather` 0): by the fragment's own shadow amount, so the hatching stops exactly on the anti-aliased shadow edge;
- feathered (`shadow_feather` > 0, metres): by the shadow tested at a point moved along the stroke by the stroke's own random amount, up to `shadow_feather` either way, so strokes overrun the edge into the lit side or stop short of it. Where a fragment is lit but a stroke could reach it, two cheap tests at `+- shadow_feather` along the stroke decide whether the shadow side needs drawing. The moved test uses the smooth kernel without the receiver-plane bias, which leaves occasional black specks along the edge on grazed faces (E022).

With `shadow_hatch_dir` 1 the shadow strokes run along the sun's direction projected onto the surface (constant per face for a remote sun) instead of the surface's hatch axis. Solid black inside a shadow (only if the shadow light is below `band_black`) follows the fragment's amount.

### Tone composition

After the light, in the forward pass and the composite (E014 to E019):

- **Solid black and white ink.** The solid black coverage is written to fill A. In the composite, major lines (contours and strokes up to `white_line_rank`) that fall inside it are drawn in white ink (`white_lines`); `white_hatch` scratches sparse white rules along the first hatch direction into the black, `white_hatch_pitch` times the hatch pitch; `speckle` puts white specks in it: one candidate dot per world-anchored cell on the face, cells kept near `speckle_cell_px` pixels by octaves crossfaded like the hatching, dot radius `speckle_px`. These specks, and the broken white rules, are the "sparkles in the shadows". `fog_black` darkens the atmosphere over black so a black shape stays black into the distance.
- **Tone variation**: `tone_noise` adds world 3D value noise of wavelength `tone_noise_len` to the light, soft and irregular; its boundaries are then feathered by the strokes (below). `panel_tone` (a random offset per `id_fine`) is still available but off: it made rectangles of tone whose edges were straight panel borders (E024).
- **Patch hatching** (`hatch_patch`, E019): see the next section.
- **Crosshatch** is off in both presets (`band_cross` 0, below `band_black`, so the second layer is never drawn; darker tone comes from heavier, and in `nihei` denser, single-direction lines; E023). When on, the second layer runs at `cross_angle` degrees from the first, varied by `cross_angle_jitter` per element (`id_coarse`) or, with `cross_angle_by_panel`, per panel.
- **Indication** (`indication`, off by default): lifts the light half of the single-hatch band to white away from shadow edges and draws lit surfaces near a shadow edge down into hatching, using the fraction of a ring of 12 taps of radius `indication_radius` around the point (in the scene cascade) that is lit.
- **Sky**: `sky_tone` `Some(x)` paints the background a flat tone from paper (0) to ink (1); `None` keeps the fogged paper.

### Ruled hatching

Per fragment, with face normal `N`, hatch axis `A` (from the vertex) and `B = normalize(N x A)`:

- Layer 1: lines run along `A`. Spacing coordinate `u = dot(P, B)`, along-line coordinate `v = dot(P, A)`. Present when the jittered light is below `band_hatch`.
- Layer 2 (crosshatch): lines at angle `alpha = cross_angle + jitter` from layer 1, along `D2 = cos(alpha) A + sin(alpha) B`, spaced along `E2 = sin(alpha) A - cos(alpha) B` (at 90 degrees: along `B`, `u = dot(P, A)`, `v = dot(P, B)`). Present below `band_cross`. Its target pitch is multiplied by `cross_pitch_scale`, its width by `cross_width_scale`, its gap is `cross_break_gap`.

Octave selection for one layer (`s = hatch_spacing`, `T = hatch_target_px * px_scale * pitch_scale / (1 + hatch_density_gain * d)`). Everything that decides what is drawn uses a rotation-invariant footprint (E020); only anti-aliasing uses the true screen derivative:

```
r, d = distance and unit ray from the eye to P;  a = radians per pixel at the image centre
foot(e) = r * a * |e - N (e.d) / (N.d)|          metres per pixel of the coordinate dot(P, e),
                                                 as a pixel at the image centre would see it
g    = foot(B)                                   (u = dot(P, B), lines along A)
raw  = log2(T * g / s)
kf   = clamp(raw, 0, hatch_max_octave);  k0 = floor(kf);  f = kf - k0
s0   = s * 2^k0                                   octave-k0 spacing, pitch between T/2 and T pixels
even lines: u = j * 2 * s0   (also the lines of octave k0 + 1), weight 1
odd  lines: u = (2j + 1) * s0,                                   weight 1 - f
cov  = max(cov(nearest even), (1 - f) * cov(nearest odd))
```

With linear fading the mean ink density varies by at most 6% over an octave cycle (maximum of `(2 - f) 2^f / 2` at `f = 2 - 1/ln 2`), so no compensation is applied. Past `hatch_max_octave` the coverage is blended towards its mean `width / pitch * (1 - gap)` by `saturate(raw - kmax)`.

`foot` depends only on the eye position and the surface, so turning the camera in place changes nothing that is drawn. The screen derivative `length(vec2(dpdx(u), dpdy(u)))`, used until E020 (`--hatch-lod screen` restores it), is not rotation-invariant: a rectilinear image magnifies a surface by up to `1 / cos^2` of the angle off the view axis, 1.35 octaves at the side edge of a 70 degree 16:9 view, so a point on a wall slid through octave crossfades as the view turned and everything keyed to the octave (patch layout, speck cells) re-formed. Line widths and taper lengths are likewise set in these invariant pixels and converted to true pixels for coverage (`w_true = w * foot / g_true`), otherwise lines kept a constant screen width while their spacing grew towards the image edge and tone thinned as a surface moved across the screen.

Each line has a stable identity: its index `n` in units of the finest spacing, which is the same whichever octave it is drawn in. Its level is `ctz(n)` (how many octaves it survives). Per line, from `hash(n)`:

- width `= hatch_line_px * px_scale * width_scale * (1 + (h - 0.5) * hatch_width_jitter) * (1 + hatch_width_gain * d)`, where `d = saturate((threshold - L) / (threshold - band_black))` is how far into the band the fragment is;
- breakup, per line (`hatch_patch` 0): 1D value noise along `v` with segment length `hatch_break_len * 2^level`; the line is absent where the noise is below the layer's gap (`hatch_break_gap`, or `cross_break_gap` for the crosshatch). Because the segment length scales with the line's level, lines that survive to coarse octaves (seen far away) break up at a similar screen scale. When the breakup is minified (`dv/dpixel * 2 > segment length`) the threshold is replaced by its mean;
- breakup in patches (`hatch_patch` 1, the default): the layout depends only on a line's own identity, never on the octave being drawn (E020). A line of level `l` belongs to column `floor(n / (hatch_patch_lines * 2^l))`, so at any octave a column holds about `hatch_patch_lines` drawn lines and the columns of level `l + 1` are unions of two of level `l`. Along the lines, runs start from a world grid of length `R0 * 2^11` (`R0 = hatch_break_len * hatch_patch_len`) with a phase per top-level column; going down one level each run is split once at a random point between 25% and 75%, chosen per run and per column. A line of level `l` uses the runs reached after splitting down to `l` (a loop of `11 - l` steps). Runs of level `l` are unions of runs of level `l - 1`, so as the octave changes with distance, lines fade individually and keep their strokes; nothing re-forms. Within its run a stroke starts after `hatch_end_jitter * h^2` of the run and ends `run * (gap * (0.6 + 0.8 h') + hatch_end_jitter * h'')` before its end (`h` per line, `h'` per run), so a group starts raggedly and ends raggedly. Until E020 the layout was cut at the drawn octave and switched to the next octave's in the middle of the crossfade, which re-formed the patches as the view turned;
- ends: the distance in pixels to the end of a stroke is known (exactly for patch runs, to first order from the noise derivative for per-line breakup, from the light's change per pixel at a band edge), so every end is anti-aliased along the line and, with `hatch_taper_px > 0`, the width tapers to nothing over its last `hatch_taper_px` pixels; at the end where a stroke starts the taper is a quarter of that, so strokes are blunt where they start and trail off where they stop. The E007 spindles came from tapering over a fixed range of noise values; tapering over a distance in pixels does not have that problem;
- band jitter: `(noise(v / (band_jitter_len * 2^level)) - 0.5) * band_jitter` is added to `L` before the band test, so lines end raggedly near a band edge. Per line only: shared per patch run (E019 to E025) it made whole runs switch band at once, flat-topped rectangles (E026);
- fading: every way a stroke fades (octave crossfade of the odd lines, band edge, break, shadow gate) narrows it, keeping one edge straight (side random per stroke), instead of lowering its opacity, so every visible mark is full ink (E026). Width is capped at `hatch_width_max_px`;
- feathering (`hatch_feather`, in line spacings): each stroke's end at a band edge is moved along the line by `hatch_feather * hatch_spacing * 2^level * (h - 0.35)` metres, by offsetting the light it is tested against by the light gradient along the line (solved from screen derivatives) times that distance, capped at 0.2. Lines of higher level run further, so at a tone boundary every second or fourth stroke continues past the others and the edge thins out as a hand's would, instead of every line stopping on one contour (E024).

Coverage across a line is the overlap of the line with a tent filter of radius one pixel at distance `|u - u_line| / g` pixels (`tent_cov` in `common.wgsl`). This is analytic in the derivative and smoother than a box, which matters for thin diagonal lines (E006).

Solid black uses the same ragged edge: black where `L + jitter < band_black`, with the jitter of the strip the fragment lies in. That strip is the nearest octave-`k0` line blended towards the nearest even line by `f`, so it is continuous across octave changes.

Debug: `--hatch-lod isotropic|fixed|screen` swaps the octave choice for `length(fwidth(P))`, octave 0, or the screen derivative used before E020 (`Renderer::set_hatch_lod`); coverage keeps using the true derivative.

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
fog   = mix(fog_colour, ink, saturate(max(fog_ao * (1 - ao), fog_black * black)))   (ao = 1 for background)
base  = mix(paper, fog, f)                          (sky with sky_tone = Some(x): mix(paper, ink, x))
ink.R = max(R, B * (1 - black * white_lines));  ink.G = max(G, A * black * white_lines)
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
| `fog_black` | 0 | Fog darkening towards ink over solid black |
| `sun_strength` | 0.5 | Weight of the sun term |
| `ambient` | 0.5 | Weight of `ao` |
| `sun_occlusion` | 0 | Sun term scaled by `mix(1, ao, x)` (a stand-in from before shadows) |
| `band_hatch` | 0.62 | Light below this is hatched |
| `band_cross` | 0 | Below this, crosshatched; below `band_black`, never (no crosshatch, E023; 0.45 before) |
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
| `hatch_density_gain` | 1 | Pitch divided by `1 + gain * d`: darker is denser (carries dark tone without crosshatch, E023) |
| `hatch_feather` | 6 | Feathering of band edges, in line spacings (E024) |
| `hatch_end_jitter` | 0.1 | Ragged starts and ends inside patch runs, fraction of a run |
| `hatch_patch` | 1 | Patch hatching on (1) or per-line breakup (0) (E019) |
| `hatch_patch_lines` | 8 | Patch column width in lines |
| `hatch_patch_len` | 4 | Patch run length in breakup lengths |
| `hatch_taper_px` | 6 px | Width taper at a stroke's trailing end (a quarter at its start); 0 hard ends |
| `cross_break_gap` | 0.45 | Crosshatch gap fraction (layer 1 uses `hatch_break_gap`) |
| `shadow_feather` | 0 m | Feathered shadow edge: strokes test the shadow up to this far along themselves; 0 crisp |
| `hatch_taper_width` | 5 | Stroke taper length in multiples of its width (at least `hatch_taper_px`), +-40% per stroke; one edge stays straight (E026) |
| `hatch_width_max_px` | 2 px | Cap on hatch line width; near walls get more lines, not thicker ones (E026) |
| `shadow_break_gap` | 0.22 | Gap fraction of strokes inside a cast shadow |
| `shadow_hatch_dir` | 0 | Shadow strokes along the surface axis (0) or the sun's projected direction (1) |
| `cross_pitch_scale` | 1.6 | Crosshatch pitch relative to layer 1 |
| `cross_width_scale` | 0.8 | Crosshatch width relative to layer 1 |
| `cross_angle` | 90 deg | Angle between the layers |
| `cross_angle_jitter` | 0 deg | Random variation of that angle, +- half |
| `cross_angle_by_panel` | 0 | 0: per element, 1: per panel |
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
| `sun_shadow` | 0 | Shadow strength; 0 skips the shadow pass (the test scene is closed, so it would be one shadow) |
| `shadow_ambient` | 1 | Ambient multiplier in shadow |
| `shadow_light_min`, `shadow_light_max` | 0, 1 | Clamp of the light in shadow; below `band_black` every shadow is solid black |
| `shadow_attached` | 0 | Light of faces turned away from the sun moved towards the shadow light |
| `shadow_jitter`, `shadow_jitter_len` | 0 m, 1 m | Hand-cut edge: lookup offset amplitude and wavelength |
| `tone_noise`, `tone_noise_len` | 0, 20 m | World noise added to the light |
| `panel_tone` | 0 | Light offset per panel |
| `indication`, `indication_radius` | 0, 3 m | See Tone composition |
| `pool_ao`, `pool_width` | 0, 0.05 | Light goes to black below this ao |
| `underside_black` | 0 | Down-facing faces darkened by `-N.y` |
| `white_lines`, `white_line_rank` | 0, 1 | Lines inside black turned white, up to this stroke rank |
| `white_hatch`, `white_hatch_pitch` | 0, 3 | White rules in black, strength and pitch scale |
| `speckle`, `speckle_px`, `speckle_cell_px` | 0, 0.7 px, 9 px | White specks in black: share of cells, radius, cell pitch |
| `sky_tone` | `None` | Background: fogged paper, or a flat tone 0..1 |

`StylePreset::set(name, value)` sets any field from text (`StylePreset::FIELDS` lists them); `mega_capture --set name=value` uses it.

### The `nihei_vast` preset

`StylePreset::nihei_vast()` is `nihei` with these changes, for spaces hundreds of metres deep. `StylePreset::for_scene("shaft")` returns it; `by_name` and `NAMES` list both presets.

| Field | nihei_vast | Why |
|---|---|---|
| `fog_start`, `fog_density`, `fog_max` | 40 m, 0.0026 /m, 0.8 | Structure 300 to 600 m away must stay legible (about 60% faded at 600 m) |
| `fog_ao` | 1.5 | The abyss fades to dark, not to paper |
| `ambient` | 0.85 | Walls near the rim reach white, and walls turned from the sun stay light enough for hatched shadows to separate from them (0.75 until E021) |
| `sun_strength` | 0.4 | |
| `sun_occlusion` | 1 | No sunlit floors at the bottom of the shaft |
| `band_cross` | 0 | No crosshatch (E023; 0.36 until then) |
| `hatch_width_gain` | 1.0 | Idem |
| `rank_fade_start` | 500, 140, 40, 16 m | |
| `rank_fade_end` | 900, 260, 80, 32 m | Rank 0 (massing, bridge decks, shaft corners) draws the whole 600 m; panel seams are gone by 80 m |
| `rank_min_len_px` | 3, 5, 8, 12 px | |
| `contour_fade_start`, `contour_fade_end` | 220 m, 650 m | Coarse-id contours outline elements all the way down the shaft |
| `sun_occlusion` | 0 | Replaced by the shadow pass |
| `sun_shadow` | 1 | Shadows on |
| `shadow_ambient`, `shadow_light_min`, `shadow_light_max` | 0.55, 0.36, 0.48 | Cast shadow is hatching, darker where the ambient is low, never black (owner, E021, E026; solid black in E011) |
| `shadow_feather` | 0.8 m | Feathered shadow edge: staggered stroke ends, no ruled line (owner's frame 000558, E026) |
| `hatch_width_max_px` | 1.6 px | Width cap (E026) |
| `shadow_break_gap` | 0.04 | Shadow strokes nearly continuous, so the shadow reads as one shape |
| `shadow_attached` | 0 | Faces turned away from the sun keep their ambient tone, lighter than cast shadow (0.3 in E011) |
| `hatch_break_gap`, `hatch_end_jitter` | 0.05, 0.1 | Fewer gaps: the E021 sweep found larger gaps read as confetti |
| `hatch_density_gain` | 1 | With the width cap, spacing carries darkness (E026; 0 in E021) |
| `hatch_feather` | 6 | As `nihei` |
| `tone_noise`, `tone_noise_len` | 0.08, 15 m | Soft, irregular tone variation instead of panel rectangles (E024) |
| `shadow_jitter`, `shadow_jitter_len` | 0.1 m, 1.2 m | Slightly hand-cut edge, visible near, invisible far (E013) |
| `fog_black` | 1 | Black stays black into the distance |
| `white_lines`, `white_line_rank` | 1, 1 | Contours, massing and major edges turn white inside black; panel seams vanish (E016) |
| `white_hatch`, `white_hatch_pitch` | 0.6, 6 | Sparse white rules in black keep the direction of a black wall (E016) |
| `speckle` | 0.1 | White specks in black (E016; the owner's "sparkles") |
| `panel_tone` | 0 | Off: its rectangles of tone had straight panel-border edges (E024; 0.08 in E014) |
| `cross_angle`, `cross_angle_jitter` | 75 deg, 30 deg | Used only if crosshatch is switched back on (E015) |
| `sky_tone` | `Some(0)` | The opening is a white shape against the black (E017) |

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
| `shadow` | Cast-shadow amount in black over a cascade tint (0 white, 1 blue, 2 green, 3 red) |

### Measured (Apple M3 Max, Metal)

Frame time from `mega_capture --bench 200`, the lowest of three runs (wall clock over back-to-back submits, so it includes the CPU cull, cascade fit and submission; an upper bound on GPU time): 1.60 to 1.85 ms at 1280x720 and 3.37 to 5.27 ms at 2560x1440 over `shaft_spawn`, `shaft_rim_down`, `shaft_look_up`, `shaft_bridge_walk`, `shaft_hall_enter` (shadows on), and 0.76 and 2.29 ms for `still_hall` (shadows off). With black shadows (E018) the shaft was 0.96 to 1.53 and 2.85 to 4.13 ms: hatching costs more than solid black, which skips the hatch layers. Before shadows: 0.49 to 0.58 and 1.58 to 1.74 ms. Single runs vary by up to 5x when another program uses the GPU.

Temporal stability (`--metrics`, final view; blurred reprojected MAD per unit of mean ink, lower is steadier; pixels off by more than 64 levels; see E001 for the method). Rotation and walking-speed probes are in E020.

| Path, stretch | final | pops |
|---|---|---|
| `corridor_walk` t 0.05..0.062 | 0.019 | 0.46% |
| `corridor_strafe` t 0.4..0.6 | 0.009 | 0.36% |
| `hall_orbit` t 0..0.012 | 0.021 | 0.43% |
| `shaft_bridge_walk` t 0.3..0.33 | 0.025 | 1.47% |
| `shaft_stair_climb` t 0.4..0.42 | 0.023 | 0.58% |
| `shaft_rim_down` t 0..1 | 0.014 | 0.35% |
| `shaft_spawn` t 0.4..0.6 | 0.018 | 1.15% |
| `shaft_hall_enter` t 0.5..0.52 | 0.021 | 1.13% |
| `shaft_look_up` t 0..0.1 | 0.015 | 0.49% |

The shaft figures per unit of ink rose from E018 because the large blacks, which have no error, became dense hatching; pops come from resampling heavy lines at 2 to 4 px pitch, which a rigid fixed-octave render shares (E020).

### Files

- `crates/render/src/style.rs`: `StylePreset`, `nihei()`, `nihei_vast()`, `for_scene`, `by_name`, GPU packing.
- `crates/render/src/renderer.rs`: targets, pipelines, CPU stroke cull, shadow resources and passes, `DebugView`, `Sun`, `HatchLod`, `StrokeVisibility`, `ShadowFilter`.
- `crates/render/src/shadow.rs`: cascade fitting (`fit`, `Cascades`, `SHADOW_SIZE`, `SHADOW_CASCADES`, `CAMERA_SPLITS`), with tests.
- `crates/render/shaders/shadow.wgsl`: the depth-only shadow pass.
- `crates/render/shaders/common.wgsl`: uniforms, hashes, noise, filtered coverage, octahedral encoding, fullscreen triangle. Prepended to every pass.
- `crates/render/shaders/forward.wgsl`, `strokes.wgsl`, `contours.wgsl`, `contour_resolve.wgsl`, `composite.wgsl`: the passes.
- `crates/capture/src/main.rs`: `--view`, `--metrics`, `--bench`, `--set`, the sun options and the diagnostic switches.

## Open questions

- Tone band edges followed per-vertex AO interpolated over 3 to 4 m panels. For generated scenes the kit now tessellates faces to 1.5 to 2 m and bakes a smooth AO (E009, `kit-of-parts.md`); the test scene is unchanged. Parts narrower than the tessellation still cannot occlude cleanly.
- `fog_ao` still reads the baked ao; with shadows it could take the shadow state of the air instead (light shafts would need that anyway).
- Thin occluders (railings) under one texel of the cascade in use cast dotted shadow edges at long range. Excluding rank-3 parts from the shadow pass, or a minimum-width rasterisation for the shadow pass, would fix it.
- Heavy hatching at 2 to 4 px pitch on distant walls resamples into moire in motion (E020): it is rigid, but busy. A coarser target pitch for heavy lines, or merging dense dark hatching into tone at distance, would calm it.
- Strokes still run to the border of a face; stopping short of creases by varying amounts needs the distance to the face border, which the forward pass does not have (an edge-distance attribute from the generator would provide it).
- Light through small openings (decay milestone): works down to about two texels; the far-field ray march must carry openings in its occupancy grid or distant holes will go dark.
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

### Review notes, 2026-11-16 (Cat, from the windowed shaft and the E011 to E019 sheets)

- "It's looking messier, but I think that might be because the hatching all swims around as I rotate the camera and move around." Cause found and fixed in E020: the octave was chosen from screen derivatives, which change as the view turns, and the E019 patch layout hung off the octave.
- Large completely black shadow areas: "I'd like them if they were much less common, like in dark corners." "The sparkles are good but there should be a lot less black shadow overall." Done in E021: black covers 0.1 to 3% of a shaft view against 5 to 64% before; specks and white rules stay where black occurs.
- Of the E011 treatments, "hatch" looks best: cast shadows are now dense single hatching (E021, E022).
- Crosshatching "always seems to look messy for some reason I can't nail down": diagnosis and options in E023; off by default.
- The E019 patch version "definitely looks better; it's not there yet but it's moving in the right direction" (E025).
- "There just seems to be too many hard edges where the hatching all seems to start at once, following a straight line": feathered tone boundaries, ragged run starts, soft tone noise instead of panel rectangles (E024). Not done: strokes stopping short of creases and face borders (the forward pass does not know the distance to a face's border).

### Review notes, 2026-11-16, second set (Cat, comparing the shaft against the reference pages)

- The current shaft resembles the single-stair reference page, but that page handles tone differently: surfaces in the light are mostly white; hatching becomes heavy only as a surface gets dark and far from the viewer; pitch black begins only in the far distance; and the hatching blends seamlessly into the black. Ours rules every shaded surface densely from near to far.
- Other reference pages (the descending tower over the tiled plain in particular) shade with a greyscale gradient, a paper texture and subtle halftone, with hatching present but much sparser. This may suit a moving 3D image better than dense ruling and should be tried.
- Orchestrator's reading of the frames at this commit: rotation swim is fixed and the straight-edged blocks are gone, but the image is busy. Nearly every wall is a field of ruled lines, so nothing rests and the lit forms do not stand out.

## Related experiments

E001 (temporal metric), E002 (hatch octave selection), E003 (stroke depth tolerance), E004 (distance behaviour of lines), E005 (contour anti-aliasing), E006 (tent versus box coverage), E007 (tone and lighting for the test scene), E008 (distance on the 600 m shaft), E010 (tone and light for the shaft), E011 (shadow treatment), E012 (shadow edge and moving sun), E013 (hand-cut edge), E014 (uneven hatching), E015 (crosshatch angle), E016 (graphic blacks and white ink), E017 (abyss and sky), E018 (before and after), E019 (hatch stroke ends and patches), E020 (swimming under rotation), E021 (shadow as hatching, rare black), E022 (hatched shadow edge), E023 (crosshatch), E024 (feathered boundaries), E025 (nested patches).
