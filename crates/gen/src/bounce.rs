//! One bounce of sun and sky light, baked per vertex on the CPU.
//!
//! For each distinct vertex (position and normal), a fixed set of cosine-distributed rays is
//! traced against the scene's own triangles. At each hit on a front face the light leaving
//! that surface is taken as the renderer's own direct light there, split into its two parts:
//!
//! - **sun**: `max(N.S, 0)` times the sun's visibility at the hit, interpolated from a shadow
//!   ray per vertex of the hit triangle;
//! - **sky**: the hit's baked `ao` (the openness and local occlusion the renderer uses as its
//!   ambient term), interpolated over the triangle.
//!
//! The mean over the rays of each part is stored (a cosine-weighted mean is irradiance over
//! pi, so with an albedo of 1 it is the light the receiver gets back). Back faces and misses
//! contribute nothing: direct sky is already in `ao`. The renderer multiplies the two parts by
//! its sun and ambient weights and a strength from the preset, so the sun's intensity and the
//! preset can change without re-baking; only a new sun direction needs a new bake.
//!
//! The ray directions are the same for every vertex (a Fibonacci spiral in the vertex's
//! tangent frame, as in `ao.rs`), so the result is smooth from vertex to vertex rather than
//! noisy. See docs/features/bounced-light.md.

use crate::mesh::MeshData;
use glam::Vec3;
use std::collections::HashMap;

/// Where the sun is, for the bake.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BounceSun {
    /// A remote sun: unit direction towards it.
    Direction(Vec3),
    /// A near sun at this position.
    Point(Vec3),
}

impl BounceSun {
    fn towards(&self, p: Vec3) -> (Vec3, f32) {
        match *self {
            BounceSun::Direction(d) => (d, f32::INFINITY),
            BounceSun::Point(s) => {
                let v = s - p;
                let l = v.length().max(1e-6);
                (v / l, l)
            }
        }
    }
}

/// Bake settings.
#[derive(Clone, Copy, Debug)]
pub struct BounceSettings {
    /// Gather rays per vertex.
    pub rays: usize,
    /// Worker threads (0: all available).
    pub threads: usize,
}

impl Default for BounceSettings {
    fn default() -> Self {
        Self { rays: 64, threads: 0 }
    }
}

#[derive(Clone, Copy)]
struct Tri {
    p0: Vec3,
    e1: Vec3,
    e2: Vec3,
    n: Vec3,
    /// Unique-vertex index of each corner.
    v: [u32; 3],
}

#[derive(Clone, Copy)]
struct Node {
    lo: Vec3,
    hi: Vec3,
    /// Leaf: first triangle; inner: index of the right child (left is the next node).
    start: u32,
    /// Leaf: triangle count; inner: 0.
    count: u32,
}

/// The scene's triangles in a bounding volume hierarchy, with the distinct vertices the bake
/// is computed at. Build once per scene; [`BounceBaker::bake`] can then be called again for
/// each new sun direction.
pub struct BounceBaker {
    tris: Vec<Tri>,
    nodes: Vec<Node>,
    /// Distinct (position, normal) pairs and their ao.
    upos: Vec<Vec3>,
    unrm: Vec<Vec3>,
    uao: Vec<f32>,
    /// Vertex index to distinct index.
    remap: Vec<u32>,
}

impl BounceBaker {
    pub fn new(mesh: &MeshData) -> Self {
        let mut map: HashMap<[u32; 6], u32> = HashMap::new();
        let (mut upos, mut unrm, mut uao) = (Vec::new(), Vec::new(), Vec::new());
        let remap: Vec<u32> = mesh
            .vertices
            .iter()
            .map(|v| {
                let k = [v.position[0].to_bits(), v.position[1].to_bits(), v.position[2].to_bits(), v.normal[0].to_bits(), v.normal[1].to_bits(), v.normal[2].to_bits()];
                *map.entry(k).or_insert_with(|| {
                    upos.push(Vec3::from(v.position));
                    unrm.push(Vec3::from(v.normal).normalize());
                    uao.push(v.ao);
                    (upos.len() - 1) as u32
                })
            })
            .collect();
        let tris: Vec<Tri> = mesh
            .indices
            .chunks_exact(3)
            .map(|t| {
                let p: Vec<Vec3> = t.iter().map(|&i| Vec3::from(mesh.vertices[i as usize].position)).collect();
                Tri {
                    p0: p[0],
                    e1: p[1] - p[0],
                    e2: p[2] - p[0],
                    n: Vec3::from(mesh.vertices[t[0] as usize].normal).normalize(),
                    v: [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]],
                }
            })
            .collect();
        let mut b = Self { tris, nodes: Vec::new(), upos, unrm, uao, remap };
        b.build();
        b
    }

    /// Number of distinct vertices the bake is computed at.
    pub fn sample_count(&self) -> usize {
        self.upos.len()
    }

    fn build(&mut self) {
        let n = self.tris.len();
        if n == 0 {
            return;
        }
        let cent: Vec<Vec3> = self.tris.iter().map(|t| t.p0 + (t.e1 + t.e2) / 3.0).collect();
        let mut order: Vec<u32> = (0..n as u32).collect();
        let mut nodes = Vec::with_capacity(2 * n / 2 + 1);
        self.build_rec(&mut nodes, &mut order, &cent, 0, n);
        self.tris = order.iter().map(|&i| self.tris[i as usize]).collect();
        self.nodes = nodes;
    }

    fn tri_bounds(t: &Tri) -> (Vec3, Vec3) {
        let (a, b, c) = (t.p0, t.p0 + t.e1, t.p0 + t.e2);
        (a.min(b).min(c), a.max(b).max(c))
    }

    /// Binned SAH build over `order[s..e]`.
    fn build_rec(&self, nodes: &mut Vec<Node>, order: &mut [u32], cent: &[Vec3], s: usize, e: usize) -> usize {
        let idx = nodes.len();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let (mut clo, mut chi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for &i in &order[s..e] {
            let (a, b) = Self::tri_bounds(&self.tris[i as usize]);
            lo = lo.min(a);
            hi = hi.max(b);
            clo = clo.min(cent[i as usize]);
            chi = chi.max(cent[i as usize]);
        }
        nodes.push(Node { lo, hi, start: s as u32, count: (e - s) as u32 });
        let count = e - s;
        if count <= 4 {
            return idx;
        }
        const BINS: usize = 12;
        let ext = chi - clo;
        let area = |a: Vec3, b: Vec3| {
            let d = (b - a).max(Vec3::ZERO);
            d.x * d.y + d.y * d.z + d.z * d.x
        };
        let mut best = (f32::MAX, 0usize, 0f32);
        for axis in 0..3 {
            if ext[axis] < 1e-6 {
                continue;
            }
            let mut bl = [(Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), 0u32); BINS];
            let scale = BINS as f32 / ext[axis];
            for &i in &order[s..e] {
                let b = (((cent[i as usize][axis] - clo[axis]) * scale) as usize).min(BINS - 1);
                let (a, c) = Self::tri_bounds(&self.tris[i as usize]);
                bl[b].0 = bl[b].0.min(a);
                bl[b].1 = bl[b].1.max(c);
                bl[b].2 += 1;
            }
            // Sweep from the left and the right.
            let mut left = [0f32; BINS];
            let mut lc = [0u32; BINS];
            let (mut a, mut c, mut k) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), 0u32);
            for j in 0..BINS {
                a = a.min(bl[j].0);
                c = c.max(bl[j].1);
                k += bl[j].2;
                left[j] = area(a, c);
                lc[j] = k;
            }
            let (mut a, mut c, mut k) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), 0u32);
            for j in (1..BINS).rev() {
                a = a.min(bl[j].0);
                c = c.max(bl[j].1);
                k += bl[j].2;
                let cost = left[j - 1] * lc[j - 1] as f32 + area(a, c) * k as f32;
                if lc[j - 1] > 0 && k > 0 && cost < best.0 {
                    best = (cost, axis, clo[axis] + j as f32 / scale);
                }
            }
        }
        let mut mid;
        if best.0 < f32::MAX {
            let (axis, split) = (best.1, best.2);
            let slice = &mut order[s..e];
            let mut i = 0;
            for j in 0..slice.len() {
                if cent[slice[j] as usize][axis] < split {
                    slice.swap(i, j);
                    i += 1;
                }
            }
            mid = s + i;
        } else {
            mid = s;
        }
        if mid == s || mid == e {
            // All centroids coincide (or no useful split): halve by position in the list.
            mid = s + count / 2;
        }
        self.build_rec(nodes, order, cent, s, mid);
        let right = self.build_rec(nodes, order, cent, mid, e);
        nodes[idx].start = right as u32;
        nodes[idx].count = 0;
        idx
    }

    /// Closest hit along `o + d t`, `t` in `(0, tmax)`: triangle, t, barycentrics u and v.
    fn closest(&self, o: Vec3, d: Vec3, tmax: f32) -> Option<(u32, f32, f32, f32)> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = d.recip();
        let mut best: Option<(u32, f32, f32, f32)> = None;
        let mut tbest = tmax;
        let mut stack = [0u32; 64];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let node = &self.nodes[stack[sp] as usize];
            if slab(node.lo, node.hi, o, inv, tbest).is_none() {
                continue;
            }
            if node.count > 0 {
                for ti in node.start..node.start + node.count {
                    if let Some((t, u, v)) = tri_hit(&self.tris[ti as usize], o, d, tbest) {
                        tbest = t;
                        best = Some((ti, t, u, v));
                    }
                }
            } else {
                // Visit the nearer child first.
                let (l, r) = (stack_index(node, true, stack[sp]), node.start);
                let nl = &self.nodes[l as usize];
                let nr = &self.nodes[r as usize];
                let tl = slab(nl.lo, nl.hi, o, inv, tbest).unwrap_or(f32::MAX);
                let tr = slab(nr.lo, nr.hi, o, inv, tbest).unwrap_or(f32::MAX);
                if sp + 2 > stack.len() {
                    continue;
                }
                if tl < tr {
                    stack[sp] = r;
                    stack[sp + 1] = l;
                } else {
                    stack[sp] = l;
                    stack[sp + 1] = r;
                }
                sp += 2;
            }
        }
        best
    }

    /// Whether anything lies along `o + d t`, `t` in `(0, tmax)`.
    fn occluded(&self, o: Vec3, d: Vec3, tmax: f32) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let inv = d.recip();
        let mut stack = [0u32; 64];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let ni = stack[sp];
            let node = &self.nodes[ni as usize];
            if slab(node.lo, node.hi, o, inv, tmax).is_none() {
                continue;
            }
            if node.count > 0 {
                for ti in node.start..node.start + node.count {
                    if tri_hit(&self.tris[ti as usize], o, d, tmax).is_some() {
                        return true;
                    }
                }
            } else if sp + 2 <= stack.len() {
                stack[sp] = stack_index(node, true, ni);
                stack[sp + 1] = node.start;
                sp += 2;
            }
        }
        false
    }

    /// Bake for `sun`: per vertex of the mesh this baker was built from, `[sun, sky]` bounce
    /// (see the module documentation).
    pub fn bake(&self, sun: BounceSun, settings: BounceSettings) -> Vec<[f32; 2]> {
        let nu = self.upos.len();
        let threads = if settings.threads == 0 { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) } else { settings.threads };
        // 1. Sun visibility at every distinct vertex: N.S times a shadow ray.
        let direct = par_map(nu, threads, |i| {
            let (p, n) = (self.upos[i], self.unrm[i]);
            let (s, dist) = sun.towards(p);
            let ns = n.dot(s);
            if ns <= 0.0 {
                return 0.0;
            }
            let o = p + n * 0.03;
            if self.occluded(o, s, dist) {
                0.0
            } else {
                ns
            }
        });
        // 2. Gather.
        let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
        let rays = settings.rays.max(1);
        let dirs: Vec<Vec3> = (0..rays)
            .map(|i| {
                let r = ((i as f32 + 0.5) / rays as f32).sqrt();
                let a = i as f32 * golden;
                Vec3::new(r * a.cos(), r * a.sin(), (1.0 - r * r).max(0.0).sqrt())
            })
            .collect();
        let gathered = par_map(nu, threads, |i| {
            let (p, n) = (self.upos[i], self.unrm[i]);
            let t = if n.y.abs() < 0.9 { Vec3::Y.cross(n).normalize() } else { Vec3::X.cross(n).normalize() };
            let bt = n.cross(t);
            let o = p + n * 0.03;
            let (mut sun_acc, mut sky_acc) = (0.0f32, 0.0f32);
            for d in &dirs {
                let dir = t * d.x + bt * d.y + n * d.z;
                if let Some((ti, _, u, v)) = self.closest(o, dir, f32::INFINITY) {
                    let tri = &self.tris[ti as usize];
                    if tri.n.dot(dir) >= 0.0 {
                        continue; // back face: inside a solid or behind a single-sided panel
                    }
                    let w = [1.0 - u - v, u, v];
                    for k in 0..3 {
                        let vi = tri.v[k] as usize;
                        sun_acc += w[k] * direct[vi];
                        sky_acc += w[k] * self.uao[vi];
                    }
                }
            }
            [sun_acc / rays as f32, sky_acc / rays as f32]
        });
        self.remap.iter().map(|&u| gathered[u as usize]).collect()
    }
}

fn stack_index(_node: &Node, _left: bool, me: u32) -> u32 {
    // The left child is stored right after its parent.
    me + 1
}

fn par_map<T: Send + Copy + Default, F: Fn(usize) -> T + Sync>(n: usize, threads: usize, f: F) -> Vec<T> {
    let mut out = vec![T::default(); n];
    if n == 0 {
        return out;
    }
    let chunk = 256;
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out_ptr = SendPtr(out.as_mut_ptr());
    std::thread::scope(|sc| {
        for _ in 0..threads.max(1) {
            sc.spawn(|| {
                let ptr = &out_ptr;
                loop {
                    let s = next.fetch_add(chunk, std::sync::atomic::Ordering::Relaxed);
                    if s >= n {
                        break;
                    }
                    for i in s..(s + chunk).min(n) {
                        // SAFETY: each index is written by exactly one thread (disjoint chunks
                        // from the atomic counter), and `out` outlives the scope.
                        unsafe { *ptr.0.add(i) = f(i) };
                    }
                }
            });
        }
    });
    out
}

struct SendPtr<T>(*mut T);
unsafe impl<T> Sync for SendPtr<T> {}
unsafe impl<T> Send for SendPtr<T> {}

/// Entry distance of a ray into a box, if it enters before `tmax`.
fn slab(lo: Vec3, hi: Vec3, o: Vec3, inv: Vec3, tmax: f32) -> Option<f32> {
    let t0 = (lo - o) * inv;
    let t1 = (hi - o) * inv;
    let tn = t0.min(t1);
    let tf = t0.max(t1);
    let enter = tn.max_element().max(0.0);
    let exit = tf.min_element().min(tmax);
    (enter <= exit).then_some(enter)
}

/// Moller-Trumbore, both sides.
fn tri_hit(t: &Tri, o: Vec3, d: Vec3, tmax: f32) -> Option<(f32, f32, f32)> {
    let p = d.cross(t.e2);
    let det = t.e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t.p0;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(t.e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let tt = t.e2.dot(q) * inv;
    (tt > 1e-4 && tt < tmax).then_some((tt, u, v))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sunlit floor with a roof slab above part of it: under the roof the floor gets no sun
    /// but some bounce from the lit floor beside it; in the open, bounce from nothing.
    #[test]
    fn shade_next_to_light_gets_bounce() {
        let mut m = MeshData::default();
        let quad = |m: &mut MeshData, c: [Vec3; 4], n: Vec3| {
            let base = m.vertices.len() as u32;
            for p in c {
                m.vertices.push(crate::Vertex { position: p.into(), normal: n.into(), hatch_axis: [1.0, 0.0, 0.0], id_fine: 1 + base, id_coarse: 1, ao: 1.0 });
            }
            m.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        };
        // Floor (y = 0) facing up, 40 x 40, in 2 m tiles so there are vertices under the roof.
        for i in 0..20 {
            for j in 0..20 {
                let (x, z) = (-20.0 + i as f32 * 2.0, -20.0 + j as f32 * 2.0);
                quad(&mut m, [Vec3::new(x, 0.0, z), Vec3::new(x, 0.0, z + 2.0), Vec3::new(x + 2.0, 0.0, z + 2.0), Vec3::new(x + 2.0, 0.0, z)], Vec3::Y);
            }
        }
        // Roof underside (y = 3) facing down over x, z in -4..4, and its top facing up.
        quad(&mut m, [Vec3::new(-4.0, 3.0, -4.0), Vec3::new(4.0, 3.0, -4.0), Vec3::new(4.0, 3.0, 4.0), Vec3::new(-4.0, 3.0, 4.0)], -Vec3::Y);
        quad(&mut m, [Vec3::new(-4.0, 3.01, -4.0), Vec3::new(-4.0, 3.01, 4.0), Vec3::new(4.0, 3.01, 4.0), Vec3::new(4.0, 3.01, -4.0)], Vec3::Y);
        let baker = BounceBaker::new(&m);
        let b = baker.bake(BounceSun::Direction(Vec3::Y), BounceSettings { rays: 64, threads: 2 });
        let at = |x: f32, y: f32, z: f32| {
            let i = m.vertices.iter().position(|v| v.position == [x, y, z] && v.normal[1] > 0.5).unwrap();
            b[i]
        };
        let under = at(0.0, 0.0, 0.0);
        let open = at(-16.0, 0.0, -16.0);
        // The roof's underside is unlit, so under the roof the floor sees almost no bounce;
        // the roof's underside itself sees the whole lit floor.
        let roof = b[m.vertices.iter().position(|v| v.normal[1] < -0.5).unwrap()];
        assert!(roof[0] > 0.5, "roof underside {roof:?}");
        assert!(open[0] < 0.01, "open floor sees nothing above it: {open:?}");
        assert!(under[0] < roof[0], "{under:?} {roof:?}");
        assert!(b.iter().all(|v| v[0].is_finite() && v[1].is_finite() && v[0] >= 0.0 && v[0] <= 1.0));
    }

    /// Bake time on the shaft (release build, `cargo test --release -p mega_gen -- --ignored
    /// --nocapture shaft_bake_time`).
    #[test]
    #[ignore]
    fn shaft_bake_time() {
        let s = crate::scenes::by_name("shaft").unwrap();
        let t0 = std::time::Instant::now();
        let baker = BounceBaker::new(&s.mesh);
        let t1 = std::time::Instant::now();
        let sun = BounceSun::Direction(Vec3::new(-0.26, 0.96, -0.09).normalize());
        let b = baker.bake(sun, BounceSettings::default());
        let t2 = std::time::Instant::now();
        let mean = b.iter().fold([0.0f32; 2], |a, v| [a[0] + v[0], a[1] + v[1]]);
        println!(
            "shaft: {} vertices, {} distinct, BVH {:.0} ms, bake {:.0} ms, mean sun {:.3} sky {:.3}",
            s.mesh.vertices.len(),
            baker.sample_count(),
            (t1 - t0).as_secs_f64() * 1e3,
            (t2 - t1).as_secs_f64() * 1e3,
            mean[0] / b.len() as f32,
            mean[1] / b.len() as f32
        );
    }

    #[test]
    fn closest_hit_matches_brute_force() {
        let s = crate::scenes::by_name("test").unwrap();
        let baker = BounceBaker::new(&s.mesh);
        let mut seed = 1u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / (1u32 << 24) as f32
        };
        for _ in 0..200 {
            let o = Vec3::new(rnd() * 20.0 - 10.0, rnd() * 8.0, rnd() * 40.0 - 5.0);
            let d = Vec3::new(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5).normalize();
            let fast = baker.closest(o, d, f32::INFINITY).map(|h| h.1);
            let mut slow: Option<f32> = None;
            for t in &baker.tris {
                if let Some((tt, _, _)) = tri_hit(t, o, d, slow.unwrap_or(f32::INFINITY)) {
                    slow = Some(tt);
                }
            }
            match (fast, slow) {
                (Some(a), Some(b)) => assert!((a - b).abs() < 1e-3, "{a} {b}"),
                (None, None) => {}
                other => panic!("{other:?}"),
            }
            assert_eq!(baker.occluded(o, d, f32::INFINITY), slow.is_some());
        }
    }
}
