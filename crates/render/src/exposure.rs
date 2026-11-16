//! Exposure adaptation (E073): a gain on the light value, driven by where the eye is, not by
//! the image, so it does not jump when the view turns towards a window.
//!
//! At upload the scene's vertices are binned on a coarse grid; each cell keeps the mean of
//! the baked `ao` (openness times local occlusion) and, once the bounce bake arrives, of the
//! two bounce terms. Each frame the cells within `RADIUS` of the eye are averaged (weighted by
//! distance) into one "local light" value in the same units as the shader's light, mapped to
//! a target gain, and the gain follows the target slowly: brightening over about
//! `RISE_S`, darkening faster over `FALL_S`, frame-rate independent.

use glam::Vec3;
use mega_gen::MeshData;
use std::collections::HashMap;

/// Grid cell in metres.
const CELL: f32 = 3.0;
/// Neighbourhood radius in metres.
const RADIUS: f32 = 7.0;
/// Time constants in seconds (to 63% of the way): brightening, darkening.
pub const RISE_S: f32 = 0.7;
pub const FALL_S: f32 = 0.35;

#[derive(Default, Clone, Copy)]
struct Cell {
    ao: f32,
    sun: f32,
    sky: f32,
    n: f32,
}

#[derive(Default)]
pub struct Exposure {
    cells: HashMap<(i32, i32, i32), Cell>,
    /// Vertex index to cell key, to add the bounce bake later.
    vertex_cell: Vec<(i32, i32, i32)>,
    gain: f32,
    settled: bool,
    pub last_local: f32,
    pub last_target: f32,
}

fn key(p: Vec3) -> (i32, i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32, (p.z / CELL).floor() as i32)
}

impl Exposure {
    pub fn new(mesh: &MeshData) -> Self {
        let mut cells: HashMap<(i32, i32, i32), Cell> = HashMap::new();
        let mut vertex_cell = Vec::with_capacity(mesh.vertices.len());
        for v in &mesh.vertices {
            let k = key(Vec3::from(v.position));
            let c = cells.entry(k).or_default();
            c.ao += v.ao;
            c.n += 1.0;
            vertex_cell.push(k);
        }
        Self { cells, vertex_cell, gain: 1.0, settled: false, last_local: 0.0, last_target: 1.0 }
    }

    /// Adds the baked bounce (per vertex `[sun, sky]`), replacing any earlier bake.
    pub fn set_bounce(&mut self, bounce: &[[f32; 2]]) {
        for c in self.cells.values_mut() {
            c.sun = 0.0;
            c.sky = 0.0;
        }
        for (k, b) in self.vertex_cell.iter().zip(bounce) {
            if let Some(c) = self.cells.get_mut(k) {
                c.sun += b[0];
                c.sky += b[1];
            }
        }
    }

    /// The light around `eye` in the shader's units: `ambient * ao` plus the bounce terms
    /// (weighted as the shader does), averaged over the cells within `RADIUS`.
    pub fn local_light(&self, eye: Vec3, ambient: f32, sun_w: f32, sky_w: f32) -> f32 {
        let (cx, cy, cz) = key(eye);
        let r = (RADIUS / CELL).ceil() as i32;
        let (mut acc, mut wsum) = (0.0f32, 0.0f32);
        for dz in -r..=r {
            for dy in -r..=r {
                for dx in -r..=r {
                    let k = (cx + dx, cy + dy, cz + dz);
                    let Some(c) = self.cells.get(&k) else { continue };
                    let centre = Vec3::new((k.0 as f32 + 0.5) * CELL, (k.1 as f32 + 0.5) * CELL, (k.2 as f32 + 0.5) * CELL);
                    let d = centre.distance(eye);
                    if d > RADIUS {
                        continue;
                    }
                    // Weighted by distance and by how many vertices the cell holds (so a cell
                    // grazing one wall does not count as much as a room's floor).
                    let w = (1.0 - d / RADIUS).powi(2) * c.n.sqrt();
                    let l = (ambient * c.ao + sun_w * c.sun + sky_w * c.sky) / c.n;
                    acc += w * l;
                    wsum += w;
                }
            }
        }
        if wsum > 0.0 {
            acc / wsum
        } else {
            1.0
        }
    }

    /// Moves the gain towards `target` over `dt` seconds (or at once if not settled yet).
    pub fn step(&mut self, target: f32, dt: f32) -> f32 {
        self.last_target = target;
        if !self.settled {
            self.gain = target;
            self.settled = true;
            return self.gain;
        }
        let tau = if target > self.gain { RISE_S } else { FALL_S };
        let k = 1.0 - (-dt.max(0.0) / tau).exp();
        self.gain += (target - self.gain) * k;
        self.gain
    }

    /// Start adapting from `gain` (for studying the transition).
    pub fn start_from(&mut self, gain: f32) {
        self.gain = gain;
        self.settled = true;
    }

    /// Jump to the target on the next step.
    pub fn settle(&mut self) {
        self.settled = false;
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }
}

/// The target gain for a local light `l`: 1 at and above `bright`, `max` at and below `dark`,
/// smooth between.
pub fn target_gain(l: f32, dark: f32, bright: f32, max: f32) -> f32 {
    let t = if bright > dark { ((l - dark) / (bright - dark)).clamp(0.0, 1.0) } else { 1.0 };
    let s = t * t * (3.0 - 2.0 * t);
    max + (1.0 - max) * s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_maps_and_adapts_smoothly() {
        assert_eq!(target_gain(1.0, 0.1, 0.5, 2.0), 1.0);
        assert_eq!(target_gain(0.05, 0.1, 0.5, 2.0), 2.0);
        let mut e = Exposure { gain: 1.0, settled: true, ..Default::default() };
        // Brightening is slower than darkening, and frame-rate independent.
        let mut a = e.gain;
        for _ in 0..60 {
            a = e.step(2.0, 1.0 / 60.0);
        }
        let mut f = Exposure { gain: 1.0, settled: true, ..Default::default() };
        let b = f.step(2.0, 1.0);
        assert!((a - b).abs() < 1e-3, "{a} {b}");
        assert!(a > 1.5 && a < 2.0);
        let mut g = Exposure { gain: 2.0, settled: true, ..Default::default() };
        assert!(g.step(1.0, 1.0) < 1.1);
    }
}
