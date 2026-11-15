//! Ambient occlusion baked from the scene's own solids.
//!
//! Two terms, multiplied:
//!
//! - **Openness**, supplied by the scene: a smooth, analytic estimate of how much sky light
//!   reaches a point at the scale of the whole space (in the shaft, a function of depth below
//!   the rim and of distance into the walls). It stands in for multi-bounce light.
//! - **Local occlusion**, computed here: a fixed set of cosine-distributed rays per vertex,
//!   cast against the collider boxes within `radius`. A hit at distance `t` occludes by
//!   `(1 - t / radius)^2`, so the value varies smoothly as a vertex moves and there is no
//!   step at the edge of the search radius.
//!
//! The ray directions are the same for every vertex (a Fibonacci spiral rotated into the
//! vertex's tangent frame), so the result is a deterministic function of position and
//! normal. See docs/features/kit-of-parts.md and experiment E009.

use crate::mesh::Collider;
use glam::Vec3;

/// One solid as an oriented box, prepared for slab tests.
#[derive(Clone, Copy, Debug)]
struct Solid {
    centre: Vec3,
    axes: [Vec3; 3],
    half: Vec3,
}

/// The scene's solids in a uniform grid, for short ray queries.
pub struct Occluders {
    solids: Vec<Solid>,
    origin: Vec3,
    cell: f32,
    dims: [usize; 3],
    /// Per cell, indices into `solids`.
    cells: Vec<Vec<u32>>,
    dirs: Vec<Vec3>,
    pub radius: f32,
}

impl Occluders {
    /// Build from colliders. `cell` is the grid cell size in metres, `rays` the number of
    /// directions per vertex, `radius` the search distance.
    pub fn new(colliders: &[Collider], cell: f32, rays: usize, radius: f32) -> Self {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        let solids: Vec<Solid> = colliders
            .iter()
            .map(|c| {
                let (b0, b1) = c.bounds();
                lo = lo.min(Vec3::from(b0));
                hi = hi.max(Vec3::from(b1));
                let (centre, axes, half) = c.to_obb();
                Solid { centre: centre.into(), axes: [axes[0].into(), axes[1].into(), axes[2].into()], half: half.into() }
            })
            .collect();
        if solids.is_empty() {
            lo = Vec3::ZERO;
            hi = Vec3::ONE;
        }
        let origin = lo - Vec3::splat(radius + cell);
        let size = hi + Vec3::splat(radius + cell) - origin;
        let dims = [(size.x / cell).ceil() as usize, (size.y / cell).ceil() as usize, (size.z / cell).ceil() as usize];
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
        // Cosine-distributed directions about +Z: a Fibonacci spiral on the unit disc,
        // lifted onto the hemisphere.
        let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
        let dirs = (0..rays)
            .map(|i| {
                let r = ((i as f32 + 0.5) / rays as f32).sqrt();
                let a = i as f32 * golden;
                Vec3::new(r * a.cos(), r * a.sin(), (1.0 - r * r).max(0.0).sqrt())
            })
            .collect();
        Self { solids, origin, cell, dims, cells, dirs, radius }
    }

    /// Indices of solids whose cells overlap the box `p - r .. p + r`, sorted, unique.
    fn candidates(&self, p: Vec3, r: f32, out: &mut Vec<u32>) {
        out.clear();
        let a = ((p - Vec3::splat(r) - self.origin) / self.cell).floor().max(Vec3::ZERO);
        let b = ((p + Vec3::splat(r) - self.origin) / self.cell).floor();
        let clampi = |v: f32, d: usize| (v.max(0.0) as usize).min(d - 1);
        for z in clampi(a.z, self.dims[2])..=clampi(b.z, self.dims[2]) {
            for y in clampi(a.y, self.dims[1])..=clampi(b.y, self.dims[1]) {
                for x in clampi(a.x, self.dims[0])..=clampi(b.x, self.dims[0]) {
                    out.extend_from_slice(&self.cells[(z * self.dims[1] + y) * self.dims[0] + x]);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }

    /// Local openness at `p` with normal `n`: 1 with nothing within `radius`, falling
    /// towards 0 in a closed corner.
    pub fn local(&self, p: Vec3, n: Vec3) -> f32 {
        thread_local! {
            static CAND: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        let n = n.normalize();
        // Tangent frame (deterministic for a given normal).
        let t = if n.y.abs() < 0.9 { Vec3::Y.cross(n).normalize() } else { Vec3::X.cross(n).normalize() };
        let bt = n.cross(t);
        let o = p + n * 0.02;
        CAND.with(|c| {
            let mut cand = c.borrow_mut();
            self.candidates(o, self.radius, &mut cand);
            if cand.is_empty() {
                return 1.0;
            }
            let mut occl = 0.0;
            for d in &self.dirs {
                let dir = t * d.x + bt * d.y + n * d.z;
                let mut tmin = self.radius;
                for &i in cand.iter() {
                    if let Some(th) = ray_obb(o, dir, &self.solids[i as usize], tmin) {
                        tmin = th;
                    }
                }
                if tmin < self.radius {
                    let k = 1.0 - tmin / self.radius;
                    occl += k * k;
                }
            }
            1.0 - occl / self.dirs.len() as f32
        })
    }
}

/// Entry distance of the ray into the box, if it is in `0..tmax`. A ray starting inside the
/// box ignores it: faces that lie inside another solid (a round column's facets inside its
/// own octagonal collider, a plinth's top under a column) must not count as buried.
fn ray_obb(o: Vec3, d: Vec3, s: &Solid, tmax: f32) -> Option<f32> {
    let rel = o - s.centre;
    if (0..3).all(|i| rel.dot(s.axes[i]).abs() < s.half[i]) {
        return None;
    }
    let (mut t0, mut t1) = (0.0f32, tmax);
    for i in 0..3 {
        let ax = s.axes[i];
        let e = rel.dot(ax);
        let f = d.dot(ax);
        let h = s.half[i];
        if f.abs() < 1e-8 {
            if e.abs() > h {
                return None;
            }
        } else {
            let inv = 1.0 / f;
            let (mut a, mut b) = ((-h - e) * inv, (h - e) * inv);
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            t0 = t0.max(a);
            t1 = t1.min(b);
            if t0 > t1 {
                return None;
            }
        }
    }
    Some(t0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_space_is_open_and_corners_are_dark() {
        // A floor slab and a wall meeting along x = 0.
        let floor = Collider::Aabb { min: [-10.0, -1.0, -10.0], max: [10.0, 0.0, 10.0] };
        let wall = Collider::Aabb { min: [-1.0, 0.0, -10.0], max: [0.0, 10.0, 10.0] };
        let occ = Occluders::new(&[floor, wall], 4.0, 64, 4.0);
        let far = occ.local(Vec3::new(8.0, 0.0, 0.0), Vec3::Y);
        let corner = occ.local(Vec3::new(0.05, 0.0, 0.0), Vec3::Y);
        let mid = occ.local(Vec3::new(1.5, 0.0, 0.0), Vec3::Y);
        assert!(far > 0.99, "far {far}");
        assert!(corner < mid && mid < far, "corner {corner} mid {mid} far {far}");
        assert!(corner < 0.8, "corner {corner}");
    }

    #[test]
    fn local_ao_is_continuous() {
        let floor = Collider::Aabb { min: [-10.0, -1.0, -10.0], max: [10.0, 0.0, 10.0] };
        let wall = Collider::Aabb { min: [-1.0, 0.0, -10.0], max: [0.0, 10.0, 10.0] };
        let occ = Occluders::new(&[floor, wall], 4.0, 48, 4.0);
        let mut prev = occ.local(Vec3::new(0.05, 0.0, 0.0), Vec3::Y);
        for k in 1..400 {
            let x = 0.05 + k as f32 * 0.01;
            let a = occ.local(Vec3::new(x, 0.0, 0.0), Vec3::Y);
            assert!((a - prev).abs() < 0.03, "jump {prev} -> {a} at x {x}");
            prev = a;
        }
    }
}
