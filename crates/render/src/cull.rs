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
    /// The scene's space id ([`ANY_SPACE`] for grid chunks, which portal culling never drops).
    pub space: u32,
}

/// Space id of a grid chunk: visible whatever the portal flood finds.
pub const ANY_SPACE: u32 = u32::MAX - 1;

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
        chunks.push(Chunk { start, count: indices.len() as u32 - start, lo, hi, space: ANY_SPACE });
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
        chunks.push(Chunk { start, count: out.len() as u32 - start, lo, hi, space: ANY_SPACE });
    }
    (out, chunks)
}

/// The scene's own chunks (per space, from the generator), as index and stroke chunks.
pub fn scene_chunks(mesh: &MeshData) -> (Vec<Chunk>, Vec<Chunk>) {
    let tri = mesh.chunks.iter().map(|c| Chunk { start: c.index_start, count: c.index_count, lo: Vec3::from(c.min), hi: Vec3::from(c.max), space: c.space }).collect();
    let st = mesh.chunks.iter().map(|c| Chunk { start: c.stroke_start, count: c.stroke_count, lo: Vec3::from(c.min), hi: Vec3::from(c.max), space: c.space }).collect();
    (tri, st)
}

/// Portal visibility (E072): spaces, the portals between them, and boxes to locate the
/// camera. Built once per scene.
#[derive(Default)]
pub struct Portals {
    portals: Vec<mega_gen::ScenePortal>,
    by_space: HashMap<u32, Vec<usize>>,
    boxes: Vec<mega_gen::SpaceBox>,
}

/// A screen rectangle in NDC: x0, y0, x1, y1.
type Rect = [f32; 4];

const FULL: Rect = [-1.0, -1.0, 1.0, 1.0];

fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let r = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
    (r[0] < r[2] && r[1] < r[3]).then_some(r)
}

fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && a[1] <= b[1] && a[2] >= b[2] && a[3] >= b[3]
}

impl Portals {
    pub fn new(portals: &[mega_gen::ScenePortal], boxes: &[mega_gen::SpaceBox]) -> Self {
        let mut by_space: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, p) in portals.iter().enumerate() {
            by_space.entry(p.a).or_default().push(i);
            by_space.entry(p.b).or_default().push(i);
        }
        Self { portals: portals.to_vec(), by_space, boxes: boxes.to_vec() }
    }

    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty() || self.portals.is_empty()
    }

    /// The spaces whose boxes contain `eye`, each box widened by `pad` metres (so a camera
    /// straddling a doorway counts as in both spaces).
    pub fn spaces_at(&self, eye: Vec3, pad: f32) -> Vec<u32> {
        let mut out: Vec<u32> = self
            .boxes
            .iter()
            .filter(|b| (0..3).all(|i| eye[i] >= b.min[i] - pad && eye[i] <= b.max[i] + pad))
            .map(|b| b.space)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The portal's screen rectangle, clipped to the screen; the whole screen if any corner is
    /// at or behind the near plane (conservative), None if it is entirely behind.
    fn rect(view_proj: Mat4, q: &[[f32; 3]; 4]) -> Option<Rect> {
        let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        let mut behind = 0;
        let mut near = false;
        for c in q {
            let p = view_proj * Vec3::from(*c).extend(1.0);
            if p.w <= 0.0 {
                behind += 1;
                near = true;
                continue;
            }
            if p.w < 0.2 {
                near = true;
            }
            let (x, y) = (p.x / p.w, p.y / p.w);
            r = [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)];
        }
        if behind == 4 {
            return None;
        }
        if near {
            return Some(FULL);
        }
        intersect(r, FULL)
    }

    /// Spaces visible from `eye` through portals inside the camera frustum: a flood from the
    /// camera's spaces, each portal narrowing the screen rectangle seen through it. None when
    /// the camera is in no space (flying outside, inside a wall): then nothing is culled by
    /// portals.
    pub fn visible(&self, view_proj: Mat4, eye: Vec3) -> Option<HashMap<u32, Rect>> {
        let start = self.spaces_at(eye, 0.3);
        if start.is_empty() {
            return None;
        }
        let mut seen: HashMap<u32, Rect> = HashMap::new();
        let mut queue = std::collections::VecDeque::new();
        for s in start {
            seen.insert(s, FULL);
            queue.push_back(s);
        }
        let mut steps = 0;
        while let Some(s) = queue.pop_front() {
            steps += 1;
            if steps > 20000 {
                break;
            }
            let rs = seen[&s];
            let Some(list) = self.by_space.get(&s) else { continue };
            for &i in list {
                let p = &self.portals[i];
                let other = if p.a == s { p.b } else { p.a };
                let Some(pr) = Self::rect(view_proj, &p.quad) else { continue };
                let Some(r) = intersect(rs, pr) else { continue };
                match seen.get_mut(&other) {
                    Some(o) if contains(*o, r) => {}
                    Some(o) => {
                        *o = [o[0].min(r[0]), o[1].min(r[1]), o[2].max(r[2]), o[3].max(r[3])];
                        queue.push_back(other);
                    }
                    None => {
                        seen.insert(other, r);
                        queue.push_back(other);
                    }
                }
            }
        }
        Some(seen)
    }
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
        let c = |s, n| Chunk { start: s, count: n, lo: Vec3::ZERO, hi: Vec3::ZERO, space: ANY_SPACE };
        let ch = [c(0, 3), c(3, 6), c(9, 3), c(12, 3)];
        let mut k = 0;
        let runs = visible_runs(&ch, |_| {
            k += 1;
            k != 3
        });
        assert_eq!(runs, vec![(0, 9), (12, 3)]);
    }
}
