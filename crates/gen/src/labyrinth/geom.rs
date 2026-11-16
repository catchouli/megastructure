//! The plan built from the kit of parts: every space, ledge, bridge, stair, arch and opening
//! emits faces, ranked strokes and colliders through [`MeshBuilder`].
//!
//! Spaces are cavities in solid mass. Each space draws only its own inward faces, inset from
//! its cells (`Room::inset`, `passage_inset`), so two neighbouring spaces have a wall of the
//! two insets between them; openings are cut through both faces and lined by a reveal.

use super::graph::*;
use super::grid::*;
use super::plan::*;
use super::*;
use crate::builder::{grid_lines, MeshBuilder};
use crate::kit::{self, Frame, Rect2, Stair, Wall};
use crate::mesh::StrokeKind;
use crate::seed::{layer, Rng};
use glam::Vec3;
use std::collections::BTreeMap;

pub fn build(p: &Plan, b: &mut MeshBuilder) {
    for ri in 0..p.rooms.len() {
        match p.rooms[ri].kind {
            SpaceKind::Void | SpaceKind::LightWell => void_room(p, b, ri),
            SpaceKind::GreatHall => hall(p, b, ri),
            SpaceKind::Undercroft => undercroft(p, b, ri),
            SpaceKind::StairTower => tower(p, b, ri),
            _ => small_room(p, b, ri),
        }
    }
    for pass in &p.passages {
        passage(p, b, pass);
    }
    for li in 0..p.ledges.len() {
        ledge(p, b, li);
    }
    bridges_and_stairs(p, b);
    arch(p, b);
    gallery(p, b);
    for o in &p.portals {
        portal_reveal(b, o);
    }
    skin(p, b);
}

fn rng(p: &Plan, space: usize, what: &str) -> Rng {
    Rng::for_id(p.seed, &format!("{}/{what}", p.graph.spaces[space].sid), layer::ELEMENTS)
}

/// Openings of the plan's portals lying in the given wall plane and range: (u0, u1, y0, y1).
fn holes(p: &Plan, x_plane: bool, plane: f32, u0: f32, u1: f32, y0: f32, y1: f32) -> Vec<[f32; 4]> {
    p.portals
        .iter()
        .filter(|o| o.x_plane == x_plane && ((o.p0 - plane).abs() < 2e-3 || (o.p1 - plane).abs() < 2e-3))
        .filter(|o| o.u1 > u0 + 1e-3 && o.u0 < u1 - 1e-3 && o.y1 > y0 + 1e-3 && o.y0 < y1 - 1e-3)
        .map(|o| [o.u0.max(u0), o.u1.min(u1), o.y0.max(y0), o.y1.min(y1)])
        .collect()
}

/// A vertical wall face in a plane of constant x (`x_plane`) or z, its normal `nsign` along
/// that axis, spanning world `u0..u1` along the other horizontal axis and `y0..y1`. Seams and
/// holes in world coordinates.
#[allow(clippy::too_many_arguments)]
fn vface(b: &mut MeshBuilder, x_plane: bool, plane: f32, nsign: f32, u0: f32, u1: f32, y0: f32, y1: f32, thick: f32, seams_u: &[f32], seams_v: &[f32], hs: &[[f32; 4]], step: f32, opening_rank: Option<u32>, seam_rank: u32) {
    if u1 - u0 < 0.02 || y1 - y0 < 0.02 {
        return;
    }
    let (origin, u, n) = if x_plane { (Vec3::new(plane, y0, u0), Vec3::Z, Vec3::X * nsign) } else { (Vec3::new(u0, y0, plane), Vec3::X, Vec3::Z * nsign) };
    let f = Frame::new(origin, u, Vec3::Y, n);
    let (lu, lv) = (u1 - u0, y1 - y0);
    let su: Vec<f32> = seams_u.iter().map(|s| s - u0).filter(|&s| s > 0.05 && s < lu - 0.05).collect();
    let sv: Vec<f32> = seams_v.iter().map(|s| s - y0).filter(|&s| s > 0.05 && s < lv - 0.05).collect();
    let rects: Vec<Rect2> = hs.iter().map(|h| Rect2::new(h[0] - u0, h[1] - u0, h[2] - y0, h[3] - y0)).filter(|r| r.u1 - r.u0 > 0.05 && r.v1 - r.v0 > 0.05).collect();
    kit::wall(b, &Wall { frame: f, size: (lu, lv), thickness: thick, seams_u: &su, seams_v: &sv, openings: &rects, hatch: Vec3::Y, step, opening_rank, seam_rank });
}

/// A horizontal face at `y` (facing up or down) over `x0..x1, z0..z1`, with seams and holes
/// (x0, x1, z0, z1) in world coordinates.
#[allow(clippy::too_many_arguments)]
fn hface(b: &mut MeshBuilder, y: f32, up: bool, x0: f32, x1: f32, z0: f32, z1: f32, thick: f32, seams_x: &[f32], seams_z: &[f32], hs: &[[f32; 4]], step: f32, seam_rank: u32, hatch: Vec3) {
    if x1 - x0 < 0.02 || z1 - z0 < 0.02 {
        return;
    }
    let n = if up { Vec3::Y } else { -Vec3::Y };
    let f = Frame::new(Vec3::new(x0, y, z0), Vec3::X, Vec3::Z, n);
    let (lu, lv) = (x1 - x0, z1 - z0);
    let su: Vec<f32> = seams_x.iter().map(|s| s - x0).filter(|&s| s > 0.05 && s < lu - 0.05).collect();
    let sv: Vec<f32> = seams_z.iter().map(|s| s - z0).filter(|&s| s > 0.05 && s < lv - 0.05).collect();
    let rects: Vec<Rect2> = hs.iter().map(|h| Rect2::new(h[0] - x0, h[1] - x0, h[2] - z0, h[3] - z0)).filter(|r| r.u1 - r.u0 > 0.05 && r.v1 - r.v0 > 0.05).collect();
    kit::wall(b, &Wall { frame: f, size: (lu, lv), thickness: thick, seams_u: &su, seams_v: &sv, openings: &rects, hatch, step, opening_rank: None, seam_rank });
}

/// A crease from `a` to `c` with gaps (distances along it).
fn crease_gapped(b: &mut MeshBuilder, a: Vec3, c: Vec3, n0: Vec3, n1: Vec3, rank: u32, gaps: &[(f32, f32)]) {
    let len = (c - a).length();
    if len < 0.05 {
        return;
    }
    let dir = (c - a) / len;
    let mut g: Vec<(f32, f32)> = gaps.to_vec();
    g.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let mut s = 0.0;
    for (g0, g1) in g {
        if g0 > s + 0.05 {
            b.stroke(a + dir * s, a + dir * g0.min(len), n0, n1, StrokeKind::Crease, rank);
        }
        s = s.max(g1);
    }
    if len > s + 0.05 {
        b.stroke(a + dir * s, c, n0, n1, StrokeKind::Crease, rank);
    }
}

fn seams_at(rng: &mut Rng, lo: f32, hi: f32, count: usize, snap: f32) -> Vec<f32> {
    let mut v: Vec<f32> = (0..count).map(|_| (rng.range(lo, hi) / snap).round() * snap).filter(|&s| s > lo + 0.5 && s < hi - 0.5).collect();
    v.sort_by(|a, c| a.partial_cmp(c).unwrap());
    v.dedup_by(|a, c| (*a - *c).abs() < 0.5);
    v
}

/// The cell boundaries strictly inside `lo..hi` along an axis, every `every` cells.
fn cell_lines(lo: f32, hi: f32, every: i32) -> Vec<f32> {
    let a = (lo / CELL).ceil() as i32;
    let c = (hi / CELL).floor() as i32;
    (a..=c).filter(|k| k % every == 0).map(|k| k as f32 * CELL).filter(|&x| x > lo + 0.3 && x < hi - 0.3).collect()
}

fn storey_lines(lo: f32, hi: f32, every: i32) -> Vec<f32> {
    let a = (lo / LEVEL).ceil() as i32;
    let c = (hi / LEVEL).floor() as i32;
    (a..=c).filter(|k| k % every == 0).map(|k| k as f32 * LEVEL).filter(|&y| y > lo + 0.3 && y < hi - 0.3).collect()
}

/// Which ranks a box room draws its edges at.
struct BoxStyle {
    vertical: u32,
    floor: u32,
    ceiling: u32,
    /// Seams on walls: vertical every this many cells, horizontal every this many storeys.
    wall_cells: i32,
    wall_storeys: i32,
    floor_cells: i32,
    step: f32,
    seam_rank: u32,
}

/// The six faces of a box room (no ceiling if open to the sky), its holes, seams and creases.
/// `extra` adds holes per side (the arch).
fn room_box(p: &Plan, b: &mut MeshBuilder, r: &Room, st: &BoxStyle, extra: &[(Dir, [f32; 4])]) {
    let lo = r.inner_min();
    let hi = r.inner_max();
    for d in Dir::ALL {
        let plane = r.face(d);
        let (u0, u1) = r.side_span(d);
        let mut hs = holes(p, d.is_x(), plane, u0, u1, lo.y, hi.y);
        hs.extend(extra.iter().filter(|e| e.0 == d).map(|e| e.1));
        let su = cell_lines(u0, u1, st.wall_cells);
        let sv = storey_lines(lo.y, hi.y, st.wall_storeys);
        vface(b, d.is_x(), plane, -d.sign(), u0, u1, lo.y, hi.y, r.inset, &su, &sv, &hs, st.step, Some(1), st.seam_rank);
        // Floor edge, broken where a door or arch meets the floor.
        let gaps: Vec<(f32, f32)> = hs.iter().filter(|h| (h[2] - lo.y).abs() < 0.05).map(|h| (h[0] - u0, h[1] - u0)).collect();
        let n_wall = -d.vec();
        let (a, c) = if d.is_x() { (Vec3::new(plane, lo.y, u0), Vec3::new(plane, lo.y, u1)) } else { (Vec3::new(u0, lo.y, plane), Vec3::new(u1, lo.y, plane)) };
        crease_gapped(b, a, c, n_wall, Vec3::Y, st.floor, &gaps);
        let top = Vec3::Y * (hi.y - lo.y);
        if r.open_top {
            b.stroke(a + top, c + top, n_wall, Vec3::Y, StrokeKind::Crease, 0);
        } else {
            b.stroke(a + top, c + top, n_wall, -Vec3::Y, StrokeKind::Crease, st.ceiling);
        }
    }
    for (x, z, nx, nz) in [(lo.x, lo.z, 1.0, 1.0), (hi.x, lo.z, -1.0, 1.0), (hi.x, hi.z, -1.0, -1.0), (lo.x, hi.z, 1.0, -1.0)] {
        b.stroke(Vec3::new(x, lo.y, z), Vec3::new(x, hi.y, z), Vec3::X * nx, Vec3::Z * nz, StrokeKind::Crease, st.vertical);
    }
    let sx = cell_lines(lo.x, hi.x, st.floor_cells);
    let sz = cell_lines(lo.z, hi.z, st.floor_cells);
    let fh = holes_h(p, lo.y, lo.x, hi.x, lo.z, hi.z);
    hface(b, lo.y, true, lo.x, hi.x, lo.z, hi.z, SLAB, &sx, &sz, &fh, st.step, 2, Vec3::X);
    if !r.open_top {
        hface(b, hi.y, false, lo.x, hi.x, lo.z, hi.z, SLAB, &sx, &sz, &[], st.step.max(2.0), 3, Vec3::X);
    }
}

/// Holes in a floor: none yet (kept for breakage later).
fn holes_h(_p: &Plan, _y: f32, _x0: f32, _x1: f32, _z0: f32, _z1: f32) -> Vec<[f32; 4]> {
    Vec::new()
}

fn void_room(p: &Plan, b: &mut MeshBuilder, ri: usize) {
    let r = &p.rooms[ri];
    let mut rng = rng(p, r.space, "geom");
    let lo = r.inner_min();
    let hi = r.inner_max();
    let mut extra = Vec::new();
    if p.arch.void == ri {
        let a = &p.arch;
        let y0 = p.rooms[a.hall].floor_y();
        extra.push((Dir::ZN, [a.x - a.w * 0.5, a.x + a.w * 0.5, y0, y0 + a.height]));
    }
    // Walls: few joints, irregular; string courses at a handful of storeys.
    for d in Dir::ALL {
        let plane = r.face(d);
        let (u0, u1) = r.side_span(d);
        let mut hs = holes(p, d.is_x(), plane, u0, u1, lo.y, hi.y);
        hs.extend(extra.iter().filter(|e| e.0 == d).map(|e| e.1));
        let su = seams_at(&mut rng, u0, u1, 2, CELL);
        let n_courses = rng.int(4, 7) as usize;
        let sv = seams_at(&mut rng, lo.y + 4.0, hi.y - 4.0, n_courses, 1.0);
        vface(b, d.is_x(), plane, -d.sign(), u0, u1, lo.y, hi.y, r.inset, &su, &sv, &hs, 2.0, Some(1), 2);
        let n_wall = -d.vec();
        let (a, c) = if d.is_x() { (Vec3::new(plane, lo.y, u0), Vec3::new(plane, lo.y, u1)) } else { (Vec3::new(u0, lo.y, plane), Vec3::new(u1, lo.y, plane)) };
        b.stroke(a, c, n_wall, Vec3::Y, StrokeKind::Crease, 1);
        let top = Vec3::Y * (hi.y - lo.y);
        b.stroke(a + top, c + top, n_wall, Vec3::Y, StrokeKind::Crease, 0);
    }
    for (x, z, nx, nz) in [(lo.x, lo.z, 1.0, 1.0), (hi.x, lo.z, -1.0, 1.0), (hi.x, hi.z, -1.0, -1.0), (lo.x, hi.z, 1.0, -1.0)] {
        b.stroke(Vec3::new(x, lo.y, z), Vec3::new(x, hi.y, z), Vec3::X * nx, Vec3::Z * nz, StrokeKind::Crease, 0);
    }
    let sx = seams_at(&mut rng, lo.x, hi.x, 3, 1.0);
    let sz = seams_at(&mut rng, lo.z, hi.z, 3, 1.0);
    hface(b, lo.y, true, lo.x, hi.x, lo.z, hi.z, 2.0, &sx, &sz, &[], 2.0, 2, Vec3::X);
    // Fallen blocks on the floor of the deep void.
    if r.kind == SpaceKind::Void {
        for _ in 0..6 {
            let (cx, cz) = (rng.range(lo.x + 4.0, hi.x - 4.0), rng.range(lo.z + 4.0, hi.z - 4.0));
            let (sx, sy, sz) = (rng.range(1.5, 4.5), rng.range(1.0, 3.5), rng.range(1.5, 4.5));
            kit::aa_block(b, Vec3::new(cx - sx, lo.y, cz - sz), Vec3::new(cx + sx, lo.y + sy, cz + sz), [true, true, false, true, true, true], Vec3::Y, 1.5, [Some(1), Some(1), Some(1)], true);
        }
    }
    // Beams across, at storeys clear of ledges and stairs, ends buried in the walls.
    let busy: Vec<i32> = p.ledges.iter().filter(|l| l.room == ri).flat_map(|l| [l.level - 1, l.level, l.level + 1, l.level + 2]).collect();
    let n_beams = if r.kind == SpaceKind::Void { 4 } else { 2 };
    let mut busy_rects: Vec<(Dir, [f32; 4])> = Vec::new();
    let mut placed = 0;
    for k in (r.b.y0 + 2..NY - 1).rev() {
        if placed >= n_beams {
            break;
        }
        if busy.contains(&k) || !rng.chance(0.35) {
            continue;
        }
        let y = k as f32 * LEVEL + 4.9;
        let along_x = rng.chance(0.5);
        let (w, dep) = (rng.range(1.4, 2.4), rng.range(1.8, 3.0));
        if along_x {
            let z = rng.range(lo.z + 4.0, hi.z - 4.0);
            kit::beam(b, Vec3::new(lo.x - 0.5, y, z), Vec3::new(hi.x + 0.5, y, z), w, dep, 0, 2.0);
            for d in [Dir::XN, Dir::XP] {
                busy_rects.push((d, [z - w, z + w, y - dep, y + dep]));
            }
        } else {
            let x = rng.range(lo.x + 4.0, hi.x - 4.0);
            if (x - p.arch.x).abs() < 6.0 {
                continue;
            }
            kit::beam(b, Vec3::new(x, y, lo.z - 0.5), Vec3::new(x, y, hi.z + 0.5), w, dep, 0, 2.0);
            for d in [Dir::ZN, Dir::ZP] {
                busy_rects.push((d, [x - w, x + w, y - dep, y + dep]));
            }
        }
        placed += 1;
    }
    wall_clutter(p, b, ri, &mut rng, &busy_rects);
}

/// What a void wall must keep clear: openings, ledges, bridges' ends, the wall stair, the
/// corners (ledges on the next wall), as rects (u0, u1, y0, y1) on each side.
fn void_wall_busy(p: &Plan, ri: usize) -> Vec<(Dir, [f32; 4])> {
    let r = &p.rooms[ri];
    let mut out = Vec::new();
    for d in Dir::ALL {
        let plane = r.face(d);
        let (u0, u1) = r.side_span(d);
        for h in holes(p, d.is_x(), plane, u0, u1, r.floor_y(), r.ceiling_y()) {
            out.push((d, [h[0] - 2.0, h[1] + 2.0, h[2] - 2.0, h[3] + 2.5]));
        }
        if p.arch.void == ri && d == Dir::ZN {
            let a = &p.arch;
            let y0 = p.rooms[a.hall].floor_y();
            out.push((d, [a.x - a.w * 0.5 - 2.0, a.x + a.w * 0.5 + 2.0, y0 - 3.0, y0 + a.height + 2.0]));
        }
        // Corners.
        out.push((d, [u0 - 1.0, u0 + 6.5, -1e4, 1e4]));
        out.push((d, [u1 - 6.5, u1 + 1.0, -1e4, 1e4]));
    }
    for l in p.ledges.iter().filter(|l| l.room == ri) {
        let y = l.level as f32 * LEVEL;
        out.push((l.side, [l.span.0 - 2.0, l.span.1 + 2.0, y - 4.5, y + 3.5]));
    }
    for ws in &p.wall_stairs {
        let l = &p.ledges[ws.lower];
        if l.room != ri {
            continue;
        }
        let y = l.level as f32 * LEVEL;
        let run = wall_stair_run(ws.levels);
        let (a, c) = if ws.dir > 0.0 { (ws.u0, ws.u0 + run) } else { (ws.u0 - run, ws.u0) };
        out.push((l.side, [a - 2.0, c + 2.0, y - 2.0, y + ws.levels as f32 * LEVEL + 3.5]));
    }
    out
}

fn overlaps(a: &[f32; 4], c: &[f32; 4]) -> bool {
    a[0] < c[1] && c[0] < a[1] && a[2] < c[3] && c[2] < a[3]
}

/// Buttress piers and accreted blocks on a void's walls, in the space left by everything
/// that must stay clear: the walls stop being blank planes.
fn wall_clutter(p: &Plan, b: &mut MeshBuilder, ri: usize, rng: &mut Rng, extra: &[(Dir, [f32; 4])]) {
    let r = &p.rooms[ri];
    let mut busy = void_wall_busy(p, ri);
    busy.extend_from_slice(extra);
    let (ylo, yhi) = (r.floor_y(), r.ceiling_y());
    let big = r.kind == SpaceKind::Void;
    for d in Dir::ALL {
        let plane = r.face(d);
        let (u0, u1) = r.side_span(d);
        let out = -d.vec();
        let along = d.along();
        let at = |u: f32, y: f32, o: f32| if d.is_x() { Vec3::new(plane + out.x * o, y, u) } else { Vec3::new(u, y, plane + out.z * o) };
        // Buttresses: piers in the free vertical stretches at a few positions.
        let n_b = if big { rng.int(1, 3) } else { rng.int(0, 1) };
        for _ in 0..n_b {
            let u = rng.range(u0 + 7.0, u1 - 7.0);
            let (hw, dep) = (rng.range(0.9, 1.6), rng.range(0.8, 1.6));
            let mut cuts: Vec<(f32, f32)> = busy.iter().filter(|(s, q)| *s == d && q[0] < u + hw && u - hw < q[1]).map(|(_, q)| (q[2], q[3])).collect();
            cuts.sort_by(|a, c| a.0.partial_cmp(&c.0).unwrap());
            let mut y = ylo;
            let mut spans = Vec::new();
            for (c0, c1) in cuts {
                if c0 > y {
                    spans.push((y, c0));
                }
                y = y.max(c1);
            }
            if yhi - 2.0 > y {
                spans.push((y, yhi - 2.0));
            }
            for (a, c) in spans {
                if c - a < 9.0 {
                    continue;
                }
                let centre = at(u, (a + c) * 0.5, dep * 0.5);
                kit::block(b, centre, [along, Vec3::Y, out], Vec3::new(hw, (c - a) * 0.5, dep * 0.5), [true, true, a > ylo + 0.1, true, false, true], Vec3::Y, 2.0, [Some(1), Some(1), Some(2)], true);
                busy.push((d, [u - hw - 0.5, u + hw + 0.5, a, c]));
            }
        }
        // Accretions: blocks of very different sizes, some with a smaller block stacked on them
        // and dark slots in their faces.
        let tries = if big { 70 } else { 30 };
        let want = if big { rng.int(6, 11) } else { rng.int(2, 4) };
        let mut placed = 0;
        for _ in 0..tries {
            if placed >= want {
                break;
            }
            let wmax = if rng.chance(0.3) { 14.0 } else { 7.0 };
            let w = rng.range(2.5, wmax);
            let hmax = if rng.chance(0.3) { 18.0 } else { 7.0 };
            let h = rng.range(2.5, hmax);
            let dep = rng.range(1.2, 4.5);
            let u = rng.range(u0 + w * 0.5, u1 - w * 0.5);
            let y = rng.range(ylo + 4.0, yhi - h - 6.0);
            let rect = [u - w * 0.5 - 1.0, u + w * 0.5 + 1.0, y - 1.0, y + h + 1.0];
            if busy.iter().any(|(s, q)| *s == d && overlaps(q, &rect)) {
                continue;
            }
            busy.push((d, rect));
            placed += 1;
            let centre = at(u, y + h * 0.5, dep * 0.5);
            kit::block(b, centre, [along, Vec3::Y, out], Vec3::new(w * 0.5, h * 0.5, dep * 0.5), [true, true, true, true, false, true], Vec3::Y, 1.5, [Some(1), Some(1), Some(2)], true);
            // Joints across its face.
            let rows = (h / 2.2).floor() as usize;
            for k in 1..rows {
                if rng.chance(0.5) {
                    let yy = y + h * k as f32 / rows as f32;
                    b.stroke(at(u - w * 0.5, yy, dep + 0.002), at(u + w * 0.5, yy, dep + 0.002), out, out, StrokeKind::Seam, 2);
                }
            }
            // A dark slot (a shallow recess) in a big face.
            if w > 4.0 && h > 3.5 && rng.chance(0.5) {
                let sw = rng.range(1.0, (w - 2.0).min(4.0));
                let sh = rng.range(0.6, 1.4);
                let f = Frame::new(at(u - sw * 0.5, y + h * 0.5 - sh * 0.5, dep + 0.001), along, Vec3::Y, out);
                let rc = Rect2::new(0.0, sw, 0.0, sh);
                kit::recess(b, f, rc, 0.6_f32.min(dep - 0.3), 1.0, Some(2));
                // The front face behind the slot is hidden by the recess back; draw its outline.
                for (a0, b0, a1, b1) in [(0.0, 0.0, sw, 0.0), (0.0, sh, sw, sh), (0.0, 0.0, 0.0, sh), (sw, 0.0, sw, sh)] {
                    b.stroke(f.at(a0, b0, 0.0), f.at(a1, b1, 0.0), out, out, StrokeKind::Crease, 2);
                }
            }
            // A smaller block on top, set back.
            if rng.chance(0.4) {
                let (w2, h2, d2) = (w * rng.range(0.3, 0.7), rng.range(1.0, 3.5), dep * rng.range(0.4, 0.8));
                let u2 = u + rng.range(-0.5, 0.5) * (w - w2);
                if !busy.iter().any(|(s, q)| *s == d && overlaps(q, &[u2 - w2 * 0.5, u2 + w2 * 0.5, y + h + 0.2, y + h + h2 + 0.5]) && !overlaps(q, &rect)) {
                    let c2 = at(u2, y + h + h2 * 0.5, d2 * 0.5);
                    kit::block(b, c2, [along, Vec3::Y, out], Vec3::new(w2 * 0.5, h2 * 0.5, d2 * 0.5), [true, true, false, true, false, true], Vec3::Y, 1.5, [Some(1), Some(2), Some(2)], true);
                    busy.push((d, [u2 - w2 * 0.5 - 1.0, u2 + w2 * 0.5 + 1.0, y + h, y + h + h2 + 1.0]));
                }
            }
        }
    }
}

fn hall(p: &Plan, b: &mut MeshBuilder, ri: usize) {
    let r = &p.rooms[ri];
    let mut rng = rng(p, r.space, "geom");
    let a = &p.arch;
    let extra = [(Dir::ZP, [a.x - a.w * 0.5, a.x + a.w * 0.5, r.floor_y(), r.floor_y() + a.height])];
    room_box(p, b, r, &BoxStyle { vertical: 0, floor: 1, ceiling: 0, wall_cells: 2, wall_storeys: 2, floor_cells: 2, step: 2.0, seam_rank: 2 }, &extra);
    let lo = r.inner_min();
    let hi = r.inner_max();
    let w = hi.x - lo.x;
    let h = hi.y - lo.y;
    // Two rows of giant columns from the arch end, one fallen to a stump; beams along each
    // row and across the hall at every column pair.
    let rows = [lo.x + w * 0.27, hi.x - w * 0.27];
    let mut zs = Vec::new();
    let mut z = hi.z - 12.0;
    while z > lo.z + 10.0 {
        zs.push(z);
        z -= 16.0;
    }
    let stump = (rng.int(0, 1) as usize, rng.int(1, zs.len().max(2) as i32 - 1) as usize);
    let beam_y = hi.y - 1.6;
    for (ri2, &cx) in rows.iter().enumerate() {
        for (k, &cz) in zs.iter().enumerate() {
            if (ri2, k) == stump {
                kit::column_round(b, Vec3::new(cx, lo.y, cz), 1.9, rng.range(7.0, 11.0), 24, 3.2, 0.45, 1.5);
                // Its drums lie on the floor.
                for d in 0..3 {
                    let c = Vec3::new(cx + rng.range(-6.0, 6.0), lo.y, cz + 4.0 + d as f32 * 3.4);
                    kit::aa_block(b, c - Vec3::new(1.5, 0.0, 1.3), c + Vec3::new(1.5, 2.6, 1.3), [true, true, false, true, true, true], Vec3::Y, 1.5, [Some(1), Some(2), Some(2)], true);
                }
                continue;
            }
            kit::column_round(b, Vec3::new(cx, lo.y, cz), 1.9, h - 3.2, 24, 6.0, 0.45, 1.5);
        }
        kit::beam(b, Vec3::new(cx, beam_y, lo.z), Vec3::new(cx, beam_y, hi.z), 3.0, 3.2, 1, 2.0);
    }
    for &cz in &zs {
        kit::beam(b, Vec3::new(lo.x, beam_y - 0.1, cz), Vec3::new(hi.x, beam_y - 0.1, cz), 2.2, 3.0, 1, 2.0);
    }
    // Pilasters on the long wall opposite the gallery, between doors.
    let side = p.gallery.side.opposite();
    let face = r.face(side);
    let out = -side.vec();
    for &cz in &zs {
        let busy = p.portals.iter().any(|o| o.x_plane && (o.p0 - face).abs() < 2e-3 || (o.p1 - face).abs() < 2e-3 && o.u1 > cz - 2.5 && o.u0 < cz + 2.5);
        if busy {
            continue;
        }
        let c = Vec3::new(face + out.x * 0.6, lo.y + h * 0.5, cz);
        kit::block(b, c, [Vec3::X, Vec3::Y, Vec3::Z], Vec3::new(0.6, h * 0.5, 1.2), [side == Dir::XP, side == Dir::XN, false, false, true, true], Vec3::Y, 2.0, [None, Some(1), None], true);
    }
}

fn gallery(p: &Plan, b: &mut MeshBuilder) {
    let g = &p.gallery;
    let r = &p.rooms[g.hall];
    let face = r.face(g.side);
    let out = -g.side.vec();
    let y = g.level as f32 * LEVEL;
    let len = g.z1 - g.z0;
    let c = Vec3::new(face + out.x * g.depth * 0.5, y - 0.5, (g.z0 + g.z1) * 0.5);
    // Axes: z, y, out. Hide the end against the arch wall and the side against the wall.
    kit::block(b, c, [Vec3::Z, Vec3::Y, out], Vec3::new(len * 0.5, 0.5, g.depth * 0.5), [true, false, true, true, false, true], Vec3::Z, 1.5, [Some(1), Some(2), Some(2)], true);
    let front = face + out.x * (g.depth - 0.1);
    kit::railing(b, Vec3::new(front, y, g.z0 + 0.08), Vec3::new(front, y, g.z1), out, 1.5, 1);
    kit::railing(b, Vec3::new(face + out.x * 2.7, y, g.z0 + 0.08), Vec3::new(front, y, g.z0 + 0.08), -Vec3::Z, 1.5, 1);
    let mut z = g.z0 + 2.5;
    while z < g.z1 - 1.0 {
        kit::beam(b, Vec3::new(face, y - 1.0 - 3.0, z), Vec3::new(face + out.x * (g.depth - 0.6), y - 1.0, z), 0.6, 0.7, 2, 1.5);
        z += 5.0;
    }
    let lo = r.floor_y();
    kit::stair(b, &Stair {
        foot: Vec3::new(face + out.x * 1.35, lo, g.stair_foot),
        dir: Vec3::Z,
        width: 2.5,
        height: y - lo,
        risers: Some(3 * RISERS_PER_LEVEL),
        max_risers: 27,
        landing: 2.0,
        waist: 0.4,
        wall: Some(-out),
        railing: true,
        enclosed: false,
    });
}

fn undercroft(p: &Plan, b: &mut MeshBuilder, ri: usize) {
    let r = &p.rooms[ri];
    let mut rng = rng(p, r.space, "geom");
    room_box(p, b, r, &BoxStyle { vertical: 1, floor: 1, ceiling: 1, wall_cells: 2, wall_storeys: 1, floor_cells: 2, step: 2.0, seam_rank: 2 }, &[]);
    let lo = r.inner_min();
    let hi = r.inner_max();
    let h = hi.y - lo.y;
    let xs: Vec<f32> = ((r.b.x0 + 2)..r.b.x1 - 1).step_by(2).map(|x| x as f32 * CELL).collect();
    let zs: Vec<f32> = ((r.b.z0 + 2)..r.b.z1 - 1).step_by(2).map(|z| z as f32 * CELL).collect();
    for &x in &xs {
        for &z in &zs {
            kit::column_square(b, Vec3::new(x, lo.y, z), 1.4, h, 2.8, 1.5);
        }
    }
    for &z in &zs {
        kit::beam(b, Vec3::new(lo.x, hi.y - 0.7, z), Vec3::new(hi.x, hi.y - 0.7, z), 1.4, 1.4, 2, 2.0);
    }
    for _ in 0..4 {
        let (cx, cz) = (rng.range(lo.x + 3.0, hi.x - 3.0), rng.range(lo.z + 3.0, hi.z - 3.0));
        let (sx, sy, sz) = (rng.range(0.8, 1.6), rng.range(0.6, 1.4), rng.range(0.8, 1.6));
        if xs.iter().any(|&x| (x - cx).abs() < sx + 1.0) && zs.iter().any(|&z| (z - cz).abs() < sz + 1.0) {
            continue;
        }
        kit::aa_block(b, Vec3::new(cx - sx, lo.y, cz - sz), Vec3::new(cx + sx, lo.y + sy, cz + sz), [true, true, false, true, true, true], Vec3::Y, 1.5, [Some(1), Some(2), Some(2)], true);
    }
}

fn small_room(p: &Plan, b: &mut MeshBuilder, ri: usize) {
    let r = &p.rooms[ri];
    room_box(p, b, r, &BoxStyle { vertical: 1, floor: 1, ceiling: 1, wall_cells: 1, wall_storeys: 1, floor_cells: 1, step: 1.5, seam_rank: 2 }, &[]);
    let lo = r.inner_min();
    let hi = r.inner_max();
    if r.kind == SpaceKind::Junction && r.b.width() >= 3 && r.b.depth() >= 3 {
        let c = (lo + hi) * 0.5;
        kit::column_square(b, Vec3::new(c.x, lo.y, c.z), 1.8, hi.y - lo.y, 3.0, 1.5);
    }
    if r.kind == SpaceKind::Loggia {
        // A railing across each opening of the colonnade, at the outer edge; one bay left open.
        let mut rng = rng(p, r.space, "geom");
        let gaps: Vec<&Portal> = p.portals.iter().filter(|o| o.kind == PortalKind::Exterior && o.walk && o.between.contains(&Some(r.space))).collect();
        let skip = rng.int(0, gaps.len() as i32 - 1) as usize;
        for (k, o) in gaps.iter().enumerate() {
            if k == skip && gaps.len() > 2 {
                continue;
            }
            let outer = if o.between[0] == Some(r.space) { o.p1 - 0.15 } else { o.p0 + 0.15 };
            let (a, c, out) = if o.x_plane {
                (Vec3::new(outer, o.y0, o.u0), Vec3::new(outer, o.y0, o.u1), Vec3::X * if o.between[0] == Some(r.space) { 1.0 } else { -1.0 })
            } else {
                (Vec3::new(o.u0, o.y0, outer), Vec3::new(o.u1, o.y0, outer), Vec3::Z * if o.between[0] == Some(r.space) { 1.0 } else { -1.0 })
            };
            kit::railing(b, a, c, out, 1.3, 1);
        }
    }
}

/// The stair tower: flights turning round an open well, 3 m a side, a landing in every
/// corner, so every storey has a landing in corner 0 or corner 2 where doors open.
fn tower(p: &Plan, b: &mut MeshBuilder, ri: usize) {
    let r = &p.rooms[ri];
    room_box(p, b, r, &BoxStyle { vertical: 1, floor: 1, ceiling: 1, wall_cells: 3, wall_storeys: 1, floor_cells: 3, step: 2.0, seam_rank: 2 }, &[]);
    let lo = r.inner_min();
    let hi = r.inner_max();
    let w = 2.2;
    let l = hi.x - lo.x;
    let ext = l - 2.0 * w - 18.0 * kit::STAIR_RUN;
    let rise = LEVEL * 0.5;
    let sides = 2 * (r.b.y1 - 1 - r.b.y0) as usize;
    let starts = [Vec3::new(lo.x + w, 0.0, lo.z + w * 0.5), Vec3::new(hi.x - w * 0.5, 0.0, lo.z + w), Vec3::new(hi.x - w, 0.0, hi.z - w * 0.5), Vec3::new(lo.x + w * 0.5, 0.0, hi.z - w)];
    let dirs = [Vec3::X, Vec3::Z, -Vec3::X, -Vec3::Z];
    let walls = [-Vec3::Z, Vec3::X, Vec3::Z, -Vec3::X];
    let corners = [(lo.x, lo.z), (hi.x - w, lo.z), (hi.x - w, hi.z - w), (lo.x, hi.z - w)];
    for s in 0..sides {
        let c = s % 4;
        let y = lo.y + s as f32 * rise;
        let start = starts[c].with_y(y);
        let (dir, wall) = (dirs[c], walls[c]);
        let well = -wall;
        if s > 0 {
            // The extension along the wall from the corner landing to the flight.
            let centre = start + dir * (ext * 0.5) - Vec3::Y * 0.25;
            kit::block(b, centre, [dir, Vec3::Y, well], Vec3::new(ext * 0.5, 0.25, w * 0.5), [false, false, true, true, false, true], dir, 1.5, [Some(2), None, Some(3)], true);
        }
        let e = well * (w * 0.5 - 0.06);
        kit::railing(b, start + e, start + dir * ext + e, well, 1.2, 1);
        kit::stair(b, &Stair { foot: start + dir * ext, dir, width: w, height: rise, risers: Some(RISERS_PER_LEVEL / 2), max_risers: 18, landing: 1.0, waist: 0.3, wall: Some(wall), railing: true, enclosed: false });
        // Landing in the next corner.
        let cn = (c + 1) % 4;
        let (x0, z0) = corners[cn];
        let top = y + rise;
        kit::aa_block(b, Vec3::new(x0, top - 0.5, z0), Vec3::new(x0 + w, top, z0 + w), [true, true, true, true, true, true], Vec3::X, 1.5, [Some(2), None, Some(2)], true);
        if s + 1 == sides {
            // The top landing: rail its open edge towards the next (absent) flight.
            let st = starts[cn].with_y(top);
            let side = dirs[cn].cross(Vec3::Y).normalize();
            kit::railing(b, st + side * (w * 0.5), st - side * (w * 0.5), dirs[cn], 1.2, 1);
        }
    }
}

/// One piece of a passage: a column of the grid with a floor (or a stair) and a ceiling.
#[derive(Clone, Copy, Debug)]
struct Piece {
    x: i32,
    z: i32,
    y0: f32,
    y1: f32,
    flat: bool,
    /// Per side, the neighbouring piece's floor and ceiling if the passage continues there.
    conn: [Option<(f32, f32)>; 4],
}

fn pieces(pass: &Passage) -> Vec<Piece> {
    let h = pass.height();
    let mut out: Vec<Piece> = Vec::new();
    let push = |out: &mut Vec<Piece>, p: Piece| {
        if let Some(prev) = out.last().copied() {
            let d = Dir::ALL.into_iter().find(|d| {
                let (dx, dz) = d.d();
                prev.x + dx == p.x && prev.z + dz == p.z
            });
            let d = d.expect("passage pieces adjacent");
            let n = out.len() - 1;
            out[n].conn[d.index()] = Some((p.y0, p.y1));
            let mut p = p;
            p.conn[d.opposite().index()] = Some((prev.y0, prev.y1));
            out.push(p);
        } else {
            out.push(p);
        }
    };
    for s in &pass.path {
        match *s {
            Step::Flat(c) => {
                let y0 = c.y as f32 * LEVEL;
                push(&mut out, Piece { x: c.x, z: c.z, y0, y1: y0 + h, flat: true, conn: [None; 4] });
            }
            Step::Stair { from, dir, up } => {
                let lo = if up { from.y } else { from.y - 1 };
                for k in 1..=3 {
                    let c = from.step(dir, k);
                    push(&mut out, Piece { x: c.x, z: c.z, y0: lo as f32 * LEVEL, y1: (lo + 1) as f32 * LEVEL + h, flat: false, conn: [None; 4] });
                }
            }
        }
    }
    out
}

#[derive(Clone, Copy, Debug)]
struct WallPart {
    x_plane: bool,
    plane: f32,
    nsign: f32,
    u0: f32,
    u1: f32,
    y0: f32,
    y1: f32,
    thick: f32,
    /// Draw the floor line and ceiling line.
    bottom: bool,
    top: bool,
}

fn passage(p: &Plan, b: &mut MeshBuilder, pass: &Passage) {
    let t = pass.inset();
    let service = pass.kind == SpaceKind::ServicePassage;
    let (rank_line, rank_corner) = if service { (2, 2) } else { (1, 1) };
    let ps = pieces(pass);
    let mut parts: Vec<WallPart> = Vec::new();
    let floor_coarse = b.new_coarse();
    let mut rng = rng(p, pass.space, "geom");
    let dado = if service { None } else { Some(rng.range(1.0, 1.6)) };
    for pc in &ps {
        let xs = [pc.x as f32 * CELL, pc.x as f32 * CELL + t, (pc.x + 1) as f32 * CELL - t, (pc.x + 1) as f32 * CELL];
        let zs = [pc.z as f32 * CELL, pc.z as f32 * CELL + t, (pc.z + 1) as f32 * CELL - t, (pc.z + 1) as f32 * CELL];
        let mut open = [[false; 3]; 3];
        open[1][1] = true;
        open[0][1] = pc.conn[Dir::XN.index()].is_some();
        open[2][1] = pc.conn[Dir::XP.index()].is_some();
        open[1][0] = pc.conn[Dir::ZN.index()].is_some();
        open[1][2] = pc.conn[Dir::ZP.index()].is_some();
        for i in 0..3 {
            for j in 0..3 {
                if !open[i][j] {
                    continue;
                }
                for d in Dir::ALL {
                    let (dx, dz) = d.d();
                    let (ni, nj) = (i as i32 + dx, j as i32 + dz);
                    let x_plane = d.is_x();
                    let plane = match d {
                        Dir::XN => xs[i],
                        Dir::XP => xs[i + 1],
                        Dir::ZN => zs[j],
                        Dir::ZP => zs[j + 1],
                    };
                    let (u0, u1) = if x_plane { (zs[j], zs[j + 1]) } else { (xs[i], xs[i + 1]) };
                    let nsign = -d.sign();
                    if (0..3).contains(&ni) && (0..3).contains(&nj) {
                        if !open[ni as usize][nj as usize] {
                            parts.push(WallPart { x_plane, plane, nsign, u0, u1, y0: pc.y0, y1: pc.y1, thick: t, bottom: pc.flat, top: true });
                        }
                    } else if let Some((ny0, ny1)) = pc.conn[d.index()] {
                        if ny0 > pc.y0 + 1e-3 {
                            parts.push(WallPart { x_plane, plane, nsign, u0, u1, y0: pc.y0, y1: ny0.min(pc.y1), thick: 0.3, bottom: false, top: false });
                        }
                        if ny1 < pc.y1 - 1e-3 {
                            parts.push(WallPart { x_plane, plane, nsign, u0, u1, y0: ny1.max(pc.y0), y1: pc.y1, thick: 0.3, bottom: true, top: true });
                        }
                    }
                }
            }
        }
        // Floor (flat pieces) and ceiling, over the open sub-squares, one panel each.
        for (y, up) in [(pc.y0, true), (pc.y1, false)] {
            if up && !pc.flat {
                continue;
            }
            let fine = b.panel();
            for i in 0..3 {
                for j in 0..3 {
                    if !open[i][j] {
                        continue;
                    }
                    let (x0, x1, z0, z1) = (xs[i], xs[i + 1], zs[j], zs[j + 1]);
                    let us = grid_lines(x1 - x0, 2.0, &[]);
                    let vs = grid_lines(z1 - z0, 2.0, &[]);
                    let n = if up { Vec3::Y } else { -Vec3::Y };
                    b.grid_face(Vec3::new(x0, y, z0), Vec3::X, Vec3::Z, &us, &vs, n, Vec3::X, fine, floor_coarse);
                }
            }
        }
        let (cx0, cx1, cz0, cz1) = (xs[0], xs[3], zs[0], zs[3]);
        if pc.flat {
            b.collider(crate::mesh::Collider::Aabb { min: [cx0, pc.y0 - SLAB, cz0], max: [cx1, pc.y0, cz1] });
        }
        b.collider(crate::mesh::Collider::Aabb { min: [cx0, pc.y1, cz0], max: [cx1, pc.y1 + 0.4, cz1] });
        // Floor joint across the passage at each boundary it continues through.
        for d in [Dir::XP, Dir::ZP] {
            if pc.flat && pc.conn[d.index()].is_some_and(|c| (c.0 - pc.y0).abs() < 1e-3) {
                let (a, c) = if d.is_x() { (Vec3::new(xs[3], pc.y0, zs[1]), Vec3::new(xs[3], pc.y0, zs[2])) } else { (Vec3::new(xs[1], pc.y0, zs[3]), Vec3::new(xs[2], pc.y0, zs[3])) };
                b.stroke(a, c, Vec3::Y, Vec3::Y, StrokeKind::Seam, 3);
            }
        }
    }
    // Merge collinear parts along the passage, then emit them as walls.
    let key = |w: &WallPart| (w.x_plane, (w.plane * 1000.0).round() as i64, w.nsign > 0.0, (w.y0 * 1000.0).round() as i64, (w.y1 * 1000.0).round() as i64, (w.thick * 1000.0).round() as i64, w.bottom, w.top);
    type Key = (bool, i64, bool, i64, i64, i64, bool, bool);
    let mut groups: BTreeMap<Key, Vec<WallPart>> = BTreeMap::new();
    for w in parts {
        groups.entry(key(&w)).or_default().push(w);
    }
    let mut verticals: Vec<(i64, i64, i64, i64, i64)> = Vec::new();
    for (_, mut v) in groups {
        v.sort_by(|a, c| a.u0.partial_cmp(&c.u0).unwrap());
        let mut merged: Vec<WallPart> = Vec::new();
        for w in v {
            if let Some(m) = merged.last_mut() {
                if (w.u0 - m.u1).abs() < 1e-3 {
                    m.u1 = w.u1;
                    continue;
                }
            }
            merged.push(w);
        }
        for w in merged {
            let mut hs = holes(p, w.x_plane, w.plane, w.u0, w.u1, w.y0, w.y1);
            // Closed doors along the walls: shallow door-sized recesses, the human scale.
            let mut niches = Vec::new();
            if w.bottom && w.top && (w.thick - t).abs() < 1e-3 && w.u1 - w.u0 >= CELL - 2.0 * t - 0.01 {
                let mut u = (w.u0 / CELL).floor() * CELL + CELL * 0.5;
                while u < w.u1 {
                    let r = [u - 0.8, u + 0.8, w.y0, w.y0 + kit::DOOR_HEIGHT + 0.2];
                    if r[0] > w.u0 + 0.3 && r[1] < w.u1 - 0.3 && !hs.iter().any(|h| overlaps(h, &[r[0] - 0.6, r[1] + 0.6, r[2], r[3]])) && rng.chance(0.14) {
                        let dw = if service { kit::DOOR_WIDTH } else { rng.range(1.2, 2.0) };
                        let dh = if service { kit::DOOR_HEIGHT } else { rng.range(2.2, 3.4) };
                        niches.push([u - dw * 0.5, u + dw * 0.5, w.y0, w.y0 + dh]);
                    }
                    u += CELL;
                }
            }
            hs.extend(niches.iter().copied());
            let su = cell_lines(w.u0, w.u1, if service { 1 } else { 2 });
            let mut sv = Vec::new();
            if let (Some(dd), true) = (dado, w.bottom) {
                sv.push(w.y0 + dd);
            }
            vface(b, w.x_plane, w.plane, w.nsign, w.u0, w.u1, w.y0, w.y1, w.thick, &su, &sv, &hs, 2.0, Some(2), 3);
            let n = if w.x_plane { Vec3::X * w.nsign } else { Vec3::Z * w.nsign };
            for h in &niches {
                let origin = if w.x_plane { Vec3::new(w.plane, 0.0, 0.0) } else { Vec3::new(0.0, 0.0, w.plane) };
                let f = Frame::new(origin, if w.x_plane { Vec3::Z } else { Vec3::X }, Vec3::Y, n);
                kit::recess(b, f, Rect2::new(h[0], h[1], h[2], h[3]), (t - 0.08).min(0.35), 1.0, None);
            }
            let at = |u: f32, y: f32| if w.x_plane { Vec3::new(w.plane, y, u) } else { Vec3::new(u, y, w.plane) };
            if w.bottom {
                let gaps: Vec<(f32, f32)> = hs.iter().filter(|h| (h[2] - w.y0).abs() < 0.05).map(|h| (h[0] - w.u0, h[1] - w.u0)).collect();
                crease_gapped(b, at(w.u0, w.y0), at(w.u1, w.y0), n, Vec3::Y, rank_line, &gaps);
            }
            if w.top {
                b.stroke(at(w.u0, w.y1), at(w.u1, w.y1), n, -Vec3::Y, StrokeKind::Crease, rank_line);
            }
            for u in [w.u0, w.u1] {
                let k = |v: f32| (v * 100.0).round() as i64;
                let p0 = at(u, w.y0);
                let key = (k(p0.x), k(p0.z), k(w.y0), k(w.y1), 0);
                if !verticals.contains(&key) {
                    verticals.push(key);
                    b.stroke(p0, at(u, w.y1), n, if w.x_plane { Vec3::Z } else { Vec3::X }, StrokeKind::Crease, rank_corner);
                }
            }
        }
    }
    // Stairs.
    let w = CELL - 2.0 * t;
    for s in &pass.path {
        if let Step::Stair { from, dir, up } = *s {
            let lo = if up { from.y } else { from.y - 1 };
            let c = from.floor_centre();
            let (foot, d) = if up { (c + dir.vec() * (CELL * 0.5), dir.vec()) } else { (c + dir.vec() * (CELL * 3.5), -dir.vec()) };
            kit::stair(b, &Stair { foot: foot.with_y(lo as f32 * LEVEL), dir: d, width: w, height: LEVEL, risers: Some(RISERS_PER_LEVEL), max_risers: 18, landing: 1.2, waist: 0.3, wall: None, railing: false, enclosed: true });
        }
    }
    // Frames across tall corridors at every other cell boundary on straight runs.
    if !service {
        for (k, pc) in ps.iter().enumerate() {
            if !pc.flat || k % 2 == 1 {
                continue;
            }
            for d in [Dir::XP, Dir::ZP] {
                let Some((ny0, _)) = pc.conn[d.index()] else { continue };
                let perp: [Dir; 2] = if d.is_x() { [Dir::ZN, Dir::ZP] } else { [Dir::XN, Dir::XP] };
                if (ny0 - pc.y0).abs() > 1e-3 || perp.iter().any(|q| pc.conn[q.index()].is_some()) {
                    continue;
                }
                let next = ps.get(k + 1);
                if next.is_none_or(|n| !n.flat || perp.iter().any(|q| n.conn[q.index()].is_some())) {
                    continue;
                }
                let along = d.vec();
                let across = along.cross(Vec3::Y).normalize();
                let centre = Vec3::new((pc.x as f32 + 0.5) * CELL, pc.y0, (pc.z as f32 + 0.5) * CELL) + along * (CELL * 0.5);
                let half_w = CELL * 0.5 - t;
                let hh = pc.y1 - pc.y0;
                for sg in [-1.0f32, 1.0] {
                    let c = centre + across * (sg * (half_w - 0.15)) + Vec3::Y * (hh * 0.5);
                    kit::block(b, c, [along, Vec3::Y, across], Vec3::new(0.3, hh * 0.5, 0.15), [true, true, false, false, sg > 0.0, sg < 0.0], Vec3::Y, 1.5, [None, Some(2), None], true);
                }
                let c = centre + Vec3::Y * (hh - 0.3);
                kit::block(b, c, [along, Vec3::Y, across], Vec3::new(0.3, 0.3, half_w - 0.3), [true, true, true, false, false, false], across, 1.5, [Some(2), None, Some(2)], false);
            }
        }
    }
}

fn wall_point(side: Dir, face: f32, out: f32, u: f32, y: f32) -> Vec3 {
    let o = -side.sign() * out;
    if side.is_x() {
        Vec3::new(face + o, y, u)
    } else {
        Vec3::new(u, y, face + o)
    }
}

fn ledge(p: &Plan, b: &mut MeshBuilder, li: usize) {
    let l = &p.ledges[li];
    let r = &p.rooms[l.room];
    let face = r.face(l.side);
    let out = -l.side.vec();
    let along = l.side.along();
    let y = l.level as f32 * LEVEL;
    let (u0, u1) = l.span;
    let len = u1 - u0;
    if len < 0.5 {
        return;
    }
    let centre = wall_point(l.side, face, l.depth * 0.5, (u0 + u1) * 0.5, y - 0.45);
    kit::block(b, centre, [along, Vec3::Y, out], Vec3::new(len * 0.5, 0.45, l.depth * 0.5), [true, true, true, true, false, true], along, 1.5, [Some(1), Some(2), Some(2)], true);
    let mut rng = Rng::for_id(p.seed, &format!("{}/geom", p.graph.spaces[l.space].sid), layer::ELEMENTS);
    let fr = l.depth - 0.1;
    let mut gaps: Vec<(f32, f32)> = l.rail_gaps.iter().map(|&(a, c)| (a - u0, c - u0)).collect();
    if l.rail == Rail::Broken {
        let g0 = rng.range(0.2, 0.6) * len;
        gaps.push((g0, g0 + rng.range(3.0, 6.0)));
    }
    if l.rail != Rail::None {
        kit::railing_with_gaps(b, wall_point(l.side, face, fr, u0, y), wall_point(l.side, face, fr, u1, y), out, 1.5, 1, &gaps);
        for (k, (u, inward)) in [(u0 + 0.08, along), (u1 - 0.08, -along)].into_iter().enumerate() {
            let start = if l.open_end[k] { 1.9 } else { 0.05 };
            kit::railing(b, wall_point(l.side, face, start, u, y), wall_point(l.side, face, fr, u, y), -inward, 1.5, 1);
        }
    }
    // Corbels under the slab.
    let n = (len / 5.0).floor().max(1.0) as usize;
    for k in 0..n {
        let u = u0 + len * (k as f32 + 0.5) / n as f32;
        kit::beam(b, wall_point(l.side, face, 0.0, u, y - 0.9 - 2.6), wall_point(l.side, face, l.depth - 0.7, u, y - 0.9), 0.55, 0.7, 2, 1.5);
    }
}

fn bridges_and_stairs(p: &Plan, b: &mut MeshBuilder) {
    for br in &p.bridges {
        let (la, lb) = (&p.ledges[br.a], &p.ledges[br.b]);
        let y = la.level as f32 * LEVEL;
        let a = wall_point(la.side, p.rooms[la.room].face(la.side), la.depth, br.u, y);
        let c = wall_point(lb.side, p.rooms[lb.room].face(lb.side), lb.depth, br.u, y);
        kit::bridge(b, &kit::Bridge { a, c, width: br.width, deck: 0.8, girder: if br.girder { Some((2.4, 1.8)) } else { None }, ribs: Some(4.0), railings: br.railings, rank: 0 });
    }
    for ws in &p.wall_stairs {
        let l = &p.ledges[ws.lower];
        let r = &p.rooms[l.room];
        let face = r.face(l.side);
        let out = -l.side.vec();
        let foot = wall_point(l.side, face, 0.9, ws.u0, l.level as f32 * LEVEL);
        kit::stair(b, &Stair {
            foot,
            dir: l.side.along() * ws.dir,
            width: 1.6,
            height: ws.levels as f32 * LEVEL,
            risers: Some(ws.levels as usize * RISERS_PER_LEVEL),
            max_risers: 24,
            landing: 1.2,
            waist: 0.35,
            wall: Some(-out),
            railing: true,
            enclosed: false,
        });
    }
}

fn arch(p: &Plan, b: &mut MeshBuilder) {
    let a = &p.arch;
    let hall = &p.rooms[a.hall];
    let void = &p.rooms[a.void];
    let zh = hall.face(Dir::ZP);
    let zv = void.face(Dir::ZN);
    let y0 = hall.floor_y();
    let frame = Frame::new(Vec3::new(a.x, y0, zh), Vec3::X, Vec3::Y, -Vec3::Z);
    kit::arch(b, &kit::Arch { frame, w: a.w, height: a.height, span: a.span, spring: a.spring, ring: a.ring, voussoirs: 21, thickness: zv - zh, facets: 3 });
    hface(b, y0, true, a.x - a.span * 0.5, a.x + a.span * 0.5, zh, zv, 1.0, &[], &[], &[], 2.0, 2, Vec3::Z);
}

fn portal_reveal(b: &mut MeshBuilder, o: &Portal) {
    let depth = o.p1 - o.p0;
    if depth < 0.02 {
        return;
    }
    let (origin, u, n) = if o.x_plane { (Vec3::new(o.p0, 0.0, 0.0), Vec3::Z, -Vec3::X) } else { (Vec3::new(0.0, 0.0, o.p0), Vec3::X, -Vec3::Z) };
    let f = Frame::new(origin, u, Vec3::Y, n);
    kit::reveal(b, f, Rect2::new(o.u0, o.u1, o.y0, o.y1), depth, 2.0, o.walk, Some(3));
}

/// The outside of the region: four walls and a roof, with holes where windows, loggias and
/// the voids open. No colliders (nothing walks out there).
fn skin(p: &Plan, b: &mut MeshBuilder) {
    let (x0, x1, z0, z1) = (-SKIN, NX as f32 * CELL + SKIN, -SKIN, NZ as f32 * CELL + SKIN);
    let (ybot, ytop) = (-60.0, NY as f32 * LEVEL);
    let mut rng = Rng::for_id(p.seed, "skin", layer::ELEMENTS);
    for (d, plane) in [(Dir::XN, x0), (Dir::XP, x1), (Dir::ZN, z0), (Dir::ZP, z1)] {
        let (u0, u1) = if d.is_x() { (z0, z1) } else { (x0, x1) };
        let hs = holes(p, d.is_x(), plane, u0, u1, ybot, ytop);
        let su = seams_at(&mut rng, u0, u1, 4, CELL);
        let sv = seams_at(&mut rng, ybot + 10.0, ytop - 4.0, 6, LEVEL);
        vface(b, d.is_x(), plane, d.sign(), u0, u1, ybot, ytop, 0.0, &su, &sv, &hs, 6.0, Some(1), 2);
    }
    let roof_holes: Vec<[f32; 4]> = p.rooms.iter().filter(|r| r.open_top).map(|r| [r.face(Dir::XN), r.face(Dir::XP), r.face(Dir::ZN), r.face(Dir::ZP)]).collect();
    hface(b, ytop, true, x0, x1, z0, z1, 0.0, &[], &[], &roof_holes, 8.0, 2, Vec3::X);
    for (x, z) in [(x0, z0), (x1, z0), (x1, z1), (x0, z1)] {
        let nx = if x == x0 { -Vec3::X } else { Vec3::X };
        let nz = if z == z0 { -Vec3::Z } else { Vec3::Z };
        b.stroke(Vec3::new(x, ybot, z), Vec3::new(x, ytop, z), nx, nz, StrokeKind::Crease, 0);
    }
    for (a, c, n) in [
        (Vec3::new(x0, ytop, z0), Vec3::new(x1, ytop, z0), -Vec3::Z),
        (Vec3::new(x1, ytop, z0), Vec3::new(x1, ytop, z1), Vec3::X),
        (Vec3::new(x1, ytop, z1), Vec3::new(x0, ytop, z1), Vec3::Z),
        (Vec3::new(x0, ytop, z1), Vec3::new(x0, ytop, z0), -Vec3::X),
    ] {
        b.stroke(a, c, n, Vec3::Y, StrokeKind::Crease, 0);
    }
}
