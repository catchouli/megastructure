//! Helpers shared by the test scene and (later) the real generator.

use crate::mesh::{Collider, MeshData, Stroke, StrokeKind, Vertex};
use glam::Vec3;

/// Ambient occlusion lookup: world position and face normal to a 0..1 value.
pub type AoFn<'a> = &'a dyn Fn(Vec3, Vec3) -> f32;

/// Which faces of a box to emit.
#[derive(Clone, Copy, Debug)]
pub struct FaceMask {
    pub x_neg: bool,
    pub x_pos: bool,
    pub y_neg: bool,
    pub y_pos: bool,
    pub z_neg: bool,
    pub z_pos: bool,
}

impl FaceMask {
    pub const ALL: FaceMask = FaceMask { x_neg: true, x_pos: true, y_neg: true, y_pos: true, z_neg: true, z_pos: true };
    /// Everything except the top and bottom (a column that runs floor to ceiling).
    pub const SIDES: FaceMask = FaceMask { y_neg: false, y_pos: false, ..FaceMask::ALL };
    /// Everything except the bottom (a platform standing on the floor).
    pub const NO_BOTTOM: FaceMask = FaceMask { y_neg: false, ..FaceMask::ALL };
}

/// Default hatch axis for a face: the projection of world up onto the face if it is not
/// near-horizontal, otherwise the longer of the two in-face edge directions `u` and `v`.
pub fn auto_hatch(normal: Vec3, u: Vec3, v: Vec3) -> Vec3 {
    let up = Vec3::Y - normal * normal.dot(Vec3::Y);
    if up.length() > 0.2 {
        return up.normalize();
    }
    let d = if u.length_squared() >= v.length_squared() { u } else { v };
    let d = d - normal * normal.dot(d);
    d.normalize()
}

/// Simple hand-rolled AO for an axis-aligned room: darker the closer a point is to the room
/// planes other than the one it lies on. Good enough to show concave corners.
pub fn box_ao(min: Vec3, max: Vec3) -> impl Fn(Vec3, Vec3) -> f32 {
    move |p, n| {
        let own = if n.x.abs() >= n.y.abs() && n.x.abs() >= n.z.abs() {
            0
        } else if n.y.abs() >= n.z.abs() {
            1
        } else {
            2
        };
        let mut dark = 0.0;
        for axis in 0..3 {
            if axis == own {
                continue;
            }
            let d = (p[axis] - min[axis]).abs().min((max[axis] - p[axis]).abs());
            dark += 0.4 * (-d / 2.0).exp();
        }
        (1.0 - dark).clamp(0.25, 1.0)
    }
}

fn splitmix(mut x: u64) -> u32 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (x ^ (x >> 31)) as u32
}

/// Accumulates faces and strokes into a [`MeshData`], handing out unique ids.
#[derive(Default)]
pub struct MeshBuilder {
    pub mesh: MeshData,
    /// Solids for the walking controller, emitted by the same element code as the faces.
    pub colliders: Vec<Collider>,
    next_fine: u32,
    next_coarse: u32,
}

impl MeshBuilder {
    pub fn new() -> Self {
        // Id 0 is reserved for "nothing" (sky, background), so ids start at 1.
        Self { mesh: MeshData::default(), colliders: Vec::new(), next_fine: 1, next_coarse: 1 }
    }

    pub fn finish(self) -> MeshData {
        self.mesh
    }

    /// A fresh coarse id (one per wall, column, stair flight, ...).
    pub fn new_coarse(&mut self) -> u32 {
        let id = self.next_coarse;
        self.next_coarse += 1;
        id
    }

    /// A fresh fine id (one per panel). Faces of one panel must be emitted consecutively.
    pub fn new_fine(&mut self) -> u32 {
        let id = self.next_fine;
        self.next_fine += 1;
        id
    }

    fn push_face(&mut self, corners: &[Vec3], normal: Vec3, hatch: Vec3, coarse: u32, ao: AoFn) {
        let fine = self.new_fine();
        // Wind counter-clockwise as seen from the side the normal points to.
        let geo = (corners[1] - corners[0]).cross(corners[corners.len() - 1] - corners[0]);
        let flip = geo.dot(normal) < 0.0;
        let base = self.mesh.vertices.len() as u32;
        let mut ordered: Vec<Vec3> = corners.to_vec();
        if flip {
            ordered[1..].reverse();
        }
        for p in &ordered {
            self.mesh.vertices.push(Vertex {
                position: p.to_array(),
                normal: normal.to_array(),
                hatch_axis: hatch.to_array(),
                id_fine: fine,
                id_coarse: coarse,
                ao: ao(*p, normal),
            });
        }
        for i in 1..ordered.len() as u32 - 1 {
            self.mesh.indices.extend_from_slice(&[base, base + i, base + i + 1]);
        }
    }

    /// One flat quad. `corners` go around the perimeter in either direction; winding is fixed
    /// up to match `normal`. `hatch` must lie in the face.
    pub fn quad(&mut self, corners: [Vec3; 4], normal: Vec3, hatch: Vec3, coarse: u32, ao: AoFn) {
        self.push_face(&corners, normal.normalize(), hatch.normalize(), coarse, ao);
    }

    /// One flat triangle.
    pub fn tri(&mut self, corners: [Vec3; 3], normal: Vec3, hatch: Vec3, coarse: u32, ao: AoFn) {
        self.push_face(&corners, normal.normalize(), hatch.normalize(), coarse, ao);
    }

    /// The parallelogram `origin + s*u + t*v` split into panels of at most `panel` metres,
    /// each its own quad and `id_fine`. Interior panel seams are emitted as seam strokes
    /// (rank 2); the outer boundary is left to the caller.
    #[allow(clippy::too_many_arguments)]
    pub fn panelled_quad(&mut self, origin: Vec3, u: Vec3, v: Vec3, normal: Vec3, panel: f32, coarse: u32, ao: AoFn) {
        let hatch = auto_hatch(normal.normalize(), u, v);
        self.panelled_quad_hatched(origin, u, v, normal, hatch, panel, coarse, ao);
    }

    /// [`MeshBuilder::panelled_quad`] with an explicit hatch axis (projected into the face).
    #[allow(clippy::too_many_arguments)]
    pub fn panelled_quad_hatched(&mut self, origin: Vec3, u: Vec3, v: Vec3, normal: Vec3, hatch: Vec3, panel: f32, coarse: u32, ao: AoFn) {
        let normal = normal.normalize();
        let nu = ((u.length() / panel).ceil() as usize).max(1);
        let nv = ((v.length() / panel).ceil() as usize).max(1);
        let hatch = (hatch - normal * normal.dot(hatch)).normalize();
        let at = |i: usize, j: usize| origin + u * (i as f32 / nu as f32) + v * (j as f32 / nv as f32);
        for i in 0..nu {
            for j in 0..nv {
                self.quad([at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)], normal, hatch, coarse, ao);
            }
        }
        for i in 1..nu {
            self.stroke(at(i, 0), at(i, nv), normal, normal, StrokeKind::Seam, 2);
        }
        for j in 1..nv {
            self.stroke(at(0, j), at(nu, j), normal, normal, StrokeKind::Seam, 2);
        }
    }

    /// An axis-aligned box with outward normals, faces panelled. Emits no strokes; use
    /// [`MeshBuilder::box_edges`] for those.
    pub fn panelled_box(&mut self, min: Vec3, max: Vec3, faces: FaceMask, panel: f32, coarse: u32, ao: AoFn) {
        let d = max - min;
        let (dx, dy, dz) = (Vec3::X * d.x, Vec3::Y * d.y, Vec3::Z * d.z);
        if faces.x_neg {
            self.panelled_quad(min, dz, dy, -Vec3::X, panel, coarse, ao);
        }
        if faces.x_pos {
            self.panelled_quad(min + dx, dz, dy, Vec3::X, panel, coarse, ao);
        }
        if faces.y_neg {
            self.panelled_quad(min, dx, dz, -Vec3::Y, panel, coarse, ao);
        }
        if faces.y_pos {
            self.panelled_quad(min + dy, dx, dz, Vec3::Y, panel, coarse, ao);
        }
        if faces.z_neg {
            self.panelled_quad(min, dx, dy, -Vec3::Z, panel, coarse, ao);
        }
        if faces.z_pos {
            self.panelled_quad(min + dz, dx, dy, Vec3::Z, panel, coarse, ao);
        }
    }

    /// Crease strokes along the edges of a box between each pair of emitted faces.
    /// `sign` is 1.0 for a solid box (outward normals) and -1.0 for a room (inward normals).
    pub fn box_edges(&mut self, min: Vec3, max: Vec3, faces: FaceMask, sign: f32, rank: u32) {
        let fm = [
            (faces.x_neg, 0usize, false),
            (faces.x_pos, 0, true),
            (faces.y_neg, 1, false),
            (faces.y_pos, 1, true),
            (faces.z_neg, 2, false),
            (faces.z_pos, 2, true),
        ];
        let normal = |axis: usize, pos: bool| {
            let mut n = Vec3::ZERO;
            n[axis] = if pos { 1.0 } else { -1.0 } * sign;
            n
        };
        let plane = |axis: usize, pos: bool| if pos { max[axis] } else { min[axis] };
        for (fa, &(on_a, ax_a, pos_a)) in fm.iter().enumerate() {
            for &(on_b, ax_b, pos_b) in fm.iter().skip(fa + 1) {
                if !on_a || !on_b || ax_a == ax_b {
                    continue;
                }
                let axis_free = 3 - ax_a - ax_b;
                let mut a = Vec3::ZERO;
                a[ax_a] = plane(ax_a, pos_a);
                a[ax_b] = plane(ax_b, pos_b);
                let mut b = a;
                a[axis_free] = min[axis_free];
                b[axis_free] = max[axis_free];
                self.stroke(a, b, normal(ax_a, pos_a), normal(ax_b, pos_b), StrokeKind::Crease, rank);
            }
        }
    }

    /// Emit one ink stroke. Weight and seed are derived from rank and stroke index.
    pub fn stroke(&mut self, a: Vec3, b: Vec3, n0: Vec3, n1: Vec3, kind: StrokeKind, rank: u32) {
        let weight = match rank {
            0 => 1.6,
            1 => 1.0,
            2 => 0.6,
            _ => 0.4,
        };
        let seed = splitmix(self.mesh.strokes.len() as u64 ^ ((kind as u64) << 40));
        self.mesh.strokes.push(Stroke {
            a: a.to_array(),
            b: b.to_array(),
            n0: n0.normalize().to_array(),
            n1: n1.normalize().to_array(),
            kind: kind as u32,
            rank,
            weight,
            seed,
            instance: 0,
        });
    }

    pub fn crease(&mut self, a: Vec3, b: Vec3, n0: Vec3, n1: Vec3) {
        self.stroke(a, b, n0, n1, StrokeKind::Crease, 1);
    }

    pub fn seam(&mut self, a: Vec3, b: Vec3, n: Vec3) {
        self.stroke(a, b, n, n, StrokeKind::Seam, 2);
    }
}

/// Grid lines from 0 to `len`: every multiple of `step` plus the given breaks, sorted, with
/// near-duplicates (closer than 1 mm) merged. Always starts at 0 and ends at `len`.
pub fn grid_lines(len: f32, step: f32, breaks: &[f32]) -> Vec<f32> {
    let mut v: Vec<f32> = Vec::new();
    if step.is_finite() && step > 0.0 {
        let n = (len / step).floor() as usize;
        for k in 0..=n {
            v.push(k as f32 * step);
        }
    } else {
        v.push(0.0);
    }
    v.push(len);
    v.extend(breaks.iter().copied().filter(|&b| b > 0.0 && b < len));
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut out: Vec<f32> = Vec::with_capacity(v.len());
    for x in v {
        match out.last() {
            Some(&l) if x - l < 1e-3 => {
                // Keep the exact end value.
                if (x - len).abs() < 1e-6 {
                    *out.last_mut().unwrap() = len;
                }
            }
            _ => out.push(x),
        }
    }
    out
}

impl MeshBuilder {
    /// Begin the faces of one panel: a fresh fine id.
    pub fn panel(&mut self) -> u32 {
        self.new_fine()
    }

    /// Emit the rectangle `origin + s*u + t*v` (s, t in 0..1) as a grid of shared-vertex
    /// quads cut at the given grid lines (metres along `u` and `v`, as from [`grid_lines`]),
    /// with fine id `fine`. AO is set to 1; call [`MeshBuilder::bake_ao`] once the scene is
    /// complete. `hatch` is projected into the face. The caller must emit all faces of one
    /// fine id consecutively.
    #[allow(clippy::too_many_arguments)]
    pub fn grid_face(&mut self, origin: Vec3, u_dir: Vec3, v_dir: Vec3, us: &[f32], vs: &[f32], normal: Vec3, hatch: Vec3, fine: u32, coarse: u32) {
        let normal = normal.normalize();
        let hatch = (hatch - normal * normal.dot(hatch)).normalize();
        let base = self.mesh.vertices.len() as u32;
        let nu = us.len();
        for &t in vs {
            for &s in us {
                let p = origin + u_dir * s + v_dir * t;
                self.mesh.vertices.push(Vertex {
                    position: p.to_array(),
                    normal: normal.to_array(),
                    hatch_axis: hatch.to_array(),
                    id_fine: fine,
                    id_coarse: coarse,
                    ao: 1.0,
                });
            }
        }
        let flip = u_dir.cross(v_dir).dot(normal) < 0.0;
        for j in 0..vs.len() - 1 {
            for i in 0..nu - 1 {
                let a = base + (j * nu + i) as u32;
                let b = a + 1;
                let c = a + nu as u32 + 1;
                let d = a + nu as u32;
                if flip {
                    self.mesh.indices.extend_from_slice(&[a, c, b, a, d, c]);
                } else {
                    self.mesh.indices.extend_from_slice(&[a, b, c, a, c, d]);
                }
            }
        }
    }

    /// A planar rectangle `origin + s*u + t*v` tessellated into cells of at most `step`
    /// metres, as one panel with its own fine id. Returns the fine id.
    #[allow(clippy::too_many_arguments)]
    pub fn rect(&mut self, origin: Vec3, u: Vec3, v: Vec3, normal: Vec3, hatch: Vec3, step: f32, coarse: u32) -> u32 {
        let (lu, lv) = (u.length(), v.length());
        let fine = self.panel();
        let us = grid_lines(lu, step.min(lu), &[]);
        let vs = grid_lines(lv, step.min(lv), &[]);
        self.grid_face(origin, u / lu, v / lv, &us, &vs, normal, hatch, fine, coarse);
        fine
    }

    /// A planar polygon fan (convex, given in order) with its own fine id and ao 1. For
    /// small shapes that need no tessellation (step sides, arch spandrel slivers).
    pub fn polygon(&mut self, corners: &[Vec3], normal: Vec3, hatch: Vec3, fine: u32, coarse: u32) {
        let normal = normal.normalize();
        let hatch = (hatch - normal * normal.dot(hatch)).normalize();
        let geo = (corners[1] - corners[0]).cross(corners[corners.len() - 1] - corners[0]);
        let flip = geo.dot(normal) < 0.0;
        let base = self.mesh.vertices.len() as u32;
        let mut ordered: Vec<Vec3> = corners.to_vec();
        if flip {
            ordered[1..].reverse();
        }
        for p in &ordered {
            self.mesh.vertices.push(Vertex {
                position: p.to_array(),
                normal: normal.to_array(),
                hatch_axis: hatch.to_array(),
                id_fine: fine,
                id_coarse: coarse,
                ao: 1.0,
            });
        }
        for i in 1..ordered.len() as u32 - 1 {
            self.mesh.indices.extend_from_slice(&[base, base + i, base + i + 1]);
        }
    }

    /// A curved panel with smooth shading: an `nu` by `nv` grid of vertices from
    /// `at(i, j) = (position, normal, hatch)`, two triangles per cell, all with fine id `fine`.
    /// Each cell is wound to face its first corner's normal. AO is set to 1.
    pub fn smooth_grid(&mut self, nu: usize, nv: usize, at: &dyn Fn(usize, usize) -> (Vec3, Vec3, Vec3), fine: u32, coarse: u32) {
        let base = self.mesh.vertices.len() as u32;
        for j in 0..nv {
            for i in 0..nu {
                let (p, n, h) = at(i, j);
                let n = n.normalize();
                let h = (h - n * n.dot(h)).normalize();
                self.mesh.vertices.push(Vertex { position: p.to_array(), normal: n.to_array(), hatch_axis: h.to_array(), id_fine: fine, id_coarse: coarse, ao: 1.0 });
            }
        }
        let pos = |k: u32, v: &Vec<Vertex>| Vec3::from(v[k as usize].position);
        for j in 0..nv - 1 {
            for i in 0..nu - 1 {
                let a = base + (j * nu + i) as u32;
                let bb = a + 1;
                let c = a + nu as u32 + 1;
                let d = a + nu as u32;
                let g = (pos(bb, &self.mesh.vertices) - pos(a, &self.mesh.vertices)).cross(pos(d, &self.mesh.vertices) - pos(a, &self.mesh.vertices));
                let n = Vec3::from(self.mesh.vertices[a as usize].normal);
                if g.dot(n) >= 0.0 {
                    self.mesh.indices.extend_from_slice(&[a, bb, c, a, c, d]);
                } else {
                    self.mesh.indices.extend_from_slice(&[a, c, bb, a, d, c]);
                }
            }
        }
    }

    pub fn collider(&mut self, c: Collider) {
        self.colliders.push(c);
    }

    /// Replace every vertex's ao by `f(position, normal)`, clamped to 0..1. Runs on all
    /// cores; the result does not depend on the thread count.
    pub fn bake_ao(&mut self, f: &(dyn Fn(Vec3, Vec3) -> f32 + Sync)) {
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
        let verts = &mut self.mesh.vertices;
        let chunk = verts.len().div_ceil(threads).max(1);
        std::thread::scope(|s| {
            for part in verts.chunks_mut(chunk) {
                s.spawn(move || {
                    for v in part {
                        v.ao = f(Vec3::from(v.position), Vec3::from(v.normal)).clamp(0.0, 1.0);
                    }
                });
            }
        });
    }

    /// The finished mesh and collider list.
    pub fn finish_with_colliders(self) -> (MeshData, Vec<Collider>) {
        (self.mesh, self.colliders)
    }
}
