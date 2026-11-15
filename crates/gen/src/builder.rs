//! Helpers shared by the test scene and (later) the real generator.

use crate::mesh::{MeshData, Stroke, StrokeKind, Vertex};
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
            dark += 0.3 * (-d / 1.5).exp();
        }
        (1.0 - dark).clamp(0.35, 1.0)
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
    next_fine: u32,
    next_coarse: u32,
}

impl MeshBuilder {
    pub fn new() -> Self {
        // Id 0 is reserved for "nothing" (sky, background), so ids start at 1.
        Self { mesh: MeshData::default(), next_fine: 1, next_coarse: 1 }
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

    fn new_fine(&mut self) -> u32 {
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
        let normal = normal.normalize();
        let nu = ((u.length() / panel).ceil() as usize).max(1);
        let nv = ((v.length() / panel).ceil() as usize).max(1);
        let hatch = auto_hatch(normal, u, v);
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
