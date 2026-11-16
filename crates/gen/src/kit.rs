//! Kit of parts: parametric elements that emit triangles, ink strokes and colliders together
//! through [`MeshBuilder`].
//!
//! Conventions, shared by every element (see docs/features/kit-of-parts.md):
//!
//! - **Ids.** Each element (a wall, a slab, a column, a stair flight) gets its own
//!   `id_coarse`; each panel inside it its own `id_fine`, so far contours outline elements and
//!   near contours outline panels.
//! - **Stroke ranks.** 0 massing silhouettes and the largest forms; 1 structural creases;
//!   2 panel seams and secondary joints; 3 fine detail. Curved parts emit strokes only at real
//!   creases and at rings or joints, never at facet edges.
//! - **Hatch axes** follow the form: along beams and bridges, vertical on walls and columns,
//!   along the run on stairs.
//! - **Tessellation.** Faces are cut into cells of at most `step` metres so that the AO baked
//!   afterwards ([`MeshBuilder::bake_ao`]) is interpolated over small triangles and tone band
//!   edges do not follow panel outlines.
//! - **Human scale.** Stairs rise 0.15 to 0.18 m and run 0.30 m whatever their length;
//!   railings are 1.1 m; doors 2.2 m.

use crate::builder::{grid_lines, MeshBuilder};
use crate::mesh::{Collider, StrokeKind};
use glam::Vec3;
use std::f32::consts::PI;

pub const STAIR_RISE_MIN: f32 = 0.15;
pub const STAIR_RISE_MAX: f32 = 0.18;
pub const STAIR_RUN: f32 = 0.30;
pub const RAILING_HEIGHT: f32 = 1.1;
pub const DOOR_HEIGHT: f32 = 2.2;
pub const DOOR_WIDTH: f32 = 1.2;

/// A right-handed-or-not orthonormal frame: `u` along, `v` up, `n` out of the face.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub origin: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    pub n: Vec3,
}

impl Frame {
    pub fn new(origin: Vec3, u: Vec3, v: Vec3, n: Vec3) -> Self {
        Self { origin, u: u.normalize(), v: v.normalize(), n: n.normalize() }
    }

    /// `origin + u*a + v*b + n*c`.
    pub fn at(&self, a: f32, b: f32, c: f32) -> Vec3 {
        self.origin + self.u * a + self.v * b + self.n * c
    }
}

/// A rectangle in a wall's (u, v) coordinates, metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect2 {
    pub u0: f32,
    pub u1: f32,
    pub v0: f32,
    pub v1: f32,
}

impl Rect2 {
    pub fn new(u0: f32, u1: f32, v0: f32, v1: f32) -> Self {
        Self { u0: u0.min(u1), u1: u0.max(u1), v0: v0.min(v1), v1: v0.max(v1) }
    }
    fn contains(&self, u: f32, v: f32) -> bool {
        u > self.u0 && u < self.u1 && v > self.v0 && v < self.v1
    }
}

/// Project `h` into the face with normal `n`; if it is nearly perpendicular to the face,
/// fall back to vertical, then to `alt`.
pub fn face_hatch(n: Vec3, h: Vec3, alt: Vec3) -> Vec3 {
    for cand in [h, Vec3::Y, alt, Vec3::X, Vec3::Z] {
        let p = cand - n * n.dot(cand);
        if p.length() > 0.3 * cand.length().max(1e-6) {
            return p.normalize();
        }
    }
    n.any_orthonormal_vector()
}

/// A box collider: axis-aligned when the axes are, oriented otherwise.
pub fn solid(centre: Vec3, axes: [Vec3; 3], half: Vec3) -> Collider {
    let aligned = axes.iter().all(|a| {
        let m = a.abs();
        (m.max_element() - 1.0).abs() < 1e-5 && (m.x + m.y + m.z - 1.0).abs() < 1e-5
    });
    if aligned {
        let mut ext = Vec3::ZERO;
        for i in 0..3 {
            ext += axes[i].abs() * half[i];
        }
        Collider::Aabb { min: (centre - ext).to_array(), max: (centre + ext).to_array() }
    } else {
        Collider::Obb { centre: centre.to_array(), axes: [axes[0].to_array(), axes[1].to_array(), axes[2].to_array()], half: half.to_array() }
    }
}

/// Which faces of an oriented block to emit: `[-axis0, +axis0, -axis1, +axis1, -axis2, +axis2]`.
pub type Faces = [bool; 6];
pub const ALL_FACES: Faces = [true; 6];

/// An oriented box element. `axes` orthonormal, `half` its half extents. Faces are
/// tessellated to `step`; each face is its own panel. Edges between two emitted faces get a
/// crease stroke with the rank `edge_ranks[k]`, where k is the axis the edge runs along
/// (None for no stroke). Adds the box as a collider when `collide`. Returns the coarse id.
#[allow(clippy::too_many_arguments)]
pub fn block(b: &mut MeshBuilder, centre: Vec3, axes: [Vec3; 3], half: Vec3, faces: Faces, hatch: Vec3, step: f32, edge_ranks: [Option<u32>; 3], collide: bool) -> u32 {
    let coarse = b.new_coarse();
    for i in 0..3 {
        let (j, k) = ((i + 1) % 3, (i + 2) % 3);
        for (si, s) in [-1.0f32, 1.0].into_iter().enumerate() {
            if !faces[i * 2 + si] {
                continue;
            }
            let n = axes[i] * s;
            let origin = centre + axes[i] * (s * half[i]) - axes[j] * half[j] - axes[k] * half[k];
            let h = face_hatch(n, hatch, axes[j]);
            b.rect(origin, axes[j] * (2.0 * half[j]), axes[k] * (2.0 * half[k]), n, h, step, coarse);
        }
    }
    for i in 0..3 {
        for j in (i + 1)..3 {
            let k = 3 - i - j;
            let Some(rank) = edge_ranks[k] else { continue };
            for si in [-1.0f32, 1.0] {
                for sj in [-1.0f32, 1.0] {
                    let fi = faces[i * 2 + (si > 0.0) as usize];
                    let fj = faces[j * 2 + (sj > 0.0) as usize];
                    if !fi || !fj {
                        continue;
                    }
                    let m = centre + axes[i] * (si * half[i]) + axes[j] * (sj * half[j]);
                    b.stroke(m - axes[k] * half[k], m + axes[k] * half[k], axes[i] * si, axes[j] * sj, StrokeKind::Crease, rank);
                }
            }
        }
    }
    if collide {
        b.collider(solid(centre, axes, half));
    }
    coarse
}

/// An axis-aligned block from `min` to `max`.
#[allow(clippy::too_many_arguments)]
pub fn aa_block(b: &mut MeshBuilder, min: Vec3, max: Vec3, faces: Faces, hatch: Vec3, step: f32, edge_ranks: [Option<u32>; 3], collide: bool) -> u32 {
    block(b, (min + max) * 0.5, [Vec3::X, Vec3::Y, Vec3::Z], (max - min) * 0.5, faces, hatch, step, edge_ranks, collide)
}

/// A flat wall face with panel seams and rectangular openings.
pub struct Wall<'a> {
    /// `origin` is the face's (u=0, v=0) corner; `n` points away from the solid.
    pub frame: Frame,
    pub size: (f32, f32),
    /// Depth of solid behind the face, for colliders; 0 adds none.
    pub thickness: f32,
    /// Panel joints, metres along u and v. Each panel gets its own fine id.
    pub seams_u: &'a [f32],
    pub seams_v: &'a [f32],
    pub openings: &'a [Rect2],
    pub hatch: Vec3,
    pub step: f32,
    /// Rank of crease strokes around openings (None for none).
    pub opening_rank: Option<u32>,
    pub seam_rank: u32,
}

/// Emit a [`Wall`]. Returns its coarse id.
pub fn wall(b: &mut MeshBuilder, w: &Wall) -> u32 {
    let coarse = b.new_coarse();
    let f = w.frame;
    let (lu, lv) = w.size;
    let mut bu: Vec<f32> = w.seams_u.to_vec();
    let mut bv: Vec<f32> = w.seams_v.to_vec();
    for o in w.openings {
        bu.extend([o.u0, o.u1]);
        bv.extend([o.v0, o.v1]);
    }
    let gu = grid_lines(lu, w.step, &bu);
    let gv = grid_lines(lv, w.step, &bv);
    let panel_u = grid_lines(lu, f32::INFINITY, w.seams_u);
    let panel_v = grid_lines(lv, f32::INFINITY, w.seams_v);
    let hatch = face_hatch(f.n, w.hatch, f.u);
    // Lines strictly inside (a, b) plus both ends.
    let span = |g: &[f32], a: f32, bnd: f32| -> Vec<f32> {
        let mut v = vec![a];
        v.extend(g.iter().copied().filter(|&x| x > a + 1e-3 && x < bnd - 1e-3));
        v.push(bnd);
        v
    };
    for pj in 0..panel_v.len() - 1 {
        for pi in 0..panel_u.len() - 1 {
            let (pu0, pu1, pv0, pv1) = (panel_u[pi], panel_u[pi + 1], panel_v[pj], panel_v[pj + 1]);
            // Sub-cells of the panel cut by opening edges.
            let cu = span(&bu_sorted(&bu, pu0, pu1), pu0, pu1);
            let cv = span(&bu_sorted(&bv, pv0, pv1), pv0, pv1);
            let mut fine = None;
            for cj in 0..cv.len() - 1 {
                for ci in 0..cu.len() - 1 {
                    let (u0, u1, v0, v1) = (cu[ci], cu[ci + 1], cv[cj], cv[cj + 1]);
                    let (mu, mv) = ((u0 + u1) * 0.5, (v0 + v1) * 0.5);
                    if w.openings.iter().any(|o| o.contains(mu, mv)) {
                        continue;
                    }
                    let id = *fine.get_or_insert_with(|| b.panel());
                    let us: Vec<f32> = span(&gu, u0, u1).iter().map(|x| x - u0).collect();
                    let vs: Vec<f32> = span(&gv, v0, v1).iter().map(|x| x - v0).collect();
                    b.grid_face(f.at(u0, v0, 0.0), f.u, f.v, &us, &vs, f.n, hatch, id, coarse);
                }
            }
        }
    }
    // Seam strokes, interrupted by openings.
    let cut = |b: &mut MeshBuilder, fixed_u: bool, x: f32, len: f32, rank: u32, kind: StrokeKind| {
        let mut gaps: Vec<(f32, f32)> = w
            .openings
            .iter()
            .filter(|o| if fixed_u { x > o.u0 && x < o.u1 } else { x > o.v0 && x < o.v1 })
            .map(|o| if fixed_u { (o.v0, o.v1) } else { (o.u0, o.u1) })
            .collect();
        gaps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut s = 0.0;
        let mut segs = Vec::new();
        for (g0, g1) in gaps {
            if g0 > s {
                segs.push((s, g0));
            }
            s = s.max(g1);
        }
        if s < len {
            segs.push((s, len));
        }
        for (a, c) in segs {
            if c - a < 1e-3 {
                continue;
            }
            let (pa, pc) = if fixed_u { (f.at(x, a, 0.0), f.at(x, c, 0.0)) } else { (f.at(a, x, 0.0), f.at(c, x, 0.0)) };
            b.stroke(pa, pc, f.n, f.n, kind, rank);
        }
    };
    for &s in w.seams_u.iter().filter(|&&s| s > 0.0 && s < lu) {
        cut(b, true, s, lv, w.seam_rank, StrokeKind::Seam);
    }
    for &s in w.seams_v.iter().filter(|&&s| s > 0.0 && s < lv) {
        cut(b, false, s, lu, w.seam_rank, StrokeKind::Seam);
    }
    if let Some(rank) = w.opening_rank {
        for o in w.openings {
            let edges = [
                (o.u0, o.v0, o.u1, o.v0, f.v),
                (o.u0, o.v1, o.u1, o.v1, -f.v),
                (o.u0, o.v0, o.u0, o.v1, f.u),
                (o.u1, o.v0, o.u1, o.v1, -f.u),
            ];
            for (a0, b0, a1, b1, nn) in edges {
                let on_border = (a0 == a1 && (a0 <= 0.0 || a0 >= lu)) || (b0 == b1 && (b0 <= 0.0 || b0 >= lv));
                if !on_border {
                    b.stroke(f.at(a0, b0, 0.0), f.at(a1, b1, 0.0), f.n, nn, StrokeKind::Crease, rank);
                }
            }
        }
    }
    if w.thickness > 0.0 {
        wall_colliders(b, f, lu, lv, w.thickness, w.openings);
    }
    coarse
}

fn bu_sorted(v: &[f32], a: f32, b: f32) -> Vec<f32> {
    let mut out: Vec<f32> = v.iter().copied().filter(|&x| x > a + 1e-3 && x < b - 1e-3).collect();
    out.sort_by(|x, y| x.partial_cmp(y).unwrap());
    out.dedup_by(|x, y| (*x - *y).abs() < 1e-3);
    out
}

/// Box colliders for a wall slab with rectangular holes: horizontal bands between opening
/// edges, each split around the openings that cross it.
fn wall_colliders(b: &mut MeshBuilder, f: Frame, lu: f32, lv: f32, thick: f32, openings: &[Rect2]) {
    let mut vb: Vec<f32> = vec![0.0, lv];
    for o in openings {
        vb.extend([o.v0.clamp(0.0, lv), o.v1.clamp(0.0, lv)]);
    }
    vb.sort_by(|a, c| a.partial_cmp(c).unwrap());
    vb.dedup_by(|a, c| (*a - *c).abs() < 1e-4);
    for k in 0..vb.len() - 1 {
        let (v0, v1) = (vb[k], vb[k + 1]);
        let vm = (v0 + v1) * 0.5;
        let mut holes: Vec<(f32, f32)> = openings.iter().filter(|o| vm > o.v0 && vm < o.v1).map(|o| (o.u0, o.u1)).collect();
        holes.sort_by(|a, c| a.0.partial_cmp(&c.0).unwrap());
        let mut s = 0.0;
        let mut add = |a: f32, c: f32| {
            if c - a > 1e-3 {
                let centre = f.at((a + c) * 0.5, vm, -thick * 0.5);
                b.collider(solid(centre, [f.u, f.v, f.n], Vec3::new((c - a) * 0.5, (v1 - v0) * 0.5, thick * 0.5)));
            }
        };
        for (h0, h1) in holes {
            add(s, h0.max(s));
            s = s.max(h1);
        }
        add(s, lu);
    }
}

/// A room-like recess behind a wall opening (a niche, a doorway's darkness, a portal): the
/// back, the two sides, the ceiling and the floor of a box `depth` deep behind the opening
/// `r` of `frame` (faces point into the recess). Adds colliders for its floor, back, sides
/// and ceiling as thin slabs.
pub fn recess(b: &mut MeshBuilder, frame: Frame, r: Rect2, depth: f32, step: f32, edge_rank: Option<u32>) -> u32 {
    let coarse = b.new_coarse();
    let f = frame;
    let (w, h) = (r.u1 - r.u0, r.v1 - r.v0);
    let p = |a: f32, c: f32, d: f32| f.at(a, c, -d);
    // Back (faces +n), sides, ceiling, floor.
    b.rect(p(r.u0, r.v0, depth), f.u * w, f.v * h, f.n, f.v, step, coarse);
    b.rect(p(r.u0, r.v0, 0.0), -f.n * depth, f.v * h, f.u, f.v, step, coarse);
    b.rect(p(r.u1, r.v0, 0.0), -f.n * depth, f.v * h, -f.u, f.v, step, coarse);
    b.rect(p(r.u0, r.v1, 0.0), f.u * w, -f.n * depth, -f.v, f.u, step, coarse);
    b.rect(p(r.u0, r.v0, 0.0), f.u * w, -f.n * depth, f.v, f.u, step, coarse);
    if let Some(rank) = edge_rank {
        for (a, c) in [(r.u0, r.v0), (r.u1, r.v0), (r.u0, r.v1), (r.u1, r.v1)] {
            b.stroke(p(a, c, 0.0), p(a, c, depth), f.n, f.n, StrokeKind::Crease, rank);
        }
    }
    let t = 0.5;
    let axes = [f.u, f.v, f.n];
    let mid = (r.u0 + r.u1) * 0.5;
    b.collider(solid(f.at(mid, r.v0 + h * 0.5, -depth - t * 0.5), axes, Vec3::new(w * 0.5, h * 0.5, t * 0.5)));
    b.collider(solid(f.at(mid, r.v0 - t * 0.5, -depth * 0.5), axes, Vec3::new(w * 0.5, t * 0.5, depth * 0.5)));
    b.collider(solid(f.at(mid, r.v1 + t * 0.5, -depth * 0.5), axes, Vec3::new(w * 0.5, t * 0.5, depth * 0.5)));
    b.collider(solid(f.at(r.u0 - t * 0.5, r.v0 + h * 0.5, -depth * 0.5), axes, Vec3::new(t * 0.5, h * 0.5, depth * 0.5)));
    b.collider(solid(f.at(r.u1 + t * 0.5, r.v0 + h * 0.5, -depth * 0.5), axes, Vec3::new(t * 0.5, h * 0.5, depth * 0.5)));
    coarse
}

/// The faces of a rectangular hole through a wall (a door's or window's thickness): the two
/// jambs, the lintel and the sill, from the plane of `frame` back `depth` into `-n`, facing
/// into the hole. A walkable threshold (`walk`) gets a slab collider under it; the jambs and
/// lintel are covered by the colliders of the walls either side. Strokes along the hole's four
/// corners at `edge_rank`.
pub fn reveal(b: &mut MeshBuilder, frame: Frame, r: Rect2, depth: f32, step: f32, walk: bool, edge_rank: Option<u32>) -> u32 {
    let coarse = b.new_coarse();
    let f = frame;
    let (w, h) = (r.u1 - r.u0, r.v1 - r.v0);
    let p = |a: f32, c: f32, d: f32| f.at(a, c, -d);
    b.rect(p(r.u0, r.v0, 0.0), -f.n * depth, f.v * h, f.u, f.v, step, coarse);
    b.rect(p(r.u1, r.v0, 0.0), -f.n * depth, f.v * h, -f.u, f.v, step, coarse);
    b.rect(p(r.u0, r.v1, 0.0), f.u * w, -f.n * depth, -f.v, f.u, step, coarse);
    b.rect(p(r.u0, r.v0, 0.0), f.u * w, -f.n * depth, f.v, f.n, step, coarse);
    if let Some(rank) = edge_rank {
        for (a, c) in [(r.u0, r.v0), (r.u1, r.v0), (r.u0, r.v1), (r.u1, r.v1)] {
            b.stroke(p(a, c, 0.0), p(a, c, depth), f.n, f.n, StrokeKind::Crease, rank);
        }
    }
    if walk {
        let axes = [f.u, f.v, f.n];
        b.collider(solid(f.at((r.u0 + r.u1) * 0.5, r.v0 - 0.3, -depth * 0.5), axes, Vec3::new(w * 0.5 + 0.2, 0.3, depth * 0.5 + 0.05)));
    }
    coarse
}

/// A railing drawn in ink only: a top rail at `RAILING_HEIGHT`, a mid rail and posts, from
/// `a` to `c` (points on the walking surface; may slope). `out` is the horizontal direction
/// away from the walking side. Adds a thin collider. The rails are lines, not geometry, so
/// they fade by rank with distance instead of aliasing into sub-pixel triangles.
pub fn railing(b: &mut MeshBuilder, a: Vec3, c: Vec3, out: Vec3, post_spacing: f32, top_rank: u32) {
    let h = RAILING_HEIGHT;
    let up = Vec3::Y;
    let len = (c - a).length();
    if len < 0.05 {
        return;
    }
    b.stroke(a + up * h, c + up * h, up, out, StrokeKind::Border, top_rank);
    b.stroke(a + up * (h * 0.5), c + up * (h * 0.5), up, out, StrokeKind::Border, 3);
    let n = (len / post_spacing).round().max(1.0) as usize;
    for i in 0..=n {
        let p = a + (c - a) * (i as f32 / n as f32);
        b.stroke(p, p + up * h, out, out, StrokeKind::Border, 3);
    }
    // Collider: a thin wall along the rail, its height measured vertically.
    let along = (c - a) / len;
    let across = along.cross(up).normalize();
    let tilt_up = across.cross(along).normalize();
    let cos = tilt_up.y.max(0.2);
    let centre = (a + c) * 0.5 + up * (h * 0.5);
    b.collider(solid(centre, [along, tilt_up, across], Vec3::new(len * 0.5, h * 0.5 * cos, 0.03)));
}

/// Railing along `a..c` with gaps (each a range of distance along it, metres).
pub fn railing_with_gaps(b: &mut MeshBuilder, a: Vec3, c: Vec3, out: Vec3, post_spacing: f32, top_rank: u32, gaps: &[(f32, f32)]) {
    let len = (c - a).length();
    let dir = (c - a) / len;
    let mut gaps: Vec<(f32, f32)> = gaps.to_vec();
    gaps.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let mut s = 0.0;
    for (g0, g1) in gaps {
        if g0 > s + 0.2 {
            railing(b, a + dir * s, a + dir * g0, out, post_spacing, top_rank);
        }
        s = s.max(g1);
    }
    if len > s + 0.2 {
        railing(b, a + dir * s, c, out, post_spacing, top_rank);
    }
}

/// Number of risers and the exact rise for a height, within the stair limits. Every height
/// from 0.75 m up has a solution; some lower heights (for example 0.2 m) have none, and get
/// the nearest count.
pub fn stair_risers(height: f32) -> (usize, f32) {
    let mut n = (height / 0.168).round().max(1.0) as usize;
    while height / n as f32 > STAIR_RISE_MAX {
        n += 1;
    }
    while n > 1 && height / ((n - 1) as f32) <= STAIR_RISE_MAX && height / (n as f32) < STAIR_RISE_MIN {
        n -= 1;
    }
    (n, height / n as f32)
}

/// A straight stair against a wall or free-standing, broken into flights by landings.
pub struct Stair {
    /// Centre of the bottom edge of the first riser, on the lower floor.
    pub foot: Vec3,
    /// Horizontal unit direction of travel going up.
    pub dir: Vec3,
    pub width: f32,
    /// Total height climbed.
    pub height: f32,
    /// Riser count, if the caller has fitted the stair to a run; otherwise chosen by
    /// [`stair_risers`]. The rise must come out within the limits.
    pub risers: Option<usize>,
    /// Most risers in one flight before a landing.
    pub max_risers: usize,
    /// Landing length along `dir`.
    pub landing: f32,
    /// Thickness of the slab under the steps.
    pub waist: f32,
    /// Side of the wall, as a horizontal unit vector from the stair centre towards it
    /// (perpendicular to `dir`), or None for a free-standing stair.
    pub wall: Option<Vec3>,
    pub railing: bool,
    /// Walls on both sides (a stair in a passage): no side faces, soffit or railings.
    pub enclosed: bool,
}

/// What a built stair measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StairInfo {
    /// Centre of the top edge, on the upper floor.
    pub top: Vec3,
    pub risers: usize,
    pub rise: f32,
    pub run: f32,
    /// Horizontal length from foot to top.
    pub length: f32,
}

/// Emit a [`Stair`]: per flight, treads, risers, the open side(s) and the sloping soffit;
/// flat landings between flights; a ramp collider per flight, a box per landing, railings
/// on the open side.
pub fn stair(b: &mut MeshBuilder, s: &Stair) -> StairInfo {
    let dir = Vec3::new(s.dir.x, 0.0, s.dir.z).normalize();
    let side = dir.cross(Vec3::Y).normalize(); // horizontal, perpendicular to the run
    let (n, rise) = match s.risers {
        Some(n) => (n, s.height / n as f32),
        None => stair_risers(s.height),
    };
    assert!((STAIR_RISE_MIN - 1e-4..=STAIR_RISE_MAX + 1e-4).contains(&rise), "stair rise {rise} out of limits");
    let run = STAIR_RUN;
    let flights = n.div_ceil(s.max_risers.max(1));
    let per = n.div_ceil(flights);
    let hw = s.width * 0.5;
    let open_sides: Vec<f32> = match s.wall {
        _ if s.enclosed => vec![],
        Some(wv) => vec![if wv.dot(side) > 0.0 { -1.0 } else { 1.0 }],
        None => vec![-1.0, 1.0],
    };
    let slope = rise / run;
    let cos = 1.0 / (1.0 + slope * slope).sqrt();
    let waist_v = s.waist / cos; // vertical thickness of the sloping slab
    let mut at = s.foot; // centre of the bottom of the current flight
    let mut left = n;
    let mut first = true;
    while left > 0 {
        let m = per.min(left);
        left -= m;
        if !first {
            // Landing before this flight.
            let len = s.landing;
            let top = at.y;
            let centre = at + dir * (len * 0.5) - Vec3::Y * (s.waist * 0.5);
            let mut faces = ALL_FACES;
            // Axes: dir, Y, side. Hide the face against the wall.
            if let Some(wv) = s.wall {
                faces[if wv.dot(side) > 0.0 { 5 } else { 4 }] = false;
            }
            if s.enclosed {
                faces[4] = false;
                faces[5] = false;
                faces[2] = false;
            }
            block(b, centre, [dir, Vec3::Y, side], Vec3::new(len * 0.5, s.waist * 0.5, hw), faces, dir, 1.5, [Some(2), Some(2), Some(3)], true);
            if s.railing {
                for &sd in &open_sides {
                    let e = side * (sd * (hw - 0.05));
                    railing(b, Vec3::new(at.x, top, at.z) + e, Vec3::new(at.x, top, at.z) + dir * len + e, side * sd, 1.2, 1);
                }
            }
            at += dir * len;
        }
        first = false;
        flight(b, at, dir, side, hw, m, rise, run, waist_v, &open_sides, s.wall.is_some() && !s.enclosed, s.railing, !s.enclosed);
        at += dir * (m as f32 * run) + Vec3::Y * (m as f32 * rise);
    }
    StairInfo { top: at, risers: n, rise, run, length: (at - s.foot).with_y(0.0).length() }
}

#[allow(clippy::too_many_arguments)]
fn flight(b: &mut MeshBuilder, foot: Vec3, dir: Vec3, side: Vec3, hw: f32, m: usize, rise: f32, run: f32, waist_v: f32, open_sides: &[f32], against_wall: bool, rail: bool, soffit: bool) {
    let coarse = b.new_coarse();
    let y0 = foot.y;
    let pt = |s: f32, y: f32, w: f32| Vec3::new(foot.x, y, foot.z) + dir * s + side * w;
    let bottom = |s: f32| y0 + s * rise / run - waist_v;
    for i in 0..m {
        let (s0, s1) = (i as f32 * run, (i + 1) as f32 * run);
        let (ya, yb) = (y0 + i as f32 * rise, y0 + (i + 1) as f32 * rise);
        // Riser faces back down the stair, hatched across; tread faces up, hatched along the run.
        let f = b.panel();
        b.polygon(&[pt(s0, ya, -hw), pt(s0, ya, hw), pt(s0, yb, hw), pt(s0, yb, -hw)], -dir, side, f, coarse);
        let f = b.panel();
        b.polygon(&[pt(s0, yb, -hw), pt(s0, yb, hw), pt(s1, yb, hw), pt(s1, yb, -hw)], Vec3::Y, dir, f, coarse);
        // Nosing, fine detail.
        b.stroke(pt(s0, yb, -hw), pt(s0, yb, hw), -dir, Vec3::Y, StrokeKind::Crease, 3);
    }
    let len = m as f32 * run;
    for &sd in open_sides {
        let w = sd * hw;
        let f = b.panel();
        let n = side * sd;
        for i in 0..m {
            let (s0, s1) = (i as f32 * run, (i + 1) as f32 * run);
            let yb = y0 + (i + 1) as f32 * rise;
            b.polygon(&[pt(s0, bottom(s0), w), pt(s1, bottom(s1), w), pt(s1, yb, w), pt(s0, yb, w)], n, dir + Vec3::Y * (rise / run), f, coarse);
        }
        // Sawtooth profile of the open side (secondary), soffit edge (structural).
        for i in 0..m {
            let (s0, s1) = (i as f32 * run, (i + 1) as f32 * run);
            let (ya, yb) = (y0 + i as f32 * rise, y0 + (i + 1) as f32 * rise);
            b.stroke(pt(s0, ya, w), pt(s0, yb, w), n, -dir, StrokeKind::Crease, 2);
            b.stroke(pt(s0, yb, w), pt(s1, yb, w), n, Vec3::Y, StrokeKind::Crease, 2);
        }
        b.stroke(pt(0.0, bottom(0.0), w), pt(len, bottom(len), w), n, -Vec3::Y, StrokeKind::Crease, 1);
        if rail {
            let e = -sd * 0.05;
            railing(b, pt(0.0, y0 + rise, w + e), pt(len, y0 + rise * (m as f32 + 1.0), w + e).with_y(y0 + rise + len * rise / run), n, 1.2, 1);
        }
    }
    // Soffit: the underside of the sloping slab, tessellated for AO.
    let slope_dir = (dir * run + Vec3::Y * rise).normalize();
    let soffit_n = side.cross(slope_dir).normalize();
    let soffit_n = if soffit_n.y > 0.0 { -soffit_n } else { soffit_n };
    if soffit {
        b.rect(pt(0.0, bottom(0.0), -hw), slope_dir * (len / slope_dir.with_y(0.0).length()), side * (2.0 * hw), soffit_n, slope_dir, 1.5, coarse);
    }
    if against_wall {
        let w = -open_sides[0] * hw;
        b.stroke(pt(0.0, bottom(0.0), w), pt(len, bottom(len), w), soffit_n, -open_sides[0] * side, StrokeKind::Crease, 2);
    }
    b.collider(Collider::Ramp {
        start: pt(0.0, y0, 0.0).to_array(),
        end: pt(len, y0 + m as f32 * rise, 0.0).to_array(),
        half_width: hw,
        thickness: waist_v * cos_of(rise, run),
    });
}

fn cos_of(rise: f32, run: f32) -> f32 {
    run / (rise * rise + run * run).sqrt()
}

/// A square column from `base` (centre of its foot) up `height`, side `size`, with drum
/// joints every `drum` metres (rank 2) and rings at foot and head (rank 1).
pub fn column_square(b: &mut MeshBuilder, base: Vec3, size: f32, height: f32, drum: f32, step: f32) -> u32 {
    let coarse = b.new_coarse();
    let h = size * 0.5;
    let joints = grid_lines(height, drum, &[]);
    let sides = [(Vec3::X, Vec3::Z), (-Vec3::X, -Vec3::Z), (Vec3::Z, -Vec3::X), (-Vec3::Z, Vec3::X)];
    for j in 0..joints.len() - 1 {
        let fine = b.panel();
        for &(n, t) in &sides {
            let o = base + n * h - t * h + Vec3::Y * joints[j];
            let us = grid_lines(size, step, &[]);
            let vs = grid_lines(joints[j + 1] - joints[j], step, &[]);
            b.grid_face(o, t, Vec3::Y, &us, &vs, n, Vec3::Y, fine, coarse);
        }
    }
    for &(n, t) in &sides {
        let c = base + n * h + t * h;
        b.stroke(c, c + Vec3::Y * height, n, t, StrokeKind::Crease, 1);
        for (k, &y) in joints.iter().enumerate() {
            let rank = if k == 0 || k == joints.len() - 1 { 1 } else { 2 };
            let a = base + n * h - t * h + Vec3::Y * y;
            b.stroke(a, a + t * size, n, Vec3::Y, StrokeKind::Seam, rank);
        }
    }
    b.collider(solid(base + Vec3::Y * (height * 0.5), [Vec3::X, Vec3::Y, Vec3::Z], Vec3::new(h, height * 0.5, h)));
    coarse
}

/// A round column of `sides` flat facets (no strokes at facet edges: the silhouette comes
/// from contours), drum joints every `drum` metres as rings (rank 2), foot and head rings
/// (rank 1), and square plinth and abacus blocks `collar` wider than the shaft.
#[allow(clippy::too_many_arguments)]
pub fn column_round(b: &mut MeshBuilder, base: Vec3, radius: f32, height: f32, sides: usize, drum: f32, collar: f32, step: f32) -> u32 {
    let plinth = (radius * 0.6).max(0.4);
    let r_out = radius + collar;
    // Plinth and abacus are their own elements.
    aa_block(b, base + Vec3::new(-r_out, 0.0, -r_out), base + Vec3::new(r_out, plinth, r_out), [true, true, false, true, true, true], Vec3::Y, step, [Some(1), Some(1), Some(1)], true);
    let top = base.y + height;
    aa_block(b, Vec3::new(base.x - r_out, top - plinth, base.z - r_out), Vec3::new(base.x + r_out, top, base.z + r_out), [true, true, true, false, true, true], Vec3::Y, step, [Some(1), Some(1), Some(1)], true);
    let coarse = b.new_coarse();
    let (y0, y1) = (base.y + plinth, top - plinth);
    let joints: Vec<f32> = grid_lines(y1 - y0, drum, &[]).iter().map(|x| x + y0).collect();
    let ang = |k: usize| 2.0 * PI * k as f32 / sides as f32;
    let rim = |k: usize, y: f32| Vec3::new(base.x + radius * ang(k).cos(), y, base.z + radius * ang(k).sin());
    // Smooth normals: each vertex carries its radial direction, so the shading is round and
    // only the silhouette shows the facets.
    for j in 0..joints.len() - 1 {
        let fine = b.panel();
        let vs: Vec<f32> = grid_lines(joints[j + 1] - joints[j], step, &[]).iter().map(|v| v + joints[j]).collect();
        let at = |i: usize, jj: usize| {
            let a = ang(i);
            (rim(i, vs[jj]), Vec3::new(a.cos(), 0.0, a.sin()), Vec3::Y)
        };
        b.smooth_grid(sides + 1, vs.len(), &at, fine, coarse);
    }
    for (jj, &y) in joints.iter().enumerate() {
        let rank = if jj == 0 || jj == joints.len() - 1 { 1 } else { 2 };
        for k in 0..sides {
            let mid = (ang(k) + ang(k + 1)) * 0.5;
            let n = Vec3::new(mid.cos(), 0.0, mid.sin());
            b.stroke(rim(k, y), rim(k + 1, y), n, Vec3::Y, StrokeKind::Seam, rank);
        }
    }
    // Octagonal collider: an axis-aligned square and the same square turned 45 degrees.
    let c = Vec3::new(base.x, (y0 + y1) * 0.5, base.z);
    let half = Vec3::new(radius, (y1 - y0) * 0.5, radius);
    b.collider(solid(c, [Vec3::X, Vec3::Y, Vec3::Z], half));
    let d = Vec3::new(1.0, 0.0, 1.0).normalize();
    b.collider(solid(c, [d, Vec3::Y, d.cross(Vec3::Y).normalize()], half));
    coarse
}

/// A beam of rectangular section from `a` to `c` (centres of its end faces), `width` across
/// and `depth` in the vertical plane containing the axis. Hatched along its length. Long
/// edges get `rank`; end edges one rank finer.
pub fn beam(b: &mut MeshBuilder, a: Vec3, c: Vec3, width: f32, depth: f32, rank: u32, step: f32) -> u32 {
    let along = (c - a).normalize();
    let across = if along.y.abs() > 0.99 { Vec3::X } else { along.cross(Vec3::Y).normalize() };
    let up = across.cross(along).normalize();
    let half = Vec3::new((c - a).length() * 0.5, depth * 0.5, width * 0.5);
    let fine_rank = (rank + 1).min(3);
    block(b, (a + c) * 0.5, [along, up, across], half, ALL_FACES, along, step, [Some(rank), Some(fine_rank), Some(fine_rank)], true)
}

/// A bridge deck from `a` to `c` (centres of the deck's top surface at its ends, level),
/// with an optional girder below, transverse ribs drawn on the soffit, and railings.
pub struct Bridge {
    pub a: Vec3,
    pub c: Vec3,
    pub width: f32,
    pub deck: f32,
    /// Girder (depth below the deck, width), or None.
    pub girder: Option<(f32, f32)>,
    /// Spacing of soffit ribs (strokes), or None.
    pub ribs: Option<f32>,
    pub railings: bool,
    /// Rank of the deck's long edges (0 for a bridge that should read from far away).
    pub rank: u32,
}

pub fn bridge(b: &mut MeshBuilder, br: &Bridge) -> u32 {
    let along = (br.c - br.a).with_y(0.0).normalize();
    let across = along.cross(Vec3::Y).normalize();
    let len = (br.c - br.a).with_y(0.0).length();
    let mid = (br.a + br.c) * 0.5;
    let hw = br.width * 0.5;
    let axes = [along, Vec3::Y, across];
    // Ends are buried in walls or landings.
    let faces = [false, false, true, true, true, true];
    let coarse = block(b, mid - Vec3::Y * (br.deck * 0.5), axes, Vec3::new(len * 0.5, br.deck * 0.5, hw), faces, along, 2.0, [Some(br.rank), Some(2), Some(2)], true);
    if let Some((gd, gw)) = br.girder {
        block(b, mid - Vec3::Y * (br.deck + gd * 0.5), axes, Vec3::new(len * 0.5, gd * 0.5, gw * 0.5), [false, false, true, false, true, true], along, 2.0, [Some(br.rank.max(1)), None, None], true);
    }
    if let Some(sp) = br.ribs {
        let n = (len / sp).floor() as usize;
        let y = mid.y - br.deck - 0.002;
        let gw = br.girder.map(|g| g.1 * 0.5).unwrap_or(0.0);
        for i in 1..n {
            let p = br.a.with_y(y) + along * (len * i as f32 / n as f32);
            for sgn in [-1.0f32, 1.0] {
                b.stroke(p + across * (sgn * gw), p + across * (sgn * hw), -Vec3::Y, -Vec3::Y, StrokeKind::Seam, 2);
            }
        }
    }
    if br.railings {
        for sgn in [-1.0f32, 1.0] {
            let e = across * (sgn * (hw - 0.08));
            railing(b, br.a + e, br.c + e, across * sgn, 1.5, 1);
        }
    }
    coarse
}

/// A stack of galleries recessed into a wall: per level a floor slab edge-on to the void
/// (projecting past the façade), a railing, a back wall with doors into dark rooms, and
/// optionally piers on the façade line. `frame.origin` is on the façade plane at u = 0 and
/// y = 0; `frame.n` points into the void; `frame.v` is up.
pub struct Gallery<'a> {
    pub frame: Frame,
    pub u0: f32,
    pub u1: f32,
    /// Floor heights (tops of slabs, as frame v coordinates), ascending, `storey` apart or more.
    pub levels: &'a [f32],
    pub storey: f32,
    pub depth: f32,
    pub slab: f32,
    pub projection: f32,
    pub bay: f32,
    pub piers: bool,
    /// Doors per bay are present where this returns true, given (level index, bay index).
    pub door: &'a dyn Fn(usize, usize) -> bool,
    /// Railing gaps: (level index, u start, u end).
    pub gaps: &'a [(usize, f32, f32)],
}

impl Gallery<'_> {
    /// The opening this stack needs in its wall, in the wall frame's (u, v).
    pub fn opening(&self) -> Rect2 {
        let top = *self.levels.last().unwrap() + self.storey - self.slab;
        Rect2::new(self.u0, self.u1, self.levels[0] - self.slab, top)
    }
}

pub fn gallery(b: &mut MeshBuilder, g: &Gallery) {
    let f = g.frame;
    let run = g.u1 - g.u0;
    let open = g.opening();
    let axes = [f.u, f.v, f.n];
    let step = 1.5;
    // Recess sides and ceiling.
    let h = open.v1 - open.v0;
    let c = b.new_coarse();
    b.rect(f.at(g.u0, open.v0, -g.depth), f.n * g.depth, f.v * h, f.u, f.v, step, c);
    b.rect(f.at(g.u1, open.v0, -g.depth), f.n * g.depth, f.v * h, -f.u, f.v, step, c);
    b.rect(f.at(g.u0, open.v1, -g.depth), f.u * run, f.n * g.depth, -f.v, f.u, step, c);
    for (a, rank) in [(g.u0, 1), (g.u1, 1)] {
        b.stroke(f.at(a, open.v1, -g.depth), f.at(a, open.v1, 0.0), -f.v, f.u, StrokeKind::Crease, rank);
    }
    b.collider(solid(f.at(g.u0 - 0.5, (open.v0 + open.v1) * 0.5, -g.depth * 0.5), axes, Vec3::new(0.5, h * 0.5, g.depth * 0.5)));
    b.collider(solid(f.at(g.u1 + 0.5, (open.v0 + open.v1) * 0.5, -g.depth * 0.5), axes, Vec3::new(0.5, h * 0.5, g.depth * 0.5)));
    b.collider(solid(f.at((g.u0 + g.u1) * 0.5, open.v1 + 0.5, -g.depth * 0.5), axes, Vec3::new(run * 0.5, 0.5, g.depth * 0.5)));

    let bays = ((run / g.bay).round() as usize).max(1);
    let bay = run / bays as f32;
    for (li, &y) in g.levels.iter().enumerate() {
        // Slab: top at y, from the back wall to the projection.
        let (n0, n1) = (-g.depth, g.projection);
        let centre = f.at((g.u0 + g.u1) * 0.5, y - g.slab * 0.5, (n0 + n1) * 0.5);
        let half = Vec3::new(run * 0.5, g.slab * 0.5, (n1 - n0) * 0.5);
        let faces = [false, false, true, true, false, true];
        block(b, centre, axes, half, faces, f.u, step, [Some(1), None, None], true);
        // Back wall for this storey, with doors.
        let next = g.levels.get(li + 1).map(|&n| n - g.slab).unwrap_or(open.v1);
        let wall_h = next - y;
        let mut doors = Vec::new();
        for k in 0..bays {
            if (g.door)(li, k) && wall_h > DOOR_HEIGHT + 0.3 {
                let cu = (k as f32 + 0.5) * bay;
                doors.push(Rect2::new(cu - DOOR_WIDTH * 0.5, cu + DOOR_WIDTH * 0.5, 0.0, DOOR_HEIGHT));
            }
        }
        let back = Frame::new(f.at(g.u0, y, -g.depth), f.u, f.v, f.n);
        let seams: Vec<f32> = (1..bays).map(|k| k as f32 * bay).collect();
        wall(b, &Wall { frame: back, size: (run, wall_h), thickness: 0.6, seams_u: &seams, seams_v: &[], openings: &doors, hatch: Vec3::Y, step, opening_rank: Some(2), seam_rank: 3 });
        for d in &doors {
            recess(b, back, *d, 2.5, step, None);
        }
        // Floor/back-wall and ceiling/back-wall creases.
        b.stroke(f.at(g.u0, y, -g.depth), f.at(g.u1, y, -g.depth), f.v, f.n, StrokeKind::Crease, 2);
        // Piers on the façade line between bays.
        if g.piers {
            for k in 1..bays {
                let cu = g.u0 + k as f32 * bay;
                // No pier where a bridge meets the railing gap on any level.
                if g.gaps.iter().any(|gp| cu > gp.1 - 0.5 && cu < gp.2 + 0.5) {
                    continue;
                }
                let pc = f.at(cu, (y + next) * 0.5, -0.35);
                block(b, pc, axes, Vec3::new(0.35, (next - y) * 0.5, 0.35), [true, true, false, false, true, true], f.v, step, [None, Some(1), None], true);
            }
        }
        // Railing on the slab edge.
        let gaps: Vec<(f32, f32)> = g.gaps.iter().filter(|gp| gp.0 == li).map(|gp| (gp.1 - g.u0, gp.2 - g.u0)).collect();
        let e = g.projection - 0.1;
        railing_with_gaps(b, f.at(g.u0, y, e), f.at(g.u1, y, e), f.n, 1.5, 1, &gaps);
    }
}

/// A true round arch through a wall: it fills the rectangle `[-w/2, w/2] x [0, height]` of
/// `frame` (centred on u = 0, v = 0 at floor level) with a semicircular opening of `span`
/// springing at `spring`, a ring of `voussoirs` stones `ring` deep, spandrels, jambs and a
/// curved soffit `thickness` deep (into -n). The back face is emitted too. Strokes: the
/// intrados and jambs (rank 0, the opening's outline), the extrados (rank 1), voussoir
/// joints on both faces (rank 2) and along the soffit (rank 3). No facet edges.
pub struct Arch {
    pub frame: Frame,
    pub w: f32,
    pub height: f32,
    pub span: f32,
    pub spring: f32,
    pub ring: f32,
    pub voussoirs: usize,
    pub thickness: f32,
    /// Facets per voussoir along the curve.
    pub facets: usize,
}

impl Arch {
    pub fn opening(&self) -> Rect2 {
        Rect2::new(-self.w * 0.5, self.w * 0.5, 0.0, self.height)
    }
}

pub fn arch(b: &mut MeshBuilder, a: &Arch) -> u32 {
    let coarse = b.new_coarse();
    let f = a.frame;
    let r = a.span * 0.5;
    let rr = r + a.ring;
    let hw = a.w * 0.5;
    let top = a.height - a.spring; // rectangle height above the springing line
    assert!(hw > rr && top > rr, "arch rectangle too small for its ring");
    let nseg = a.voussoirs * a.facets;
    let th = |i: usize| PI * i as f32 / nseg as f32; // 0 at the right springer, PI at the left
    let circ = |rad: f32, t: f32, depth: f32| f.at(rad * t.cos(), a.spring + rad * t.sin(), -depth);
    let step = 1.5;
    for (face, depth, nrm) in [(0, 0.0, f.n), (1, a.thickness, -f.n)] {
        // Jambs: piers either side below the springing line.
        for sgn in [-1.0f32, 1.0] {
            let (u0, u1) = if sgn < 0.0 { (-hw, -r) } else { (r, hw) };
            b.rect(f.at(u0, 0.0, -depth), f.u * (u1 - u0), f.v * a.spring, nrm, f.v, step, coarse);
        }
        // Voussoirs: annulus sectors, one panel each, split radially for AO.
        for k in 0..a.voussoirs {
            let fine = b.panel();
            for j in 0..a.facets {
                let i = k * a.facets + j;
                let (t0, t1) = (th(i), th(i + 1));
                b.polygon(&[circ(r, t0, depth), circ(r, t1, depth), circ(rr, t1, depth), circ(rr, t0, depth)], nrm, f.v, fine, coarse);
            }
        }
        // Spandrels: between the extrados and the rectangle, by angle, with the two upper
        // corners inserted, split radially.
        let corner_r = (top.atan2(hw), PI - top.atan2(hw));
        let mut angles: Vec<f32> = (0..=nseg).map(th).collect();
        angles.extend([corner_r.0, corner_r.1, PI * 0.5]);
        angles.sort_by(|x, y| x.partial_cmp(y).unwrap());
        angles.dedup_by(|x, y| (*x - *y).abs() < 1e-5);
        let rect_pt = |t: f32| {
            let (c, s) = (t.cos(), t.sin());
            let k = if c.abs() < 1e-6 { top / s } else { (hw / c.abs()).min(if s > 1e-6 { top / s } else { f32::MAX }) };
            (c * k, s * k)
        };
        let fine = b.panel();
        let splits = 4;
        for w in angles.windows(2) {
            let (t0, t1) = (w[0], w[1]);
            let (p0, p1) = (rect_pt(t0), rect_pt(t1));
            let e0 = (rr * t0.cos(), rr * t0.sin());
            let e1 = (rr * t1.cos(), rr * t1.sin());
            for s in 0..splits {
                let (fa, fb) = (s as f32 / splits as f32, (s + 1) as f32 / splits as f32);
                let lerp = |e: (f32, f32), p: (f32, f32), x: f32| f.at(e.0 + (p.0 - e.0) * x, a.spring + e.1 + (p.1 - e.1) * x, -depth);
                let quad = [lerp(e0, p0, fa), lerp(e1, p1, fa), lerp(e1, p1, fb), lerp(e0, p0, fb)];
                if (quad[2] - quad[3]).length() < 1e-4 {
                    b.polygon(&[quad[0], quad[1], quad[2]], nrm, f.v, fine, coarse);
                } else {
                    b.polygon(&quad, nrm, f.v, fine, coarse);
                }
            }
        }
        // Strokes on this face.
        let (rank_out, rank_ring) = if face == 0 { (0, 1) } else { (1, 2) };
        for i in 0..nseg {
            let (t0, t1) = (th(i), th(i + 1));
            let tm = (t0 + t1) * 0.5;
            let inward = -(f.u * tm.cos() + f.v * tm.sin());
            b.stroke(circ(r, t0, depth), circ(r, t1, depth), nrm, inward, StrokeKind::Crease, rank_out);
            b.stroke(circ(rr, t0, depth), circ(rr, t1, depth), nrm, nrm, StrokeKind::Seam, rank_ring);
        }
        for k in 1..a.voussoirs {
            let t = th(k * a.facets);
            b.stroke(circ(r, t, depth), circ(rr, t, depth), nrm, nrm, StrokeKind::Seam, 2);
        }
        for sgn in [-1.0f32, 1.0] {
            b.stroke(f.at(sgn * r, 0.0, -depth), f.at(sgn * r, a.spring, -depth), nrm, -f.u * sgn, StrokeKind::Crease, rank_out);
            b.stroke(f.at(sgn * r, a.spring, -depth), f.at(sgn * rr, a.spring, -depth), nrm, nrm, StrokeKind::Seam, 2);
        }
    }
    // Jamb reveals through the thickness and the curved soffit, hatched along the curve.
    for sgn in [-1.0f32, 1.0] {
        b.rect(f.at(sgn * r, 0.0, 0.0), -f.n * a.thickness, f.v * a.spring, -f.u * sgn, f.v, step, coarse);
    }
    for k in 0..a.voussoirs {
        let fine = b.panel();
        // Smooth normals along the curve, one strip per voussoir.
        let ds = grid_lines(a.thickness, step, &[]);
        let at = |j: usize, jj: usize| {
            let t = th(k * a.facets + j);
            let inward = -(f.u * t.cos() + f.v * t.sin());
            let tangent = -f.u * t.sin() + f.v * t.cos();
            (circ(r, t, ds[jj]), inward, tangent)
        };
        b.smooth_grid(a.facets + 1, ds.len(), &at, fine, coarse);
        if k > 0 {
            let t = th(k * a.facets);
            let inward = -(f.u * t.cos() + f.v * t.sin());
            b.stroke(circ(r, t, 0.0), circ(r, t, a.thickness), inward, inward, StrokeKind::Seam, 3);
        }
    }
    for sgn in [-1.0f32, 1.0] {
        b.stroke(f.at(sgn * r, a.spring, 0.0), f.at(sgn * r, a.spring, -a.thickness), -f.u * sgn, -f.u * sgn, StrokeKind::Seam, 3);
    }
    // Colliders: piers, and horizontal slices of the masonry above the springing line.
    let axes = [f.u, f.v, f.n];
    let tc = -a.thickness * 0.5;
    let ht = a.thickness * 0.5;
    for sgn in [-1.0f32, 1.0] {
        let cu = sgn * (r + hw) * 0.5;
        b.collider(solid(f.at(cu, a.spring * 0.5, tc), axes, Vec3::new((hw - r) * 0.5, a.spring * 0.5, ht)));
    }
    let slices = 8;
    for k in 0..slices {
        let (y0, y1) = (r * k as f32 / slices as f32, r * (k + 1) as f32 / slices as f32);
        let x_in = (r * r - y0 * y0).max(0.0).sqrt();
        for sgn in [-1.0f32, 1.0] {
            let cu = sgn * (x_in + hw) * 0.5;
            b.collider(solid(f.at(cu, a.spring + (y0 + y1) * 0.5, tc), axes, Vec3::new((hw - x_in) * 0.5, (y1 - y0) * 0.5, ht)));
        }
    }
    b.collider(solid(f.at(0.0, a.spring + (r + top) * 0.5, tc), axes, Vec3::new(hw, (top - r) * 0.5, ht)));
    coarse
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(b: MeshBuilder) -> (crate::MeshData, Vec<Collider>) {
        let (m, c) = b.finish_with_colliders();
        m.check_invariants().unwrap();
        (m, c)
    }

    #[test]
    fn stair_risers_stay_in_limits() {
        for k in 1..4000 {
            let h = k as f32 * 0.037 + 0.75;
            let (n, rise) = stair_risers(h);
            assert!((STAIR_RISE_MIN - 1e-4..=STAIR_RISE_MAX + 1e-4).contains(&rise), "height {h}: {n} risers of {rise}");
            assert!((n as f32 * rise - h).abs() < 1e-3);
        }
    }

    #[test]
    fn stair_geometry_and_ramp() {
        let mut b = MeshBuilder::new();
        let s = Stair { foot: Vec3::new(1.0, 10.0, 2.0), dir: Vec3::Z, width: 1.4, height: 23.3, risers: None,
            max_risers: 40, landing: 1.5, waist: 0.3, wall: Some(Vec3::X), railing: true, enclosed: false };
        let info = stair(&mut b, &s);
        assert!((info.top.y - 33.3).abs() < 1e-3, "{info:?}");
        let (m, cols) = check(b);
        // Every up-facing tread is within one rise above some ramp's surface.
        let ramps: Vec<_> = cols.iter().filter_map(|c| if let Collider::Ramp { start, end, .. } = c { Some((Vec3::from(*start), Vec3::from(*end))) } else { None }).collect();
        assert_eq!(ramps.len(), info.risers.div_ceil(40));
        for (a, c) in &ramps {
            let slope = (c.y - a.y) / (*c - *a).with_y(0.0).length();
            assert!(slope.atan().to_degrees() < 40.0);
        }
        let mut treads = 0;
        for v in m.vertices.iter().filter(|v| v.normal[1] > 0.99 && v.position[0] > 0.29 && v.position[0] < 1.71) {
            let p = Vec3::from(v.position);
            let along = |q: Vec3| (q - s.foot).dot(Vec3::Z);
            let ok = ramps.iter().any(|(a, c)| {
                let (sa, sc) = (along(*a), along(*c));
                let s0 = along(p);
                if s0 < sa - 1e-3 || s0 > sc + 1e-3 {
                    return false;
                }
                let y = a.y + (c.y - a.y) * (s0 - sa) / (sc - sa);
                p.y >= y - 1e-3 && p.y <= y + info.rise + 1e-3
            });
            if !ok {
                // Landings are flat at a ramp's end height.
                assert!(ramps.iter().any(|(_, c)| (c.y - p.y).abs() < 1e-3), "tread vertex {p:?} not over a ramp");
            }
            treads += 1;
        }
        assert!(treads > 0);
    }

    #[test]
    fn wall_with_openings_is_valid_and_has_holes() {
        let mut b = MeshBuilder::new();
        let f = Frame::new(Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z);
        let openings = [Rect2::new(2.0, 3.2, 0.0, 2.2), Rect2::new(6.0, 9.0, 4.0, 7.0)];
        wall(&mut b, &Wall { frame: f, size: (12.0, 10.0), thickness: 1.0, seams_u: &[4.0, 8.0], seams_v: &[5.0], openings: &openings, hatch: Vec3::Y, step: 1.5, opening_rank: Some(2), seam_rank: 2 });
        let (m, cols) = check(b);
        // No triangle centroid inside an opening.
        for t in m.indices.chunks(3) {
            let c = t.iter().map(|&i| Vec3::from(m.vertices[i as usize].position)).sum::<Vec3>() / 3.0;
            assert!(!openings.iter().any(|o| o.contains(c.x, c.y)), "triangle in opening at {c:?}");
        }
        // Area adds up.
        let area: f32 = m.indices.chunks(3).map(|t| {
            let p: Vec<Vec3> = t.iter().map(|&i| Vec3::from(m.vertices[i as usize].position)).collect();
            (p[1] - p[0]).cross(p[2] - p[0]).length() * 0.5
        }).sum();
        assert!((area - (120.0 - 1.2 * 2.2 - 9.0)).abs() < 1e-2, "area {area}");
        // Colliders do not cover the openings.
        for o in &openings {
            let p = [(o.u0 + o.u1) * 0.5, (o.v0 + o.v1) * 0.5, -0.5];
            assert!(!cols.iter().any(|c| c.contains(p)));
        }
        assert!(cols.iter().any(|c| c.contains([0.5, 0.5, -0.5])));
    }

    #[test]
    fn every_element_is_valid() {
        let mut b = MeshBuilder::new();
        let f = Frame::new(Vec3::new(0.0, 0.0, 0.0), Vec3::X, Vec3::Y, Vec3::Z);
        let a = Arch { frame: f, w: 40.0, height: 34.0, span: 26.0, spring: 16.0, ring: 2.0, voussoirs: 17, thickness: 6.0, facets: 3 };
        arch(&mut b, &a);
        column_round(&mut b, Vec3::new(50.0, 0.0, 0.0), 1.5, 30.0, 20, 6.0, 0.4, 1.5);
        column_square(&mut b, Vec3::new(60.0, 0.0, 0.0), 2.0, 30.0, 5.0, 1.5);
        beam(&mut b, Vec3::new(0.0, 40.0, 0.0), Vec3::new(30.0, 50.0, 20.0), 2.0, 3.0, 1, 2.0);
        bridge(&mut b, &Bridge { a: Vec3::new(0.0, 20.0, 30.0), c: Vec3::new(60.0, 20.0, 30.0), width: 4.0, deck: 0.8, girder: Some((2.0, 2.0)), ribs: Some(5.0), railings: true, rank: 0 });
        let gf = Frame::new(Vec3::new(0.0, 0.0, 80.0), Vec3::X, Vec3::Y, -Vec3::Z);
        let door = |l: usize, k: usize| (l + k) % 2 == 0;
        gallery(&mut b, &Gallery { frame: gf, u0: 0.0, u1: 30.0, levels: &[10.0, 15.0, 20.0], storey: 5.0, depth: 4.0, slab: 0.6, projection: 0.8, bay: 6.0, piers: true, door: &door, gaps: &[(1, 10.0, 14.0)] });
        let (m, cols) = check(b);
        assert!(!cols.is_empty());
        assert!(m.strokes.iter().any(|s| s.rank == 0));
        for c in &cols {
            let (lo, hi) = c.bounds();
            assert!((0..3).all(|i| lo[i] < hi[i]), "{c:?}");
        }
    }
}
