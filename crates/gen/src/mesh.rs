use bytemuck::{Pod, Zeroable};

/// One vertex of a flat-shaded face. Vertices are never shared between faces, so every face
/// carries its own normal, hatch axis and ids.
///
/// Layout is 48 bytes with no padding; the vertex buffer layout in `mega_render` mirrors it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// Unit vector lying in the face, along its long or vertical direction. Hatching runs
    /// along this axis.
    pub hatch_axis: [f32; 3],
    /// Unique per panel.
    pub id_fine: u32,
    /// Unique per larger part (a wall, a column, a stair flight).
    pub id_coarse: u32,
    /// Baked ambient occlusion, 0 (dark) to 1 (open).
    pub ao: f32,
}

/// Kind of ink line.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeKind {
    Crease = 0,
    Seam = 1,
    Crack = 2,
    Border = 3,
    White = 4,
}

/// An ink line segment with the normals of its two adjacent faces.
///
/// 68 bytes, all 4-byte fields. In WGSL declare it with scalar members or `array<f32, 3>`
/// (not `vec3<f32>`, which would be 16-byte aligned) so the array stride is 68.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Stroke {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub n0: [f32; 3],
    pub n1: [f32; 3],
    /// A [`StrokeKind`] as u32.
    pub kind: u32,
    /// 0 is most important, 3 least.
    pub rank: u32,
    /// Base weight, relative.
    pub weight: f32,
    pub seed: u32,
    /// 0 for static geometry; reserved for instanced parts.
    pub instance: u32,
}

#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub strokes: Vec<Stroke>,
    /// Groups of triangles and strokes for culling, each one contiguous range of `indices`
    /// and of `strokes`, covering both completely and in order. Empty means the whole mesh
    /// is one chunk.
    pub chunks: Vec<Chunk>,
}

/// Space id of geometry outside every space (the outer skin of a region).
pub const OUTSIDE: u32 = u32::MAX;

/// A contiguous group of triangles and strokes belonging to one space (or one height band of
/// a tall space), for culling.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Chunk {
    /// Stable id of the space (the scene's own numbering; [`OUTSIDE`] for the outer skin).
    /// Several chunks may share one space (height bands of a tall void).
    pub space: u32,
    /// Bounds of the chunk's vertices and stroke ends.
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Range of `MeshData::indices` (a multiple of 3).
    pub index_start: u32,
    pub index_count: u32,
    /// Range of `MeshData::strokes`.
    pub stroke_start: u32,
    pub stroke_count: u32,
}

/// A convex box covering part of a space's interior, so the renderer can find which space the
/// camera is in. A space may have several (an L-shaped passage, a stair, a doorway).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpaceBox {
    pub space: u32,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// An opening between two spaces, for portal culling: the space ids either side and the
/// opening's quad (four corners in order round it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScenePortal {
    pub a: u32,
    pub b: u32,
    pub quad: [[f32; 3]; 4],
}

impl MeshData {
    /// Axis-aligned bounds of the vertices, or None if empty.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let first = self.vertices.first()?;
        let mut lo = first.position;
        let mut hi = first.position;
        for v in &self.vertices {
            for i in 0..3 {
                lo[i] = lo[i].min(v.position[i]);
                hi[i] = hi[i].max(v.position[i]);
            }
        }
        Some((lo, hi))
    }
}

impl MeshData {
    /// A 64-bit FNV-1a hash of every vertex, index and stroke, bit for bit. Two builds of the
    /// same scene and seed must give the same value.
    pub fn content_hash(&self) -> u64 {
        let mut h = Fnv::new();
        h.bytes(bytemuck::cast_slice(&self.vertices));
        h.bytes(bytemuck::cast_slice(&self.indices));
        h.bytes(bytemuck::cast_slice(&self.strokes));
        h.0
    }

    /// Check the invariants the renderer relies on; returns the first violation.
    ///
    /// - indices in range, a whole number of triangles;
    /// - unit normals and hatch axes, the hatch axis lying in the face, ao in 0..1;
    /// - every triangle wound counter-clockwise about its vertices' normal and not degenerate;
    /// - id 0 never used; all vertices of one `id_fine` share one `id_coarse` and one
    ///   contiguous run of the vertex buffer, and every triangle lies in one `id_fine`;
    /// - strokes have unit normals, rank 0..3, a valid kind and nonzero length.
    pub fn check_invariants(&self) -> Result<(), String> {
        use glam::Vec3;
        use std::collections::HashMap;
        if !self.indices.len().is_multiple_of(3) {
            return Err(format!("{} indices is not a whole number of triangles", self.indices.len()));
        }
        if let Some(&i) = self.indices.iter().find(|&&i| i as usize >= self.vertices.len()) {
            return Err(format!("index {i} out of range ({} vertices)", self.vertices.len()));
        }
        for (i, v) in self.vertices.iter().enumerate() {
            let n = Vec3::from(v.normal);
            let h = Vec3::from(v.hatch_axis);
            if !n.is_finite() || (n.length() - 1.0).abs() > 1e-4 {
                return Err(format!("vertex {i}: normal {n:?} not unit"));
            }
            if !h.is_finite() || (h.length() - 1.0).abs() > 1e-4 {
                return Err(format!("vertex {i}: hatch axis {h:?} not unit"));
            }
            if n.dot(h).abs() > 1e-3 {
                return Err(format!("vertex {i}: hatch axis {h:?} not in the face (normal {n:?})"));
            }
            if !(0.0..=1.0).contains(&v.ao) {
                return Err(format!("vertex {i}: ao {} outside 0..1", v.ao));
            }
            if !Vec3::from(v.position).is_finite() {
                return Err(format!("vertex {i}: position not finite"));
            }
            if v.id_fine == 0 || v.id_coarse == 0 {
                return Err(format!("vertex {i}: id 0 is reserved for background"));
            }
        }
        for t in self.indices.chunks(3) {
            let vs: Vec<&Vertex> = t.iter().map(|&i| &self.vertices[i as usize]).collect();
            let p: Vec<Vec3> = vs.iter().map(|v| Vec3::from(v.position)).collect();
            let n = Vec3::from(vs[0].normal);
            let g = (p[1] - p[0]).cross(p[2] - p[0]);
            if g.length() < 1e-9 {
                return Err(format!("degenerate triangle {p:?}"));
            }
            if g.dot(n) <= 0.0 {
                return Err(format!("triangle {p:?} wound against normal {n:?}"));
            }
            if vs.iter().any(|v| v.id_fine != vs[0].id_fine) {
                return Err(format!("triangle {p:?} spans fine ids"));
            }
        }
        // Fine ids: one coarse id each, contiguous in the buffer.
        let mut runs: HashMap<u32, (usize, usize, u32)> = HashMap::new();
        for (i, v) in self.vertices.iter().enumerate() {
            let e = runs.entry(v.id_fine).or_insert((i, i, v.id_coarse));
            if e.2 != v.id_coarse {
                return Err(format!("fine id {} appears with coarse ids {} and {}", v.id_fine, e.2, v.id_coarse));
            }
            if e.1 + 1 < i {
                return Err(format!("fine id {} is not one contiguous run (vertices {}..{} then {i})", v.id_fine, e.0, e.1));
            }
            e.1 = i;
        }
        for (k, s) in self.strokes.iter().enumerate() {
            let (n0, n1) = (Vec3::from(s.n0), Vec3::from(s.n1));
            if (n0.length() - 1.0).abs() > 1e-4 || (n1.length() - 1.0).abs() > 1e-4 {
                return Err(format!("stroke {k}: normals not unit"));
            }
            if s.rank > 3 || s.kind > 4 {
                return Err(format!("stroke {k}: rank {} kind {}", s.rank, s.kind));
            }
            let (a, b) = (Vec3::from(s.a), Vec3::from(s.b));
            if !a.is_finite() || !b.is_finite() || (a - b).length() < 1e-4 {
                return Err(format!("stroke {k}: degenerate {a:?} {b:?}"));
            }
        }
        Ok(())
    }
}

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 ^= x as u64;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

/// A solid for the walking controller. Positions in metres, Y up. Nothing in this crate
/// consumes colliders; they are emitted alongside the triangles by the same element code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Collider {
    /// Axis-aligned box.
    Aabb { min: [f32; 3], max: [f32; 3] },
    /// Oriented box: `centre + axes[i] * s * half[i]` for s in -1..1. Axes are orthonormal.
    Obb { centre: [f32; 3], axes: [[f32; 3]; 3], half: [f32; 3] },
    /// A stair flight's walking surface: a sloped slab whose top surface runs from the
    /// centre of the bottom edge `start` (on the lower floor, at the foot of the first riser)
    /// to the centre of the top edge `end` (on the upper floor, at the top of the last
    /// riser), `half_width` either side, `thickness` deep below the surface. Tread tops lie
    /// between the surface and one rise above it.
    Ramp { start: [f32; 3], end: [f32; 3], half_width: f32, thickness: f32 },
}

impl Collider {
    /// The collider as an oriented box (axis-aligned boxes get identity axes). For a ramp,
    /// axis 0 runs up the slope, axis 1 is the surface normal, axis 2 across.
    pub fn to_obb(&self) -> ([f32; 3], [[f32; 3]; 3], [f32; 3]) {
        use glam::Vec3;
        match *self {
            Collider::Aabb { min, max } => {
                let (lo, hi) = (Vec3::from(min), Vec3::from(max));
                (((lo + hi) * 0.5).to_array(), [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], ((hi - lo) * 0.5).to_array())
            }
            Collider::Obb { centre, axes, half } => (centre, axes, half),
            Collider::Ramp { start, end, half_width, thickness } => {
                let (s, e) = (Vec3::from(start), Vec3::from(end));
                let along = (e - s).normalize();
                let across = along.cross(Vec3::Y).normalize();
                let up = across.cross(along).normalize();
                let centre = (s + e) * 0.5 - up * (thickness * 0.5);
                (centre.to_array(), [along.to_array(), up.to_array(), across.to_array()], [(e - s).length() * 0.5, thickness * 0.5, half_width])
            }
        }
    }

    /// Axis-aligned bounds.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        use glam::Vec3;
        if let Collider::Aabb { min, max } = *self {
            return (min, max);
        }
        let (c, axes, half) = self.to_obb();
        let mut ext = Vec3::ZERO;
        for i in 0..3 {
            ext += Vec3::from(axes[i]).abs() * half[i];
        }
        let c = Vec3::from(c);
        ((c - ext).to_array(), (c + ext).to_array())
    }

    /// Whether the point lies inside (or on) the solid.
    pub fn contains(&self, p: [f32; 3]) -> bool {
        use glam::Vec3;
        let (c, axes, half) = self.to_obb();
        let d = Vec3::from(p) - Vec3::from(c);
        (0..3).all(|i| d.dot(Vec3::from(axes[i])).abs() <= half[i] + 1e-4)
    }
}
