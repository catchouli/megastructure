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
