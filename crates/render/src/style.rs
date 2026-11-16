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

    /// Preset names accepted by [`StylePreset::by_name`].
    pub const NAMES: &'static [&'static str] = &["nihei", "nihei_vast"];

    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "nihei" => Some(Self::nihei()),
            "nihei_vast" => Some(Self::nihei_vast()),
            _ => None,
        }
    }

    /// The preset a named scene is tuned for (`nihei` for anything unknown).
    pub fn for_scene(scene: &str) -> Self {
        match scene {
            "shaft" => Self::nihei_vast(),
            _ => Self::nihei(),
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
}
