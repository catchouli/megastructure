//! Chunked culling (E069): the scene's triangles and strokes are grouped at upload into
//! spatial chunks (grid cells), each a contiguous range of the index or stroke buffer with
//! an axis-aligned bound. Each frame the chunks are tested against the camera frustum (main
//! passes, stroke cull) and against each shadow cascade's footprint (shadow passes); only
//! whole chunks are dropped, by conservative tests, so the picture is unchanged.
//!
//! The grouping is by triangle centroid on a uniform grid. A generator that knows its spaces
//! could supply better chunks (one per space, with the portal graph for occlusion); see
//! "Culling" in docs/features/ink-renderer.md.

use glam::{Mat4, Vec3, Vec4};
use mega_gen::{MeshData, Stroke};
use std::collections::HashMap;

/// Grid cell size in metres.
pub const CHUNK_CELL: f32 = 24.0;

/// A contiguous range of the index (or stroke) buffer and its bound.
#[derive(Clone, Copy, Debug)]
pub struct Chunk {
    pub start: u32,
    pub count: u32,
    pub lo: Vec3,
    pub hi: Vec3,
}

fn cell_of(p: Vec3) -> (i32, i32, i32) {
    ((p.x / CHUNK_CELL).floor() as i32, (p.y / CHUNK_CELL).floor() as i32, (p.z / CHUNK_CELL).floor() as i32)
}

/// The mesh's indices regrouped by chunk, and the chunks.
pub fn chunk_triangles(mesh: &MeshData) -> (Vec<u32>, Vec<Chunk>) {
    let mut groups: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
    for (t, tri) in mesh.indices.chunks_exact(3).enumerate() {
        let c = tri.iter().map(|&i| Vec3::from(mesh.vertices[i as usize].position)).fold(Vec3::ZERO, |a, b| a + b) / 3.0;
        groups.entry(cell_of(c)).or_default().push(t);
    }
    let mut keys: Vec<_> = groups.keys().copied().collect();
    keys.sort();
    let mut indices = Vec::with_capacity(mesh.indices.len());
    let mut chunks = Vec::with_capacity(keys.len());
    for k in keys {
        let start = indices.len() as u32;
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for &t in &groups[&k] {
            for &i in &mesh.indices[3 * t..3 * t + 3] {
                let p = Vec3::from(mesh.vertices[i as usize].position);
                lo = lo.min(p);
                hi = hi.max(p);
                indices.push(i);
            }
        }
        chunks.push(Chunk { start, count: indices.len() as u32 - start, lo, hi });
    }
    (indices, chunks)
}

/// The strokes regrouped by chunk (by midpoint), and the chunks.
pub fn chunk_strokes(strokes: &[Stroke]) -> (Vec<Stroke>, Vec<Chunk>) {
    let mut groups: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
    for (i, s) in strokes.iter().enumerate() {
        groups.entry(cell_of((Vec3::from(s.a) + Vec3::from(s.b)) * 0.5)).or_default().push(i);
    }
    let mut keys: Vec<_> = groups.keys().copied().collect();
    keys.sort();
    let mut out = Vec::with_capacity(strokes.len());
    let mut chunks = Vec::with_capacity(keys.len());
    for k in keys {
        let start = out.len() as u32;
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for &i in &groups[&k] {
            let s = strokes[i];
            for p in [Vec3::from(s.a), Vec3::from(s.b)] {
                lo = lo.min(p);
                hi = hi.max(p);
            }
            out.push(s);
        }
        chunks.push(Chunk { start, count: out.len() as u32 - start, lo, hi });
    }
    (out, chunks)
}

fn corners(lo: Vec3, hi: Vec3) -> [Vec4; 8] {
    let mut c = [Vec4::ZERO; 8];
    for (i, v) in c.iter_mut().enumerate() {
        *v = Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z }).extend(1.0);
    }
    c
}

/// The planes of a view-projection's frustum (reversed-Z, infinite far): left, right,
/// bottom, top, near, each as `dot(plane, p) >= 0` inside, widened by `margin` in x and y.
pub fn frustum_planes(view_proj: Mat4, margin: f32) -> [Vec4; 5] {
    let m = view_proj.transpose();
    let (r0, r1, r2, r3) = (m.x_axis, m.y_axis, m.z_axis, m.w_axis);
    [r3 * margin + r0, r3 * margin - r0, r3 * margin + r1, r3 * margin - r1, r3 - r2]
}

/// Whether the box can be inside the frustum (false only if all corners lie outside one plane).
pub fn box_in_planes(planes: &[Vec4], lo: Vec3, hi: Vec3) -> bool {
    let c = corners(lo, hi);
    !planes.iter().any(|pl| c.iter().all(|p| pl.dot(*p) < 0.0))
}

/// Whether the box can cast into a shadow cascade: tested in the cascade's x and y only, so
/// casters anywhere along the light are kept.
pub fn box_in_cascade(m: Mat4, lo: Vec3, hi: Vec3) -> bool {
    let r = m.transpose();
    let (r0, r1, r3) = (r.x_axis, r.y_axis, r.w_axis);
    // Small margin for the filter footprint and the jittered lookups.
    let k = 1.05;
    let planes = [r3 * k + r0, r3 * k - r0, r3 * k + r1, r3 * k - r1];
    box_in_planes(&planes, lo, hi)
}

/// Distance from `p` to the box (0 inside).
pub fn box_distance(p: Vec3, lo: Vec3, hi: Vec3) -> f32 {
    (lo - p).max(p - hi).max(Vec3::ZERO).length()
}

/// The chunks passing `test`, as merged index ranges `(start, count)`.
pub fn visible_runs(chunks: &[Chunk], mut test: impl FnMut(&Chunk) -> bool) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for c in chunks {
        if c.count == 0 || !test(c) {
            continue;
        }
        match runs.last_mut() {
            Some(r) if r.0 + r.1 == c.start => r.1 += c.count,
            _ => runs.push((c.start, c.count)),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Camera;

    #[test]
    fn chunks_cover_every_triangle_and_stroke_once() {
        let s = mega_gen::scenes::by_name("test").unwrap();
        let (idx, ch) = chunk_triangles(&s.mesh);
        assert_eq!(idx.len(), s.mesh.indices.len());
        assert_eq!(ch.iter().map(|c| c.count as usize).sum::<usize>(), idx.len());
        let mut a: Vec<[u32; 3]> = s.mesh.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        let mut b: Vec<[u32; 3]> = idx.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        let (st, sc) = chunk_strokes(&s.mesh.strokes);
        assert_eq!(st.len(), s.mesh.strokes.len());
        assert_eq!(sc.iter().map(|c| c.count as usize).sum::<usize>(), st.len());
    }

    #[test]
    fn frustum_keeps_what_is_ahead_and_drops_what_is_behind() {
        let cam = Camera::new(Vec3::ZERO, 0.0, 0.0);
        let pl = frustum_planes(cam.view_proj(1.0), 1.0);
        assert!(box_in_planes(&pl, Vec3::new(-1.0, -1.0, 9.0), Vec3::new(1.0, 1.0, 11.0)));
        assert!(!box_in_planes(&pl, Vec3::new(-1.0, -1.0, -11.0), Vec3::new(1.0, 1.0, -9.0)));
        // A box spanning the eye is kept.
        assert!(box_in_planes(&pl, Vec3::splat(-1.0), Vec3::splat(1.0)));
    }

    #[test]
    fn runs_merge_adjacent_chunks() {
        let c = |s, n| Chunk { start: s, count: n, lo: Vec3::ZERO, hi: Vec3::ZERO };
        let ch = [c(0, 3), c(3, 6), c(9, 3), c(12, 3)];
        let mut k = 0;
        let runs = visible_runs(&ch, |_| {
            k += 1;
            k != 3
        });
        assert_eq!(runs, vec![(0, 9), (12, 3)]);
    }
}
