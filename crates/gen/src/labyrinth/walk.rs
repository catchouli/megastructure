//! Reachability in collider space: a flood fill of standing places from the spawn, within
//! the walker's limits (`docs/features/player-movement.md`): a step of at most 0.3 m between
//! neighbouring places 0.5 m apart, slopes only as steep as the stair ramps allow, and a
//! capsule 1.8 m tall and 0.3 m in radius clear of every collider. No falls: every place
//! found can be walked back from. It knows nothing about the plan except where to start;
//! the plan's anchors and doors are checked against what it finds.

use crate::mesh::Collider;
use glam::Vec3;
use std::collections::{HashMap, HashSet, VecDeque};

pub const STEP: f32 = 0.3;
pub const RADIUS: f32 = 0.3;
pub const HEIGHT: f32 = 1.8;
/// Grid spacing of the flood fill.
pub const SPACING: f32 = 0.5;

/// A walker's limits for the flood fill.
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    pub height: f32,
    pub radius: f32,
    /// Largest step between neighbouring places.
    pub step: f32,
    /// The capsule is checked from this height above the feet (below it, only the shin point
    /// must be clear: what is lower is stepped onto).
    pub lift: f32,
}

impl Profile {
    /// Standing: 1.8 m by 0.3 m, step 0.3 m (stricter than the walker's 0.45 m).
    pub const STAND: Profile = Profile { height: HEIGHT, radius: RADIUS, step: STEP, lift: STEP + RADIUS + 0.05 };
    /// Crouched, for vents: 0.9 m by 0.3 m, step 0.45 m. Contract with crates/game.
    pub const CROUCH: Profile = Profile { height: 0.9, radius: 0.3, step: 0.45, lift: 0.6 };
}

#[derive(Clone, Copy)]
struct Solid {
    c: Vec3,
    a: [Vec3; 3],
    h: Vec3,
    /// A horizontal top (an axis within a degree of vertical), or None.
    top_axis: Option<usize>,
    ramp: Option<(Vec3, Vec3, f32)>,
}

impl Solid {
    fn dist(&self, p: Vec3) -> f32 {
        let d = p - self.c;
        let mut q = Vec3::ZERO;
        for i in 0..3 {
            let x = d.dot(self.a[i]);
            q[i] = (x.abs() - self.h[i]).max(0.0);
        }
        q.length()
    }

    fn inside(&self, p: Vec3) -> bool {
        let d = p - self.c;
        (0..3).all(|i| d.dot(self.a[i]).abs() < self.h[i] - 1e-4)
    }

    /// Height of the walkable top over (x, z), if this solid has one there.
    fn top_at(&self, x: f32, z: f32) -> Option<f32> {
        if let Some((s, e, hw)) = self.ramp {
            let d = (e - s).with_y(0.0);
            let len = d.length();
            let dir = d / len;
            let rel = Vec3::new(x, 0.0, z) - s.with_y(0.0);
            let t = rel.dot(dir);
            let w = (rel - dir * t).length();
            if (-0.01..=len + 0.01).contains(&t) && w <= hw {
                return Some(s.y + (e.y - s.y) * (t / len).clamp(0.0, 1.0));
            }
            return None;
        }
        let i = self.top_axis?;
        let d = Vec3::new(x - self.c.x, 0.0, z - self.c.z);
        for j in 0..3 {
            if j == i {
                continue;
            }
            let a = self.a[j];
            // Horizontal axes only matter in plan.
            let ah = a.with_y(0.0);
            if d.dot(ah).abs() > self.h[j] + 1e-4 {
                return None;
            }
        }
        Some(self.c.y + self.h[i] * self.a[i].y.abs())
    }
}

pub struct World {
    solids: Vec<Solid>,
    origin: Vec3,
    cell: f32,
    dims: [usize; 3],
    cells: Vec<Vec<u32>>,
}

impl World {
    pub fn new(colliders: &[Collider]) -> Self {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        let solids: Vec<Solid> = colliders
            .iter()
            .map(|c| {
                let (b0, b1) = c.bounds();
                lo = lo.min(Vec3::from(b0));
                hi = hi.max(Vec3::from(b1));
                let (centre, axes, half) = c.to_obb();
                let a = [Vec3::from(axes[0]), Vec3::from(axes[1]), Vec3::from(axes[2])];
                let ramp = if let Collider::Ramp { start, end, half_width, .. } = *c { Some((Vec3::from(start), Vec3::from(end), half_width)) } else { None };
                let top_axis = if ramp.is_some() { None } else { (0..3).find(|&i| a[i].y.abs() > 0.9998) };
                Solid { c: centre.into(), a, h: half.into(), top_axis, ramp }
            })
            .collect();
        let cell = 4.0;
        let origin = lo - Vec3::splat(cell);
        let size = hi + Vec3::splat(cell) - origin;
        let dims = [(size.x / cell).ceil() as usize + 1, (size.y / cell).ceil() as usize + 1, (size.z / cell).ceil() as usize + 1];
        let mut cells = vec![Vec::new(); dims[0] * dims[1] * dims[2]];
        for (i, c) in colliders.iter().enumerate() {
            let (b0, b1) = c.bounds();
            let a = ((Vec3::from(b0) - origin) / cell).floor();
            let b = ((Vec3::from(b1) - origin) / cell).floor();
            for z in a.z as usize..=(b.z as usize).min(dims[2] - 1) {
                for y in a.y as usize..=(b.y as usize).min(dims[1] - 1) {
                    for x in a.x as usize..=(b.x as usize).min(dims[0] - 1) {
                        cells[(z * dims[1] + y) * dims[0] + x].push(i as u32);
                    }
                }
            }
        }
        Self { solids, origin, cell, dims, cells }
    }

    fn near(&self, lo: Vec3, hi: Vec3, out: &mut Vec<u32>) {
        out.clear();
        let a = ((lo - self.origin) / self.cell).floor().max(Vec3::ZERO);
        let b = ((hi - self.origin) / self.cell).floor();
        let cl = |v: f32, d: usize| (v.max(0.0) as usize).min(d - 1);
        for z in cl(a.z, self.dims[2])..=cl(b.z, self.dims[2]) {
            for y in cl(a.y, self.dims[1])..=cl(b.y, self.dims[1]) {
                for x in cl(a.x, self.dims[0])..=cl(b.x, self.dims[0]) {
                    out.extend_from_slice(&self.cells[(z * self.dims[1] + y) * self.dims[0] + x]);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }

    /// Walkable tops over (x, z) between `y0` and `y1`.
    pub fn tops(&self, x: f32, z: f32, y0: f32, y1: f32) -> Vec<f32> {
        let mut c = Vec::new();
        self.near(Vec3::new(x, y0, z), Vec3::new(x, y1, z), &mut c);
        let mut out: Vec<f32> = c.iter().filter_map(|&i| self.solids[i as usize].top_at(x, z)).filter(|&t| t >= y0 && t <= y1).collect();
        out.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out
    }

    /// Whether a walker can stand with its feet at `p`: the capsule above the step height is
    /// clear of every solid, and nothing solid at shin height.
    pub fn clear(&self, p: Vec3) -> bool {
        self.clear_for(p, &Profile::STAND)
    }

    pub fn clear_for(&self, p: Vec3, pr: &Profile) -> bool {
        let mut c = Vec::new();
        let r = pr.radius;
        self.near(p - Vec3::new(r + 0.1, 0.0, r + 0.1), p + Vec3::new(r + 0.1, pr.height + 0.1, r + 0.1), &mut c);
        let shin = p + Vec3::Y * 0.15;
        let (y0, y1) = (pr.lift, (pr.height - r).max(pr.lift));
        for &i in &c {
            let s = &self.solids[i as usize];
            if s.inside(shin) {
                return false;
            }
            for k in 0..6 {
                let y = y0 + k as f32 * (y1 - y0) / 5.0;
                if s.dist(p + Vec3::Y * y) < r - 0.01 {
                    return false;
                }
            }
        }
        true
    }
}

impl World {
    /// Whether nothing solid lies on the segment (railings and other parts thinner than
    /// 10 cm do not block a view). Sampled every 0.2 m.
    pub fn ray_clear(&self, a: Vec3, c: Vec3) -> Result<(), Vec3> {
        let n = ((c - a).length() / 0.2).ceil().max(1.0) as usize;
        let mut cand = Vec::new();
        for i in 0..=n {
            let p = a + (c - a) * (i as f32 / n as f32);
            self.near(p, p, &mut cand);
            for &k in &cand {
                let s = &self.solids[k as usize];
                if s.h.min_element() >= 0.05 && s.inside(p) {
                    return Err(p);
                }
            }
        }
        Ok(())
    }
}

/// What the flood fill found.
pub struct Reach {
    /// Standing places, keyed by plan grid index, with their heights.
    places: HashMap<(i32, i32), Vec<f32>>,
    pub count: usize,
}

impl Reach {
    /// Whether a place within `r` metres in plan and `dy` in height was reached.
    pub fn near(&self, p: Vec3, r: f32, dy: f32) -> bool {
        let k = |v: f32| (v / SPACING).round() as i32;
        let n = (r / SPACING).ceil() as i32;
        for dz in -n..=n {
            for dx in -n..=n {
                let key = (k(p.x) + dx, k(p.z) + dz);
                let q = Vec3::new(key.0 as f32 * SPACING, 0.0, key.1 as f32 * SPACING);
                if (q - p.with_y(0.0)).length() > r + 1e-3 {
                    continue;
                }
                if let Some(ys) = self.places.get(&key) {
                    if ys.iter().any(|&y| (y - p.y).abs() <= dy) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Flood fill from the feet position `start` (snapped to the nearest top within 0.5 m).
pub fn flood(world: &World, start: Vec3) -> Reach {
    flood_with(world, start, &Profile::STAND)
}

/// Flood fill for a walker profile.
pub fn flood_with(world: &World, start: Vec3, pr: &Profile) -> Reach {
    let step = pr.step;
    let k = |v: f32| (v / SPACING).round() as i32;
    let qy = |y: f32| (y * 50.0).round() as i32;
    let (sx, sz) = (k(start.x), k(start.z));
    let y0 = world.tops(sx as f32 * SPACING, sz as f32 * SPACING, start.y - 0.5, start.y + 0.5).into_iter().min_by(|a, b| (a - start.y).abs().partial_cmp(&(b - start.y).abs()).unwrap());
    let mut places: HashMap<(i32, i32), Vec<f32>> = HashMap::new();
    let mut seen: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut queue = VecDeque::new();
    let mut clear_cache: HashMap<(i32, i32, i32), bool> = HashMap::new();
    let mut is_clear = |world: &World, x: i32, z: i32, y: f32| *clear_cache.entry((x, z, qy(y))).or_insert_with(|| world.clear_for(Vec3::new(x as f32 * SPACING, y, z as f32 * SPACING), pr));
    if let Some(y) = y0 {
        if is_clear(world, sx, sz, y) {
            seen.insert((sx, sz, qy(y)));
            queue.push_back((sx, sz, y));
        }
    }
    let mut count = 0;
    while let Some((x, z, y)) = queue.pop_front() {
        count += 1;
        places.entry((x, z)).or_default().push(y);
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (nx, nz) = (x + dx, z + dz);
            for ny in world.tops(nx as f32 * SPACING, nz as f32 * SPACING, y - step - 0.01, y + step + 0.01) {
                let key = (nx, nz, qy(ny));
                if seen.contains(&key) {
                    continue;
                }
                if !is_clear(world, nx, nz, ny) {
                    continue;
                }
                if dx != 0 && dz != 0 {
                    // Diagonal: both orthogonal neighbours must be standable too (no corner cutting).
                    let a = world.tops(nx as f32 * SPACING, z as f32 * SPACING, y.min(ny) - step - 0.01, y.max(ny) + step + 0.01);
                    let b = world.tops(x as f32 * SPACING, nz as f32 * SPACING, y.min(ny) - step - 0.01, y.max(ny) + step + 0.01);
                    let ok_a = a.iter().any(|&t| is_clear(world, nx, z, t));
                    let ok_b = b.iter().any(|&t| is_clear(world, x, nz, t));
                    if !ok_a || !ok_b {
                        continue;
                    }
                }
                seen.insert(key);
                queue.push_back((nx, nz, ny));
            }
        }
    }
    Reach { places, count }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aabb(min: [f32; 3], max: [f32; 3]) -> Collider {
        Collider::Aabb { min, max }
    }

    #[test]
    fn floods_a_floor_and_stops_at_walls_and_high_steps() {
        let cols = vec![
            aabb([0.0, -1.0, 0.0], [10.0, 0.0, 4.0]),
            // A wall across at x 5 with a door gap z 1.5..2.5.
            aabb([5.0, 0.0, 0.0], [5.4, 3.0, 1.5]),
            aabb([5.0, 0.0, 2.5], [5.4, 3.0, 4.0]),
            aabb([5.0, 2.2, 1.5], [5.4, 3.0, 2.5]),
            // A 0.25 m step at x 8.. and a 0.5 m block beyond 9.
            aabb([8.0, 0.0, 0.0], [10.0, 0.25, 4.0]),
            aabb([9.0, 0.25, 0.0], [10.0, 0.75, 4.0]),
        ];
        let w = World::new(&cols);
        let r = flood(&w, Vec3::new(1.0, 0.0, 2.0));
        assert!(r.near(Vec3::new(7.0, 0.0, 2.0), 0.5, 0.1), "through the door");
        assert!(r.near(Vec3::new(8.5, 0.25, 2.0), 0.5, 0.1), "up the step");
        assert!(!r.near(Vec3::new(9.5, 0.75, 2.0), 0.4, 0.1), "not up the block");
        // A door too low for the capsule.
        let mut low = cols.clone();
        low[3] = aabb([5.0, 1.6, 1.5], [5.4, 3.0, 2.5]);
        let r = flood(&World::new(&low), Vec3::new(1.0, 0.0, 2.0));
        assert!(!r.near(Vec3::new(7.0, 0.0, 2.0), 0.5, 0.1), "through a 1.6 m door");
    }
}
