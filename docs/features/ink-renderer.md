# Ink renderer

## Status

In progress, 2026-11-15. Milestone M0 frame working on the test scene: forward MRT pass with ruled hatching and tone bands, stroke pass, contour pass with its own anti-aliasing resolve, composite with atmosphere, ten debug views. Sun shadow pass (cascaded, orthographic or perspective, refitted every frame) feeding the light before band quantisation, and tone composition working on the One Shaft (E011 to E025): cast shadows drawn as dense hatching with a crisp or feathered edge, solid black only in recesses with white ink inside it, rotation-invariant hatching (E020), nested patch hatching with tapered, ragged ends, feathered tone boundaries, no crosshatch by default. Black-and-white presets `nihei` (test scene; shadows off) and `nihei_vast` (the One Shaft, M1; see `one-shaft.md`). Since E027 (2026-11-16) three more presets try two continuous tone models side by side, for the owner to choose between: `nihei_fall` (model A, ruling that comes in only as surfaces fall into darkness and merges continuously into black), `nihei_wash` (model B, graded grey wash printed as fine screentone, sparse ruling) and `nihei_hybrid` (wash for mid tones, model A's ruling at depth). They use a new ruled stroke (weight profile along the stroke, width a fraction of the pitch, tone conserved across distance). Since E035 to E039 (2026-11-16) the default for both scenes is `nihei_ink` (the shaft) and `nihei_ink_small` (the test scene): the wash as the base, with a soft, seam-free grain fixed to the surface (E036); ruling only as a distance effect, sparse, coming in with distance and depth below the eye and closing into black far down the shaft (E035); and one bounce of sun and sky light (E037, `bounced-light.md`). Since E040 to E042 the wash is a smooth gradient on surface-anchored textured paper with a subtle, soft round-dot halftone, instead of the thresholded grain, and cast shadows have soft edges that widen with distance from the occluder. Since E044 to E048 the paper is a baked fibre tile (no cloud), the halftone is capped at 5 cm pitch, the soft shadow keeps a 15 cm minimum penumbra in every cascade, near-axis contours are redrawn from a line fit (less crawl), the bounce bake is smoothed, and the light's tone noise is off in `nihei_ink`. `nihei_ink_wash` is the same without ruling. Every older preset is still selectable by name and draws byte-identically to before; `MEGA_PRESET` selects any of them without code changes (see "Tone models"). Not yet built: the sky pass, light shafts, screentone, the far-field shadow ray march, the compute cull prepass, world-keyed contour wobble, colour presets.

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
| 1 | Forward | raster, MRT, depth test | vertices (slot 0, `Vertex`, 48 bytes), bounce (slot 1, 8 bytes, `bounced-light.md`), shadow maps | depth, id, normal, fill, ink, tone |
| 2 | Contours | fullscreen fragment | id, depth, normal | contour mask |
| 3 | Ink lines | two draws in one pass, max blend | `visible`, strokes, depth; contour mask | ink |
| 3a | Strokes | instanced quads, 6 vertices per stroke, vertex pulling | | ink B and A (G for white strokes) |
| 3b | Contour resolve | fullscreen fragment | contour mask | ink B and A |
| 4 | Composite | fullscreen fragment | fill, ink, depth (id, normal for debug) | caller's colour target |

The contour resolve has a second stage since E045 for lines within about 11 degrees of vertical or horizontal: the line's sub-pixel centre in each of the 17 rows (or columns) around the pixel, a least-squares line through them, and the line redrawn with box-filtered coverage at its mean width, blended in by the fit's quality. Both axes are tried for every line pixel and the better fit wins (the 3x3 tensor reads a stair-stepped near-vertical line as exactly vertical, and at a jog as a corner). Only single, straight lines qualify (every row holds 0.3 to 3.5 of mask, centres within 0.8 px of the fit), so ends, junctions, curves and close parallel lines keep the first stage. This stops the 1 px jogs of nearly vertical silhouettes (columns, wall edges) crawling along the line as the camera moves.

The contour pass is a fullscreen fragment pass, not compute. It writes a fragment per pixel, its only neighbourhood is 3x3, and a fragment pass can write the R8 mask and later max-blend into the `Rg8Unorm` ink target through fixed-function blending; compute would need a storage-texture write path and a separate merge. The split into detection (2) and resolve (3b) exists so the binary mask can be anti-aliased before it is merged (E005).

### Targets

| Target | Format | Contents |
|---|---|---|
| depth | `Depth32Float` | reversed-Z, infinite far plane; cleared to 0, test `Greater` |
| id | `Rg32Uint` | R `id_fine`, G `id_coarse`; 0 is background |
| normal | `Rg16Float` | octahedral world normal |
| fill | `Rgba8Unorm` | R palette index / 255 (always 0 in black and white), G tone band / 3, B ao, A solid black coverage (read by the composite for white lines and `fog_black`) |
| ink | `Rgba8Unorm` | R black ink from the forward pass (hatching, solid black), G white ink (white strokes, specks, white rules), B line ink (strokes and contours), A the major lines among B (contours and strokes of rank up to `white_line_rank`) |
| tone | `Rg8Unorm` | R wash: grey ink coverage 0..1 under the lines, already printed as screentone and halftone where the preset asks (continuous tone models only; 0 with the band model) (E027). G the surface-anchored paper relief as 0.5 + 2 h (0.5 when unused; E040). Written by the forward pass, read by the composite |
| sun shadow | `Depth32Float`, 2048 x 2048 x 4 layers | one cascade per layer; not resized with the frame |
| contour mask | `R8Unorm` | contour strength before the resolve |

All but the shadow maps are recreated on `resize`. The line passes write B and A rather than R so the composite can tell lines from hatching: where a line falls inside solid black, its major part (A) is drawn in white ink by `white_lines` and the rest (B) vanishes into the black. Only the composite writes to the caller's target, which may be any colour format; if it is sRGB the composite pre-linearises so the stored values stay display values.

### Light and bands

Two light values are computed per fragment, the light with the sun unobstructed and the light inside a shadow:

```
n       = 1 - underside_black * max(-N.y, 0)          (times smoothstep(pool_ao -+ pool_width, ao) if pool_ao > 0)
noise   = tone_noise * (2 vnoise3(P / tone_noise_len) - 1) + panel_tone * (2 hash(id_fine) - 1)
bounced = bounce * sun_strength * intensity * sun_b + bounce_sky * ambient * sky_b     (baked per vertex, bounced-light.md; 0 unless the preset asks)
lit     = (sun_strength * intensity * max(N.S, 0) * mix(1, ao, sun_occlusion) + ambient * ao + noise) * n + bounced
dark    = clamp((ambient * ao * shadow_ambient + noise) * n, shadow_light_min, shadow_light_max) + bounced
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
paper = paper * (1 + paper_texture * (n - 0.5))       n: screen-anchored fibre noise (paper_noise), 0 if paper_texture is 0
wash  = wash + paper_texture * 6 (n - 0.5) wash (1 - wash)        granulation, strongest in mid greys
base  = mix(mix(paper, ink, wash), fog, f)         (sky with sky_tone = Some(x): mix(paper, ink, x))
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

### Tone models (E027 to E034)

`tone_model` chooses how light becomes ink. 0 is everything above (bands, `hatch_*`), used by `nihei` and `nihei_vast`, which are unchanged (their frames are byte-identical to the commit before E027). 1 is a continuous model with two independent parts, ruling and wash; the three new presets are settings of it.

**Tone signal.** The light (`lit` and the shadow light `dark`, as above) is first darkened by depth into the structure and distance:

```
darkf = ao ^ dark_ao * exp(-max(r - dark_dist_start, 0) / dark_dist_len) * exp(-max(eye_y - y, 0) / dark_down_len)
c     = ((fall_white - L * darkf) / (fall_white - fall_black)) ^ fall_gamma, clamped 0..1      ruling coverage
w     = ((wash_white - L * darkf) / (wash_white - wash_black)) ^ wash_gamma * wash_max * wash  wash coverage
```

Each factor is off at 0. `dark_down_len` (depth below the eye) is what makes looking down the shaft fall into darkness while looking across it does not; `r` is the eye distance. Both are functions of the eye position only, so they are rigid under rotation. The lit and shadow sides are computed separately and blended at the anti-aliased shadow edge exactly as in the band model.

**Ruling (`rule_layer` in `forward.wgsl`).** One continuous quantity c from paper to solid black:

- Width is a fraction of the local pitch: a line of octave pitch P pixels gets width `c P`. With nested octaves (`rule_lod` 0), the even lines (which survive into the next octave) get `c P (1 + f)` and the odd ones `c P (1 - f)` (f the octave crossfade), so the odd lines narrow to nothing while the survivors gain exactly that width and mean coverage is constant across the octave. Back away from a wall and lines get thinner and closer together in proportion; there is no pixel width cap. The octave target is `rule_target_px * (height/720)^rule_res_exp` pixels divided by `1 + rule_density c` (darker is denser: new lines come in between).
- `rule_lod` 1 has one world pitch (`rule_pitch`) and never drops a line; where the pitch falls below `rule_handover_px` it dissolves into flat tone c over a factor of two in pitch (E029).
- Coverage lost to breaks and ragged run ends (`hatch_break_gap * (1 - c)`, `0.83 hatch_end_jitter`) is put back into width. Below `rule_min_px` real pixels a stroke stops thinning and breaks into dashes of the same mean coverage.
- Past half coverage the white between lines is a sliver. A merge term (`c + 0.4 m^2`, `m = smoothstep(0.55, 1, c)`) widens lines further as c approaches 1 and fades out every modulation below, so slivers thin, break and close into black with no boundary (E031). Above c 0.985 it is solid. Strokes are otherwise capped at 0.85 of their share of the pitch, so neighbours never fuse by accident.
- Coverage is the clamped sum of the five nearest lines (nearest even, its even neighbours, the two odd lines between), so touching lines do not leave a seam.
- Each stroke (a patch run, as before) has a weight profile along its length: heavy where the pen goes down, thinning steadily (`1 - 0.96 t^1.1`, normalised to mean 1) to a hairline that gives out, over at least 24 stroke widths so a short stroke ends bluntly rather than as a wedge (`rule_profile` blends it in). The heavy end is the darker end: from the sign of the tone gradient along the line where there is one, else random per stroke; inside a cast shadow, towards the occluder (the sun's direction projected on the surface; sign checked by eye on frame 000558). On top: low-frequency pressure along the stroke (`rule_pressure`) and weight per stroke (`rule_vary`). Ends get a 1.5 px blunt ramp.
- `--hatch-lod fixed` and `screen` act inside `rule_layer` (octave 0, or the screen derivative) so the rigid baseline still has real widths.
- Black for white lines, specks and `fog_black`: `smoothstep(0.85, 0.985, c)`. White rules (`white_hatch`) are off in these presets: the slivers are the white rules.

**Cast shadows.** As the band model (feathered strokes gated per stroke), plus `shadow_edge_line` (an ink line of that width on the shadow's 0.5 iso-line, rigid because the edge is) and `shadow_hatch_near` (shadow strokes kept only within `indication_radius` of the edge, from the same ring of taps as indication). Options compared in E033.

**Wash and screentone.** The wash `w` goes to the tone target. A threshold map T (0..1) prints it: `mix(w, saturate((w - T) / max(fwidth(T), 0.05) + 0.5), screentone)`. `screentone_kind` 0 is a dot screen `0.5 - 0.25 (cos 2 pi x + cos 2 pi y)`, 1 a grain (value noise, mezzotint-like), 2 their mean; `screentone_px` the pitch at 720 lines. `screentone_anchor`:

- 0 screen: a 45 degree screen in pixels. Print-true; shower-doors as anything moves.
- 1 view sphere: an equi-angular cube map of the view direction, the dot lattice aligned to the cube edges (the pitch rounded so a quarter turn holds a whole number of cells) so it mirrors seamlessly across them; grain is 3D noise on the direction. Fixed under rotation, slides under translation.
- 2 surface: in the surface frame (dots) or world position (grain), octaves chosen from the rotation-invariant footprint and crossfaded; the octave-(k+1) dot screen's dots are every other dot of octave k. Rigid; dots change size with distance. Grain is stretched into streaks on grazing surfaces (3D noise seen edge-on).

**Surface grain, E036** (`screentone_aniso` 1, used by the `nihei_ink` presets). The E032 surface grain had three faults, now fixed behind fields so the old grain stays reachable:

- *Seams.* It blended octave k (one seed) with octave k + 1 (another), and at the next integer octave the pair became k + 1 and k + 2 with the seeds the other way round, so the drawn field swapped at every integer octave, along lines of equal footprint: the straight lines the owner saw changing detail as he walked. Now each octave pair has its own seed, a function of the octave numbers only, so the field at the end of a crossfade is the field at the start of the next; the blend is renormalised by its standard deviation so contrast does not dip halfway.
- *Streaks.* One octave from the larger footprint made cells many pixels long across a grazing floor. Now each surface axis has its own octave from its own rotation-invariant footprint (2D noise in the surface frame, four fields blended bilinearly, the plane's offset hashed into the seed).
- *Swimming.* The noise lattice was 0.4 of a cell, sub-pixel, so the threshold aliased and re-formed under any sub-pixel motion; rotation hid it because it keeps the footprint. `screentone_detail` weights that fine lattice against one at 0.8 of the cell (1.6 to 3 px); the default is 0.3 fine.
- *Hardness.* The threshold's ramp was one pixel, so each grain was a two-level blob. `screentone_soft` widens it, in threshold units: at 2 the grain is a continuous modulation of the grey.

`nihei_ink` uses aniso 1, detail 0.3, soft 2 and `screentone` 0.2, and `paper_texture` 0.04 (0.06 before; it is screen-anchored and so slides over the wash when walking). Walking error on the wash falls from 1.8 to about 0.6 levels (E036).

Screen-anchored `paper_texture` (scale `paper_texture_px`) is the only screen-anchored element besides `paper_grain`. `wash_speckle` puts the white specks in dark wash too. The composite multiplies the wash into the paper before the fog and the lines; ink stays ink and paper stays paper, greys come only from the wash.

**Paper and halftone, E040** (the `nihei_ink` presets). The owner on the E036 grain: "too much like TV static or a QR code. Actually, the completely smooth version looks the best, but I would like the end result to be more textured." The base is now the smooth wash (`screentone` 0) and two low-contrast layers sit on it.

- *Paper.* A relief `h` of the sheet: tooth (irregular fine relief, a few fibres). (E040 also had a large faint mottle; removed in E049.) It lifts and lowers the paper colour everywhere, white included (`paper *= 1 + h`), and the wash settles a little more in the hollows, most in mid greys (`wash -= paper_take * 4 h' w (1 - w)`, `h'` the normalised relief), like wash on rough paper rather than a flat multiply. Two anchorings (`paper_anchor`):
  - 0, screen: `paper_sheet` in the composite, value noise at `paper_tooth_px` pixels. A faint static sheet. Kept for comparison.
  - 1, surface (default): `paper_surface` in the forward pass, written to tone G. (As built in E040; since E044 and E049 see "Fibre paper" and "Scanned paper" below.) Twelve fixed world scales (`paper_cell` = 2 mm times 1, 2, 4 ... 2048), each one value-noise field with its own seed and its own rotation in the surface frame. A layer is present while its cell is between `paper_tooth_px` and `paper_mottle_px` pixels on screen (rotation-invariant footprint), faded in and out by amplitude; the small end carries tooth, the large end mottle. Walking towards a wall, finer layers fade in over coarser ones that stay, so the paper refines rather than swapping; nothing reaches pixel scale; the sum is normalised to the contrast of about four layers, so far away and on foreshortened surfaces, where fewer layers are large enough, it fades towards smooth tone instead of compressing into static. It is also faded by the surface's anisotropy (`2 min(g) / max(g)`), so grazing floors go smooth.
  - Chosen: surface. Measured (E040), screen paper adds about 1 level of walking error and 3 of turning error (it shower-doors), surface paper about 0.1. The owner expected as much.
- *Halftone.* A rotated round-dot screen (`halftone_angle` 30 degrees) fixed to the surface, carrying a small part (`halftone` 0.07) of the wash, only in mid to dark tone (`smoothstep(0.1, 0.35, w) (1 - smoothstep(0.85, 0.98, w))`), never an on/off threshold of the whole tone. Dots are area-true (threshold `pi d^2`), so they stay round where the cosine screen of E032 made a checkerboard, with soft edges (`halftone_soft` 0.8 in threshold units) and a little size variation per dot. Eight fixed world pitches (`halftone_pitch` 1 cm times 1, 2 ... 128), nested (each dot of pitch 2P is a dot of pitch P), each present only while it is 3.5 to 11 pixels on screen, faded by amplitude: the screen never reaches pixel scale and at distance the halftone is gone well before the dots would shrink to pixels. Faded by anisotropy like the paper. A low-frequency wobble of the lattice was tried for irregularity and dropped: it differed between pitches, so their crossfade beat into bands on the floor.

`paper_grain` (screen-anchored per-pixel grain) and `paper_texture` (the E027 screen texture) are 0 in `nihei_ink`.

**Fibre paper, E044.** The owner: the surface paper read as "photoshop clouds". The tooth now comes from a baked tile (`crates/render/src/paper.rs`, 512 x 512 R8 with mips, generated at startup in about 20 ms): 5200 short fibres (5 to 22 texels, slightly curved, lighter or darker than the sheet, a third gathered in loose clumps), 9000 fine pits, a faint blurred white-noise tooth, normalised to zero mean and unit deviation. `paper_surface` samples it at fixed world scales (texel `paper_cell` 0.5 mm times 2^k): only the up to three layers whose texel is between `paper_tooth_px` (0.7) and 2.6 times that on screen, faded by amplitude, chosen by the smaller footprint and sampled with the true screen gradients and 16x anisotropic filtering, so a foreshortened floor shows foreshortened fibres instead of streaks; layers with texels over `paper_max_cell` (15 cm) are not drawn, so the paper fades to smooth tone beyond roughly 150 m. Each layer has its own rotation and offset. Faded only on very grazing surfaces (footprint ratio under 0.08 to 0.2). The E040 clouds (`paper_mottle`) were removed in E049; `tone_noise`, a 15 m value-noise cloud added to the light, is also 0 in `nihei_ink` (it was the dark blotch in the stair's shadow in frame 115151, E048).

**Scanned paper, E049.** The owner found the generated fibres "a bit like worms" and asked for a CC0 scan. `paper_kind` picks the relief, one layer of a texture array: 0 the generated fibre tile, 1 to 4 the ambientCG scans Paper001 (cold-pressed, watercolour-like), Paper002 (cartonage, fine even grain; the default), Paper004 (packing paper with laid lines) and Paper006 (fibrous). Sources, licence (CC0 1.0) and preparation are in `assets/paper/README.md`. Only the displacement maps are embedded (`include_bytes!`, about 2.4 MB) and prepared at startup (about 70 ms for all four): resampled whole to 512 x 512, high-passed (minus an 8-texel Gaussian), normalised; the shader restores each map's aspect (`PAPER_ASPECT`), so texels stay square on the wall. The maps already tile (measured: the wrap-around difference equals the neighbouring-row difference), so they are not blended. They are sampled through the same layers as the fibre tile (same fades, rotations and offsets per layer, anisotropic sampling). Physical scale: the window of 0.8 to 2 px per texel on screen decides what is drawn, not the paper's real size: real tooth is sub-millimetre and would vanish on walls metres away, so the tooth is deliberately shown at a constant few pixels on any wall within the fade range (15 cm texels at most, so it fades beyond roughly 150 m). A larger on-screen scale (1.4 px) made the scans mushy. Strength 0.06 for the scans (their fine grain is weaker at the same setting than the fibres' peaky marks; at 0.03 they vanish on lit walls).

**Paper001 up close, E051.** The owner liked Paper001 at distance (frame 124919) and found it harsh, like embossed stucco, 21 m closer (124923). Causes found: (1) Paper001's tooth is 6 to 10 texels across, so the layer window magnified it to 10 to 30 px bumps; it is now folded (`PAPER_FOLD`: the scan resampled to half size and repeated 2 x 2 in the tile), halving the bump size. (2) The far frame looked soft because the 15 cm texel cap was fading it there; the near frame had full strength. (3) The layer window left octave-periodic contrast dips to about 0.7 (blotchy zones on a wall seen at an angle); it is now energy-preserving (sin/cos weights in log2 of the on-screen texel size, `w_a^2 + w_b^2 = 1`). (4) The wash's settling into the tooth (`paper_take`) was normalised by the strength, so it ignored the strength and any fading; it now scales with the relief. Then: `paper_max_cell` 50 cm (fades over two octaves), strength 0.012, and a near envelope: amplitude `paper_near_amp` (0.5) at the eye rising smoothly to 1 at `paper_near_dist` (40 m), so near walls are the calmest.

**Pencil shading in shade, E052 and E053.** A second relief (`pencil_kind`, Paper004's laid lines) laid on the wash in one direction (`pencil_angle`, 45 degrees in the surface frame, the same for every layer), zero mean so tone is kept, coming in with the wash's darkness from `pencil_from` (0.15) to `pencil_to` (0.5); `pencil_cross` adds it at right angles in the darkest tone (soft crosshatching, off by default). In patches (`pencil_patches` 1, `pencil_patch_m` 2.5 m): a low-frequency world noise sets where in the tone range each spot starts, over a wide feathered ramp, so pencil arrives patch by patch rather than tracing the shadow's shape, more patches joining as it darkens; the second direction, when on, only in some patches. Fixed in the world, faded only by amplitude, and through the same distance layers as the paper. `pencil` 0.15; 0 reproduces paper alone.

**Distance shading as wash and pencil, E054** (`far_mode` 1, the `nihei_ink` default; 0 is the E035 ruling, kept for comparison). The tone that drove the distance ruling (the light darkened by `rule_dist_len` and gated by `rule_near`, `rule_fade_in`, `rule_down_gain`, through the `fall_*` curve) goes into the wash instead: `w = 1 - (1 - w)(1 - far_wash * c)`, so walls deepen with distance and depth below the eye to solid ink at the bottom of the shaft, with no ruled strokes. On it, pencil marks gather (`far_pencil` 0.6): the Paper004 relief in the pencil direction, stretched along its lines (`far_stretch` 2), at `far_pencil_px` 1 px per texel on screen and with no distance cap, thresholded softly into discrete dark marks whose threshold falls as the tone rises (marks thicken and join), plus occasional light gaps from the relief's troughs, arriving in 5 m patches. Near the black the marks fade into it; white specks and white contours inside the black follow the merge black of the far tone as before. `far_pencil` 0 is option C, the wash alone falling into black. `far_cross` adds a second direction 60 degrees round in the darkest patches (off).

**Far pencil built up like pencil, E057.** The E054 marks arrived as near-black shapes whose opacity ramped (the owner: "like something alpha fading in"). They are now procedural strokes (approach B, `far_strokes` in `forward.wgsl`): lines of strokes in the pencil direction at about `4 * far_pencil_px` pixels pitch (720 lines), held there by octaves whose coarser lines are every other line of the finer, so the octave blend thins the field instead of swapping it. Along each line strokes of 6 to 14 pitches start at random offsets, each with its own rank, slight wander and tapers (short at the start, long at the end). A stroke is drawn once the marks' tone passes its rank, so strokes appear one by one; its pressure rises with the tone, grey graphite first and near ink only at the end. A second pass 20 degrees round enters at half tone; passes combine like graphite layers (`1 - prod(1 - d)`), so overlaps darken. The marks' tone has its own, earlier onset, `far_white` (0.45; the wash's far darkening starts at `fall_white` 0.28), so faint strokes come before the wash darkens much. Patches (5 m) enter progressively and are full by the darkest tone. Occasional light gaps remain in the black. `far_cross` (E054) was removed.

**Pencil shading from a hand-shaded texture, E059** (`shade`, the `nihei_ink` default; replaces the E052 laid-line pencil and the E057 procedural far strokes, both now 0 but still available). A tonal art map generated at startup (`paper::shade_levels`, about 0.1 s): four 512 x 512 levels of graphite coverage, each holding every stroke of the level before plus more. Strokes are laid in clusters: a cluster has a direction drifting +-20 degrees about the pencil direction (from level 3 some clusters cross it by 40 to 60 degrees), strokes side by side at uneven spacing with skipped strokes, ragged ends, a pressure profile along each stroke and soft edges; later clusters press harder and a few are burnished; strokes combine like graphite, `1 - (1 - c)(1 - s)`. The tile is sampled on the surface at fixed world scales (texel 0.5 mm times 2^k), each present while its texel is `shade_px` to 4 times that on screen, with triangular weights summing to 1, each scale at its own offset and a rotation drifting +-20 degrees. The level comes from darkness from every source: the final wash (light, shadow, enclosure through `dark_ao`, the far fall) plus `shade_ao * (1 - ao)`, from `shade_from` (0.45) to `shade_to` (0.95), laid in patches (`pencil_patches`, `pencil_patch_m`) that arrive at different darknesses; between levels the coverage is mixed, so strokes fade in and never swap. Graphite darkens the wash grey on grey, `1 - (1 - w)(1 - shade * coverage * tooth)`, with `tooth` the paper relief so graphite catches Paper001 (`shade_grain`). Light and mid shade stay clean (the lit spawn view is unchanged).

**The fall into darkness, E059.** With marks off, the old far tone took the wall from light to near black between 150 and 300 m looking down from the rim. Now `fall_white` 0.3, `fall_black` 0.04, `fall_gamma` 2.4, `rule_dist_len` 380 m and `fog_ao` 1.2 (1.5 before) give, as mean luminance by distance in `shaft_rim_down` (0 black, 255 paper): 205 below 150 m, 170 to 200 m, 143 to 250 m, 97 to 300 m, 84 to 350 m, 74 to 400 m, 72 to 450 m, then 40, 35 at the bottom (before: 202, 153, 106, 64, 55, 58, 57, 40, 35). Regenerate the table with the `--view final` and `--view depth` frames of the same camera binned by distance (E059).

**The light end, E062** (`light_*`, the `nihei_ink` default). Before E062 every light value above `wash_white` (0.72) printed as bare paper. In the open shaft that is most of the frame (paper or within 20 levels of it: 87% of `shaft_rim_down`, 74% of `shaft_arch_view`), because the ambient term alone (0.85 times openness) clears it near the top. Now, above `wash_white`, the light prints a pale wash (`light_wash`, 0.18) thinning to paper at `light_white` (1.0) along `(1 - t)^light_gamma` (1.3); below `wash_white` it fades out over 0.25 of light, so the dark end is untouched (deep hall and pit histograms identical). Direct sun counts `light_sun` (4) times more at this end only, as sunlight is far brighter than sky light: sunlit faces still reach paper and the shadows on them keep their contrast, while sky-lit faces go pale grey with openness. `light_far` adds a faint pale tint with distance (`light_far_start`, `light_far_end`); off, it added little. Exposure adaptation (brighter deep in the hall, darker in the sun) was not added: it would change surface tones as the camera moves.

**The dark end and blacks, E066** (`black_*`, the `nihei_ink` default). Before E066 `nihei_ink` inherited `wash_max` 0.5 from `nihei_wash`, so the wash could not print darker than about half tone however little light there was: the darkest labyrinth views (undercroft, sealed corridor, tower well) sat at an even grey of 62 to 68 of 255, and only lines reached black. Measured: `wash_max` alone sets that floor (fog, bounce and ambient do not move it). Now `wash_max` is 0.85 with a linear curve (`wash_gamma` 1), so the dark half of the range is darker overall, and where the total light (the light value after the shadow, `ao^dark_ao` and bounce) falls below `black_point` (0.06) plus a range (`black_range` 0.2) that varies by 3 m patches (`black_patch` 0.7), the wash closes to solid ink. The open shaft's light never gets that low (its cast-shadow light is clamped to at least 0.25), so its shadows stay grey. Over solid black and over wash dark enough that black lines would vanish (coverage 0.72 to 0.85), the fragment counts as black for the composite: contours and major strokes turn white, so a sealed corridor stays walkable by its white edges, floor lines and doors. White specks appear only inside solid black (`wash_speckle` 0, `speckle` 0.06). `black_range` 0 is off.

**White lines and specks in black, E067 and E068.** The E066 white lines flipped to paper white wherever the wash passed 0.72, so on dark grey railings and floor edges turned glaring white part-way along, and in black they were heavy, uniform outlines. Now (`white_line_tone` > 0, the `nihei_ink` default) the composite fades the dark line out as the black closes in (`fill.w` 0.3 to 0.65), leaves a gap, and fades a white line in only over near-solid black (0.8 to 1). That only happens where the black itself closes in or the wash is within a few steps of it (above 0.86), never over dark grey. The white line is only the line's core (`white_line_core` 0.55, so thinner than the dark lines), light grey rather than paper (`white_line_tone` 0.55), and broken and varied in weight by world-anchored noise (`white_line_break` 0.4). `white_lines` 0 gives black with no lines at all. Inside the black, specks are now a few soft grey flecks (`speck_tone` 0.25, `speck_soft` 0.7, `speckle` 0.04), and a faint sheen of the pencil tile's darkest level and the paper tooth gives the black a surface (`black_texture` 0.1).

**The dark end shifted, and the player's light, E070.** The owner wants the darks really dark, white only as highlights. `nihei_ink` now has `wash_white` 0.95, `wash_black` 0.25, `wash_gamma` 0.8, `wash_max` 0.95, `black_point` 0.1, `black_range` 0.3 and `light_wash` 0.36: typical surfaces are dark to mid grey, paper is left to sunlit faces (via `light_sun`), and unlit enclosed places are solid black. White lines and specks in black are off by default (`white_lines` 0, `speckle` 0; the owner's choice; the code stays), the pencil sheen in black stays. The player carries a weak point light (`player_light` 0.3, `player_radius` 8 m, `player_falloff` 1.5): 0.25 m below the eye, N.L shaded so floor, walls and ceiling separate, fading to nothing with a radius varied +-15% by surface noise so the pool's edge is soft and irregular, added to the light before the wash curve (so it is lost in sunlit places). It is what makes a sealed corridor readable now that lines inside black are off. No shadows from it. The capture tool applies it too (the camera is the player).

Local light (not built beyond the player's light): sealed spaces now go pitch black. Lamps, glowing machinery or light through cracks would need, in the renderer: extra terms added to the light value before the wash curve (point or area falloff, per fragment or per vertex from a list in a storage buffer); shadows for them (cube or dual-paraboloid maps per light, or none for small soft lights); and their contribution in the bounce bake (`mega_gen::bounce` would take emitters as well as the sun). The generator would place them.

**Halftone cap, E044 to E047.** `halftone_max_pitch` (5 cm): the largest dot pitch fades out over the octave below the cap, so the dots appear only on surfaces within roughly 8 m and far walls are smooth tone (the owner's "snakeskin" was 20 to 50 cm dots on walls 20 to 60 m away). Default strength 0.05 (0.07 before). Only the up to three pitches that can fall in the 3.5 to 11 px window are evaluated.

**Soft cast-shadow edges, E041** (`shadow_soft`, the `nihei_ink` presets). A penumbra whose width on the receiver is `shadow_soft` times the distance from the receiver to the blocker, capped at `shadow_soft_max` (1 m): contact shadows stay tight and shadows cast from far (the stair onto the wall below it) grade softly, as from a sun of exaggerated size. Per lit-facing fragment:

1. Blocker search in the scene-wide cascade: 3x3 taps over half the largest penumbra, each read as four texels with bilinear weights (so the estimate moves continuously; plain texel loads stepped at texel edges and drew staircase lines in the penumbra), compared against the receiver's own plane with two texels of slack. Mean depth difference over the blocking taps, converted to metres.
2. The finest cascade in which the penumbra's radius is at most four texels; blended into the next coarser one between two and four texels, so the cascade switch (which follows the noisy blocker distance) is never a seam.
3. 5x5 tent-weighted comparison taps spread over the penumbra, against the receiver's plane, mixed with the hard edge by `saturate((r_hard - 0.75) / 1.5)` where `r_hard` is the penumbra radius in texels of the cascade the hard edge came from; below about a texel the hard edge is kept.

Since E047 the penumbra is at least `shadow_soft_min` (15 cm) wide in every cascade and the hard edge is never mixed back in: before, a near-contact penumbra narrower than a texel was soft in the 3 cm cascade and hard in the 11 cm one, so the stair's shadow in frame 115151 turned hard and serrated where it crossed into cascade 2. `shadow_soft` 0 is today's edge exactly (the code does not run). Default 0.06 (6 cm of penumbra per metre from the occluder). Failures on the way: a fixed six-texel cap per cascade made the penumbra undersampled and dotted at 0.12; a hard cascade switch drew a jagged seam along the stair's shadow.

**Distance ruling, E035** (`rule_fade_in` > 0, the `nihei_ink` presets). On top of the wash, ruling comes in only with distance. With the eye at height `eye_y` and distance `r`:

```
de    = r + rule_down_gain * max(eye_y - y, 0)                         effective distance
gate  = smoothstep(rule_near, rule_near + rule_fade_in, de)              (1 if rule_fade_in is 0)
rdark = exp(-max(de - rule_near, 0) / rule_dist_len)                      (1 if rule_dist_len is 0)
c     = fall_curve(L * darkf * rdark) * gate                             for both the lit and the shadow side
```

The wash does not see `gate` or `rdark`. Near surfaces, including deep shade and cast shadow, are carried by the wash alone; the solid black of the merge (`smoothstep(0.85, 0.985, c)`) follows `c`, so black too appears only far away. Depth below the eye counts extra, so looking down the shaft gathers ruling sooner than looking across it. Both factors depend on the eye position only, so the ruling is as rigid under rotation as before (E038: `shaft_spawn_yaw` pops 0.015% against the wash's 0.014%). Sparseness comes from a wide octave target (`rule_target_px` 9 px) that closes up as the tone darkens (`rule_density` 3: pitch divided by 1 + 3c), a light tone curve (`fall_white` 0.28), a gentler weight profile (`rule_profile` 0.4, fewer wedge-shaped strokes) and long runs (`hatch_break_len` 4 m, fewer stroke ends). `shadow_hatch_near` is off: with it, ruling and black inside cast shadow survived only near the edge and the far pit never closed (E035).

**Choosing a preset.** `mega_capture --preset <name>`; in the game, `StylePreset::for_scene` returns the preset named by the `MEGA_PRESET` environment variable when it is set (any scene; an unknown name is reported on stderr and ignored), otherwise the scene's own (`StylePreset::scene_default`). The screenshot sidecar records the preset in use, so a replay reproduces it.

```
MEGA_PRESET=nihei_fall   cargo run --release --bin megastructure -- --scene shaft
MEGA_PRESET=nihei_wash   cargo run --release --bin megastructure -- --scene shaft
MEGA_PRESET=nihei_hybrid cargo run --release --bin megastructure -- --scene shaft
MEGA_PRESET=nihei_ink_wash cargo run --release --bin megastructure -- --scene shaft   # the new wash, no ruling
MEGA_PRESET=nihei_vast   cargo run --release --bin megastructure -- --scene shaft   # the band model
cargo run --release --bin megastructure -- --scene shaft          # nihei_ink, the default since E038
```

#### Fields (defaults in `nihei`; they have no effect while `tone_model` is 0)

| Field | Default | Meaning |
|---|---|---|
| `tone_model` | 0 | 0 band hatching, 1 continuous ruling and wash |
| `rule_lod` | 0 | 0 nested octaves with conserved coverage, 1 one world pitch handing over to flat tone |
| `rule_pitch` | 0.02 m | World spacing of the ruling (octave 0, or the only one with `rule_lod` 1) |
| `rule_target_px` | 3.0 px | Octave target pitch at 720 lines |
| `rule_res_exp` | 0.6 | Target pitch scales as `(height/720)^x`: below 1, larger outputs get relatively finer ruling (3024x1832 at 0.6: 1.75x the 720 pitch instead of 2.54x) |
| `rule_density` | 0.5 | Target pitch divided by `1 + x c` |
| `rule_min_px` | 0.4 px | Real-pixel width below which strokes break into dashes |
| `rule_profile` | 0.8 | Weight profile along each stroke (0 constant width) |
| `rule_pressure` | 0.25 | Pressure variation along a stroke |
| `rule_vary` | 0.35 | Weight variation between strokes |
| `rule_handover_px` | 1.6 px | `rule_lod` 1: pitch below which ruling becomes flat tone |
| `fall_white`, `fall_black`, `fall_gamma` | 0.62, 0.30, 1 | Ruling tone curve |
| `dark_ao` | 0 | Light times `ao^x` |
| `dark_dist_start`, `dark_dist_len` | 0, 0 m | Light times `exp(-(r - start)/len)` |
| `dark_down_len` | 0 m | Light times `exp(-(eye_y - y)/len)` below the eye |
| `wash` | 0 | Wash strength (0 none) |
| `wash_white`, `wash_black`, `wash_gamma`, `wash_max` | 0.9, 0.3, 1, 0.8 | Wash curve and darkest wash |
| `screentone` | 0 | Share of the wash printed as screentone |
| `screentone_kind`, `screentone_anchor`, `screentone_px` | 0, 2, 2.5 px | Dots/grain/both; screen/sphere/surface; pitch |
| `paper_texture`, `paper_texture_px` | 0, 3 px | Screen-anchored paper texture strength and scale |
| `wash_speckle` | 0 | White specks in dark wash |
| `shadow_edge_line` | 0 px | Ink line along cast shadow edges |
| `shadow_hatch_near` | 0 | 1: shadow strokes only within `indication_radius` of the edge |
| `rule_near` | 0 m | Effective distance below which there is no ruling (with `rule_fade_in` > 0) (E035) |
| `rule_fade_in` | 0 m | Length over which ruling fades in past `rule_near`; 0 no gate (ruling at every distance, as before E035) |
| `rule_down_gain` | 0 | Weight of depth below the eye in the effective distance |
| `rule_dist_len` | 0 m | Ruled light darkened by `exp(-(de - rule_near) / len)`; 0 off |
| `screentone_aniso` | 0 | 1: per-axis, seam-free surface grain (E036); 0 the E032 grain |
| `screentone_detail` | 1 | With `screentone_aniso`: weight of the fine (sub-pixel) grain lattice against one twice as coarse |
| `screentone_soft` | 0 | Width of the screentone threshold's ramp in threshold units; 0 one pixel (hard grain), 2 a soft modulation |
| `bounce`, `bounce_sky` | 0, 0 | Strengths of bounced sun and sky light (`bounced-light.md`); 0 off and no bake |
| `halftone` | 0 | Share of the wash carried by the soft dot screen (E040); 0 off |
| `halftone_pitch` | 0.01 m | Finest dot pitch; coarser pitches double up to 128 times this |
| `halftone_soft` | 0.5 | Dot edge softness in threshold units |
| `halftone_angle` | 30 deg | Screen angle in the surface frame |
| `paper_tooth`, `paper_tooth_px` | 0, 3 px | Paper tooth amplitude; smallest scale on screen (surface) or tooth scale (screen) |
| `paper_kind` | 0 | Paper relief: 0 generated fibre, 1 Paper001, 2 Paper002, 3 Paper004, 4 Paper006 (E049) |
| `paper_near_dist`, `paper_near_amp` | 0 m, 1 | Paper amplitude rises from `near_amp` at the eye to 1 at `near_dist` (E051) |
| `pencil`, `pencil_kind` | 0, 3 | Pencil shading in shade: strength (0 off) and relief (E052) |
| `pencil_from`, `pencil_to` | 0.15, 0.5 | Wash coverage over which the pencil comes in (0.4, 0.75 in `nihei_ink` since E056) |
| `pencil_cross`, `pencil_angle` | 0, 45 deg | Second direction in the darkest tone; line direction in the surface frame |
| `pencil_patches`, `pencil_patch_m` | 0, 2.5 m | Pencil arrives in patches (E053) |
| `far_mode` | 0 | Distance shading: 0 ruling, 1 wash and pencil marks (E054) |
| `far_pencil`, `far_pencil_px`, `far_stretch` | 0, 3 px, 4 | Far pencil marks: strength (0: wash alone), on-screen size, lengthening |
| `far_wash` | 1 | Share of the far tone in the wash |
| `shade`, `shade_px` | 0, 0.9 px | Pencil shading from the hand-shaded tile: strength (0 off), on-screen texel size (E059) |
| `light_wash`, `light_white`, `light_gamma`, `light_sun` | 0, 1.25, 1.5, 0 | The light end: pale wash at `wash_white`, light at which paper is reached, shoulder shape, extra weight of direct sun there (E062) |
| `light_far`, `light_far_start`, `light_far_end` | 0, 40 m, 200 m | Faint pale tint with distance |
| `black_point`, `black_range`, `black_patch` | 0.03, 0, 0.7 | Blacks: light at which surfaces are solid black, range over which black closes in (0 off), patchiness of that range (E066) |
| `white_line_tone`, `white_line_core`, `white_line_break` | 0, 0.5, 0.35 | Restrained white lines in black: tone (0 keeps the old full white), line core kept, share broken (E067) |
| `speck_tone`, `speck_soft`, `black_texture` | 0, 0, 0 | Speck tone (0 white), softness and size variation, pencil and paper sheen inside black (E068) |
| `player_light`, `player_radius`, `player_falloff` | 0, 8 m, 1.5 | The player's light: strength (0 off), radius, falloff exponent (E070) |
| `shade_from`, `shade_to`, `shade_ao`, `shade_grain` | 0.35, 0.95, 0.3, 0.6 | Darkness range of the shading, weight of enclosure, how much graphite catches the paper tooth |
| `far_white` | 0.6 | Light level at which the far pencil starts (E057) |
| `paper_take` | 0 | How much the wash settles in the tooth's hollows |
| `paper_anchor` | 0 | 0 screen, 1 surface (fixed world scales faded by size on screen) |
| `paper_cell` | 0.002 m | With `paper_anchor` 1: finest world cell |
| `shadow_soft` | 0 | Penumbra width per metre from the occluder (E041); 0 hard edge |
| `shadow_soft_max` | 1 m | Largest penumbra width |
| `shadow_soft_min` | 0 m | Smallest penumbra width, in every cascade (E047) |
| `halftone_max_pitch` | 1000 m | Largest halftone pitch; coarser ones fade out (E044) |
| `paper_max_cell` | 1000 m | Largest fibre-tile texel of the surface paper; coarser layers are not drawn (E044) |

Old sidecars have none of these; on replay they take the named preset's defaults (with the usual warning), and with `tone_model` 0 they change nothing.

#### The three presets

All three are `nihei_vast` with these changes.

| Field | `nihei_fall` (A) | `nihei_wash` (B) | `nihei_hybrid` (C) |
|---|---|---|---|
| `tone_model` | 1 | 1 | 1 |
| `fall_white`, `fall_black`, `fall_gamma` | 0.45, 0.06, 1.4 | 0.20, 0.05, 1.0 | 0.40, 0.05, 1.5 |
| `dark_ao` | 0.6 | 0.5 | 0.8 |
| `dark_dist_start`, `dark_dist_len`, `dark_down_len` | 30, 220, 140 m | off | 30, 220, 140 m |
| `wash`, `wash_white`, `wash_black`, `wash_gamma`, `wash_max` | off | 1, 0.72, 0.15, 1.3, 0.72 | as B, `wash_max` 0.5 |
| `screentone`, kind, anchor, `screentone_px` | | 0.25, grain, surface, 2 px | as B |
| `paper_texture`, `wash_speckle` | | 0.06, 0.5 | as B |
| `shadow_light_min`, `shadow_light_max` | 0.30, 0.45 | 0.25, 0.45 | as B |
| `shadow_hatch_near`, `indication_radius` | 0 | 1, 1.5 m | as B |
| `hatch_break_len`, `hatch_break_gap`, `hatch_patch_lines`, `hatch_end_jitter` | 2.0 m, 0.06, 3, 0.35 | same | same |
| `white_hatch` | 0 | 0 | 0 |
| `fog_density`, `fog_max` | 0.0015, 0.3 (darkness is not faded) | as `nihei_vast` | as `nihei_vast` |

#### The default, `nihei_ink` (E035 to E038)

`nihei_ink` is `nihei_wash` with these changes; `nihei_ink_wash` is `nihei_ink` with `fall_white` 0 and `fall_black` -0.01 (no ruling at all); `nihei_ink_small` is `nihei_ink` with `nihei`'s fog, ambient, sun strength, line fades and sky, no shadow pass (the test scene is closed) and the ruling gated over 15 + 40 m with `fall_white` 0.28 and no down gain.

| Field | `nihei_ink` | Why |
|---|---|---|
| `screentone` | 0 | Smooth wash; the E036 grain read as static (E040). `screentone_aniso` 1, `screentone_detail` 0.3, `screentone_soft` 2 kept for anyone turning it back on |
| `paper_texture`, `paper_grain` | 0, 0 | Screen-anchored textures off (E040) |
| `paper_anchor`, `paper_kind`, `paper_cell`, `paper_max_cell`, `paper_tooth`, `paper_tooth_px`, `paper_take` | 1, 1 (Paper001), 0.5 mm, 50 cm, 0.012, 0.6 px, 0.08 | Surface-anchored Paper001 (E051; E049: Paper002 at 0.06, 15 cm, 0.8 px) |
| `paper_near_dist`, `paper_near_amp` | 40 m, 0.5 | Calmer up close (E051) |
| `pencil`, `pencil_kind`, `pencil_patches` | 0.15, 3 (Paper004), 1 | Pencil shading in shade, in patches (E052, E053) |
| `far_mode`, `far_pencil`, `pencil` | 1, 0, 0 | Distance shading as wash, no ruling; the E057 strokes and E052 laid-line pencil off (E059) |
| `shade`, `shade_from` | 0.35, 0.45 | Pencil shading from the hand-shaded tile where it is dark (E059; strength 0.35 chosen by the owner from E061, 2026-11-16) |
| `light_wash`, `light_white`, `light_gamma`, `light_sun` | 0.18, 1.0, 1.3, 4 | Sky-lit faces pale grey, sunlit faces paper (E062) |
| `wash_max`, `wash_gamma`, `black_point`, `black_range`, `black_patch`, `wash_speckle`, `speckle` | 0.85, 1.0, 0.06, 0.2, 0.7, 0, 0.04 | A striking dark end: darker mid-darks, solid black where light is very low, specks only in black (E066) |
| `white_line_tone`, `white_line_core`, `white_line_break`, `speck_tone`, `speck_soft`, `black_texture` | 0.55, 0.55, 0.4, 0.25, 0.7, 0.1 | Quiet white lines and texture in black (E067, E068), the lines and specks themselves off: `white_lines` 0, `speckle` 0 (owner, 2026-11-16) |
| `wash_white`, `wash_black`, `wash_gamma`, `wash_max`, `black_point`, `black_range`, `light_wash` | 0.95, 0.25, 0.8, 0.95, 0.1, 0.3, 0.36 | The curve shifted to the dark end (E070; supersedes the E066 values in the row above) |
| `player_light`, `player_radius`, `player_falloff` | 0.6, 10 m, 1.5 | The player's light (E070) |
| `fall_white`, `fall_black`, `fall_gamma`, `rule_dist_len`, `fog_ao` | 0.3, 0.04, 2.4, 380 m, 1.2 | A long, mostly grey fall into black (E059) |
| `halftone`, `halftone_pitch`, `halftone_soft`, `halftone_angle`, `halftone_max_pitch` | 0.05, 1 cm, 0.8, 30 deg, 5 cm | Subtle round-dot halftone in mid to dark tone, near surfaces only (E040, E044) |
| `shadow_soft`, `shadow_soft_max`, `shadow_soft_min` | 0.06, 1 m, 15 cm | Soft shadow edges widening with distance from the occluder, continuous across cascades (E041, E047) |
| `tone_noise` | 0 | The light's 15 m noise made blotches in the smooth wash (E048) |
| `fall_white`, `fall_black`, `fall_gamma` | 0.28, 0.05, 1.5 | Ruling only in dark tone |
| `rule_near`, `rule_fade_in`, `rule_down_gain`, `rule_dist_len` | 40 m, 150 m, 1, 300 m | Nothing near; gathers looking down; black only far down (E035) |
| `rule_target_px`, `rule_density` | 9 px, 3 | Sparse at light tone, closing up as it darkens |
| `rule_profile`, `hatch_break_len` | 0.4, 4 m | Fewer wedges and stroke ends |
| `shadow_hatch_near` | 0 | Far cast shadow is ruled and closes into black |
| `bounce`, `bounce_sky` | 0.6, 0.3 | Shade next to lit surfaces lifts to mid grey (E037) |

`StylePreset::scene_default` returns `nihei_ink` for `shaft` and `nihei_ink_small` for anything else.

Rule fields not listed take the defaults above. Longer runs (`hatch_break_len` 2) matter beyond looks: with short runs the ragged ends cost more coverage at mid distance than the compensation assumes (E029).

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

**Per-pass GPU time, E069** (`mega_capture --bench 40 --timing`, median; timestamp queries; on this tile-based GPU pass start timestamps overlap the previous pass, so each pass is its end minus the previous end; the composite's end timestamp reads zero and is not measured). Labyrinth round 2 (413k triangles, 46.6k strokes). Before culling, 2560x1440 / 3024x1832: shadows 0.6 ms in all; forward 7.4 to 9.0 / 12.1 to 14.4 ms; contours 0.6 / 1.0 ms; ink lines (strokes and contour resolve) 0.9 to 1.3 / 1.3 to 1.9 ms. The forward pass is the frame, and it is fragment work: a whole-scene depth-only pass costs 0.07 ms, and a depth prepass saved nothing (Apple's hidden-surface removal already shades each pixel once). Toggling features in the forward shader at 1440p on `lab_spawn` (min of 4 runs, others sharing the GPU): 6.6 ms in all, 2.9 ms with soft shadows, pencil, paper, halftone and the black texture all off; soft shadows about 1.45 ms, the black texture 1.0, pencil 0.9, paper 0.7, halftone 0.2. After culling and with the E070 look: forward 5.2 to 6.0 / 7.7 to 9.0 ms, frames 6.4 to 7.3 / 9.5 to 10.9 ms (`lab_spawn`, `lab_far_ledge_axis`, `lab_undercroft_shaft`, `lab_corridor_slot`, `lab_void_bottom_up`). Part of the forward difference is the owner's game no longer sharing the GPU; culling itself saves vertex work and CPU, little GPU time on this machine.

### Culling (E069)

`crate::cull`: at upload the index buffer is regrouped into spatial chunks (24 m grid cells by triangle centroid; 1187 chunks in labyrinth round 2), each one contiguous range with a bound; strokes likewise. Each frame: the main pass draws only chunks whose bound is inside the camera frustum (20 to 58% of triangles on the landmark views); each shadow cascade draws only chunks overlapping its square in light space (all depths kept, so off-screen casters still cast); the far cascade (fitted to the scene) is not redrawn while its matrix is unchanged, i.e. while the sun is still; the CPU stroke cull skips whole chunks outside the frustum or beyond the farthest rank fade. `Renderer::set_culling(false)` and `mega_capture --no-cull` draw everything. Images with and without culling: identical depth and shadow maps; up to 9 pixels in a 1280x720 frame differ by at most 16 levels, as 2x2 quads on dense geometry (the arch soffit), because the main pass is split into several draws; the cached far cascade frame is identical to the drawn one. A depth prepass is available (`set_depth_prepass`, `--prepass`), off by default. When a scene supplies its own chunks (per space, with portals) they should replace the grid; see Open questions.

### Bounce bake in the background (E069)

`update_bounce` starts the bake on a worker thread and returns; the frame draws without bounce until the bake arrives, then fades it in over 0.4 s (`G.misc.w`). A re-bake for a moved sun keeps the old bounce until the new one arrives. `Renderer::wait_for_bounce()` blocks and shows it at once; `mega_capture` calls it so captures are final from the first frame. The game needs no change.

Since E059 (best of three, 1280x720 / 2560x1440): `shaft_rim_down` 2.64 / 6.29 ms, `shaft_gallery_down` 2.23 / 6.50 ms, `shaft_hall_deep_in` 1.75 / 5.57 ms, `shaft_spawn` 1.99 / 6.09 ms: faster than E057 (the procedural strokes are off).

Since E057 (best of three, 1280x720 / 2560x1440): `shaft_rim_down` 3.21 / 7.38 ms, `shaft_gallery_down` 2.66 / 7.83 ms, `shaft_spawn` 2.24 / 6.97 ms; the procedural strokes cost 0.4 to 0.8 ms more than the E054 marks on views looking down.

Since E054 (`far_mode` 1, best of three, 1280x720 / 2560x1440): `shaft_rim_down` 2.80 / 6.84 ms, `shaft_gallery_down` 2.43 / 7.07 ms, `shaft_spawn` 2.14 / 6.62 ms; with the ruling (`far_mode` 0) 3.62 / 7.50, 2.97 / 8.06, 2.25 / 6.82 ms. Dropping the ruled layer saves more than the marks cost.

Since E051 to E053, `nihei_ink`: `shaft_spawn` 2.23 / 6.81 ms, `shaft_hall_enter` 2.35 / 7.22 ms, `shaft_rim_down` 3.58 / 7.40 ms at 1280x720 / 2560x1440 (best of three), about 0.2 to 0.3 ms and 0.8 to 0.9 ms more than E049, mostly the pencil layer's extra samples in shade.

Since E044 to E048, `nihei_ink` (best of three; `shaft_spawn`, `shaft_rim_down`, `shaft_hall_enter`, `shaft_gallery_down`): 2.00 to 3.28 ms at 1280x720 and 5.95 to 7.17 ms at 2560x1440 (6.70 to 7.91 ms at E042): the paper and halftone now evaluate only the layers that can be visible; the contour line fit adds a little.

Since E040 to E042, `nihei_ink` (best of three, same views as below: `shaft_spawn`, `shaft_rim_down`, `shaft_hall_enter`, `shaft_gallery_down`): 2.16 to 3.32 ms at 1280x720 and 6.70 to 7.91 ms at 2560x1440; `still_hall` (`nihei_ink_small`) 1.20 and 4.08 ms. About 0.6 to 1 ms and 2 to 2.5 ms more than E038: twelve paper layers and eight halftone pitches are evaluated per fragment (most skipped by their window), plus the soft shadow's 36 blocker loads and 25 or 50 comparison taps on lit faces near shadow.

Default since E038, `--bench 200` at 1280x720 and `--bench 100` at 2560x1440, lowest of three runs, over `shaft_spawn`, `shaft_rim_down`, `shaft_hall_enter`, `shaft_gallery_down`: `nihei_ink` 1.52 to 2.63 ms and 4.44 to 5.60 ms; `nihei_wash` on the same views 1.54 to 2.16 ms and 4.30 to 6.36 ms (the new grain evaluates four 2D fields of two lattices each, the old one two 3D fields; distance ruling skips the ruled layer near the eye, which is why the hall is faster). `still_hall` with `nihei_ink_small`: 1.05 and 3.54 ms. The bounce bake (0.5 s for the shaft, 20 ms for the test scene) happens on the first frame and when the sun moves, and is not in these figures (`bounced-light.md`).

Tone models (E027 to E034), `--bench 200` at 1280x720 and `--bench 100` at 2560x1440 over `shaft_spawn`, `shaft_rim_down`, `shaft_hall_enter`, `still_hall`: `nihei_vast` 1.19 to 1.80 and 3.74 to 4.96 ms; `nihei_fall` 1.66 to 2.32 and 4.54 to 7.44 ms; `nihei_wash` 1.15 to 2.03 and 3.74 to 6.15 ms; `nihei_hybrid` 1.39 to 2.70 and 4.31 to 8.89 ms. The ruled layer evaluates five lines per fragment where the band model evaluated two. Stability for the new presets is in E027 in `experiments.md`.

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

- `crates/render/src/paper.rs`: the generated fibre tile and the scanned paper preparation (`PAPER_KINDS`, `PAPER_ASPECT`, `kind_mips`).
- `assets/paper/`: the scanned CC0 paper maps and their README.
- `crates/render/src/style.rs`: `StylePreset`, `nihei()`, `nihei_vast()`, `nihei_fall()`, `nihei_wash()`, `nihei_hybrid()`, `nihei_ink()`, `nihei_ink_wash()`, `nihei_ink_small()`, `for_scene` (with `MEGA_PRESET`), `scene_default`, `by_name`, GPU packing.
- `crates/render/src/renderer.rs`: targets, pipelines, the bounce vertex stream and its lazy re-bake (`update_bounce`, `bounce_bake_ms`), CPU stroke cull, shadow resources and passes, `DebugView`, `Sun`, `HatchLod`, `StrokeVisibility`, `ShadowFilter`.
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
- Culling (E069): the grid chunks are a stopgap. With per-space chunks from the generator (`MeshData.chunks`: space id, bounds, index range, stroke range, triangles and strokes sorted by chunk) and portals on the `Scene` (two space ids and a quad), a potentially visible set by flooding from the camera's space through portals inside the frustum would cut enclosed scenes much further; it is the main remaining saving on vertex work. On this GPU the real cost is the forward fragment shader (soft shadows, pencil, the black texture, paper): cheaper soft shadows (`textureGather` in the blocker search, an early-out where the 3x3 filtered visibility is fully lit far from any edge) and evaluating the pencil tile once for both its uses are the next savings.
- Paper (E049): which scan, and how strong, is the owner's call (sheets E049, E050). The scans' tooth is shown at a constant few pixels, not at paper's true scale. Paper004's laid lines are directional and, rotated per layer, read as faint diagonal hatching.
- Contours (E045): close parallel lines (double silhouettes of columns) and lines steeper than 11 degrees from an axis keep the old resolve and still crawl a little; railing posts thinner than a pixel draw their strokes over the background, which the reprojection metric counts as error (a metric artefact, not shimmer). The halftone's amount and its 3.5 to 11 pixel window are tuned by eye on stills only; the owner judges at fullscreen in motion. Both cost about 1 ms at 1440p and could be cut (fewer layers, or the window computed once rather than looped).
- Soft shadows (E041): the blocker distance is a mean over taps in the 12 cm scene cascade, so a penumbra next to two occluders at different distances is a compromise; thin occluders (railings) still give dotted shadows at range, now softened rather than fixed.
- Tone models (E027 to E034): the owner chose the wash (2026-11-16) and `nihei_ink` is the default since E038. Open: whether the distance ruling's amount is right (E035 sheet); the E036 grain is off since E040; the rotation-invariant footprint is that of the image centre, so near the image corners on a close floor the per-axis grain is slightly anisotropic on screen (radial elongation); `paper_texture` is still screen-anchored; the old grain's surface streaks are fixed only with `screentone_aniso`; the wash has no chipped white along edges (needs edge distance, as for strokes stopping short of creases); the heavy end of shadow strokes is set from the sun's projected direction, checked only with a high sun; tone is conserved across distance only to about +-8% (E029), worst where patch runs are short in pixels; the hybrid costs up to 8.9 ms at 2560x1440.
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

### Review notes, 2026-11-16, third set (Cat, on the hatch strokes, relayed by the orchestrator)

- "The hatching is just too evenly weighted - the ends seem to taper down to a point suddenly like a blade, but there's very little variation in line weight otherwise. In the references it seems more like the light end is a lot thinner and lighter. And the spiky dagger end isn't there. The hatching is a lot finer in general in the references."
- "Is the thickness in screen space? As I move backwards from the wall it seems to stay about the same thickness which feels wrong. And then when you get further away they get much closer together as a result and start to blend together into thicker lines that feel jagged and daggery at the ends."
- Diagnosis confirmed: width was held in rotation-invariant pixels with a cap while pitch shrank with distance, so across an octave width/pitch doubled, tone darkened as one backed away and neighbours fused into blades. Addressed in the continuous tone models (E028, E029): width a fraction of the pitch, coverage conserved across octaves, a weight profile over the whole stroke ending in a hairline, variation between strokes, finer ruling at high resolution. `nihei` and `nihei_vast` keep the old strokes so the sheets can show "current"; the new strokes are in every new preset.

### Review notes, 2026-11-16, fourth set (Cat, on the E027 to E034 sheets)

- "Hmm ok the wash is definitely the nicest. I do like the hybrid a bit but the hatching is still kind of ugly. I guess I'd like it more if it were more sparse still, and mainly appeared as you looked into the distance down the shaft. The hatching under the stairs is still a bit over the top, but perhaps light transfer would help, because I don't think it'd realistically be that dark."
- Done: the wash is the base of the new default `nihei_ink`; ruling is a sparse distance effect (E035); bounced light lifts the shade under the stair (E037, `bounced-light.md`).

### Review notes, 2026-11-16, fifth set (Cat, `nihei_wash` in the window)

- "The wash is a really nice effect and a great base layer btw! The only issue also is that it swims as you walk around, and seems to suddenly change at certain distances for some reason, meaning that there are straight lines where you can see it changing detail - it looks a bit like tearing but I think it's just the wash becoming more coarse or something. I like the effect at rest."
- Causes and fixes in E036: the grain's octave pairs swapped seeds at every integer octave (the straight lines), and its noise lattice was sub-pixel, so it aliased under walking (the swim); rotation probes could not show either.

### Review notes, 2026-11-16, sixth set (Cat, on the wash grain)

- "I like the wash on the whole but it does feel a bit harsh and noisy still compared to a real comic. I think that's because it's just on/off (bands of one greyscale color and white with a hard edge between them). The noise itself feels a little unnatural too, but maybe it's just the hard edges I'm noticing."
- Done in E036: the grain is a soft, low-amplitude modulation of a continuous grey (`screentone_soft`, `screentone`), with a coarser, slightly clumped lattice; a ladder from the old hard grain to pure gradient is on the E036 sheet. Not tried: a different noise character (blue noise, fibre).

### Review notes, 2026-11-16, seventh set (Cat, on E035 to E039 and the windowed game)

- "Whoa, yeah the new walk is a lot better. I can see some transitions based on distance but I probably wouldn't notice if I wasn't looking for them. Mainly, the grain still looks a bit too much like TV static to me, especially over large areas in the distance. I think if we could go more for a textured paper + subtle halftone look it would be nicer."
- E036: "it's hard to tell because right now the grain looks too much like TV static or a QR code. Actually, the completely smooth version looks the best, but I would like the end result to be more textured."
- E038: "the new default reads much more like a calm drawing than the previous attempts"
- E035: "the default actually looks the best right now. I still think the hatching needs a bit of work, but since it's a lot sparser and no longer the primary shading method, it's worth putting it aside for now and focusing on the grain"
- E037: "The default and strong bounce both look good actually"
- E039: "the new way is a lot better and sells me more on the gradient+grain shading than trying to use the hatching for everything. The shadow edges feel a bit too hard though, if they were soft it might give it that nice gradient look from the comics too."
- "in general I'm finding the stills quite pretty now but when I fullscreen the app and walk around it looks a bit more jank still. I think that might be the noisy grain though."
- On anchoring the paper: "I suspect the paper texture being in screen space is going to look pretty odd when you move around unfortunately, but keeping it in world space has its own problems (it all gets compressed together in the distance). So that's a tricky one to solve! I think the solution is going to be somewhere in between like the current grain."
- Done: smooth wash with surface-anchored paper and a soft halftone (E040), soft shadow edges (E041), new overview (E042). Ruling and bounce left as they were.

### Review notes, 2026-11-16, eighth set (Cat, on E040 to E042 and the windowed game)

- "Hmm that's an improvement again, but in the distance the halftone circles get quite large and it starts to look a bit like snakeskin. And the GI looks a bit coarse and blotchy too actually which probably doesn't help. Also, at a distance the shadow under the stairs still has a very hard edge, I took a screenshot where it's just at the distance so it's half and half. I'm actually curious about how much the paper fibre texture and contour shimmer will help so it might be worth moving onto that first before worrying about it too much, but tbh the size of the halftone might just need to be capped in the distance too." (screenshot 20261116-115151)
- "Yeah I'm seeing a 'photoshop clouds' effect too that I forgot to mention ... It looks fine most of the time but when it's very obvious it looks a bit tacky."
- On fibre with or without clouds: "It might honestly combine well with the paper texture so it might be worth a comparison, but I'm leaning towards removing it."
- Done: fibre paper with no clouds by default (E044), near-axis contour fit (E045), contrast-relative pop metric (E046), halftone cap (E044, E047), soft shadow continuous across cascades (E047), smoothed bounce and no tone noise (E048).

### Review notes, 2026-11-16, ninth set (Cat, on E044 to E048)

- "1. Snakeskin, hard far edges and blotches gone (at least I'm not currently noticing anything like that as a big issue!) 2. I can't even notice the faint clouds so maybe let's just scrap it 3. The shading in really nice and smooth now, I think that's the best part currently tbh. So the paper fibre is a good addition, but it looks a bit like worms. Are we just synthesizing that ourselves? I wonder if we can just find a CC0 paper texture and see what that looks like (although I dunno if we can find a very textured paper like watercolor paper or something). AmbientCG actually has a few paper textures if you search, for example this one looks like it might be interesting on our walls in combination with the gradient shading: https://ambientcg.com/view?id=Paper002"
- "I actually thought all the surfaces on ambientCG were processed to tile already, so it might just work out. Let's try that one first and see how it looks, but it might be worth grabbing some other paper materials from there and comparing them too. It looks pretty nice in their preview so it seems promising!"
- Done: clouds removed; scanned paper with Paper002 as default and three other ambientCG candidates (E049, E050). The maps did tile as shipped.

### Review notes, 2026-11-16, tenth set (Cat, on E049 and E050)

- "Ok whoa. So Paper001 really looks the best, it's like watercolour paper as you say. I also think paper_kind=3 looks interesting too, it has a 'pencil shaded' effect which I'm very curious about. Paper001 looks especially good in the distance, but when I get closer the normal maps get really harsh and it takes on a really harsh shaded look, see the last two screenshots, the first one is the point at which it looks honestly great, and the second is just when the harsh lighting comes in. I'm not sure how to fix it but if we could have it look like the distant version in the distance but a lot softer up close with a less obvious boundary it could be really promising. I'm honestly wondering if we could use Paper004 as a kind of cross-hatching in shaded areas, or perhaps even add it as a subtle layer on top of Paper001. It'd have to blend in well either way though." (screenshots 20261116-124919 and -124923)
- On the pencil layer: "I think it might work if we carefully fade it in in patches. I think tbh our existing hatching effect is just a bit clumsy."
- Done: Paper001 default, soft at every distance and calmest near (E051, E052); Paper004 pencil shading in shade, in patches (E052, E053).

### Review notes, 2026-11-16, eleventh set (Cat, on E051 to E053)

- "Ok that looks pretty nice now! It might need tweaking but the defaults are looking really promising. I'd really like to try getting the distance shading replaced next if we can experiment with that."
- Earlier, on the ruling: "I think tbh our existing hatching effect is just a bit clumsy."
- "Btw I actually think the paper grain might be a bit too strong, and the pencil might be a bit too weak, but I'm interested about replacing the existing hatching with it before deciding on the overall balance."
- "It's a bit tricky because I mainly want the pencil shading to show up in the darkest areas, and right now a lot of the main area of the shaft is in direct light! So it's going to take some comparisons of different locations. The deepest part of that giant hall are probably the best places to compare the pencil shading, and the distance at the bottom of the shaft once we get that in."
- Done: distance shading as wash and pencil marks (E054), ruling kept as `far_mode` 0; near pencil moved to the dark end of the wash (`pencil_from` 0.4, `pencil_to` 0.75); deep-hall camera paths for comparing it (E056). Finding: the deep hall is light to mid grey and nearly uniform (walls about 172 to 184 of 255 at 1280x720), so it gives the pencil little dark tone to grade across; that is the hall's lighting and openness (generator, bounce), not something the shader should compensate. Near defaults not retuned; E055 shows a shifted balance (paper x0.6, near pencil 0.3) for the owner to judge.

### Review notes, 2026-11-16, twelfth set (Cat, on E054 to E056)

- "1. The wash + marks looks good actually. Although I think the pencil marks appear a bit too suddenly and are pretty much fully black when they do, when you look down. 2. I like the far_pencil lower because of how sudden it looks otherwise, but tbh it does just look like something alpha fading in rather than being gently shaded in in pencil as you go further down. 3. The middle column of E056 actually looks the best I think, the right column is a bit too much. Also the defaults are getting pretty close now"
- Done: wash + marks kept and the marks rebuilt as strokes that build up like pencil (E057, E058); near defaults unchanged (E056 middle column). Every scene other than `test` now gets `nihei_ink` and the shaft's sun.
- Not done: the ruling code stays, because `nihei_fall`, `nihei_wash` and `nihei_hybrid` still use it (`far_mode` 0 is only their default; `nihei_ink` no longer uses it). The per-facet banding on round columns (`shaft_hall_deep_back`) comes from flat-shaded facets with per-facet normals and ao from the generator; smoothing it needs smooth normals (or a per-part normal) from `mega_gen`, noted for the generator work.

### Review notes, 2026-11-16, thirteenth set (Cat, on E057 and E058)

- "Hmm, better, but the lines are a bit too evenly spaced so it doesn't look naturally shaded by a person. It just looks like random lines distributed out evenly. And it still doesn't seem like there's much dynamic range in how they're shaded exactly - it's mostly just black and it fades out suddenly near the top still. It should be more greys tbh and only really black near the completely black bottom. I'm actually not too sure about the procedural lines now, this is definitely an improvement but I do wonder if the approach of using a texture which blends in patches where it's really dark (e.g. very deep areas and around corners etc) would be a better approach."
- Done: a longer, mostly grey fall into black, and pencil shading from a generated hand-shaded tile laid in patches wherever it is dark, replacing the procedural strokes and the laid-line pencil (E059 to E061).

### Review notes, 2026-11-16, fourteenth set (Cat, on E059 to E061 and the windowed game)

- "Ok now *that's* really getting there. In your last comparison I think the shade 0.35 version might actually be the best ... Although the defaults are great too."
- After trying it live: "Ok yeah that's the one I think. The light sections are actually a bit too light for my tastes, in the open section of the shaft. That might be down to the scene though and not the shading. Also, perhaps HDR tone mapping would help there."
- Done: `shade` 0.35 (committed by the orchestrator); a light end with a pale wash on sky-lit faces and paper kept for sunlit ones (E062, E063).

### Review notes, 2026-11-16, fifteenth set (Cat, on the labyrinth)

- "The extra detail really helps actually, the labyrinth is basically a way nicer version of the shaft scene!"
- On the light end: "Yep looks better as is. The dynamic range still isn't quite there but I think honestly maybe the sun level just needs to be lowered too ... Auto-exposure might help too but it makes comparing different locations a bit trickier so it might be worth turning off for comparisons."
- On the labyrinth being dark: "It's dark but that's not actually a bad thing. In fact, it's dark in such a way that it never quite goes really black, which manga often does to really accentuate the dark areas (especially 'Blame!'). Do we have an ambient term or something?"
- "Can you try and get it a bit darker? I'd like a really striking dynamic range. If the completely sealed corridors get pitch black perhaps we can improve that with some kind of local lighting, but it might just be reasonable for now."
- Done: a darker dark half and solid blacks where the light is very low, with white lines and specks inside (E066). Local light noted, not built. The soft dark smudges around blocks on the labyrinth's void walls come from the baked per-vertex AO (they ease with `dark_ao` 0 and stay with bounce, soft shadows or pencil off): a generator-side bake matter.

### Review notes, 2026-11-16, sixteenth set (Cat, on E066)

- "Wait, what's with the completely white edges?"
- "I'm honestly not sure about the white speckles anymore either tbh. Perhaps if they were grey and blended in with the wall more it'd be nice. (It might be better to remove them but I'd like to see what the options are first because the extra detail is nice.)"
- Done: restrained white lines only over near-solid black, with a gradual change-over (E067); grey flecks and a pencil sheen inside black instead of white specks (E068).

### Review notes, 2026-11-16, seventeenth set (Cat, on E067 and E068, and after exploring the labyrinth)

- After running `MEGA_SET="white_lines=0,speckle=0"`: "The first one with white_lines and speckle at 0 is best, can we make that the default? I'd leave the code in for now in case we decide to try it again in the future. Overall I think we still need more range - the curve should shift more towards the dark end I think, with the really bright places being highlights and the darkest darks being even darker."
- "I want the darks to be really dark tbh. If it's a problem in the corridors, we could give the player themselves a weak point light so they can still see in the immediate vicinity - the dark souls and king's field games do this for example."
- They explored the labyrinth, had a lot of fun, and want it much bigger and more detailed once dynamic range and culling are solved.
- Done: white lines and specks off by default; the curve shifted to the dark end and the player's light (E070); culling, per-pass timing and the background bounce bake (E069).

## Related experiments

E001 (temporal metric), E002 (hatch octave selection), E003 (stroke depth tolerance), E004 (distance behaviour of lines), E005 (contour anti-aliasing), E006 (tent versus box coverage), E007 (tone and lighting for the test scene), E008 (distance on the 600 m shaft), E010 (tone and light for the shaft), E011 (shadow treatment), E012 (shadow edge and moving sun), E013 (hand-cut edge), E014 (uneven hatching), E015 (crosshatch angle), E016 (graphic blacks and white ink), E017 (abyss and sky), E018 (before and after), E019 (hatch stroke ends and patches), E020 (swimming under rotation), E021 (shadow as hatching, rare black), E022 (hatched shadow edge), E023 (crosshatch), E024 (feathered boundaries), E025 (nested patches), E026 (owner's frames), E027 to E034 (tone models, strokes, distance and level of detail, owner's frames in each model, merge into black, screentone, cast shadows, near views), E035 (ruling as a distance effect), E036 (wash grain: seams, swimming, softness), E037 (bounced light), E038 (the new default), E039 (the owner's frames in it), E040 (paper and halftone), E041 (soft shadow edges), E042 (overview), E043 (walking pops with bounce on), E044 (fibre paper), E045 (contour crawl), E046 (contrast-relative pops), E047 (the owner's frame 115151), E048 (bounce blotches), E049 (scanned paper), E050 (paper strength), E051 (Paper001 up close), E052 (the owner's frames 124919 and 124923), E053 (pencil shading in patches), E054 (distance shading as wash and pencil), E055 (far pencil strength), E056 (pencil in the darkest places), E057 (far pencil build-up), E058 (its onset), E059 (texture shading and the grey fall), E060 (the shading tile), E061 (shading strength), E062 (the light end), E063 (its exposure), E064 (the labyrinth), E066 (blacks), E067 (white lines in black), E068 (inside the black), E069 (culling and per-pass timing), E070 (the dark end and the player's light).
