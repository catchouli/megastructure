//! Collision world: the scene's colliders behind a BVH, and the capsule queries the walker
//! needs (sweep, contact, depenetration). No window, no GPU.
//!
//! Every `mega_gen::Collider` is a box (a ramp is a slanted slab). parry3d supplies the BVH
//! (built once per scene) and the reference shapes the tests check against. The capsule-versus-box
//! distance and the sweep are exact and written here: parry's `cast_shapes` for a capsule and a
//! cuboid was measured to be off by up to 1.3 cm in time of impact and by degrees in normal
//! near contact (docs/experiments.md, E101), which a walker resting a few millimetres from the
//! floor cannot live with.
//!
//! The sweep is conservative advancement: the distance between two convex shapes translating
//! along a line is a convex function of the distance travelled, so advancing by
//! `distance / closing_speed` never overshoots and converges quickly from the left.
//!
//! parry works in glam 0.33 types through `glamx`; the game uses glam 0.34, so values cross
//! this boundary as plain components.

use crate::player::{CAPSULE_HEIGHT, CAPSULE_RADIUS, SKIN, STABLE_RING_REACH, STABLE_RING_SAMPLES};
use glam::Vec3;
use mega_gen::Collider;
use parry3d::bounding_volume::Aabb;
use parry3d::math::Vector;
use parry3d::partitioning::{Bvh, BvhBuildStrategy};

fn pv(v: Vec3) -> Vector {
    Vector::new(v.x, v.y, v.z)
}

/// An oriented box: `centre + axes[i] * x * half[i]` for x in -1..1; axes orthonormal.
#[derive(Clone, Copy, Debug)]
pub struct Solid {
    pub centre: Vec3,
    pub axes: [Vec3; 3],
    pub half: Vec3,
}

/// The nearest thing a capsule sweep runs into.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    /// Fraction of the sweep travelled before stopping (0 = already touching).
    pub fraction: f32,
    /// Unit surface normal, pointing out of the obstacle towards the capsule.
    pub normal: Vec3,
    /// The point of the obstacle the capsule touches (a corner or edge point when the capsule
    /// rests on a corner, so its height is the ledge's even though the normal is slanted).
    pub point: Vec3,
}

/// Where a capsule stands relative to one box.
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    /// Gap between the capsule's surface and the box; negative when they overlap (the depth).
    pub dist: f32,
    /// Unit direction to move the capsule to separate it, pointing out of the box.
    pub normal: Vec3,
    /// The closest point of the box.
    pub point: Vec3,
}

pub struct World {
    solids: Vec<Solid>,
    bvh: Bvh,
}

impl Solid {
    pub fn new(c: &Collider) -> Solid {
        let (centre, axes, half) = c.to_obb();
        Solid { centre: Vec3::from(centre), axes: axes.map(Vec3::from), half: Vec3::from(half) }
    }

    fn local_point(&self, p: Vec3) -> Vec3 {
        let d = p - self.centre;
        Vec3::new(d.dot(self.axes[0]), d.dot(self.axes[1]), d.dot(self.axes[2]))
    }

    fn world_dir(&self, v: Vec3) -> Vec3 {
        self.axes[0] * v.x + self.axes[1] * v.y + self.axes[2] * v.z
    }

    fn world_point(&self, v: Vec3) -> Vec3 {
        self.centre + self.world_dir(v)
    }

    /// Closest approach of the segment `a..b` (world space) to the box: the segment point, the
    /// box point, and the squared distance. If the segment meets the box, the squared distance is
    /// zero and the points are the deepest point of the segment inside and itself.
    fn segment_distance(&self, a: Vec3, b: Vec3) -> SegmentApproach {
        let (la, lb) = (self.local_point(a), self.local_point(b));
        let d = lb - la;
        let h = self.half;
        // Breakpoints where the segment crosses a face plane of the box.
        let mut ts = [0.0f32; 8];
        let mut n = 1;
        ts[0] = 0.0;
        for i in 0..3 {
            if d[i].abs() > 1e-9 {
                for sgn in [-1.0f32, 1.0] {
                    let t = (sgn * h[i] - la[i]) / d[i];
                    if t > 0.0 && t < 1.0 {
                        ts[n] = t;
                        n += 1;
                    }
                }
            }
        }
        ts[n] = 1.0;
        n += 1;
        ts[..n].sort_by(|x, y| x.partial_cmp(y).unwrap());

        let clamp = |p: Vec3| p.clamp(-h, h);
        let at = |t: f32| la + d * t;
        let mut best = SegmentApproach { t: 0.0, dist_sq: f32::MAX, inside_depth: -1.0 };
        let mut consider = |t: f32| {
            let p = at(t);
            let dsq = (p - clamp(p)).length_squared();
            if dsq < best.dist_sq {
                best.t = t;
                best.dist_sq = dsq;
            }
        };
        for w in ts[..n].windows(2) {
            let (t0, t1) = (w[0], w[1]);
            consider(t0);
            if t1 - t0 < 1e-9 {
                continue;
            }
            // Inside one interval each axis is entirely inside, above or below the slab, so the
            // squared distance is a quadratic in t. Minimise it exactly.
            let tm = 0.5 * (t0 + t1);
            let pm = at(tm);
            let (mut qa, mut qb) = (0.0f32, 0.0f32);
            for i in 0..3 {
                let c = if pm[i] > h[i] {
                    h[i]
                } else if pm[i] < -h[i] {
                    -h[i]
                } else {
                    continue;
                };
                qa += d[i] * d[i];
                qb += d[i] * (la[i] - c);
            }
            if qa > 1e-12 {
                consider((-qb / qa).clamp(t0, t1));
            } else {
                consider(tm);
            }
        }
        consider(1.0);
        if best.dist_sq <= 1e-14 {
            // The segment meets the box: find its deepest point (the depth is piecewise linear
            // in t, so a breakpoint or interval end is deepest).
            best.dist_sq = 0.0;
            let mut deepest = (-1.0f32, 0.0f32);
            for &t in &ts[..n] {
                let p = at(t);
                let depth = (h - p.abs()).min_element();
                if depth > deepest.0 {
                    deepest = (depth, t);
                }
            }
            best.t = deepest.1;
            best.inside_depth = deepest.0.max(0.0);
        }
        best
    }

    /// A ray straight down from `origin`: where it first enters the box (its height), or that
    /// it starts inside the box, or None if it misses.
    fn ray_down(&self, origin: Vec3) -> Option<Probe> {
        let o = self.local_point(origin);
        let (mut t_min, mut t_max) = (0.0f32, f32::MAX);
        for i in 0..3 {
            let d = -self.axes[i].y;
            if d.abs() < 1e-9 {
                if o[i].abs() > self.half[i] {
                    return None;
                }
            } else {
                let (t1, t2) = ((-self.half[i] - o[i]) / d, (self.half[i] - o[i]) / d);
                t_min = t_min.max(t1.min(t2));
                t_max = t_max.min(t1.max(t2));
            }
        }
        if t_min > t_max {
            None
        } else if t_min > 0.0 {
            Some(Probe::Hit(origin.y - t_min))
        } else if t_max > 0.0 {
            Some(Probe::Inside)
        } else {
            None
        }
    }

    /// The capsule with its end-sphere centres at `a` and `b` and radius `r`, against this box.
    pub fn contact(&self, a: Vec3, b: Vec3, r: f32) -> Contact {
        let s = self.segment_distance(a, b);
        let la = self.local_point(a);
        let ld = self.local_point(b) - la;
        let p = la + ld * s.t;
        if s.dist_sq > 0.0 {
            let q = p.clamp(-self.half, self.half);
            let dist = s.dist_sq.sqrt();
            let n = self.world_dir((p - q) / dist);
            Contact { dist: dist - r, normal: n, point: self.world_point(q) }
        } else {
            // The axis centre line is inside the box: leave by the nearest face.
            let (mut axis, mut least) = (0, f32::MAX);
            for i in 0..3 {
                let room = self.half[i] - p[i].abs();
                if room < least {
                    least = room;
                    axis = i;
                }
            }
            let mut n = Vec3::ZERO;
            n[axis] = if p[axis] >= 0.0 { 1.0 } else { -1.0 };
            let mut on_face = p;
            on_face[axis] = n[axis] * self.half[axis];
            Contact { dist: -(least + r), normal: self.world_dir(n), point: self.world_point(on_face) }
        }
    }
}

/// Result of a downward ray against one box.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Probe {
    /// The ray enters the box's top at this height.
    Hit(f32),
    /// The ray starts inside the box.
    Inside,
}

#[derive(Clone, Copy)]
struct SegmentApproach {
    t: f32,
    dist_sq: f32,
    inside_depth: f32,
}

impl World {
    /// Build the collision world once per scene.
    pub fn new(colliders: &[Collider]) -> World {
        let solids: Vec<Solid> = colliders.iter().map(Solid::new).collect();
        let aabbs: Vec<Aabb> = colliders
            .iter()
            .map(|c| {
                let (lo, hi) = c.bounds();
                Aabb::new(Vector::new(lo[0], lo[1], lo[2]), Vector::new(hi[0], hi[1], hi[2]))
            })
            .collect();
        let bvh = Bvh::from_leaves(BvhBuildStrategy::Binned, &aabbs);
        World { solids, bvh }
    }

    /// The capsule's end-sphere centres for feet at `feet`.
    fn segment(feet: Vec3) -> (Vec3, Vec3) {
        (feet + Vec3::Y * CAPSULE_RADIUS, feet + Vec3::Y * (CAPSULE_HEIGHT - CAPSULE_RADIUS))
    }

    /// Boxes whose bounds meet the capsule's bounds at `from` and at `to`, grown by `margin`.
    fn candidates(&self, from: Vec3, to: Vec3, margin: f32) -> impl Iterator<Item = &Solid> + '_ {
        let m = Vec3::splat(CAPSULE_RADIUS + margin);
        let lo = from.min(to) - Vec3::new(m.x, margin, m.z);
        let hi = from.max(to) + Vec3::new(m.x, CAPSULE_HEIGHT + margin, m.z);
        let query = Aabb::new(pv(lo), pv(hi));
        let hits: Vec<u32> = self.bvh.intersect_aabb(&query).collect();
        hits.into_iter().map(|i| &self.solids[i as usize])
    }

    /// Sweep the capsule with its feet at `feet` along `motion`. The result is the first
    /// obstacle, with the capsule stopping `SKIN` short of it, or None if the whole motion is
    /// free. Obstacles the capsule is touching but moving away from or along are ignored.
    pub fn cast(&self, feet: Vec3, motion: Vec3) -> Option<Hit> {
        let len = motion.length();
        if len < 1e-6 {
            return None;
        }
        let dir = motion / len;
        let mut best: Option<Hit> = None;
        for s in self.candidates(feet, feet + motion, SKIN + 0.01) {
            let Some(hit) = sweep_one(s, feet, dir, len) else { continue };
            let better = match best {
                None => true,
                Some(b) => hit.fraction < b.fraction - 1e-6 || (hit.fraction < b.fraction + 1e-6 && hit.normal.dot(dir) < b.normal.dot(dir)),
            };
            if better {
                best = Some(hit);
            }
        }
        best
    }

    /// Push the capsule out of anything it overlaps (a few passes), leaving `SKIN` of gap.
    /// Touching or standing within the skin is not an overlap. Returns the new feet.
    pub fn depenetrate(&self, mut feet: Vec3) -> Vec3 {
        for _ in 0..4 {
            let (a, b) = Self::segment(feet);
            let mut push = Vec3::ZERO;
            for s in self.candidates(feet, feet, 0.0) {
                let c = s.contact(a, b, CAPSULE_RADIUS);
                if c.dist < -1e-4 {
                    push += c.normal * (SKIN - c.dist);
                }
            }
            if push.length_squared() < 1e-12 {
                break;
            }
            feet += push.clamp_length_max(0.5);
        }
        feet
    }

    /// The closest box to the capsule at `feet` (negative gap = overlap), for tests.
    #[cfg(test)]
    pub fn nearest(&self, feet: Vec3) -> Option<Contact> {
        let (a, b) = Self::segment(feet);
        self.candidates(feet, feet, 1.0).map(|s| s.contact(a, b, CAPSULE_RADIUS)).min_by(|x, y| x.dist.partial_cmp(&y.dist).unwrap())
    }

    /// Whether there is a surface straight below the middle of the capsule, within `reach`
    /// below the feet: the player stands on the ground, not on the rim of a corner.
    #[cfg(test)]
    pub fn supported_below(&self, feet: Vec3, reach: f32) -> bool {
        let origin = feet + Vec3::Y * 0.05;
        let floor = feet.y - reach;
        self.candidates(origin, origin - Vec3::Y * (reach + 0.1), 0.0).any(|s| s.ray_down(origin).is_some_and(|p| matches!(p, Probe::Hit(y) if y >= floor)))
    }

    /// Whether the capsule at `feet` stands well inside its ground: around it, at radius
    /// `radius`, at `STABLE_RING_SAMPLES` points, there is either solid at the height of the feet
    /// (a wall, a rail, a taller ledge) or ground no more than `STABLE_RING_REACH` below or above
    /// the feet. Not near a lip or the end of a bridge, where one of the points sees only air.
    pub fn supported_ring(&self, feet: Vec3, radius: f32) -> bool {
        let top = feet.y + STABLE_RING_REACH;
        let floor = feet.y - STABLE_RING_REACH;
        (0..STABLE_RING_SAMPLES).all(|i| {
            let a = std::f32::consts::TAU * i as f32 / STABLE_RING_SAMPLES as f32;
            let origin = Vec3::new(feet.x + radius * a.cos(), top, feet.z + radius * a.sin());
            let query = Aabb::new(pv(Vec3::new(origin.x - 0.01, floor - 0.1, origin.z - 0.01)), pv(Vec3::new(origin.x + 0.01, top + 0.1, origin.z + 0.01)));
            let hits: Vec<u32> = self.bvh.intersect_aabb(&query).collect();
            hits.into_iter().any(|j| match self.solids[j as usize].ray_down(origin) {
                Some(Probe::Inside) => true,
                Some(Probe::Hit(y)) => y >= floor,
                None => false,
            })
        })
    }

    /// Whether the capsule at `feet` overlaps any collider (touching within `SKIN` does not count).
    #[cfg(test)]
    pub fn overlaps(&self, feet: Vec3) -> bool {
        self.nearest(feet).is_some_and(|c| c.dist < -1e-3)
    }
}

/// Conservative advancement of the capsule against one box. The capsule is treated as swollen
/// by `SKIN`, so the sweep stops with that much gap.
fn sweep_one(s: &Solid, feet: Vec3, dir: Vec3, len: f32) -> Option<Hit> {
    let mut t = 0.0f32;
    let mut last: Option<Contact> = None;
    for _ in 0..32 {
        let (a, b) = World::segment(feet + dir * t);
        let c = s.contact(a, b, CAPSULE_RADIUS + SKIN);
        let closing = -c.normal.dot(dir);
        if closing <= 1e-5 {
            // Moving away or along the surface: by convexity it never gets closer.
            return None;
        }
        if c.dist <= 1e-5 {
            return Some(Hit { fraction: (t / len).clamp(0.0, 1.0), normal: c.normal, point: c.point });
        }
        t += c.dist / closing;
        if t > len {
            return None;
        }
        last = Some(c);
    }
    // Did not converge: stop where it is safe.
    last.map(|c| Hit { fraction: (t / len).clamp(0.0, 1.0), normal: c.normal, point: c.point })
}

#[cfg(test)]
mod tests {
    use super::*;
    use parry3d::math::Pose;
    use parry3d::query;
    use parry3d::shape::{Capsule, Cuboid};

    fn floor() -> Vec<Collider> {
        vec![Collider::Aabb { min: [-10.0, -1.0, -10.0], max: [10.0, 0.0, 10.0] }]
    }

    #[test]
    fn cast_down_stops_a_skin_above_the_floor() {
        let w = World::new(&floor());
        let h = w.cast(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, -2.0, 0.0)).expect("hit");
        let stop_y = 1.0 - 2.0 * h.fraction;
        assert!((stop_y - SKIN).abs() < 1e-4, "{stop_y}");
        assert!((h.normal - Vec3::Y).length() < 1e-6, "{:?}", h.normal);
    }

    #[test]
    fn oblique_cast_onto_a_floor_is_exact() {
        let w = World::new(&floor());
        for g in [0.01f32, 0.05, 0.2] {
            let m = Vec3::new(0.3, -0.3, -0.1);
            let h = w.cast(Vec3::new(0.0, g, 0.0), m).expect("hit");
            let y = g + m.y * h.fraction;
            assert!((y - SKIN).abs() < 1e-4, "gap {g}: stopped at {y}");
        }
    }

    #[test]
    fn sideways_motion_along_the_floor_is_free_and_into_it_is_a_hit_at_once() {
        let w = World::new(&floor());
        let feet = Vec3::new(0.0, SKIN, 0.0);
        assert!(w.cast(feet, Vec3::new(1.0, 0.0, 0.0)).is_none());
        assert!(w.cast(feet, Vec3::new(1.0, 0.1, 0.0)).is_none());
        let h = w.cast(feet, Vec3::new(1.0, -0.001, 0.0)).expect("into the floor");
        assert!(h.fraction < 1e-3 && (h.normal - Vec3::Y).length() < 1e-6);
    }

    #[test]
    fn wall_normal_points_at_the_capsule() {
        let mut c = floor();
        c.push(Collider::Aabb { min: [2.0, 0.0, -5.0], max: [3.0, 3.0, 5.0] });
        let w = World::new(&c);
        let h = w.cast(Vec3::new(0.0, SKIN, 0.0), Vec3::new(5.0, 0.0, 0.0)).expect("wall");
        assert!((h.normal - Vec3::NEG_X).length() < 1e-6, "{:?}", h.normal);
        assert!((5.0 * h.fraction - (2.0 - CAPSULE_RADIUS - SKIN)).abs() < 1e-4);
    }

    #[test]
    fn depenetrate_lifts_out_of_the_floor_and_leaves_resting_alone() {
        let w = World::new(&floor());
        let p = w.depenetrate(Vec3::new(0.0, -0.1, 0.0));
        assert!((p.y - SKIN).abs() < 1e-4 && !w.overlaps(p), "{p:?}");
        assert_eq!(w.depenetrate(Vec3::new(0.0, 0.0, 0.0)), Vec3::ZERO);
        assert_eq!(w.depenetrate(Vec3::new(0.0, SKIN, 0.0)), Vec3::new(0.0, SKIN, 0.0));
    }

    #[test]
    fn ramp_collider_is_a_slanted_slab() {
        // 30 degree ramp from (0,0,0) up to (3.464, 2, 0), top surface through both points.
        let r = Collider::Ramp { start: [0.0, 0.0, 0.0], end: [3.4641, 2.0, 0.0], half_width: 1.0, thickness: 0.3 };
        let w = World::new(&[r]);
        let h = w.cast(Vec3::new(1.732, 3.0, 0.0), Vec3::new(0.0, -4.0, 0.0)).expect("ramp");
        let stop_y = 3.0 - 4.0 * h.fraction;
        // Surface height at x = 1.732 is 1.0. The bottom sphere (plus skin) sits on the slope with
        // its centre (r + SKIN) / cos above the surface vertically; the feet are r below that.
        let cos = 30.0f32.to_radians().cos();
        assert!((stop_y - (1.0 + (CAPSULE_RADIUS + SKIN) / cos - CAPSULE_RADIUS)).abs() < 1e-3, "{stop_y}");
        assert!((h.normal.y - 30.0f32.to_radians().cos()).abs() < 1e-4, "{:?}", h.normal);
    }

    /// Small deterministic generator for the cross-check below.
    struct Lcg(u64);
    impl Lcg {
        fn f(&mut self, lo: f32, hi: f32) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            lo + (hi - lo) * ((self.0 >> 40) as f32 / (1u64 << 24) as f32)
        }
    }

    #[test]
    fn capsule_box_distance_matches_a_brute_force_search_with_parrys_point_projection() {
        use parry3d::query::PointQuery;
        let mut rng = Lcg(7);
        let (mut worst, mut worst_vs_gjk, mut checked) = (0.0f32, 0.0f32, 0);
        let cap = Capsule::new_y((CAPSULE_HEIGHT - 2.0 * CAPSULE_RADIUS) * 0.5, CAPSULE_RADIUS);
        for _ in 0..3000 {
            let half = Vec3::new(rng.f(0.05, 2.0), rng.f(0.05, 2.0), rng.f(0.05, 2.0));
            let rot = glam::Quat::from_euler(glam::EulerRot::YXZ, rng.f(0.0, std::f32::consts::TAU), rng.f(-0.5, 0.5), rng.f(-0.3, 0.3));
            let axes = [rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z];
            let centre = Vec3::new(rng.f(-1.0, 1.0), rng.f(-1.0, 1.0), rng.f(-1.0, 1.0));
            let solid = Solid { centre, axes, half };
            let feet = Vec3::new(rng.f(-3.0, 3.0), rng.f(-3.0, 3.0), rng.f(-3.0, 3.0));
            let (a, b) = World::segment(feet);
            let mine = solid.contact(a, b, CAPSULE_RADIUS);
            if mine.dist < 0.0 {
                continue;
            }
            let q = parry3d::math::Rot3::from_xyzw(rot.x, rot.y, rot.z, rot.w);
            let pose_box = Pose::from_parts(pv(centre), q);
            let cuboid = Cuboid::new(pv(half));
            // Brute force: the closest of 4000 points along the capsule axis, by parry's exact point projection.
            let brute = (0..=4000)
                .map(|i| {
                    let p = a + (b - a) * (i as f32 / 4000.0);
                    let proj = cuboid.project_point(&pose_box, pv(p), true);
                    (proj.point - pv(p)).length() - CAPSULE_RADIUS
                })
                .fold(f32::MAX, f32::min);
            worst = worst.max((brute - mine.dist).abs());
            let pose_cap = Pose::from_translation(pv(feet + Vec3::Y * CAPSULE_HEIGHT * 0.5));
            let theirs = query::distance(&pose_cap, &cap, &pose_box, &cuboid).unwrap();
            worst_vs_gjk = worst_vs_gjk.max((theirs.distance - mine.dist).abs());
            checked += 1;
        }
        assert!(checked > 1000, "only {checked} separated poses");
        assert!(worst < 2e-4, "differs from brute force by {worst}");
        // parry's own GJK distance is looser (several millimetres) than the exact one.
        println!("worst difference to parry's GJK distance: {worst_vs_gjk}");
        assert!(worst_vs_gjk < 2e-2);
    }
}
