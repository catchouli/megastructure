//! Hand-built corridor and hall, in metres, Y up.
//!
//! The corridor (4 wide, 5 high) runs along +Z from z=0 to z=110, then steps out into a
//! 10 m deep arched reveal (8 wide, 16 high) that ends in the hall wall at z=120. The hall is
//! 60 wide (x -30..30), 40 high and 150 long (z 120..270), with two rows of square columns, a
//! straight stair climbing the x=-30 wall to a landing, and a raised platform.

use crate::builder::{box_ao, FaceMask, MeshBuilder};
use crate::mesh::{MeshData, StrokeKind};
use glam::Vec3;

pub const CORRIDOR_HALF_WIDTH: f32 = 2.0;
pub const CORRIDOR_HEIGHT: f32 = 5.0;
pub const CORRIDOR_END_Z: f32 = 110.0;
pub const HALL_MIN: Vec3 = Vec3::new(-30.0, 0.0, 120.0);
pub const HALL_MAX: Vec3 = Vec3::new(30.0, 40.0, 270.0);
pub const ARCH_HALF_WIDTH: f32 = 4.0;
pub const ARCH_SPRING_Y: f32 = 12.0;
pub const STAIR_RISE: f32 = 0.17;
pub const STAIR_RUN: f32 = 0.30;
pub const STAIR_STEPS: usize = 72;
pub const ARCH_SEGMENTS: usize = 12;

const X: Vec3 = Vec3::X;
const Y: Vec3 = Vec3::Y;
const Z: Vec3 = Vec3::Z;

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// Point on the arch at index `i` of `ARCH_SEGMENTS`, going from the left springer (i=0)
/// over the crown to the right springer, on the plane z.
fn arch_pt(i: usize, z: f32) -> Vec3 {
    let th = std::f32::consts::PI * (1.0 - i as f32 / ARCH_SEGMENTS as f32);
    v(ARCH_HALF_WIDTH * th.cos(), ARCH_SPRING_Y + ARCH_HALF_WIDTH * th.sin(), z)
}

pub fn build() -> MeshData {
    let mut b = MeshBuilder::new();
    corridor(&mut b);
    reveal(&mut b);
    hall_shell(&mut b);
    columns(&mut b);
    platform(&mut b);
    stair(&mut b);
    b.finish()
}

fn corridor(b: &mut MeshBuilder) {
    let (min, max) = (v(-CORRIDOR_HALF_WIDTH, 0.0, 0.0), v(CORRIDOR_HALF_WIDTH, CORRIDOR_HEIGHT, CORRIDOR_END_Z));
    let ao = box_ao(min, max);
    let len = Z * CORRIDOR_END_Z;
    let w = X * (2.0 * CORRIDOR_HALF_WIDTH);
    let h = Y * CORRIDOR_HEIGHT;
    let c = b.new_coarse();
    b.panelled_quad(min, len, w, Y, 3.0, c, &ao); // floor
    let c = b.new_coarse();
    b.panelled_quad(min + h, len, w, -Y, 3.0, c, &ao); // ceiling
    let c = b.new_coarse();
    b.panelled_quad(min, len, h, X, 3.0, c, &ao); // left wall
    let c = b.new_coarse();
    b.panelled_quad(max - len, len, -h, -X, 3.0, c, &ao); // right wall (origin at top-right start)
    let c = b.new_coarse();
    b.panelled_quad(min, w, h, Z, 3.0, c, &ao); // end cap at z=0
    // Creases: the four long edges and the cap outline. Room normals point inward.
    let mut faces = FaceMask::ALL;
    faces.z_pos = false; // the far end is the step face, handled in `reveal`
    b.box_edges(min, max, faces, -1.0, 1);
}

/// The 10 m deep arched reveal between the corridor and the hall wall.
fn reveal(b: &mut MeshBuilder) {
    let (z0, z1) = (CORRIDOR_END_Z, HALL_MIN.z);
    let hw = ARCH_HALF_WIDTH;
    let ao = box_ao(v(-hw, 0.0, z0), v(hw, ARCH_SPRING_Y + hw, z1));
    let depth = Z * (z1 - z0);
    let ch = CORRIDOR_HALF_WIDTH;
    // Floor, side walls.
    let c = b.new_coarse();
    b.panelled_quad(v(-hw, 0.0, z0), depth, X * (2.0 * hw), Y, 3.0, c, &ao);
    let c = b.new_coarse();
    b.panelled_quad(v(-hw, 0.0, z0), depth, Y * ARCH_SPRING_Y, X, 3.0, c, &ao);
    let c = b.new_coarse();
    b.panelled_quad(v(hw, 0.0, z0), depth, Y * ARCH_SPRING_Y, -X, 3.0, c, &ao);
    // Soffit: one strip of flat facets per arc segment, hatched along the tunnel.
    let c = b.new_coarse();
    for i in 0..ARCH_SEGMENTS {
        let (p, q) = (arch_pt(i, z0), arch_pt(i + 1, z0));
        let mid = (p + q) * 0.5;
        let n = (v(0.0, ARCH_SPRING_Y, z0) - mid).normalize();
        b.quad([p, q, q + depth, p + depth], n, Z, c, &ao);
        if i > 0 {
            b.stroke(p, p + depth, n, -n, StrokeKind::Seam, 2);
        }
    }
    // Step face at z0 (normal +Z), with the 4x5 corridor hole cut out.
    let c = b.new_coarse();
    let hole_top = CORRIDOR_HEIGHT;
    let n = Z;
    b.panelled_quad(v(-hw, 0.0, z0), X * (hw - ch), Y * ARCH_SPRING_Y, n, 3.0, c, &ao);
    b.panelled_quad(v(ch, 0.0, z0), X * (hw - ch), Y * ARCH_SPRING_Y, n, 3.0, c, &ao);
    b.panelled_quad(v(-ch, hole_top, z0), X * (2.0 * ch), Y * (ARCH_SPRING_Y - hole_top), n, 3.0, c, &ao);
    let centre = v(0.0, ARCH_SPRING_Y, z0);
    for i in 0..ARCH_SEGMENTS {
        b.tri([centre, arch_pt(i, z0), arch_pt(i + 1, z0)], n, Y, c, &ao);
    }
    // Strokes on the step face: hole outline (corridor walls meet the face), plus the
    // reveal's own creases against the face.
    let hole = [v(-ch, 0.0, z0), v(ch, 0.0, z0), v(ch, hole_top, z0), v(-ch, hole_top, z0)];
    let hole_n = [Y, -X, -Y, X]; // corridor face at each edge: floor, right wall, ceiling, left wall
    for k in 0..4 {
        b.crease(hole[k], hole[(k + 1) % 4], hole_n[k], n);
    }
    b.crease(v(-hw, 0.0, z0), v(-ch, 0.0, z0), Y, n);
    b.crease(v(ch, 0.0, z0), v(hw, 0.0, z0), Y, n);
    b.crease(v(-hw, 0.0, z0), v(-hw, ARCH_SPRING_Y, z0), X, n);
    b.crease(v(hw, 0.0, z0), v(hw, ARCH_SPRING_Y, z0), -X, n);
    for i in 0..ARCH_SEGMENTS {
        let mid = (arch_pt(i, z0) + arch_pt(i + 1, z0)) * 0.5;
        b.crease(arch_pt(i, z0), arch_pt(i + 1, z0), (centre - mid).normalize(), n);
    }
    // Reveal edges running along Z: floor/wall and wall/soffit.
    b.crease(v(-hw, 0.0, z0), v(-hw, 0.0, z1), Y, X);
    b.crease(v(hw, 0.0, z0), v(hw, 0.0, z1), Y, -X);
    let first = (centre - (arch_pt(0, z0) + arch_pt(1, z0)) * 0.5).normalize();
    let last = (centre - (arch_pt(ARCH_SEGMENTS - 1, z0) + arch_pt(ARCH_SEGMENTS, z0)) * 0.5).normalize();
    b.crease(arch_pt(0, z0), arch_pt(0, z1), X, first);
    b.crease(arch_pt(ARCH_SEGMENTS, z0), arch_pt(ARCH_SEGMENTS, z1), -X, last);
}

fn hall_shell(b: &mut MeshBuilder) {
    let (min, max) = (HALL_MIN, HALL_MAX);
    let ao = box_ao(min, max);
    let d = max - min;
    let (dx, dy, dz) = (X * d.x, Y * d.y, Z * d.z);
    let panel = 4.0;
    let c = b.new_coarse();
    b.panelled_quad(min, dx, dz, Y, panel, c, &ao); // floor
    let c = b.new_coarse();
    b.panelled_quad(min + dy, dx, dz, -Y, panel, c, &ao); // ceiling
    let c = b.new_coarse();
    b.panelled_quad(min, dz, dy, X, panel, c, &ao); // left wall
    let c = b.new_coarse();
    b.panelled_quad(min + dx, dz, dy, -X, panel, c, &ao); // right wall
    let c = b.new_coarse();
    b.panelled_quad(min + dz, dx, dy, -Z, panel, c, &ao); // far wall

    // Near wall at z=120 with the arch cut out.
    let z = min.z;
    let hw = ARCH_HALF_WIDTH;
    let top = ARCH_SPRING_Y + hw;
    let c = b.new_coarse();
    b.panelled_quad(v(min.x, 0.0, z), X * (min.x.abs() - hw), dy, Z, panel, c, &ao);
    b.panelled_quad(v(hw, 0.0, z), X * (max.x - hw), dy, Z, panel, c, &ao);
    b.panelled_quad(v(-hw, top, z), X * (2.0 * hw), Y * (max.y - top), Z, panel, c, &ao);
    for i in 0..ARCH_SEGMENTS {
        let (p, q) = (arch_pt(i, z), arch_pt(i + 1, z));
        // The two segments meeting at the crown touch the top line, so they are triangles.
        let (p_top, q_top) = (top - p.y < 1e-3, top - q.y < 1e-3);
        if q_top {
            b.tri([p, q, v(p.x, top, z)], Z, Y, c, &ao);
        } else if p_top {
            b.tri([p, q, v(q.x, top, z)], Z, Y, c, &ao);
        } else {
            b.quad([p, q, v(q.x, top, z), v(p.x, top, z)], Z, Y, c, &ao);
        }
        if i > 0 && !p_top {
            b.seam(p, v(p.x, top, z), Z);
        }
    }
    // Seams around the cut-out panels.
    b.seam(v(-hw, top, z), v(hw, top, z), Z);
    b.seam(v(-hw, top, z), v(-hw, max.y, z), Z);
    b.seam(v(hw, top, z), v(hw, max.y, z), Z);
    b.seam(v(-hw, ARCH_SPRING_Y, z), v(-hw, top, z), Z);
    b.seam(v(hw, ARCH_SPRING_Y, z), v(hw, top, z), Z);

    // Arch outline, rank 0: jambs and the arc, against the reveal faces.
    let centre = v(0.0, ARCH_SPRING_Y, z);
    b.stroke(v(-hw, 0.0, z), v(-hw, ARCH_SPRING_Y, z), Z, X, StrokeKind::Crease, 0);
    b.stroke(v(hw, 0.0, z), v(hw, ARCH_SPRING_Y, z), Z, -X, StrokeKind::Crease, 0);
    for i in 0..ARCH_SEGMENTS {
        let (p, q) = (arch_pt(i, z), arch_pt(i + 1, z));
        b.stroke(p, q, Z, (centre - (p + q) * 0.5).normalize(), StrokeKind::Crease, 0);
    }

    // Hall box creases (inward normals). The near-wall/floor join is split by the doorway.
    // The near wall's edges are emitted by hand below, so mask it out of box_edges.
    let f = FaceMask { z_neg: false, ..FaceMask::ALL };
    b.box_edges(min, max, f, -1.0, 1);
    // Near wall is the box's z_neg face with normal +Z. Join with floor, ceiling, side walls:
    b.crease(v(min.x, 0.0, z), v(-hw, 0.0, z), Z, Y);
    b.crease(v(hw, 0.0, z), v(max.x, 0.0, z), Z, Y);
    b.crease(v(min.x, max.y, z), v(max.x, max.y, z), Z, -Y);
    b.crease(v(min.x, 0.0, z), v(min.x, max.y, z), Z, X);
    b.crease(v(max.x, 0.0, z), v(max.x, max.y, z), Z, -X);
}

fn columns(b: &mut MeshBuilder) {
    let hall_ao = box_ao(HALL_MIN, HALL_MAX);
    let h = HALL_MAX.y;
    for &cx in &[-12.0f32, 12.0] {
        for k in 0..6 {
            let cz = 140.0 + 22.0 * k as f32;
            let (min, max) = (v(cx - 1.0, 0.0, cz - 1.0), v(cx + 1.0, h, cz + 1.0));
            let c = b.new_coarse();
            b.panelled_box(min, max, FaceMask::SIDES, 3.5, c, &hall_ao);
            // Four vertical edges.
            b.box_edges(min, max, FaceMask::SIDES, 1.0, 1);
            // Base and head rings against floor and ceiling.
            for (y, n) in [(0.0, Y), (h, -Y)] {
                let p = [v(min.x, y, min.z), v(max.x, y, min.z), v(max.x, y, max.z), v(min.x, y, max.z)];
                let outward = [-Z, X, Z, -X];
                for i in 0..4 {
                    b.crease(p[i], p[(i + 1) % 4], outward[i], n);
                }
            }
        }
    }
}

fn platform(b: &mut MeshBuilder) {
    let ao = box_ao(HALL_MIN, HALL_MAX);
    let (min, max) = (v(-9.0, 0.0, 175.0), v(9.0, 3.0, 215.0));
    let c = b.new_coarse();
    b.panelled_box(min, max, FaceMask::NO_BOTTOM, 3.0, c, &ao);
    b.box_edges(min, max, FaceMask::NO_BOTTOM, 1.0, 1);
    // Foot ring.
    let p = [v(min.x, 0.0, min.z), v(max.x, 0.0, min.z), v(max.x, 0.0, max.z), v(min.x, 0.0, max.z)];
    let outward = [-Z, X, Z, -X];
    for i in 0..4 {
        b.crease(p[i], p[(i + 1) % 4], outward[i], Y);
    }
}

/// Straight stair along the x=-30 wall, climbing toward +Z, ending at a landing.
fn stair(b: &mut MeshBuilder) {
    let ao = box_ao(HALL_MIN, HALL_MAX);
    let c = b.new_coarse();
    let (x0, x1) = (HALL_MIN.x, HALL_MIN.x + 3.0);
    let z_start = 130.0;
    for i in 0..STAIR_STEPS {
        let z = z_start + STAIR_RUN * i as f32;
        let y0 = STAIR_RISE * i as f32;
        let y1 = y0 + STAIR_RISE;
        // Riser (faces -Z) and tread (faces +Y).
        b.quad([v(x0, y0, z), v(x1, y0, z), v(x1, y1, z), v(x0, y1, z)], -Z, Y, c, &ao);
        b.quad([v(x0, y1, z), v(x1, y1, z), v(x1, y1, z + STAIR_RUN), v(x0, y1, z + STAIR_RUN)], Y, X, c, &ao);
        // Open side (stringer), faces +X, from the floor to the tread.
        b.quad([v(x1, 0.0, z), v(x1, 0.0, z + STAIR_RUN), v(x1, y1, z + STAIR_RUN), v(x1, y1, z)], X, Y, c, &ao);
        // Nosing (convex), inner join (concave), tread edges against stringer and wall.
        b.crease(v(x0, y1, z), v(x1, y1, z), -Z, Y);
        b.crease(v(x0, y1, z + STAIR_RUN), v(x1, y1, z + STAIR_RUN), Y, -Z);
        b.crease(v(x1, y1, z), v(x1, y1, z + STAIR_RUN), Y, X);
        b.crease(v(x0, y1, z), v(x0, y1, z + STAIR_RUN), Y, X);
    }
    b.crease(v(x0, 0.0, z_start), v(x1, 0.0, z_start), -Z, Y);

    // Landing: a solid block from the floor, flush with the last tread.
    let top = STAIR_RISE * STAIR_STEPS as f32;
    let z_land = z_start + STAIR_RUN * STAIR_STEPS as f32;
    let (lmin, lmax) = (v(x0, 0.0, z_land), v(HALL_MIN.x + 6.0, top, z_land + 8.0));
    let cl = b.new_coarse();
    let faces = FaceMask { x_neg: false, y_neg: false, z_neg: false, ..FaceMask::ALL };
    b.panelled_box(lmin, lmax, faces, 3.0, cl, &ao);
    // The front face is only exposed beside the stair; the part over the stair is the last
    // tread running on into the landing top.
    b.panelled_quad(v(x1, 0.0, z_land), X * (lmax.x - x1), Y * top, -Z, 3.0, cl, &ao);
    b.seam(v(x0, top, z_land), v(x1, top, z_land), Y);
    b.box_edges(lmin, lmax, faces, 1.0, 1);
    b.crease(v(x1, top, z_land), v(lmax.x, top, z_land), -Z, Y);
    b.crease(v(lmax.x, 0.0, z_land), v(lmax.x, top, z_land), -Z, X);
    b.crease(v(x1, 0.0, z_land), v(x1, top, z_land), -Z, X);
    b.crease(v(x1, 0.0, z_land), v(lmax.x, 0.0, z_land), -Z, Y);
    // Wall join of the landing top.
    b.crease(v(x0, top, z_land), v(x0, top, lmax.z), Y, X);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn indices_in_range() {
        let m = build();
        assert!(!m.indices.is_empty());
        assert_eq!(m.indices.len() % 3, 0);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.vertices.len()));
    }

    #[test]
    fn normals_and_hatch_axes() {
        let m = build();
        for v in &m.vertices {
            let n = Vec3::from(v.normal);
            let h = Vec3::from(v.hatch_axis);
            assert!((n.length() - 1.0).abs() < 1e-4, "normal {n:?}");
            assert!((h.length() - 1.0).abs() < 1e-4, "hatch {h:?}");
            assert!(n.dot(h).abs() < 1e-3, "hatch {h:?} not perpendicular to {n:?}");
            assert!((0.0..=1.0).contains(&v.ao));
        }
    }

    #[test]
    fn winding_matches_normals() {
        let m = build();
        for t in m.indices.chunks(3) {
            let p: Vec<Vec3> = t.iter().map(|&i| Vec3::from(m.vertices[i as usize].position)).collect();
            let n = Vec3::from(m.vertices[t[0] as usize].normal);
            let g = (p[1] - p[0]).cross(p[2] - p[0]);
            assert!(g.dot(n) > 0.0, "wound against normal: {p:?} n={n:?} g={g:?}");
        }
    }

    #[test]
    fn fine_ids_are_unique_per_panel() {
        let m = build();
        // Each id_fine is one face: 3 or 4 vertices sharing normal and coarse id.
        let mut by_id: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, v) in m.vertices.iter().enumerate() {
            by_id.entry(v.id_fine).or_default().push(i);
        }
        for (id, verts) in &by_id {
            assert!(*id != 0);
            assert!(verts.len() == 3 || verts.len() == 4, "id {id} has {} vertices", verts.len());
            let first = m.vertices[verts[0]];
            for &i in verts {
                assert_eq!(m.vertices[i].normal, first.normal);
                assert_eq!(m.vertices[i].id_coarse, first.id_coarse);
            }
            // Consecutive in the buffer: the face is one contiguous run.
            assert!(verts.windows(2).all(|w| w[1] == w[0] + 1));
        }
    }

    #[test]
    fn strokes_are_sane() {
        let m = build();
        assert!(m.strokes.len() > 100);
        for s in &m.strokes {
            assert!((Vec3::from(s.n0).length() - 1.0).abs() < 1e-4);
            assert!((Vec3::from(s.n1).length() - 1.0).abs() < 1e-4);
            assert!(s.rank <= 3 && s.kind <= 4);
            assert!((Vec3::from(s.a) - Vec3::from(s.b)).length() > 1e-4, "{s:?}");
        }
        assert!(m.strokes.iter().any(|s| s.rank == 0));
    }
}
