//! Style presets: every tunable of the ink look in one place.
//!
//! A preset is plain data. The renderer copies it into a uniform buffer each frame, so a
//! second look (for example a colour preset) is only a different set of values. Pixel-sized
//! values are given for a 720-pixel-high image and scaled with the output height, so a
//! capture at 1440p reads like the same drawing at a finer pen, except `stroke_min_px`, which
//! is a hard floor in real pixels.
//!
//! See docs/features/ink-renderer.md for what each field does and why it has its value.

use bytemuck::{Pod, Zeroable};

#[derive(Clone, Debug, PartialEq)]
pub struct StylePreset {
    // Paper and ink.
    /// Paper colour, display values. Off-white.
    pub paper: [f32; 3],
    /// Black ink colour, display values. Off-black.
    pub ink: [f32; 3],
    /// Strength of the screen-anchored paper grain, 0 disables it.
    pub paper_grain: f32,

    // Atmosphere.
    /// Tone that distance fades towards.
    pub fog_colour: [f32; 3],
    /// Distance in metres before any fade.
    pub fog_start: f32,
    /// Exponential density per metre past `fog_start`.
    pub fog_density: f32,
    /// Largest fade factor (1 erases distant structure completely).
    pub fog_max: f32,
    /// How far the fog colour darkens towards ink where the surface behind it is
    /// occluded: the fog takes `mix(fog_colour, ink, fog_ao * (1 - ao))`. Air deep in a shaft
    /// is not lit, so an abyss fades to dark rather than to paper. Sky is unaffected.
    pub fog_ao: f32,
    /// How far the fog colour darkens towards ink over solid black (shadows, pools), so a
    /// black shape stays black into the distance instead of lifting to grey.
    pub fog_black: f32,

    // Light and tone bands.
    /// Weight of the sun term `intensity * max(N.L, 0)` in the light value.
    pub sun_strength: f32,
    /// Weight of the ambient term `ao` in the light value.
    pub ambient: f32,
    /// How much the sun term is scaled by the baked `ao` (0: not at all, 1: fully). With no
    /// shadow pass this is the only thing keeping sunlight off the bottom of a deep shaft.
    pub sun_occlusion: f32,
    /// Light below this is hatched (one layer).
    pub band_hatch: f32,
    /// Light below this is crosshatched (two layers).
    pub band_cross: f32,
    /// Light below this is solid black.
    pub band_black: f32,
    /// Per-line random offset added to the light value before thresholding, so band edges
    /// end in ragged line ends instead of a clean contour.
    pub band_jitter: f32,
    /// Length in metres (at the finest line level) over which that offset varies along a line.
    pub band_jitter_len: f32,

    // Ruled hatching.
    /// Finest line spacing in metres (octave 0).
    pub hatch_spacing: f32,
    /// Line pitch in pixels the octave selection aims for.
    pub hatch_target_px: f32,
    /// Hatch line width in pixels.
    pub hatch_line_px: f32,
    /// Relative random variation of line width per line.
    pub hatch_width_jitter: f32,
    /// Breakup segment length in metres at the finest line level; coarser lines scale it by
    /// two per level.
    pub hatch_break_len: f32,
    /// Fraction of each line lost to gaps, 0..1.
    pub hatch_break_gap: f32,
    /// Highest octave. Past it the lines fade to their mean tone.
    pub hatch_max_octave: f32,
    /// Line weight grows with darkness inside a band: width is multiplied by
    /// `1 + gain * d`, where d runs 0..1 from the band's own threshold down to the next
    /// darker one. Lines thicken towards black and merge into it.
    pub hatch_width_gain: f32,
    /// Patch hatching (0 off, 1 on): lines are laid in columns of `hatch_patch_lines` that
    /// start together and stop together (with a small per-line jitter), in runs about
    /// `hatch_patch_len` breakup lengths long staggered between columns, instead of each
    /// line breaking up independently. Band-edge jitter is shared per run too.
    pub hatch_patch: f32,
    /// Patch column width in lines (at the drawn octave).
    pub hatch_patch_lines: f32,
    /// Patch run length in units of `hatch_break_len` (at the drawn octave).
    pub hatch_patch_len: f32,
    /// Length in pixels over which a hatch line's width tapers to nothing at its trailing
    /// end (a third of it at its starting end). 0 gives hard, anti-aliased ends.
    pub hatch_taper_px: f32,
    /// Fraction of each crosshatch line lost to breaks (the first layer uses
    /// `hatch_break_gap`); higher leaves the crosshatch as patches over the first layer.
    pub cross_break_gap: f32,
    /// Feathering of tone boundaries, in line spacings: each stroke's end at a band edge is
    /// moved along the line by its own random amount, `hatch_feather * spacing * 2^level *
    /// (h - 0.35)` metres (h uniform per line), using the light gradient along the line. Lines
    /// that survive to coarser octaves move further, so every second or fourth stroke runs on
    /// past the others and the edge thins out instead of stopping on a line. 0 disables.
    pub hatch_feather: f32,
    /// Jitter of where a stroke starts and ends inside its patch run, as a fraction of the
    /// run (patch hatching), so a group of strokes has a ragged start and end, not a ruled one.
    pub hatch_end_jitter: f32,
    /// Feathering of the cast shadow edge in metres: each shadow stroke tests the shadow at a
    /// point moved along the stroke by its own random amount (up to this, either way), so the
    /// shadow ends in staggered stroke ends. 0 gives a crisp cut-off along the shadow edge.
    pub shadow_feather: f32,
    /// Fraction of each stroke lost to breaks inside a cast shadow (elsewhere
    /// `hatch_break_gap`): lower makes shadow hatching read as one continuous dark shape.
    pub shadow_break_gap: f32,
    /// Taper length of a hatch stroke's ends as a multiple of its width (at least
    /// `hatch_taper_px`), varied by +-40% per stroke; one edge of the stroke stays straight
    /// while it thins.
    pub hatch_taper_width: f32,
    /// Largest hatch line width in pixels (at 720 lines): near walls are drawn with more
    /// lines, not thicker ones.
    pub hatch_width_max_px: f32,
    /// Direction of the hatching inside a cast shadow: 0 the surface's own hatch axis, 1
    /// along the sun's direction projected onto the surface.
    pub shadow_hatch_dir: f32,
    /// The crosshatch layer's target pitch relative to the first layer's.
    pub cross_pitch_scale: f32,
    /// The crosshatch layer's line width relative to the first layer's.
    pub cross_width_scale: f32,

    // Strokes.
    /// Width in pixels of a weight-1 stroke at `stroke_depth_ref`.
    pub stroke_width_px: f32,
    /// Minimum drawn width in real pixels; thinner strokes keep this width and lose coverage.
    pub stroke_min_px: f32,
    /// Distance in metres at which the depth falloff is 1.
    pub stroke_depth_ref: f32,
    /// Width scales as `(depth_ref / z) ^ exponent`.
    pub stroke_depth_exp: f32,
    /// Upper bound of that falloff factor (how much nearer strokes may widen).
    pub stroke_falloff_max: f32,
    /// Length in pixels over which a stroke tapers at a real end.
    pub stroke_taper_px: f32,
    /// Width at the very end as a fraction of full width.
    pub stroke_taper_min: f32,
    /// Amplitude of the seeded width noise along the stroke, relative.
    pub stroke_noise_amp: f32,
    /// Wavelength of that noise in metres.
    pub stroke_noise_len: f32,
    /// How far a stroke runs past its real ends, in pixels.
    pub stroke_overshoot_px: f32,
    /// Visibility tolerance: a stroke fragment is hidden when it is farther than the scene
    /// depth at its centreline times `1 + tol`.
    pub stroke_depth_tol: f32,
    /// Per rank (0..3): distance in metres where the stroke starts to fade.
    pub rank_fade_start: [f32; 4],
    /// Per rank: distance where it has gone. Strokes beyond it are culled on the CPU.
    pub rank_fade_end: [f32; 4],
    /// Per rank: projected length in pixels below which the stroke fades out.
    pub rank_min_len_px: [f32; 4],

    // Contours.
    /// Fine-id edges are drawn at full strength nearer than this (metres).
    pub contour_fine_near: f32,
    /// ...and not at all beyond this; only coarse-id edges remain.
    pub contour_fine_far: f32,
    /// Depth edge threshold in units of the relative depth step a plane would produce at
    /// the same N.V; higher finds fewer edges.
    pub contour_depth_k: f32,
    /// Floor on |N.V| used to scale that threshold, so grazing planes do not divide by zero.
    pub contour_nv_min: f32,
    /// Contour ink strength (coarse-id and depth edges).
    pub contour_strength: f32,
    /// Strength of fine-id edges (panel boundaries inside one part), which mostly coincide
    /// with seam strokes.
    pub contour_fine_strength: f32,
    /// Contours start fading at this distance (metres)...
    pub contour_fade_start: f32,
    /// ...and have gone at this one.
    pub contour_fade_end: f32,

    // Sun shadow (see "Shadow pass" in docs/features/ink-renderer.md).
    /// How much the sun shadow takes away the sun term, 0..1. At 0 the shadow pass is not
    /// drawn at all.
    pub sun_shadow: f32,
    /// Ambient term multiplier inside a sun shadow: the light there is
    /// `clamp(ambient * ao * shadow_ambient, shadow_light_min, shadow_light_max)`.
    pub shadow_ambient: f32,
    /// Lower clamp of the light value inside a shadow.
    pub shadow_light_min: f32,
    /// Upper clamp of the light value inside a shadow. Below `band_black` every shadow is
    /// solid black; between `band_cross` and `band_hatch` it is single hatching.
    pub shadow_light_max: f32,
    /// How far the light of faces turned away from the sun moves towards the shadow light
    /// (0: they keep the ambient light, 1: drawn exactly like cast shadow). A blend of light
    /// values, decided per face, so it never mixes two drawings.
    pub shadow_attached: f32,
    /// Amplitude in metres of the world-anchored offset of the shadow lookup, which makes the
    /// shadow edge slightly irregular, like a hand-cut edge. 0 gives the true edge.
    pub shadow_jitter: f32,
    /// Wavelength in metres of that offset.
    pub shadow_jitter_len: f32,

    // Tone composition.
    /// Amplitude of a world-anchored low-frequency noise added to the light value, so a large
    /// surface is not ruled uniformly.
    pub tone_noise: f32,
    /// Wavelength of that noise in metres.
    pub tone_noise_len: f32,
    /// Amplitude of a random light offset per panel (`id_fine`), so tone is decided panel by
    /// panel, aligned with the structure: some panels left white, others ruled hard.
    pub panel_tone: f32,
    /// Line pitch shrinks with darkness inside a band: the target pitch is divided by
    /// `1 + gain * d` (d as for `hatch_width_gain`), so dark ends are denser as well as heavier.
    pub hatch_density_gain: f32,
    /// Indication: the light half of the single-hatch band is lifted to white, except near
    /// shadow edges (within `indication_radius`, on either side), and lit surfaces near a
    /// shadow edge are drawn down into single hatching. 0 is off.
    pub indication: f32,
    /// Radius in metres around a cast shadow within which hatching is kept (indication).
    pub indication_radius: f32,
    /// Baked ao below which surfaces pool into solid black (recesses, the deep shaft).
    /// 0 disables.
    pub pool_ao: f32,
    /// Width in ao of the ramp into that black.
    pub pool_width: f32,
    /// Down-facing faces have their light scaled by `1 - underside_black * (-n.y)`, so the
    /// undersides of bridges and galleries go black.
    pub underside_black: f32,
    /// Strokes and contours that fall inside solid black are drawn in white ink instead, by
    /// this fraction (0: black lines vanish in black, 1: fully white).
    pub white_lines: f32,
    /// Highest stroke rank that turns white inside black (contours always do): 1 keeps
    /// massing and major edges, 3 also panel seams.
    pub white_line_rank: f32,
    /// Strength of sparse white rules scratched into solid black along the first hatch
    /// direction, so a black wall still shows which way it runs. 0 disables.
    pub white_hatch: f32,
    /// Pitch of those rules relative to the hatch pitch.
    pub white_hatch_pitch: f32,
    /// Fraction of cells with a white speck inside solid black, 0 disables.
    pub speckle: f32,
    /// Speck radius in pixels.
    pub speckle_px: f32,
    /// Spacing in pixels of the speck cells (world-anchored, kept near this pitch by octaves).
    pub speckle_cell_px: f32,
    /// Angle in degrees between the crosshatch layer and the first layer (90: exact mesh).
    pub cross_angle: f32,
    /// Random variation of that angle in degrees, +- half of this, per element or per panel.
    pub cross_angle_jitter: f32,
    /// 0: the angle varies per element (`id_coarse`), 1: per panel (`id_fine`).
    pub cross_angle_by_panel: f32,
    /// Sky: `None` keeps the fogged paper of the atmosphere, `Some(x)` is a flat tone from
    /// paper (0) to ink (1).
    pub sky_tone: Option<f32>,

    // Continuous tone models (E027 onwards). `tone_model` 0 keeps the band hatching above;
    // 1 draws ruled hatching whose coverage follows a continuous tone, with an optional
    // wash. See "Tone models" in docs/features/ink-renderer.md.
    /// 0: tone bands (`band_*`, the `nihei` look). 1: continuous ruled tone (`rule_*`,
    /// `fall_*`), with the wash if `wash` > 0.
    pub tone_model: f32,
    /// Level of detail of the ruling: 0 nested octaves (lines drop out by narrowing, the
    /// survivors widen so coverage is conserved), 1 one fixed world pitch with no lines
    /// dropped, handing over to flat tone where the pitch falls below `rule_handover_px`.
    pub rule_lod: f32,
    /// World spacing of the ruling in metres (octave 0 with `rule_lod` 0; the only spacing
    /// with `rule_lod` 1).
    pub rule_pitch: f32,
    /// With `rule_lod` 0: the line pitch in pixels (at 720 lines) the octave choice aims for.
    pub rule_target_px: f32,
    /// How the ruling pitch in pixels follows the output resolution: the target is
    /// `rule_target_px * (height / 720) ^ rule_res_exp`. 1 draws the same picture at every
    /// size; below 1 a larger output gets relatively finer ruling (a finer pen).
    pub rule_res_exp: f32,
    /// Ruling gets denser with tone: the target pitch is divided by `1 + rule_density * c`.
    pub rule_density: f32,
    /// Stroke width in real pixels below which a stroke stops thinning and is
    /// broken into dashes instead, so light tone is a few fine marks rather than grey haze.
    pub rule_min_px: f32,
    /// Weight profile along each stroke, 0..1: heavy at the dark end, thinning steadily to a
    /// hairline at the light end. 0 is a constant width.
    pub rule_profile: f32,
    /// Low-frequency pressure variation of width along each stroke, relative.
    pub rule_pressure: f32,
    /// Variation of weight between strokes, relative.
    pub rule_vary: f32,
    /// Pitch in pixels (at 720 lines) below which ruling hands over to flat tone (over a
    /// factor of two).
    pub rule_handover_px: f32,
    /// Ruled tone curve: light at and above this is paper.
    pub fall_white: f32,
    /// Light at and below this is full coverage (solid black).
    pub fall_black: f32,
    /// Exponent of the curve between them (above 1 keeps mid tones light).
    pub fall_gamma: f32,
    /// Depth into the structure: the light is multiplied by `ao ^ dark_ao` (0 off).
    pub dark_ao: f32,
    /// Distance from the eye before light starts to fall off (metres).
    pub dark_dist_start: f32,
    /// Falloff length: the light is multiplied by `exp(-(d - start) / len)` (0 off).
    pub dark_dist_len: f32,
    /// Falloff with depth below the eye: the light is multiplied by
    /// `exp(-max(eye_y - y, 0) / dark_down_len)` (0 off), so looking down a shaft falls into
    /// darkness while looking across it does not.
    pub dark_down_len: f32,
    /// Wash strength, 0..1 (0: no wash).
    pub wash: f32,
    /// Wash tone curve: light at and above this has no wash.
    pub wash_white: f32,
    /// Light at and below this has the full wash.
    pub wash_black: f32,
    /// Exponent of the wash curve.
    pub wash_gamma: f32,
    /// Darkest wash, as ink coverage 0..1.
    pub wash_max: f32,
    /// How much of the wash is printed as screentone rather than smooth grey, 0..1.
    pub screentone: f32,
    /// Screentone kind: 0 dot screen, 1 grain (mezzotint), 2 both.
    pub screentone_kind: f32,
    /// Screentone anchor: 0 screen, 1 view sphere, 2 surface.
    pub screentone_anchor: f32,
    /// Screentone pitch in pixels (at 720 lines).
    pub screentone_px: f32,
    /// Strength of the screen-anchored paper texture (fibre-scale noise), 0 off.
    pub paper_texture: f32,
    /// Scale of that texture in pixels (at 720 lines).
    pub paper_texture_px: f32,
    /// White specks in dark wash (as `speckle`, but where the wash is dark), 0 off.
    pub wash_speckle: f32,
    /// Width in pixels (at 720 lines) of an ink line drawn along cast shadow edges, 0 off.
    pub shadow_edge_line: f32,
    /// 1: hatching inside a cast shadow is kept only within `indication_radius` of its edge.
    pub shadow_hatch_near: f32,

    // Ruling as a distance effect (E035 onwards). The ruling's coverage is multiplied by a
    // gate that rises from 0 to 1 with the effective distance `de = r + rule_down_gain *
    // max(eye_y - y, 0)` (r the distance from the eye), and the light the ruling reads is
    // darkened by `exp(-max(de - rule_near, 0) / rule_dist_len)`. Functions of the eye
    // position only, so rigid under rotation. The wash does not see either.
    /// Effective distance in metres below which there is no ruling (with `rule_fade_in` > 0).
    pub rule_near: f32,
    /// Length in metres over which ruling fades in past `rule_near` (smoothstep); 0 disables
    /// the gate (ruling everywhere, as before E035).
    pub rule_fade_in: f32,
    /// Weight of depth below the eye in the effective distance: looking down a shaft, walls
    /// gather ruling sooner than walls at the same distance across it.
    pub rule_down_gain: f32,
    /// Darkening of the ruled tone with effective distance past `rule_near`, metres (0 off).
    pub rule_dist_len: f32,

    // Bounced light (E037 onwards; see docs/features/bounced-light.md).
    /// Strength (albedo) of one bounce of sunlight, added to the light value as
    /// `bounce * sun_strength * intensity * sun_bounce` from the per-vertex bake. 0 off (and
    /// the bake is skipped while both strengths are 0).
    pub bounce: f32,
    /// Strength of one bounce of sky light: `bounce_sky * ambient * sky_bounce`.
    pub bounce_sky: f32,
    /// Surface grain with one octave per surface axis (1), so it stays grain-sized on
    /// grazing surfaces instead of stretching into streaks, and with octave-correlated seeds
    /// so octave changes are seamless; 0 the E032 grain.
    pub screentone_aniso: f32,
    /// Width of the screentone threshold's ramp in threshold units: 0 a one-pixel ramp (hard,
    /// two-level grain, as before E036); around 1 the grain is a soft continuous modulation
    /// of the grey with no hard edges.
    pub screentone_soft: f32,
    /// With `screentone_aniso`: weight of the fine, sub-pixel grain lattice (the E032 grain)
    /// against a lattice twice as coarse; 1 is all fine, 0 all coarse.
    pub screentone_detail: f32,

    // Paper, halftone and soft shadow edges (E040 onwards).
    /// Strength of the soft halftone: a rotated dot screen fixed to the surface that carries
    /// part of the mid-to-dark wash, faded by amplitude with its pitch on screen. 0 off.
    pub halftone: f32,
    /// Finest halftone pitch in metres; the coarser layers are 4, 16 and 64 times this.
    pub halftone_pitch: f32,
    /// Softness of the dot edges, in threshold units (0.02 hard, about 0.6 soft).
    pub halftone_soft: f32,
    /// Angle of the dot screen in the surface frame, degrees.
    pub halftone_angle: f32,
    /// Amplitude of the paper's tooth (fibres and fine relief), screen-anchored, on white
    /// paper and in tone alike. 0 off.
    pub paper_tooth: f32,
    /// Scale of the tooth in pixels at 720 lines.
    pub paper_tooth_px: f32,
    /// How much the wash settles into the tooth's hollows (darker there, mostly in mid
    /// greys). 0 a flat wash.
    pub paper_take: f32,
    /// Soft cast-shadow edges: penumbra width on the receiver per metre of distance from the
    /// occluder. 0 the hard edge.
    pub shadow_soft: f32,
    /// Largest penumbra width in metres.
    pub shadow_soft_max: f32,
    /// Paper anchoring: 0 screen (a sheet behind the image), 1 surface (fixed world scales
    /// faded by their size on screen).
    pub paper_anchor: f32,
    /// With `paper_anchor` 1: the finest world cell of the paper relief in metres; coarser
    /// layers double up to 2048 times this.
    pub paper_cell: f32,
    /// Smallest penumbra width in metres with `shadow_soft` > 0, in every cascade (E044).
    pub shadow_soft_min: f32,
    /// Largest halftone dot pitch in metres; coarser pitches fade out, so far surfaces are
    /// smooth tone instead of large dots (E044).
    pub halftone_max_pitch: f32,
    /// Largest texel of the surface paper's fibre tile in metres; coarser layers are not
    /// drawn, so far surfaces fade to smooth tone (E044).
    pub paper_max_cell: f32,
    /// Which paper relief the surface paper uses (E049): 0 the generated fibre tile, 1
    /// Paper001, 2 Paper002, 3 Paper004, 4 Paper006 (ambientCG scans, CC0; see
    /// `mega_render::paper::PAPER_KINDS` and assets/paper/README.md).
    pub paper_kind: f32,
    /// Distance in metres over which the paper's amplitude rises to full (E051).
    pub paper_near_dist: f32,
    /// Paper amplitude at the eye, relative (1: no near calming).
    pub paper_near_amp: f32,
    /// Pencil shading in shade (E052): strength of a second paper relief laid on the wash
    /// in one direction, coming in with the wash's darkness. 0 off.
    pub pencil: f32,
    /// Paper kind of the pencil layer (3: Paper004's laid lines).
    pub pencil_kind: f32,
    /// Wash coverage where the pencil starts to come in...
    pub pencil_from: f32,
    /// ...and where it is at full strength.
    pub pencil_to: f32,
    /// Strength of a second pencil direction at right angles, coming in from `pencil_to`
    /// to full wash (soft crosshatching). 0 off.
    pub pencil_cross: f32,
    /// Direction of the pencil lines in the surface frame, degrees from the hatch axis.
    pub pencil_angle: f32,
    /// How much the pencil arrives in patches (0: a pure function of tone; 1: patches), E053.
    pub pencil_patches: f32,
    /// Size of the pencil patches in metres.
    pub pencil_patch_m: f32,
    /// Distance shading (E054): 0 the distance ruling (`rule_*`, `fall_*`); 1 the tone that
    /// drove the ruling deepens the wash instead, with pencil strokes gathering on it.
    pub far_mode: f32,
    /// With `far_mode` 1: strength of the far pencil strokes (0: the wash alone).
    pub far_pencil: f32,
    /// On-screen size of the far strokes' relief, pixels per texel at 720 lines.
    pub far_pencil_px: f32,
    /// Lengthening of the far strokes along their direction.
    pub far_stretch: f32,
    /// How much of the far tone goes into the wash (1: all, to solid ink at the bottom).
    pub far_wash: f32,
    /// Light level (after the distance darkening) at which the far pencil starts, much
    /// earlier than the wash's far darkening (`fall_white`), so it builds up gradually (E057).
    pub far_white: f32,
    /// Pencil shading from the hand-shaded tile (E059): strength (0 off). Graphite darkens
    /// the wash as `1 - (1 - w)(1 - shade * coverage)`, grey on grey.
    pub shade: f32,
    /// On-screen size of the tile's texels, pixels at 720 lines (the window's lower end).
    pub shade_px: f32,
    /// Darkness (wash coverage, plus `shade_ao * (1 - ao)`) where the shading starts...
    pub shade_from: f32,
    /// ...and where it reaches the darkest level.
    pub shade_to: f32,
    /// Weight of enclosure (1 - ao) in the darkness that drives the shading.
    pub shade_ao: f32,
    /// How much the graphite catches the paper's tooth (0 none).
    pub shade_grain: f32,
}

impl StylePreset {
    /// The default look: black and white ruled ink after Tsutomu Nihei.
    pub fn nihei() -> Self {
        Self {
            paper: [0.94, 0.925, 0.89],
            ink: [0.07, 0.065, 0.06],
            paper_grain: 0.035,

            fog_colour: [0.86, 0.85, 0.82],
            fog_start: 25.0,
            fog_density: 0.006,
            fog_max: 0.85,
            fog_ao: 0.0,
            fog_black: 0.0,

            sun_strength: 0.5,
            ambient: 0.5,
            sun_occlusion: 0.0,
            band_hatch: 0.62,
            band_cross: 0.0,
            band_black: 0.3,
            band_jitter: 0.09,
            band_jitter_len: 0.6,

            hatch_spacing: 0.03,
            hatch_target_px: 4.5,
            hatch_line_px: 1.1,
            hatch_width_jitter: 0.5,
            hatch_break_len: 0.35,
            hatch_break_gap: 0.22,
            hatch_max_octave: 12.0,
            hatch_width_gain: 0.8,
            hatch_patch: 1.0,
            hatch_patch_lines: 8.0,
            hatch_patch_len: 4.0,
            hatch_taper_px: 6.0,
            cross_break_gap: 0.45,
            hatch_feather: 6.0,
            hatch_end_jitter: 0.1,
            shadow_feather: 0.0,
            shadow_break_gap: 0.22,
            hatch_taper_width: 5.0,
            hatch_width_max_px: 1.6,
            shadow_hatch_dir: 0.0,
            cross_pitch_scale: 1.6,
            cross_width_scale: 0.8,

            stroke_width_px: 1.6,
            stroke_min_px: 1.25,
            stroke_depth_ref: 6.0,
            stroke_depth_exp: 0.6,
            stroke_falloff_max: 1.6,
            stroke_taper_px: 18.0,
            stroke_taper_min: 0.3,
            stroke_noise_amp: 0.25,
            stroke_noise_len: 1.5,
            stroke_overshoot_px: 1.5,
            stroke_depth_tol: 0.005,
            rank_fade_start: [400.0, 120.0, 35.0, 15.0],
            rank_fade_end: [800.0, 220.0, 70.0, 30.0],
            rank_min_len_px: [2.0, 4.0, 8.0, 12.0],

            contour_fine_near: 25.0,
            contour_fine_far: 60.0,
            contour_depth_k: 6.0,
            contour_nv_min: 0.08,
            contour_strength: 0.9,
            contour_fine_strength: 0.45,
            contour_fade_start: 150.0,
            contour_fade_end: 400.0,

            sun_shadow: 0.0,
            shadow_ambient: 1.0,
            shadow_light_min: 0.0,
            shadow_light_max: 1.0,
            shadow_attached: 0.0,
            shadow_jitter: 0.0,
            shadow_jitter_len: 1.0,

            tone_noise: 0.0,
            tone_noise_len: 20.0,
            panel_tone: 0.0,
            hatch_density_gain: 1.0,
            indication: 0.0,
            indication_radius: 3.0,
            pool_ao: 0.0,
            pool_width: 0.05,
            underside_black: 0.0,
            white_lines: 0.0,
            white_line_rank: 1.0,
            white_hatch: 0.0,
            white_hatch_pitch: 3.0,
            speckle: 0.0,
            speckle_px: 0.7,
            speckle_cell_px: 9.0,
            cross_angle: 90.0,
            cross_angle_jitter: 0.0,
            cross_angle_by_panel: 0.0,
            sky_tone: None,

            tone_model: 0.0,
            rule_lod: 0.0,
            rule_pitch: 0.02,
            rule_target_px: 3.0,
            rule_res_exp: 0.6,
            rule_density: 0.5,
            rule_min_px: 0.4,
            rule_profile: 0.8,
            rule_pressure: 0.25,
            rule_vary: 0.35,
            rule_handover_px: 1.6,
            fall_white: 0.62,
            fall_black: 0.3,
            fall_gamma: 1.0,
            dark_ao: 0.0,
            dark_dist_start: 0.0,
            dark_dist_len: 0.0,
            dark_down_len: 0.0,
            wash: 0.0,
            wash_white: 0.9,
            wash_black: 0.3,
            wash_gamma: 1.0,
            wash_max: 0.8,
            screentone: 0.0,
            screentone_kind: 0.0,
            screentone_anchor: 2.0,
            screentone_px: 2.5,
            paper_texture: 0.0,
            paper_texture_px: 3.0,
            wash_speckle: 0.0,
            shadow_edge_line: 0.0,
            shadow_hatch_near: 0.0,
            rule_near: 0.0,
            rule_fade_in: 0.0,
            rule_down_gain: 0.0,
            rule_dist_len: 0.0,
            bounce: 0.0,
            bounce_sky: 0.0,
            screentone_aniso: 0.0,
            screentone_soft: 0.0,
            screentone_detail: 1.0,
            halftone: 0.0,
            halftone_pitch: 0.01,
            halftone_soft: 0.5,
            halftone_angle: 30.0,
            paper_tooth: 0.0,
            paper_tooth_px: 3.0,
            paper_take: 0.0,
            shadow_soft: 0.0,
            shadow_soft_max: 1.0,
            paper_anchor: 0.0,
            paper_cell: 0.002,
            shadow_soft_min: 0.0,
            halftone_max_pitch: 1000.0,
            paper_max_cell: 1000.0,
            paper_kind: 0.0,
            paper_near_dist: 0.0,
            paper_near_amp: 1.0,
            pencil: 0.0,
            pencil_kind: 3.0,
            pencil_from: 0.15,
            pencil_to: 0.5,
            pencil_cross: 0.0,
            pencil_angle: 45.0,
            pencil_patches: 0.0,
            pencil_patch_m: 2.5,
            far_mode: 0.0,
            far_pencil: 0.0,
            far_pencil_px: 3.0,
            far_stretch: 4.0,
            far_wash: 1.0,
            far_white: 0.6,
            shade: 0.0,
            shade_px: 0.9,
            shade_from: 0.35,
            shade_to: 0.95,
            shade_ao: 0.3,
            shade_grain: 0.6,
        }
    }

    pub(crate) fn to_gpu(&self) -> StyleGpu {
        let v3 = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        StyleGpu {
            paper: v3(self.paper, self.paper_grain),
            ink: v3(self.ink, 0.0),
            fog_colour: v3(self.fog_colour, 0.0),
            fog: [self.fog_start, self.fog_density, self.fog_max, self.fog_ao],
            light: [self.sun_strength, self.ambient, self.band_jitter, self.band_jitter_len],
            bands: [self.band_hatch, self.band_cross, self.band_black, self.sun_occlusion],
            cross: [self.cross_pitch_scale, self.cross_width_scale, self.cross_angle.to_radians(), self.cross_angle_jitter.to_radians()],
            hatch: [self.hatch_spacing, self.hatch_target_px, self.hatch_line_px, self.hatch_width_jitter],
            hatch2: [self.hatch_break_len, self.hatch_break_gap, self.hatch_max_octave, self.hatch_width_gain],
            stroke: [self.stroke_width_px, self.stroke_min_px, self.stroke_depth_ref, self.stroke_depth_exp],
            stroke2: [self.stroke_taper_px, self.stroke_taper_min, self.stroke_noise_amp, self.stroke_noise_len],
            stroke3: [self.stroke_overshoot_px, self.stroke_depth_tol, self.stroke_falloff_max, 0.0],
            rank_fade_start: self.rank_fade_start,
            rank_fade_end: self.rank_fade_end,
            rank_min_len: self.rank_min_len_px,
            contour: [self.contour_fine_near, self.contour_fine_far, self.contour_depth_k, self.contour_nv_min],
            contour2: [self.contour_strength, self.contour_fade_start, self.contour_fade_end, self.contour_fine_strength],
            shadow: [self.shadow_ambient, self.shadow_light_min, self.shadow_light_max, self.shadow_attached],
            shadow2: [self.shadow_jitter, self.shadow_jitter_len, self.indication, self.indication_radius],
            tone: [self.tone_noise, self.tone_noise_len, self.hatch_density_gain, self.sky_tone.unwrap_or(-1.0)],
            blacks: [self.pool_ao, self.pool_width, self.underside_black, self.white_lines],
            speckle: [self.speckle, self.speckle_px, self.speckle_cell_px, self.cross_angle_by_panel],
            fog2: [self.fog_black, self.sun_shadow, self.white_line_rank, 0.0],
            white: [self.white_hatch, self.white_hatch_pitch, self.panel_tone, self.cross_break_gap],
            patch: [self.hatch_patch, self.hatch_patch_lines, self.hatch_patch_len, self.hatch_taper_px],
            feather: [self.hatch_feather, self.hatch_end_jitter, self.shadow_feather, self.shadow_hatch_dir],
            feather2: [self.shadow_break_gap, self.hatch_taper_width, self.hatch_width_max_px, 0.0],
            rule: [self.tone_model, self.rule_lod, self.rule_pitch, self.rule_target_px],
            rule2: [self.rule_density, self.rule_min_px, self.rule_profile, self.rule_pressure],
            rule3: [self.rule_vary, self.rule_handover_px, self.shadow_edge_line, self.shadow_hatch_near],
            fall: [self.fall_white, self.fall_black, self.fall_gamma, self.dark_ao],
            dark: [self.dark_dist_start, self.dark_dist_len, self.wash_speckle, self.rule_res_exp],
            wash: [self.wash, self.wash_white, self.wash_black, self.wash_gamma],
            wash2: [self.wash_max, self.screentone, self.screentone_kind, self.screentone_anchor],
            wash3: [self.screentone_px, self.paper_texture, self.paper_texture_px, self.dark_down_len],
            far: [self.rule_near, self.rule_fade_in, self.rule_down_gain, self.rule_dist_len],
            bounce: [self.bounce, self.bounce_sky, self.screentone_aniso, 0.0],
            grain: [self.screentone_soft, self.screentone_detail, 0.0, 0.0],
            halftone: [self.halftone, self.halftone_pitch, self.halftone_soft, self.halftone_angle.to_radians()],
            paper2: [self.paper_tooth, self.paper_tooth_px, 0.0, 0.0],
            paper3: [self.paper_take, self.shadow_soft, self.shadow_soft_max, self.paper_anchor],
            paper4: [self.paper_cell, self.shadow_soft_min, self.halftone_max_pitch, 0.0],
            paper5: [self.paper_max_cell, self.paper_kind, 0.0, 0.0],
            paper6: [self.paper_near_dist, self.paper_near_amp, self.pencil, self.pencil_kind],
            paper7: [self.pencil_from, self.pencil_to, self.pencil_cross, self.pencil_angle.to_radians()],
            paper8: [self.pencil_patches, self.pencil_patch_m, 0.0, 0.0],
            paper9: [self.far_mode, self.far_pencil, self.far_pencil_px, self.far_stretch],
            paper10: [0.0, self.far_wash, self.far_white, 0.0],
            paper11: [self.shade, self.shade_px, self.shade_from, self.shade_to],
            paper12: [self.shade_ao, self.shade_grain, 0.0, 0.0],
        }
    }
}

impl StylePreset {
    /// `nihei` tuned for spaces hundreds of metres deep (the One Shaft): thinner atmosphere,
    /// rank cut-offs and contour fades pushed out so that the largest forms still draw at
    /// 600 m while panel-scale lines are gone by 100 m, and the sun scaled by baked openness.
    /// See docs/features/one-shaft.md and experiment E008.
    pub fn nihei_vast() -> Self {
        Self {
            fog_start: 40.0,
            fog_density: 0.0026,
            fog_max: 0.8,
            fog_ao: 1.5,
            ambient: 0.85,
            sun_strength: 0.4,
            hatch_width_gain: 1.0,
            rank_fade_start: [500.0, 140.0, 40.0, 16.0],
            rank_fade_end: [900.0, 260.0, 80.0, 32.0],
            rank_min_len_px: [3.0, 5.0, 8.0, 12.0],
            contour_fade_start: 220.0,
            contour_fade_end: 650.0,
            // Shadows and tone composition (E011 to E017).
            sun_occlusion: 0.0,
            sun_shadow: 1.0,
            shadow_ambient: 0.55,
            shadow_light_min: 0.36,
            shadow_light_max: 0.48,
            shadow_attached: 0.0,
            shadow_break_gap: 0.04,
            shadow_feather: 0.8,
            fog_black: 1.0,
            white_lines: 1.0,
            white_line_rank: 1.0,
            white_hatch: 0.6,
            white_hatch_pitch: 6.0,
            speckle: 0.1,
            shadow_jitter: 0.1,
            shadow_jitter_len: 1.2,
            panel_tone: 0.0,
            tone_noise: 0.08,
            tone_noise_len: 15.0,
            hatch_feather: 6.0,
            hatch_end_jitter: 0.1,
            hatch_break_gap: 0.05,
            hatch_density_gain: 1.0,
            band_cross: 0.0,
            cross_angle: 75.0,
            cross_angle_jitter: 30.0,
            sky_tone: Some(0.0),
            ..Self::nihei()
        }
    }

    /// Model A, "falling into darkness": `nihei_vast` with the continuous ruled tone. Lit
    /// and moderately lit surfaces are paper; ruling comes in only as the light falls, which
    /// it does strongly with depth into the structure and distance from the eye, and the
    /// ruling closes continuously into black in the far depth. See "Tone models" in
    /// docs/features/ink-renderer.md and E027.
    pub fn nihei_fall() -> Self {
        Self {
            tone_model: 1.0,
            fall_white: 0.45,
            fall_black: 0.06,
            fall_gamma: 1.4,
            dark_ao: 0.6,
            dark_dist_start: 30.0,
            dark_dist_len: 220.0,
            dark_down_len: 140.0,
            hatch_patch_lines: 3.0,
            hatch_end_jitter: 0.35,
            hatch_break_len: 2.0,
            shadow_light_min: 0.3,
            shadow_light_max: 0.45,
            white_hatch: 0.0,
            hatch_break_gap: 0.06,
            fog_density: 0.0015,
            fog_max: 0.3,
            ..Self::nihei_vast()
        }
    }

    /// Model B, wash and halftone: tone is a graded grey wash printed partly as screentone
    /// on textured paper; ruling only at the darkest end and along cast shadow edges.
    pub fn nihei_wash() -> Self {
        Self {
            tone_model: 1.0,
            fall_white: 0.2,
            fall_black: 0.05,
            fall_gamma: 1.0,
            dark_ao: 0.5,
            wash: 1.0,
            wash_white: 0.72,
            wash_black: 0.15,
            wash_gamma: 1.3,
            wash_max: 0.72,
            screentone: 0.25,
            screentone_kind: 1.0,
            screentone_anchor: 2.0,
            screentone_px: 2.0,
            paper_texture: 0.06,
            wash_speckle: 0.5,
            shadow_hatch_near: 1.0,
            indication_radius: 1.5,
            shadow_light_min: 0.25,
            shadow_light_max: 0.45,
            white_hatch: 0.0,
            hatch_break_gap: 0.06,
            hatch_patch_lines: 3.0,
            hatch_end_jitter: 0.35,
            hatch_break_len: 2.0,
            ..Self::nihei_vast()
        }
    }

    /// Hybrid: the wash carries the mid tones; ruling comes in with depth and distance and
    /// closes into black at the far depth, as in `nihei_fall`.
    pub fn nihei_hybrid() -> Self {
        Self {
            wash_max: 0.5,
            fall_white: 0.4,
            fall_black: 0.05,
            fall_gamma: 1.5,
            dark_ao: 0.8,
            dark_dist_start: 30.0,
            dark_dist_len: 220.0,
            dark_down_len: 140.0,
            ..Self::nihei_wash()
        }
    }

    /// The default look since E035 to E038: the wash as the base, with a soft, seam-free
    /// grain fixed to the surface; ruling only as a distance effect, sparse, coming in with
    /// distance from the eye and depth below it and merging into black only far away; and
    /// one bounce of sun and sky light, so enclosed shade next to lit surfaces lifts to mid
    /// grey. See "Tone models" in docs/features/ink-renderer.md and
    /// docs/features/bounced-light.md.
    pub fn nihei_ink() -> Self {
        Self {
            // Wash: a smooth gradient on textured paper with a subtle halftone (E040). The
            // E036 grain (`screentone`) is off; its fields stay set in case it is wanted.
            screentone_aniso: 1.0,
            screentone_detail: 0.3,
            screentone_soft: 2.0,
            screentone: 0.0,
            paper_texture: 0.0,
            paper_grain: 0.0,
            paper_anchor: 1.0,
            paper_cell: 0.0005,
            paper_max_cell: 0.5,
            // A scanned sheet since E049 (ambientCG Paper002, CC0), in place of the generated
            // fibres, which read as worms.
            // Paper001 (watercolour-like) since E051, folded so its tooth stays fine, quieter
            // up close.
            paper_kind: 1.0,
            paper_tooth: 0.012,
            paper_tooth_px: 0.6,
            paper_near_dist: 40.0,
            paper_near_amp: 0.5,
            // Pencil shading in shade from Paper004's laid lines, laid in patches (E052, E053).
            pencil: 0.0,
            pencil_kind: 3.0,
            pencil_patches: 1.0,
            // Only at the dark end of the wash (E056): light and mid shade stay clean.
            pencil_from: 0.4,
            pencil_to: 0.75,
            // Distance shading as wash and pencil marks instead of ruling (E054).
            far_mode: 1.0,
            far_pencil: 0.0,
            // Pencil shading from the hand-shaded tile, in patches where it is dark (E059);
            // it replaces the near laid-line pencil (E052) and the far strokes (E057).
            shade: 0.35,
            shade_from: 0.45,
            far_white: 0.45,
            far_pencil_px: 1.0,
            far_stretch: 2.0,
            paper_take: 0.08,
            halftone: 0.05,
            halftone_pitch: 0.01,
            halftone_soft: 0.8,
            halftone_angle: 30.0,
            // Soft cast-shadow edges, widening with distance from the occluder (E041).
            shadow_soft: 0.06,
            shadow_soft_max: 1.0,
            shadow_soft_min: 0.15,
            halftone_max_pitch: 0.05,
            // No light-level cloud either: the 15 m tone noise made soft blotches in the
            // smooth wash (E048).
            tone_noise: 0.0,
            // Ruling: nothing near, sparse at middle distance, merging into black far down.
            // A long, mostly grey fall into black (E059): light high up, mid grey for most of
            // the depth, black only at the bottom.
            fall_white: 0.3,
            fall_black: 0.04,
            fall_gamma: 2.4,
            fog_ao: 1.2,
            rule_near: 40.0,
            rule_fade_in: 150.0,
            rule_down_gain: 1.0,
            rule_dist_len: 380.0,
            rule_target_px: 9.0,
            rule_density: 3.0,
            // Calmer strokes: a gentler weight profile (fewer wedge-shaped strokes) and longer
            // runs (fewer stroke ends).
            rule_profile: 0.4,
            hatch_break_len: 4.0,
            // Ruling inside cast shadow too (far away only, by the gate): with the wash's
            // near-edge-only shadow strokes the far pit never closed into black (E035).
            shadow_hatch_near: 0.0,
            // Bounced light.
            bounce: 0.6,
            bounce_sky: 0.3,
            ..Self::nihei_wash()
        }
    }

    /// `nihei_ink` without any ruling: the wash, grain and bounced light alone.
    pub fn nihei_ink_wash() -> Self {
        Self { fall_white: 0.0, fall_black: -0.01, ..Self::nihei_ink() }
    }

    /// `nihei_ink` for the small, closed test scene: its atmosphere, light and line fades
    /// come from `nihei`, the sun casts no shadow (the scene is closed), and the ruling comes
    /// in over tens of metres rather than hundreds.
    pub fn nihei_ink_small() -> Self {
        let n = Self::nihei();
        Self {
            fog_start: n.fog_start,
            fog_density: n.fog_density,
            fog_max: n.fog_max,
            fog_ao: n.fog_ao,
            ambient: n.ambient,
            sun_strength: n.sun_strength,
            sun_shadow: 0.0,
            rank_fade_start: n.rank_fade_start,
            rank_fade_end: n.rank_fade_end,
            rank_min_len_px: n.rank_min_len_px,
            contour_fade_start: n.contour_fade_start,
            contour_fade_end: n.contour_fade_end,
            sky_tone: n.sky_tone,
            rule_near: 15.0,
            rule_fade_in: 40.0,
            fall_white: 0.28,
            rule_down_gain: 0.0,
            rule_dist_len: 60.0,
            ..Self::nihei_ink()
        }
    }

    /// Preset names accepted by [`StylePreset::by_name`].
    pub const NAMES: &'static [&'static str] =
        &["nihei", "nihei_vast", "nihei_fall", "nihei_wash", "nihei_hybrid", "nihei_ink", "nihei_ink_wash", "nihei_ink_small"];

    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "nihei" => Some(Self::nihei()),
            "nihei_vast" => Some(Self::nihei_vast()),
            "nihei_fall" => Some(Self::nihei_fall()),
            "nihei_wash" => Some(Self::nihei_wash()),
            "nihei_hybrid" => Some(Self::nihei_hybrid()),
            "nihei_ink" => Some(Self::nihei_ink()),
            "nihei_ink_wash" => Some(Self::nihei_ink_wash()),
            "nihei_ink_small" => Some(Self::nihei_ink_small()),
            _ => None,
        }
    }

    /// Environment variable that overrides [`StylePreset::for_scene`] with a named preset,
    /// so a preset can be tried in the windowed game without code changes:
    /// `MEGA_PRESET=nihei_wash cargo run --release --bin megastructure -- --scene shaft`.
    pub const ENV_OVERRIDE: &'static str = "MEGA_PRESET";

    /// The preset a named scene is tuned for (`nihei_ink_small` for `test`, `nihei_ink` for every other scene), unless the
    /// `MEGA_PRESET` environment variable names a preset, which then applies to every scene.
    /// An unknown name in `MEGA_PRESET` is reported on stderr and ignored.
    pub fn for_scene(scene: &str) -> Self {
        if let Ok(name) = std::env::var(Self::ENV_OVERRIDE) {
            let name = name.trim();
            if !name.is_empty() {
                match Self::by_name(name) {
                    Some(p) => return p,
                    None => eprintln!("{}={name}: unknown preset; available: {}", Self::ENV_OVERRIDE, Self::NAMES.join(", ")),
                }
            }
        }
        Self::scene_default(scene)
    }

    /// The preset a named scene is tuned for, ignoring `MEGA_PRESET`.
    pub fn scene_default(scene: &str) -> Self {
        // Every scene but the small closed test scene gets the shaft's look (so new
        // generated scenes, such as `labyrinth`, need no change here).
        match scene {
            "test" => Self::nihei_ink_small(),
            _ => Self::nihei_ink(),
        }
    }
}

/// Parse a comma-separated list of exactly N numbers.
fn parse_list<const N: usize>(name: &str, value: &str) -> Result<[f32; N], String> {
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    if parts.len() != N {
        return Err(format!("{name} takes {N} comma-separated numbers, got '{value}'"));
    }
    let mut out = [0.0; N];
    for (o, p) in out.iter_mut().zip(parts) {
        *o = p.parse().map_err(|_| format!("{name}: '{p}' is not a number"))?;
    }
    Ok(out)
}

impl StylePreset {
    /// Names of every field [`StylePreset::set`] accepts.
    pub const FIELDS: &'static [&'static str] = &[
        "paper",
        "ink",
        "fog_colour",
        "paper_grain",
        "fog_start",
        "fog_density",
        "fog_max",
        "fog_ao",
        "fog_black",
        "sun_strength",
        "ambient",
        "sun_occlusion",
        "band_hatch",
        "band_cross",
        "band_black",
        "band_jitter",
        "band_jitter_len",
        "hatch_spacing",
        "hatch_target_px",
        "hatch_line_px",
        "hatch_width_jitter",
        "hatch_break_len",
        "hatch_break_gap",
        "hatch_max_octave",
        "hatch_width_gain",
        "hatch_patch",
        "hatch_patch_lines",
        "hatch_patch_len",
        "hatch_taper_px",
        "cross_break_gap",
        "hatch_feather",
        "hatch_end_jitter",
        "shadow_feather",
        "shadow_break_gap",
        "hatch_taper_width",
        "hatch_width_max_px",
        "shadow_hatch_dir",
        "cross_pitch_scale",
        "cross_width_scale",
        "stroke_width_px",
        "stroke_min_px",
        "stroke_depth_ref",
        "stroke_depth_exp",
        "stroke_falloff_max",
        "stroke_taper_px",
        "stroke_taper_min",
        "stroke_noise_amp",
        "stroke_noise_len",
        "stroke_overshoot_px",
        "stroke_depth_tol",
        "contour_fine_near",
        "contour_fine_far",
        "contour_depth_k",
        "contour_nv_min",
        "contour_strength",
        "contour_fine_strength",
        "contour_fade_start",
        "contour_fade_end",
        "sun_shadow",
        "shadow_ambient",
        "shadow_light_min",
        "shadow_light_max",
        "shadow_attached",
        "shadow_jitter",
        "shadow_jitter_len",
        "tone_noise",
        "tone_noise_len",
        "panel_tone",
        "hatch_density_gain",
        "indication",
        "indication_radius",
        "pool_ao",
        "pool_width",
        "underside_black",
        "white_lines",
        "white_line_rank",
        "white_hatch",
        "white_hatch_pitch",
        "speckle",
        "speckle_px",
        "speckle_cell_px",
        "cross_angle",
        "cross_angle_jitter",
        "cross_angle_by_panel",
        "rank_fade_start",
        "rank_fade_end",
        "rank_min_len_px",
        "sky_tone",
        "tone_model",
        "rule_lod",
        "rule_pitch",
        "rule_target_px",
        "rule_res_exp",
        "rule_density",
        "rule_min_px",
        "rule_profile",
        "rule_pressure",
        "rule_vary",
        "rule_handover_px",
        "fall_white",
        "fall_black",
        "fall_gamma",
        "dark_ao",
        "dark_dist_start",
        "dark_dist_len",
        "dark_down_len",
        "wash",
        "wash_white",
        "wash_black",
        "wash_gamma",
        "wash_max",
        "screentone",
        "screentone_kind",
        "screentone_anchor",
        "screentone_px",
        "paper_texture",
        "paper_texture_px",
        "wash_speckle",
        "shadow_edge_line",
        "shadow_hatch_near",
        "rule_near",
        "rule_fade_in",
        "rule_down_gain",
        "rule_dist_len",
        "bounce",
        "bounce_sky",
        "screentone_aniso",
        "screentone_soft",
        "screentone_detail",
        "halftone",
        "halftone_pitch",
        "halftone_soft",
        "halftone_angle",
        "paper_tooth",
        "paper_tooth_px",
        "paper_take",
        "shadow_soft",
        "shadow_soft_max",
        "paper_anchor",
        "paper_cell",
        "shadow_soft_min",
        "halftone_max_pitch",
        "paper_max_cell",
        "paper_kind",
        "paper_near_dist",
        "paper_near_amp",
        "pencil",
        "pencil_kind",
        "pencil_from",
        "pencil_to",
        "pencil_cross",
        "pencil_angle",
        "pencil_patches",
        "pencil_patch_m",
        "far_mode",
        "far_pencil",
        "far_pencil_px",
        "far_stretch",
        "far_wash",
        "far_white",
        "shade",
        "shade_px",
        "shade_from",
        "shade_to",
        "shade_ao",
        "shade_grain",
    ];

    /// Set one field from text, for tools and experiments (`mega_capture --set name=value`).
    /// Scalars take a number, colours three and per-rank fields four comma-separated
    /// numbers; `sky_tone` also takes `none`.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let num = |v: &str| v.trim().parse::<f32>().map_err(|_| format!("{name}: '{v}' is not a number"));
        match name {
            "paper_grain" => self.paper_grain = num(value)?,
            "fog_start" => self.fog_start = num(value)?,
            "fog_density" => self.fog_density = num(value)?,
            "fog_max" => self.fog_max = num(value)?,
            "fog_ao" => self.fog_ao = num(value)?,
            "fog_black" => self.fog_black = num(value)?,
            "sun_strength" => self.sun_strength = num(value)?,
            "ambient" => self.ambient = num(value)?,
            "sun_occlusion" => self.sun_occlusion = num(value)?,
            "band_hatch" => self.band_hatch = num(value)?,
            "band_cross" => self.band_cross = num(value)?,
            "band_black" => self.band_black = num(value)?,
            "band_jitter" => self.band_jitter = num(value)?,
            "band_jitter_len" => self.band_jitter_len = num(value)?,
            "hatch_spacing" => self.hatch_spacing = num(value)?,
            "hatch_target_px" => self.hatch_target_px = num(value)?,
            "hatch_line_px" => self.hatch_line_px = num(value)?,
            "hatch_width_jitter" => self.hatch_width_jitter = num(value)?,
            "hatch_break_len" => self.hatch_break_len = num(value)?,
            "hatch_break_gap" => self.hatch_break_gap = num(value)?,
            "hatch_max_octave" => self.hatch_max_octave = num(value)?,
            "hatch_width_gain" => self.hatch_width_gain = num(value)?,
            "hatch_patch" => self.hatch_patch = num(value)?,
            "hatch_patch_lines" => self.hatch_patch_lines = num(value)?,
            "hatch_patch_len" => self.hatch_patch_len = num(value)?,
            "hatch_taper_px" => self.hatch_taper_px = num(value)?,
            "cross_break_gap" => self.cross_break_gap = num(value)?,
            "hatch_feather" => self.hatch_feather = num(value)?,
            "hatch_end_jitter" => self.hatch_end_jitter = num(value)?,
            "shadow_feather" => self.shadow_feather = num(value)?,
            "shadow_break_gap" => self.shadow_break_gap = num(value)?,
            "hatch_taper_width" => self.hatch_taper_width = num(value)?,
            "hatch_width_max_px" => self.hatch_width_max_px = num(value)?,
            "shadow_hatch_dir" => self.shadow_hatch_dir = num(value)?,
            "cross_pitch_scale" => self.cross_pitch_scale = num(value)?,
            "cross_width_scale" => self.cross_width_scale = num(value)?,
            "stroke_width_px" => self.stroke_width_px = num(value)?,
            "stroke_min_px" => self.stroke_min_px = num(value)?,
            "stroke_depth_ref" => self.stroke_depth_ref = num(value)?,
            "stroke_depth_exp" => self.stroke_depth_exp = num(value)?,
            "stroke_falloff_max" => self.stroke_falloff_max = num(value)?,
            "stroke_taper_px" => self.stroke_taper_px = num(value)?,
            "stroke_taper_min" => self.stroke_taper_min = num(value)?,
            "stroke_noise_amp" => self.stroke_noise_amp = num(value)?,
            "stroke_noise_len" => self.stroke_noise_len = num(value)?,
            "stroke_overshoot_px" => self.stroke_overshoot_px = num(value)?,
            "stroke_depth_tol" => self.stroke_depth_tol = num(value)?,
            "contour_fine_near" => self.contour_fine_near = num(value)?,
            "contour_fine_far" => self.contour_fine_far = num(value)?,
            "contour_depth_k" => self.contour_depth_k = num(value)?,
            "contour_nv_min" => self.contour_nv_min = num(value)?,
            "contour_strength" => self.contour_strength = num(value)?,
            "contour_fine_strength" => self.contour_fine_strength = num(value)?,
            "contour_fade_start" => self.contour_fade_start = num(value)?,
            "contour_fade_end" => self.contour_fade_end = num(value)?,
            "sun_shadow" => self.sun_shadow = num(value)?,
            "shadow_ambient" => self.shadow_ambient = num(value)?,
            "shadow_light_min" => self.shadow_light_min = num(value)?,
            "shadow_light_max" => self.shadow_light_max = num(value)?,
            "shadow_attached" => self.shadow_attached = num(value)?,
            "shadow_jitter" => self.shadow_jitter = num(value)?,
            "shadow_jitter_len" => self.shadow_jitter_len = num(value)?,
            "tone_noise" => self.tone_noise = num(value)?,
            "tone_noise_len" => self.tone_noise_len = num(value)?,
            "panel_tone" => self.panel_tone = num(value)?,
            "hatch_density_gain" => self.hatch_density_gain = num(value)?,
            "indication" => self.indication = num(value)?,
            "indication_radius" => self.indication_radius = num(value)?,
            "pool_ao" => self.pool_ao = num(value)?,
            "pool_width" => self.pool_width = num(value)?,
            "underside_black" => self.underside_black = num(value)?,
            "white_lines" => self.white_lines = num(value)?,
            "white_line_rank" => self.white_line_rank = num(value)?,
            "white_hatch" => self.white_hatch = num(value)?,
            "white_hatch_pitch" => self.white_hatch_pitch = num(value)?,
            "speckle" => self.speckle = num(value)?,
            "speckle_px" => self.speckle_px = num(value)?,
            "speckle_cell_px" => self.speckle_cell_px = num(value)?,
            "cross_angle" => self.cross_angle = num(value)?,
            "cross_angle_jitter" => self.cross_angle_jitter = num(value)?,
            "cross_angle_by_panel" => self.cross_angle_by_panel = num(value)?,
            "tone_model" => self.tone_model = num(value)?,
            "rule_lod" => self.rule_lod = num(value)?,
            "rule_pitch" => self.rule_pitch = num(value)?,
            "rule_target_px" => self.rule_target_px = num(value)?,
            "rule_res_exp" => self.rule_res_exp = num(value)?,
            "rule_density" => self.rule_density = num(value)?,
            "rule_min_px" => self.rule_min_px = num(value)?,
            "rule_profile" => self.rule_profile = num(value)?,
            "rule_pressure" => self.rule_pressure = num(value)?,
            "rule_vary" => self.rule_vary = num(value)?,
            "rule_handover_px" => self.rule_handover_px = num(value)?,
            "fall_white" => self.fall_white = num(value)?,
            "fall_black" => self.fall_black = num(value)?,
            "fall_gamma" => self.fall_gamma = num(value)?,
            "dark_ao" => self.dark_ao = num(value)?,
            "dark_dist_start" => self.dark_dist_start = num(value)?,
            "dark_dist_len" => self.dark_dist_len = num(value)?,
            "dark_down_len" => self.dark_down_len = num(value)?,
            "wash" => self.wash = num(value)?,
            "wash_white" => self.wash_white = num(value)?,
            "wash_black" => self.wash_black = num(value)?,
            "wash_gamma" => self.wash_gamma = num(value)?,
            "wash_max" => self.wash_max = num(value)?,
            "screentone" => self.screentone = num(value)?,
            "screentone_kind" => self.screentone_kind = num(value)?,
            "screentone_anchor" => self.screentone_anchor = num(value)?,
            "screentone_px" => self.screentone_px = num(value)?,
            "paper_texture" => self.paper_texture = num(value)?,
            "paper_texture_px" => self.paper_texture_px = num(value)?,
            "wash_speckle" => self.wash_speckle = num(value)?,
            "shadow_edge_line" => self.shadow_edge_line = num(value)?,
            "shadow_hatch_near" => self.shadow_hatch_near = num(value)?,
            "rule_near" => self.rule_near = num(value)?,
            "rule_fade_in" => self.rule_fade_in = num(value)?,
            "rule_down_gain" => self.rule_down_gain = num(value)?,
            "rule_dist_len" => self.rule_dist_len = num(value)?,
            "bounce" => self.bounce = num(value)?,
            "bounce_sky" => self.bounce_sky = num(value)?,
            "screentone_aniso" => self.screentone_aniso = num(value)?,
            "screentone_soft" => self.screentone_soft = num(value)?,
            "screentone_detail" => self.screentone_detail = num(value)?,
            "halftone" => self.halftone = num(value)?,
            "halftone_pitch" => self.halftone_pitch = num(value)?,
            "halftone_soft" => self.halftone_soft = num(value)?,
            "halftone_angle" => self.halftone_angle = num(value)?,
            "paper_tooth" => self.paper_tooth = num(value)?,
            "paper_tooth_px" => self.paper_tooth_px = num(value)?,
            "paper_take" => self.paper_take = num(value)?,
            "shadow_soft" => self.shadow_soft = num(value)?,
            "shadow_soft_max" => self.shadow_soft_max = num(value)?,
            "paper_anchor" => self.paper_anchor = num(value)?,
            "paper_cell" => self.paper_cell = num(value)?,
            "shadow_soft_min" => self.shadow_soft_min = num(value)?,
            "halftone_max_pitch" => self.halftone_max_pitch = num(value)?,
            "paper_max_cell" => self.paper_max_cell = num(value)?,
            "paper_kind" => self.paper_kind = num(value)?,
            "paper_near_dist" => self.paper_near_dist = num(value)?,
            "paper_near_amp" => self.paper_near_amp = num(value)?,
            "pencil" => self.pencil = num(value)?,
            "pencil_kind" => self.pencil_kind = num(value)?,
            "pencil_from" => self.pencil_from = num(value)?,
            "pencil_to" => self.pencil_to = num(value)?,
            "pencil_cross" => self.pencil_cross = num(value)?,
            "pencil_angle" => self.pencil_angle = num(value)?,
            "pencil_patches" => self.pencil_patches = num(value)?,
            "pencil_patch_m" => self.pencil_patch_m = num(value)?,
            "far_mode" => self.far_mode = num(value)?,
            "far_pencil" => self.far_pencil = num(value)?,
            "far_pencil_px" => self.far_pencil_px = num(value)?,
            "far_stretch" => self.far_stretch = num(value)?,
            "far_wash" => self.far_wash = num(value)?,
            "far_white" => self.far_white = num(value)?,
            "shade" => self.shade = num(value)?,
            "shade_px" => self.shade_px = num(value)?,
            "shade_from" => self.shade_from = num(value)?,
            "shade_to" => self.shade_to = num(value)?,
            "shade_ao" => self.shade_ao = num(value)?,
            "shade_grain" => self.shade_grain = num(value)?,
            "paper" => self.paper = parse_list::<3>(name, value)?,
            "ink" => self.ink = parse_list::<3>(name, value)?,
            "fog_colour" => self.fog_colour = parse_list::<3>(name, value)?,
            "rank_fade_start" => self.rank_fade_start = parse_list::<4>(name, value)?,
            "rank_fade_end" => self.rank_fade_end = parse_list::<4>(name, value)?,
            "rank_min_len_px" => self.rank_min_len_px = parse_list::<4>(name, value)?,
            "sky_tone" => self.sky_tone = if value.trim() == "none" { None } else { Some(num(value)?) },
            _ => return Err(format!("unknown preset field '{name}'")),
        }
        Ok(())
    }
}

impl Default for StylePreset {
    fn default() -> Self {
        Self::nihei()
    }
}

/// GPU mirror of [`StylePreset`]; must match `struct Style` in `shaders/common.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct StyleGpu {
    paper: [f32; 4],
    ink: [f32; 4],
    fog_colour: [f32; 4],
    fog: [f32; 4],
    light: [f32; 4],
    bands: [f32; 4],
    cross: [f32; 4],
    hatch: [f32; 4],
    hatch2: [f32; 4],
    stroke: [f32; 4],
    stroke2: [f32; 4],
    stroke3: [f32; 4],
    rank_fade_start: [f32; 4],
    rank_fade_end: [f32; 4],
    rank_min_len: [f32; 4],
    contour: [f32; 4],
    contour2: [f32; 4],
    shadow: [f32; 4],
    shadow2: [f32; 4],
    tone: [f32; 4],
    blacks: [f32; 4],
    speckle: [f32; 4],
    fog2: [f32; 4],
    white: [f32; 4],
    patch: [f32; 4],
    feather: [f32; 4],
    feather2: [f32; 4],
    rule: [f32; 4],
    rule2: [f32; 4],
    rule3: [f32; 4],
    fall: [f32; 4],
    dark: [f32; 4],
    wash: [f32; 4],
    wash2: [f32; 4],
    wash3: [f32; 4],
    far: [f32; 4],
    bounce: [f32; 4],
    grain: [f32; 4],
    halftone: [f32; 4],
    paper2: [f32; 4],
    paper3: [f32; 4],
    paper4: [f32; 4],
    paper5: [f32; 4],
    paper6: [f32; 4],
    paper7: [f32; 4],
    paper8: [f32; 4],
    paper9: [f32; 4],
    paper10: [f32; 4],
    paper11: [f32; 4],
    paper12: [f32; 4],
}
