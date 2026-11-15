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

    // Light and tone bands.
    /// Weight of the sun term `intensity * max(N.L, 0)` in the light value.
    pub sun_strength: f32,
    /// Weight of the ambient term `ao` in the light value.
    pub ambient: f32,
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

            sun_strength: 0.5,
            ambient: 0.5,
            band_hatch: 0.62,
            band_cross: 0.45,
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
        }
    }

    pub(crate) fn to_gpu(&self) -> StyleGpu {
        let v3 = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        StyleGpu {
            paper: v3(self.paper, self.paper_grain),
            ink: v3(self.ink, 0.0),
            fog_colour: v3(self.fog_colour, 0.0),
            fog: [self.fog_start, self.fog_density, self.fog_max, 0.0],
            light: [self.sun_strength, self.ambient, self.band_jitter, self.band_jitter_len],
            bands: [self.band_hatch, self.band_cross, self.band_black, 0.0],
            cross: [self.cross_pitch_scale, self.cross_width_scale, 0.0, 0.0],
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
        }
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
}
