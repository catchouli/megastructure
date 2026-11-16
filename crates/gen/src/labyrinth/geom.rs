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
        b.set_group(p.volume(p.rooms[ri].space));
        match p.rooms[ri].kind {
            SpaceKind::Void | SpaceKind::LightWell => void_room(p, b, ri),
            SpaceKind::GreatHall => hall(p, b, ri),
            SpaceKind::Undercroft => undercroft(p, b, ri),
            SpaceKind::StairTower => tower(p, b, ri),
            SpaceKind::Cistern => cistern(p, b, ri),
            SpaceKind::StairHall => stair_hall(p, b, ri),
            SpaceKind::Shaft => shaft(p, b, ri),
            _ => small_room(p, b, ri),
        }
    }
    for pass in &p.passages {
        b.set_group(p.volume(pass.space));
        passage(p, b, pass);
    }
    for li in 0..p.ledges.len() {
        b.set_group(p.volume(p.ledges[li].space));
        ledge(p, b, li);
    }
    bridges_and_stairs(p, b);
    b.set_group(p.volume(p.rooms[p.arch.hall].space));
    arch(p, b);
    gallery(p, b);
    for o in &p.portals {
        let side = o.between[0].or(o.between[1]).map(|s| p.volume(s)).unwrap_or(crate::mesh::OUTSIDE);
        b.set_group(side);
        portal_reveal(b, o);
        door_frame(p, b, o);
        vent_grille(p, b, o);
    }
    for sd in p.side_doors.iter().filter(|sd| sd.ajar) {
        b.set_group(p.volume(sd.room));
        ajar_leaf(p, b, sd);
    }
    b.set_group(crate::mesh::OUTSIDE);
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
    let mut sx = cell_lines(lo.x, hi.x, st.floor_cells);
    let mut sz = cell_lines(lo.z, hi.z, st.floor_cells);
    // A paved border along the walls.
    sx.extend([lo.x + 1.2, hi.x - 1.2]);
    sz.extend([lo.z + 1.2, hi.z - 1.2]);
    sx.sort_by(|a, c| a.partial_cmp(c).unwrap());
    sz.sort_by(|a, c| a.partial_cmp(c).unwrap());
    let fh = holes_h(p, lo.y, lo.x, hi.x, lo.z, hi.z);
    hface(b, lo.y, true, lo.x, hi.x, lo.z, hi.z, SLAB, &sx, &sz, &fh, st.step, 2, Vec3::X);
    if !r.open_top {
        let ch = ceiling_holes(p, r);
        hface(b, hi.y, false, lo.x, hi.x, lo.z, hi.z, SLAB, &sx, &sz, &ch, st.step.max(2.0), 3, Vec3::X);
        for h in &ch {
            for (a, c) in [((h[0], h[2]), (h[1], h[2])), ((h[1], h[2]), (h[1], h[3])), ((h[1], h[3]), (h[0], h[3])), ((h[0], h[3]), (h[0], h[2]))] {
                b.stroke(Vec3::new(a.0, hi.y, a.1), Vec3::new(c.0, hi.y, c.1), -Vec3::Y, -Vec3::Y, StrokeKind::Crease, 1);
            }
        }
    }
}

/// Holes in a room's ceiling where a light shaft comes down into it: (x0, x1, z0, z1).
fn ceiling_holes(p: &Plan, r: &Room) -> Vec<[f32; 4]> {
    p.rooms
        .iter()
        .filter(|s| s.kind == SpaceKind::Shaft && s.b.y0 == r.b.y1 && s.b.x0 >= r.b.x0 && s.b.x1 <= r.b.x1 && s.b.z0 >= r.b.z0 && s.b.z1 <= r.b.z1)
        .map(|s| [s.face(Dir::XN), s.face(Dir::XP), s.face(Dir::ZN), s.face(Dir::ZP)])
        .collect()
}

/// A light shaft: four walls from the ceiling of the room below to the roof, no floor.
fn shaft(p: &Plan, b: &mut MeshBuilder, ri: usize) {
    let r = &p.rooms[ri];
    let lo = r.inner_min() - Vec3::Y * SLAB;
    let hi = r.inner_max();
    let mut rng = rng(p, r.space, "geom");
    for d in Dir::ALL {
        let plane = r.face(d);
        let (u0, u1) = r.side_span(d);
        let sv = seams_at(&mut rng, lo.y + 3.0, hi.y - 3.0, 8, LEVEL);
        vface(b, d.is_x(), plane, -d.sign(), u0, u1, lo.y, hi.y, r.inset, &[], &sv, &[], 2.0, None, 2);
    }
    for (x, z, nx, nz) in [(lo.x, lo.z, 1.0, 1.0), (hi.x, lo.z, -1.0, 1.0), (hi.x, hi.z, -1.0, -1.0), (lo.x, hi.z, 1.0, -1.0)] {
        b.stroke(Vec3::new(x, lo.y, z), Vec3::new(x, hi.y, z), Vec3::X * nx, Vec3::Z * nz, StrokeKind::Crease, 1);
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
    for k in (r.b.y0 + 2..ny() - 1).rev() {
        if placed >= n_beams {
            break;
        }
        if busy.contains(&k) || !rng.chance(0.35) {
            continue;
        }
        let y = k as f32 * LEVEL + 4.9;
        let along_x = rng.chance(0.5);
        let (w, dep) = (rng.range(1.4, 2.4), rng.range(1.8, 3.0));
        // Keep the director's sight lines open.
        let cross = rng.range(0.15, 0.85);
        let (bmin, bmax) = if along_x {
            let z = lo.z + 4.0 + (hi.z - lo.z - 8.0) * cross;
            (Vec3::new(lo.x, y - dep, z - w), Vec3::new(hi.x, y + dep, z + w))
        } else {
            let x = lo.x + 4.0 + (hi.x - lo.x - 8.0) * cross;
            (Vec3::new(x - w, y - dep, lo.z), Vec3::new(x + w, y + dep, hi.z))
        };
        if p.views.iter().any(|v| segment_hits_box(v.from, v.to, bmin, bmax)) {
            continue;
        }
        let as_duct = rng.chance(0.5);
        let span = |b: &mut MeshBuilder, a: Vec3, c: Vec3| if as_duct { exposed_duct(b, a, c, w.max(1.6), dep.min(1.6)) } else { kit::beam(b, a, c, w, dep, 0, 2.0); };
        if along_x {
            let z = (bmin.z + bmax.z) * 0.5;
            span(b, Vec3::new(lo.x - 0.5, y, z), Vec3::new(hi.x + 0.5, y, z));
            for d in [Dir::XN, Dir::XP] {
                busy_rects.push((d, [z - w, z + w, y - dep, y + dep]));
            }
        } else {
            let x = (bmin.x + bmax.x) * 0.5;
            if (x - p.arch.x).abs() < 6.0 {
                continue;
            }
            span(b, Vec3::new(x, y, lo.z - 0.5), Vec3::new(x, y, hi.z + 0.5));
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

/// Whether the segment passes through the box (slab test).
pub fn segment_hits_box(a: Vec3, c: Vec3, lo: Vec3, hi: Vec3) -> bool {
    let d = c - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for i in 0..3 {
        if d[i].abs() < 1e-6 {
            if a[i] < lo[i] || a[i] > hi[i] {
                return false;
            }
        } else {
            let (mut u, mut v) = ((lo[i] - a[i]) / d[i], (hi[i] - a[i]) / d[i]);
            if u > v {
                std::mem::swap(&mut u, &mut v);
            }
            t0 = t0.max(u);
            t1 = t1.min(v);
            if t0 > t1 {
                return false;
            }
        }
    }
    true
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
    let big = r.kind == SpaceKind::Void || p.graph.spaces[r.space].set_piece.is_some();
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
    // The nave is centred on the arch's axis.
    let off = (w * 0.23).min(p.arch.x - lo.x - 7.0).min(hi.x - p.arch.x - 7.0).max(7.0);
    let rows = [p.arch.x - off, p.arch.x + off];
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
                    let outward = if cx < p.arch.x { -1.0 } else { 1.0 };
                    let c = Vec3::new(cx + outward * rng.range(2.5, 5.0), lo.y, cz + 4.0 + d as f32 * 3.4);
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
    // A duct hung under the ceiling along the hall, beside the wall opposite the gallery.
    {
        let side = p.gallery.side.opposite();
        let x = r.face(side) - side.sign() * 3.5;
        let y = hi.y - 6.0;
        exposed_duct(b, Vec3::new(x, y, lo.z + 3.0), Vec3::new(x, y, hi.z - 3.0), 1.6, 1.3);
        let mut z = lo.z + 6.0;
        while z < hi.z - 4.0 {
            for dx in [-0.6f32, 0.6] {
                kit::beam(b, Vec3::new(x + dx, y + 0.65, z), Vec3::new(x + dx, hi.y - 3.1, z), 0.12, 0.12, 3, 2.0);
            }
            z += 8.0;
        }
    }
    wall_bands(p, b, r, lo.y, 0.9, 0.35, true, 2);
    wall_bands(p, b, r, hi.y - 2.2, 2.2, 0.8, false, 1);
    wall_bands(p, b, r, lo.y + 14.0, 0.6, 0.3, false, 2);
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
    wall_bands(p, b, r, lo.y, 0.6, 0.3, true, 2);
    for _ in 0..4 {
        let (cx, cz) = (rng.range(lo.x + 3.0, hi.x - 3.0), rng.range(lo.z + 3.0, hi.z - 3.0));
        let (sx, sy, sz) = (rng.range(0.8, 1.6), rng.range(0.6, 1.4), rng.range(0.8, 1.6));
        if xs.iter().any(|&x| (x - cx).abs() < sx + 1.0) && zs.iter().any(|&z| (z - cz).abs() < sz + 1.0) {
            continue;
        }
        kit::aa_block(b, Vec3::new(cx - sx, lo.y, cz - sz), Vec3::new(cx + sx, lo.y + sy, cz + sz), [true, true, false, true, true, true], Vec3::Y, 1.5, [Some(1), Some(2), Some(2)], true);
    }
}


/// A band of blocks along every wall of a room at height `y` (a plinth, a string course, a
/// cornice), `h` tall and `dep` deep, broken where an opening crosses it.
#[allow(clippy::too_many_arguments)]
fn wall_bands(p: &Plan, b: &mut MeshBuilder, r: &Room, y: f32, h: f32, dep: f32, collide: bool, rank: u32) {
    let lo = r.inner_min();
    let hi = r.inner_max();
    for d in Dir::ALL {
        let plane = r.face(d);
        let (u0, u1) = r.side_span(d);
        let mut cuts: Vec<(f32, f32)> = holes(p, d.is_x(), plane, u0, u1, lo.y, hi.y).iter().filter(|q| q[2] < y + h && q[3] > y).map(|q| (q[0] - 0.05, q[1] + 0.05)).collect();
        if r.kind == SpaceKind::GreatHall && d == Dir::ZP {
            let a = &p.arch;
            cuts.push((a.x - a.w * 0.5, a.x + a.w * 0.5));
        }
        cuts.sort_by(|a, c| a.0.partial_cmp(&c.0).unwrap());
        let mut segs = Vec::new();
        let mut u = u0 + dep;
        for (c0, c1) in cuts {
            if c0 > u + 0.3 {
                segs.push((u, c0));
            }
            u = u.max(c1);
        }
        if u1 - dep > u + 0.3 {
            segs.push((u, u1 - dep));
        }
        let out = -d.vec();
        let along = d.along();
        for (a, c) in segs {
            let m = (a + c) * 0.5;
            let centre = if d.is_x() { Vec3::new(plane + out.x * dep * 0.5, y + h * 0.5, m) } else { Vec3::new(m, y + h * 0.5, plane + out.z * dep * 0.5) };
            kit::block(b, centre, [along, Vec3::Y, out], Vec3::new((c - a) * 0.5, h * 0.5, dep * 0.5), [true, true, true, true, false, true], along, 2.0, [Some(rank), None, Some(rank)], collide);
        }
    }
}

/// The cistern: a walkway round the walls two storeys above the basin floor, a forest of
/// round columns in the basin, a free-standing stair between them, corbels and a cornice.
fn cistern(p: &Plan, b: &mut MeshBuilder, ri: usize) {
    let r = &p.rooms[ri];
    room_box(p, b, r, &BoxStyle { vertical: 0, floor: 1, ceiling: 1, wall_cells: 2, wall_storeys: 1, floor_cells: 2, step: 2.0, seam_rank: 2 }, &[]);
    let lo = r.inner_min();
    let hi = r.inner_max();
    let w = CISTERN_WALK;
    let y = (r.b.y0 + 2) as f32 * LEVEL;
    let th = 0.9;
    let slabs = [
        (Vec3::new(lo.x, y - th, lo.z), Vec3::new(lo.x + w, y, hi.z)),
        (Vec3::new(hi.x - w, y - th, lo.z), Vec3::new(hi.x, y, hi.z)),
        (Vec3::new(lo.x + w, y - th, lo.z), Vec3::new(hi.x - w, y, lo.z + w)),
        (Vec3::new(lo.x + w, y - th, hi.z - w), Vec3::new(hi.x - w, y, hi.z)),
    ];
    for (a, c) in slabs {
        kit::aa_block(b, a, c, [true, true, true, true, true, true], Vec3::X, 1.5, [Some(1), Some(2), Some(1)], true);
    }
    let (foot, dir) = cistern_stair(r);
    let (ix0, ix1, iz0, iz1) = (lo.x + w, hi.x - w, lo.z + w, hi.z - w);
    let ry = y;
    let gap0 = foot.x - 0.9 - lo.x - w;
    kit::railing(b, Vec3::new(ix0 + 0.06, ry, iz0), Vec3::new(ix0 + 0.06, ry, iz1), Vec3::X, 1.5, 1);
    kit::railing(b, Vec3::new(ix1 - 0.06, ry, iz0), Vec3::new(ix1 - 0.06, ry, iz1), -Vec3::X, 1.5, 1);
    kit::railing(b, Vec3::new(ix0, ry, iz0 + 0.06), Vec3::new(ix1, ry, iz0 + 0.06), Vec3::Z, 1.5, 1);
    kit::railing_with_gaps(b, Vec3::new(ix0, ry, iz1 - 0.06), Vec3::new(ix1, ry, iz1 - 0.06), -Vec3::Z, 1.5, 1, &[(gap0, gap0 + 1.8)]);
    kit::stair(b, &Stair { foot, dir, width: 1.6, height: 2.0 * LEVEL, risers: Some(2 * RISERS_PER_LEVEL), max_risers: 24, landing: 1.2, waist: 0.35, wall: None, railing: true, enclosed: false });
    // Corbels under the walkway.
    for (a, c) in slabs.iter().take(2) {
        let x_wall = if a.x <= lo.x + 0.01 { lo.x } else { hi.x };
        let x_edge = if a.x <= lo.x + 0.01 { c.x - 0.6 } else { a.x + 0.6 };
        let mut z = lo.z + 3.0;
        while z < hi.z - 2.0 {
            kit::beam(b, Vec3::new(x_wall, y - th - 3.0, z), Vec3::new(x_edge, y - th, z), 0.6, 0.8, 2, 1.5);
            z += 6.0;
        }
    }
    // The columns of the basin, clear of the stair.
    let h = hi.y - lo.y;
    let mut x = ix0 + 4.5;
    while x < ix1 - 1.5 {
        let mut z = iz0 + 3.0;
        while z < iz1 - 1.5 {
            if (x - foot.x).abs() > 2.6 {
                kit::column_round(b, Vec3::new(x, lo.y, z), 0.85, h - 0.01, 16, 3.0, 0.3, 1.5);
            }
            z += 6.5;
        }
        x += 6.5;
    }
    wall_bands(p, b, r, hi.y - 1.2, 1.2, 0.5, false, 1);
    wall_bands(p, b, r, lo.y, 0.6, 0.3, true, 2);
}

/// The stair hall: one monumental stair up its length to a landing across its far end, piers
/// under the landing, newels at the foot, pilasters, a plinth, string course and cornice.
fn stair_hall(p: &Plan, b: &mut MeshBuilder, ri: usize) {
    let r = &p.rooms[ri];
    let Some(sh) = p.stair_hall else { return };
    room_box(p, b, r, &BoxStyle { vertical: 0, floor: 1, ceiling: 1, wall_cells: 2, wall_storeys: 1, floor_cells: 2, step: 2.0, seam_rank: 2 }, &[]);
    let lo = r.inner_min();
    let hi = r.inner_max();
    let zc = (lo.z + hi.z) * 0.5;
    let yl = (r.b.y0 + sh.levels) as f32 * LEVEL;
    let end = if sh.dir > 0.0 { hi.x } else { lo.x };
    let (lx0, lx1) = (sh.landing_edge.min(end), sh.landing_edge.max(end));
    kit::aa_block(b, Vec3::new(lx0, yl - 1.2, lo.z), Vec3::new(lx1, yl, hi.z), [sh.dir > 0.0, sh.dir < 0.0, true, true, false, false], Vec3::Z, 2.0, [Some(1), Some(1), Some(0)], true);
    let sw = 8.0;
    let front = sh.landing_edge + sh.dir * 0.08;
    kit::railing_with_gaps(b, Vec3::new(front, yl, lo.z), Vec3::new(front, yl, hi.z), -Vec3::X * sh.dir, 1.5, 1, &[(zc - lo.z - sw * 0.5, zc - lo.z + sw * 0.5)]);
    for k in [1.0f32, 3.0, 5.0] {
        let z = lo.z + (hi.z - lo.z) * k / 6.0;
        if (z - zc).abs() < sw * 0.5 + 1.0 && k != 3.0 {
            continue;
        }
        kit::column_square(b, Vec3::new(sh.landing_edge + sh.dir * 1.0, lo.y, z), 1.6, yl - 1.2 - lo.y, 3.0, 1.5);
    }
    kit::stair(b, &Stair { foot: Vec3::new(sh.foot, lo.y, zc), dir: Vec3::X * sh.dir, width: sw, height: yl - lo.y, risers: Some(sh.levels as usize * RISERS_PER_LEVEL), max_risers: 27, landing: 2.5, waist: 0.6, wall: None, railing: true, enclosed: false });
    for sg in [-1.0f32, 1.0] {
        let c = Vec3::new(sh.foot - sh.dir * 0.8, lo.y + 1.3, zc + sg * (sw * 0.5 + 0.8));
        kit::block(b, c, [Vec3::X, Vec3::Y, Vec3::Z], Vec3::new(0.8, 1.3, 0.8), [true, true, false, true, true, true], Vec3::Y, 1.0, [Some(1), Some(1), Some(1)], true);
    }
    // Pilasters on the long walls between openings.
    for d in [Dir::ZN, Dir::ZP] {
        let plane = r.face(d);
        let out = -d.vec();
        let hs = holes(p, false, plane, lo.x, hi.x, lo.y, hi.y);
        let mut x = lo.x + 2.0 * CELL;
        while x < hi.x - CELL {
            if !hs.iter().any(|q| q[0] < x + 2.0 && q[1] > x - 2.0) && (x - lx0 + 0.6) * (x - lx1 - 0.6) > 0.0 {
                let c = Vec3::new(x, (lo.y + hi.y) * 0.5, plane + out.z * 0.45);
                kit::block(b, c, [Vec3::X, Vec3::Y, Vec3::Z], Vec3::new(0.7, (hi.y - lo.y) * 0.5, 0.45), [true, true, false, false, d == Dir::ZP, d == Dir::ZN], Vec3::Y, 2.0, [None, Some(1), None], true);
            }
            x += 2.0 * CELL;
        }
    }
    wall_bands(p, b, r, lo.y, 0.7, 0.3, true, 2);
    wall_bands(p, b, r, hi.y - 1.4, 1.4, 0.6, false, 1);
}

fn small_room(p: &Plan, b: &mut MeshBuilder, ri: usize) {
    let r = &p.rooms[ri];
    // A lantern's walls rise to the roof: coarser cells there.
    let step = if r.open_top { 4.0 } else { 1.5 };
    room_box(p, b, r, &BoxStyle { vertical: 1, floor: 1, ceiling: 1, wall_cells: 1, wall_storeys: 1, floor_cells: 1, step, seam_rank: 2 }, &[]);
    let lo = r.inner_min();
    let hi = r.inner_max();
    let storey_top = if r.open_top { (r.b.y0 + 2) as f32 * LEVEL - SLAB } else { hi.y };
    if r.kind == SpaceKind::Junction && r.style == 0 && r.b.width() >= 3 && r.b.depth() >= 3 {
        let c = (lo + hi) * 0.5;
        kit::column_square(b, Vec3::new(c.x, lo.y, c.z), 1.8, hi.y - lo.y, 3.0, 1.5);
    }
    if r.kind == SpaceKind::Junction && r.style == 1 {
        // A crossing: chamfered corners up to the lantern's ring.
        for (cx, cz, sx, sz) in [(lo.x, lo.z, 1.0, 1.0), (hi.x, lo.z, -1.0, 1.0), (hi.x, hi.z, -1.0, -1.0), (lo.x, hi.z, 1.0, -1.0)] {
            let n = Vec3::new(sx, 0.0, sz).normalize();
            let t = Vec3::new(-sz, 0.0, sx).normalize();
            // A slab across the corner, kept inside the room's walls.
            let c = Vec3::new(cx, (lo.y + storey_top) * 0.5, cz) + n * 1.2;
            kit::block(b, c, [t, Vec3::Y, n], Vec3::new(1.7, (storey_top - lo.y) * 0.5, 0.15), [false, false, false, true, false, true], Vec3::Y, 1.5, [Some(1), None, None], true);
        }
        if r.open_top {
            // The ring the lantern rises from.
            wall_bands(p, b, r, storey_top - 0.9, 0.9, 0.7, false, 1);
        }
    }
    if r.kind == SpaceKind::SideRoom {
        side_room(p, b, r);
    }
    if matches!(r.kind, SpaceKind::Junction | SpaceKind::Antechamber | SpaceKind::Chamber | SpaceKind::Loggia | SpaceKind::SideRoom) {
        wall_bands(p, b, r, lo.y, 0.45, 0.18, false, 2);
        if !r.open_top {
            wall_bands(p, b, r, hi.y - 0.7, 0.7, 0.35, false, 2);
        }
    }
    if r.kind == SpaceKind::Antechamber {
        // Beams across under the ceiling at each cell boundary.
        let along_x = r.b.width() >= r.b.depth();
        let n = if along_x { r.b.width() } else { r.b.depth() };
        for k in 1..n {
            let u = if along_x { r.b.x0 + k } else { r.b.z0 + k } as f32 * CELL;
            let (a, c) = if along_x { (Vec3::new(u, hi.y - 0.45, lo.z), Vec3::new(u, hi.y - 0.45, hi.z)) } else { (Vec3::new(lo.x, hi.y - 0.45, u), Vec3::new(hi.x, hi.y - 0.45, u)) };
            kit::beam(b, a, c, 0.6, 0.9, 2, 1.5);
        }
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

/// What a side room holds, by its style: 0 a cell (a bench, a raised niche), 1 a store room
/// (shelving, crates), 2 a shrine (stepped plinth, altar, stele, two small columns), 3 a
/// machine closet (cabinets, pipes floor to ceiling).
fn side_room(p: &Plan, b: &mut MeshBuilder, r: &Room) {
    let lo = r.inner_min();
    let hi = r.inner_max();
    let Some(sd) = p.side_doors.iter().find(|sd| sd.room == r.space) else { return };
    let mut rng = rng(p, r.space, "furnish");
    // The back wall faces the door; `inw` points from it into the room.
    let back = sd.dir;
    let inw = -back.vec();
    let along = back.along();
    let face = r.face(back);
    let (u0, u1) = r.side_span(back);
    let um = (u0 + u1) * 0.5;
    let at = |u: f32, y: f32, d: f32| if back.is_x() { Vec3::new(face, y, u) } else { Vec3::new(u, y, face) } + inw * d;
    let axes = [along, Vec3::Y, inw];
    let h = hi.y - lo.y;
    let fl = lo.y;
    match r.style {
        0 => {
            kit::block(b, at(um, fl + 0.25, 0.4), axes, Vec3::new(((u1 - u0) * 0.5 - 0.3).min(1.1), 0.25, 0.4), [true, true, false, true, false, true], along, 1.0, [Some(2), Some(2), Some(2)], true);
            kit::block(b, at(um - 0.6, fl + 1.6, 0.06), axes, Vec3::new(0.3, 0.4, 0.06), [true, true, true, true, false, true], Vec3::Y, 1.0, [Some(2), Some(2), Some(2)], false);
        }
        1 => {
            // Shelving along the two side walls, crates on the floor.
            for sg in [-1.0f32, 1.0] {
                let u = if sg < 0.0 { u0 + 0.3 } else { u1 - 0.3 };
                let depth = (hi - lo).with_y(0.0).length().min(6.0);
                let len = ((if back.is_x() { hi.x - lo.x } else { hi.z - lo.z }) - 1.2).max(1.0);
                let c = at(u, fl + 1.3, 0.6 + len * 0.5);
                kit::block(b, c, axes, Vec3::new(0.3, 1.3, len * 0.5), [true, true, false, true, true, true], Vec3::Y, 1.0, [Some(2), Some(2), Some(2)], true);
                for k in 1..5 {
                    let y = fl + 2.6 * k as f32 / 5.0;
                    let x = u - sg * 0.301;
                    let (a, cc) = (at(x, y, 0.6), at(x, y, 0.6 + len));
                    b.stroke(a, cc, -along * sg, Vec3::Y, StrokeKind::Seam, 2);
                }
                let _ = depth;
            }
            // Crates between the shelves, clear of the way in.
            let room_depth = if back.is_x() { hi.x - lo.x } else { hi.z - lo.z };
            for _ in 0..if room_depth > 4.5 { rng.int(2, 4) } else { 0 } {
                let (sx, sy, sz) = (rng.range(0.3, 0.6), rng.range(0.3, 0.7), rng.range(0.3, 0.6));
                let c = at(um + rng.range(-1.0, 1.0), fl + sy, rng.range(1.0, room_depth - 2.6));
                kit::block(b, c, [Vec3::X, Vec3::Y, Vec3::Z], Vec3::new(sx, sy, sz), [true, true, false, true, true, true], Vec3::Y, 1.0, [Some(2), Some(2), Some(2)], true);
            }
        }
        2 => {
            for (k, (hw, hy, d)) in [(1.6f32, 0.18f32, 1.2f32), (1.2, 0.36, 0.9)].into_iter().enumerate() {
                let _ = k;
                kit::block(b, at(um, fl + hy * 0.5, d * 0.5), axes, Vec3::new(hw, hy * 0.5, d * 0.5), [true, true, false, true, false, true], along, 1.0, [Some(2), Some(2), Some(2)], true);
            }
            kit::block(b, at(um, fl + 0.36 + 0.5, 0.45), axes, Vec3::new(0.6, 0.5, 0.35), [true, true, false, true, false, true], along, 1.0, [Some(1), Some(1), Some(1)], true);
            kit::block(b, at(um, fl + 0.86 + 1.0, 0.12), axes, Vec3::new(0.45, 1.0, 0.1), [true, true, false, true, false, true], Vec3::Y, 1.0, [Some(1), Some(2), Some(2)], false);
            for sg in [-1.0f32, 1.0] {
                kit::column_round(b, at(um + sg * 1.3, fl, 0.9), 0.18, (h - 0.2).min(3.2), 10, 1.5, 0.08, 1.0);
            }
        }
        _ => {
            for k in 0..rng.int(2, 3) {
                let u = u0 + 0.6 + k as f32 * 1.1;
                if u > u1 - 0.5 {
                    break;
                }
                let ch = rng.range(1.4, 2.4);
                let c = at(u, fl + ch * 0.5, 0.35);
                kit::block(b, c, axes, Vec3::new(0.5, ch * 0.5, 0.35), [true, true, false, true, false, true], Vec3::Y, 1.0, [Some(1), Some(1), Some(2)], true);
                for j in 1..4 {
                    let y = fl + ch * j as f32 / 4.0;
                    b.stroke(at(u - 0.45, y, 0.701), at(u + 0.45, y, 0.701), inw, inw, StrokeKind::Seam, 3);
                }
            }
            for k in 0..3 {
                let u = u1 - 0.3 - k as f32 * 0.35;
                pipe(b, at(u, fl, 0.25), at(u, hi.y, 0.25), 0.1);
            }
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
                let y0 = c.y as f32 * LEVEL + pass.style.floor_off();
                push(&mut out, Piece { x: c.x, z: c.z, y0, y1: y0 + h, flat: true, conn: [None; 4] });
            }
            Step::Stair { from, dir, up } => {
                let lo = if up { from.y } else { from.y - 1 };
                for k in 1..=3 {
                    let c = from.step(dir, k);
                    let off = pass.style.floor_off();
                    push(&mut out, Piece { x: c.x, z: c.z, y0: lo as f32 * LEVEL + off, y1: (lo + 1) as f32 * LEVEL + h + off, flat: false, conn: [None; 4] });
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
    let style = pass.style;
    let vent = style == PassStyle::Vent;
    let dado = if service || vent || style == PassStyle::Slot { None } else { Some(rng.range(1.0, 1.6)) };
    let conduit = !vent && matches!(style, PassStyle::Plain | PassStyle::Slot | PassStyle::Gallery | PassStyle::Duct) && rng.chance(0.4);
    // Gratings over dark pits in a few floor cells.
    let gratings: Vec<bool> = (0..pieces(pass).len()).map(|_| !vent && rng.chance(0.05)).collect();
    let straight = |pc: &Piece| {
        let c: Vec<bool> = (0..4).map(|i| pc.conn[i].is_some()).collect();
        (c[0] && c[1] && !c[2] && !c[3]) || (c[2] && c[3] && !c[0] && !c[1])
    };
    let vaulted: Vec<bool> = ps.iter().map(|pc| style == PassStyle::Vaulted && pc.flat && straight(pc)).collect();
    let r_v = (CELL - 2.0 * t) * 0.5;
    let spring = style.height() - r_v;
    for (k, pc) in ps.iter().enumerate() {
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
                            let top = if vaulted[k] { pc.y0 + spring } else { pc.y1 };
                            parts.push(WallPart { x_plane, plane, nsign, u0, u1, y0: pc.y0, y1: top, thick: t, bottom: pc.flat, top: true });
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
            if !up && vaulted[k] {
                let axis_x = pc.conn[Dir::XP.index()].is_some();
                vault(b, pc, axis_x, pc.y0 + spring, r_v, floor_coarse);
                // Where the vault meets a piece without one, close the space above it.
                for d in if axis_x { [Dir::XN, Dir::XP] } else { [Dir::ZN, Dir::ZP] } {
                    let nb = [k.wrapping_sub(1), k + 1].into_iter().filter_map(|i| ps.get(i).map(|q| (i, q))).find(|(_, q)| {
                        let (dx, dz) = d.d();
                        q.x == pc.x + dx && q.z == pc.z + dz
                    });
                    if nb.is_some_and(|(i, _)| !vaulted[i]) {
                        spandrel(b, pc, d, pc.y0 + spring, r_v, pc.y1, floor_coarse);
                    }
                }
                continue;
            }
            let fine = b.panel();
            for i in 0..3 {
                for j in 0..3 {
                    if !open[i][j] || (up && gratings[k] && i == 1 && j == 1) {
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
        if pc.flat && gratings[k] {
            grating(b, xs[1], xs[2], zs[1], zs[2], pc.y0, floor_coarse);
        }
        // Flanges of a vent at each cell boundary it continues through.
        if vent && pc.flat {
            for d in [Dir::XP, Dir::ZP] {
                if pc.conn[d.index()].is_some() {
                    let (a0, a1) = if d.is_x() { ((xs[3], zs[1]), (xs[3], zs[2])) } else { ((xs[1], zs[3]), (xs[2], zs[3])) };
                    for (y, nn) in [(pc.y0 + 0.003, Vec3::Y), (pc.y1 - 0.003, -Vec3::Y)] {
                        b.stroke(Vec3::new(a0.0, y, a0.1), Vec3::new(a1.0, y, a1.1), nn, nn, StrokeKind::Seam, 2);
                    }
                    for (u, v) in [a0, a1] {
                        b.stroke(Vec3::new(u, pc.y0, v), Vec3::new(u, pc.y1, v), Vec3::Y, Vec3::Y, StrokeKind::Seam, 2);
                    }
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
            // Features along the walls, every few cells: closed doors (leaves, studs, a
            // threshold), raised niches that are plainly niches, and missing panels showing
            // the structure behind.
            let mut feats: Vec<([f32; 4], u8)> = Vec::new();
            if !vent && w.bottom && w.top && (w.thick - t).abs() < 1e-3 && w.u1 - w.u0 >= CELL - 2.0 * t - 0.01 {
                let mut u = (w.u0 / CELL).floor() * CELL + CELL * 0.5;
                while u < w.u1 {
                    let roll = rng.f();
                    let kind = if roll < 0.07 { 0 } else if roll < 0.14 { 1 } else if roll < 0.19 && !service { 2 } else { 9 };
                    let r = match kind {
                        0 => {
                            let dw = if service { kit::DOOR_WIDTH } else { rng.range(1.2, 1.8) };
                            let dh = if service { kit::DOOR_HEIGHT } else { rng.range(2.2, 3.0) };
                            [u - dw * 0.5, u + dw * 0.5, w.y0, w.y0 + dh]
                        }
                        1 => [u - 0.45, u + 0.45, w.y0 + 0.95, w.y0 + 2.2],
                        _ => [u - 0.8, u + 0.8, w.y0 + 0.7, w.y0 + 3.0],
                    };
                    if kind != 9 && r[0] > w.u0 + 0.3 && r[1] < w.u1 - 0.3 && r[3] < w.y1 - 0.3 && !hs.iter().any(|h| overlaps(h, &[r[0] - 0.6, r[1] + 0.6, r[2] - 0.3, r[3] + 0.3])) {
                        feats.push((r, kind));
                    }
                    u += CELL;
                }
            }
            hs.extend(feats.iter().map(|f| f.0));
            let su = cell_lines(w.u0, w.u1, if service { 1 } else { 2 });
            let mut sv = Vec::new();
            if let (Some(dd), true) = (dado, w.bottom) {
                sv.push(w.y0 + dd);
            }
            vface(b, w.x_plane, w.plane, w.nsign, w.u0, w.u1, w.y0, w.y1, w.thick, &su, &sv, &hs, 2.0, Some(2), 3);
            let n = if w.x_plane { Vec3::X * w.nsign } else { Vec3::Z * w.nsign };
            for &(h, kind) in &feats {
                let origin = if w.x_plane { Vec3::new(w.plane, 0.0, 0.0) } else { Vec3::new(0.0, 0.0, w.plane) };
                let u_ax = if w.x_plane { Vec3::Z } else { Vec3::X };
                let f = Frame::new(origin, u_ax, Vec3::Y, n);
                let depth = (t - 0.08).min(0.35);
                kit::recess(b, f, Rect2::new(h[0], h[1], h[2], h[3]), depth, 1.0, Some(2));
                let (cu, wd, hh) = ((h[0] + h[1]) * 0.5, h[1] - h[0], h[3] - h[2]);
                match kind {
                    0 => {
                        // A door leaf in the recess: rails, studs, a threshold.
                        let c = f.at(cu, h[2] + hh * 0.5, -depth + 0.04);
                        kit::block(b, c, [u_ax, Vec3::Y, n], Vec3::new(wd * 0.5 - 0.04, hh * 0.5 - 0.02, 0.03), [false, false, false, false, false, true], Vec3::Y, 1.0, [None, None, None], false);
                        for k in 1..4 {
                            let y = h[2] + hh * k as f32 / 4.0;
                            b.stroke(f.at(h[0] + 0.06, y, -depth + 0.075), f.at(h[1] - 0.06, y, -depth + 0.075), n, n, StrokeKind::Seam, 2);
                            for j in 0..4 {
                                let uu = h[0] + 0.15 + (wd - 0.3) * j as f32 / 3.0;
                                b.stroke(f.at(uu, y + 0.04, -depth + 0.075), f.at(uu, y + 0.09, -depth + 0.075), n, n, StrokeKind::Seam, 3);
                            }
                        }
                        let tc = f.at(cu, h[2] + 0.04, -depth * 0.5);
                        kit::block(b, tc, [u_ax, Vec3::Y, n], Vec3::new(wd * 0.5, 0.04, depth * 0.5 + 0.05), [false, false, false, true, false, true], u_ax, 1.0, [Some(3), None, None], false);
                    }
                    1 => {
                        // A sill under the niche.
                        let c = f.at(cu, h[2] - 0.06, 0.08);
                        kit::block(b, c, [u_ax, Vec3::Y, n], Vec3::new(wd * 0.5 + 0.12, 0.06, 0.1), [true, true, true, true, false, true], u_ax, 1.0, [Some(2), None, Some(3)], false);
                    }
                    _ => {
                        // Ribs and a rail of the structure behind a missing panel.
                        for k in 0..3 {
                            let uu = h[0] + wd * (k as f32 + 0.5) / 3.0;
                            let c = f.at(uu, h[2] + hh * 0.5, -depth + 0.12);
                            kit::block(b, c, [u_ax, Vec3::Y, n], Vec3::new(0.07, hh * 0.5, 0.1), [true, true, false, false, false, true], Vec3::Y, 1.0, [None, Some(2), None], false);
                        }
                        let c = f.at(cu, h[2] + hh * 0.6, -depth + 0.18);
                        kit::block(b, c, [u_ax, Vec3::Y, n], Vec3::new(wd * 0.5, 0.06, 0.05), [false, false, true, true, false, true], u_ax, 1.0, [Some(2), None, None], false);
                        // Broken edge of the panel left in the wall.
                        b.stroke(f.at(h[0], h[3] - 0.2, 0.002), f.at(h[0] + 0.4, h[3], 0.002), n, n, StrokeKind::Crack, 2);
                    }
                }
            }
            // A conduit along the wall under the ceiling, on brackets.
            if conduit && w.bottom && w.top && (w.thick - t).abs() < 1e-3 && w.u1 - w.u0 > 3.0 {
                let y = w.y1 - 0.5;
                if !hs.iter().any(|h| h[3] > y - 0.25 && h[2] < y + 0.25) {
                    let at3 = |u: f32, yy: f32, o: f32| if w.x_plane { Vec3::new(w.plane, yy, u) } else { Vec3::new(u, yy, w.plane) } + n * o;
                    pipe(b, at3(w.u0 + 0.1, y, 0.22), at3(w.u1 - 0.1, y, 0.22), 0.09);
                    let mut u = w.u0 + 1.0;
                    while u < w.u1 - 0.5 {
                        let u_ax = if w.x_plane { Vec3::Z } else { Vec3::X };
                        kit::block(b, at3(u, y - 0.12, 0.12), [u_ax, Vec3::Y, n], Vec3::new(0.04, 0.03, 0.12), [true, true, true, true, false, true], u_ax, 1.0, [None, None, None], false);
                        u += 2.0;
                    }
                }
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
            if style == PassStyle::Vent {
                // A sloped duct floor, crawled up or slid down.
                let foot = foot.with_y(lo as f32 * LEVEL + VENT_LIP);
                let rise = d * (3.0 * CELL) + Vec3::Y * LEVEL;
                let across = d.cross(Vec3::Y).normalize();
                let n = across.cross(rise).normalize();
                let n = if n.y < 0.0 { -n } else { n };
                let coarse = b.new_coarse();
                b.rect(foot - across * (w * 0.5), rise, across * w, n, rise, 1.5, coarse);
                b.collider(crate::mesh::Collider::Ramp { start: foot.to_array(), end: (foot + rise).to_array(), half_width: w * 0.5, thickness: 0.3 });
                for k in 1..6 {
                    let p = foot + rise * (k as f32 / 6.0);
                    b.stroke(p - across * (w * 0.5), p + across * (w * 0.5), n, n, StrokeKind::Seam, 3);
                }
            } else {
                kit::stair(b, &Stair { foot: foot.with_y(lo as f32 * LEVEL), dir: d, width: w, height: LEVEL, risers: Some(RISERS_PER_LEVEL), max_risers: 18, landing: 1.2, waist: 0.3, wall: None, railing: false, enclosed: true });
            }
        }
    }
    // Members across the passage at cell boundaries on straight runs, by section.
    let (every, member, beam): (usize, Option<(f32, f32)>, Option<f32>) = match style {
        PassStyle::Plain | PassStyle::Gallery => (2, Some((0.3, 0.15)), Some(0.6)),
        PassStyle::Colonnade => (1, Some((0.45, 0.35)), Some(1.0)),
        PassStyle::Low => (1, None, Some(0.45)),
        PassStyle::Duct => (1, None, Some(0.35)),
        _ => (0, None, None),
    };
    for (k, pc) in ps.iter().enumerate() {
        if !pc.flat || every == 0 || k % every != 0 {
            continue;
        }
        for d in [Dir::XP, Dir::ZP] {
            let Some((ny0, _)) = pc.conn[d.index()] else { continue };
            let perp: [Dir; 2] = if d.is_x() { [Dir::ZN, Dir::ZP] } else { [Dir::XN, Dir::XP] };
            if (ny0 - pc.y0).abs() > 1e-3 || perp.iter().any(|q| pc.conn[q.index()].is_some()) {
                continue;
            }
            let next = ps.iter().find(|n| { let (dx, dz) = d.d(); n.x == pc.x + dx && n.z == pc.z + dz && (n.y0 - pc.y0).abs() < 1e-3 });
            if next.is_none_or(|n| !n.flat || perp.iter().any(|q| n.conn[q.index()].is_some())) {
                continue;
            }
            let along = d.vec();
            let across = along.cross(Vec3::Y).normalize();
            let centre = Vec3::new((pc.x as f32 + 0.5) * CELL, pc.y0, (pc.z as f32 + 0.5) * CELL) + along * (CELL * 0.5);
            let half_w = CELL * 0.5 - t;
            let hh = pc.y1 - pc.y0;
            if let Some((ma, md)) = member {
                for sg in [-1.0f32, 1.0] {
                    let c = centre + across * (sg * (half_w - md)) + Vec3::Y * (hh * 0.5);
                    kit::block(b, c, [along, Vec3::Y, across], Vec3::new(ma, hh * 0.5, md), [true, true, false, false, sg > 0.0, sg < 0.0], Vec3::Y, 1.5, [None, Some(2), None], true);
                    if style == PassStyle::Colonnade {
                        // Base and capital.
                        for (yy, hy) in [(0.25, 0.25), (hh - 1.25, 0.15)] {
                            let c2 = centre + across * (sg * (half_w - md - 0.08)) + Vec3::Y * yy;
                            kit::block(b, c2, [along, Vec3::Y, across], Vec3::new(ma + 0.1, hy, md + 0.08), [true, true, false, true, sg > 0.0, sg < 0.0], Vec3::Y, 1.5, [Some(2), None, Some(3)], false);
                        }
                    }
                }
            }
            if let Some(bd) = beam {
                let c = centre + Vec3::Y * (hh - bd * 0.5);
                kit::block(b, c, [along, Vec3::Y, across], Vec3::new(0.3, bd * 0.5, half_w - 0.01), [true, true, true, false, false, false], across, 1.5, [Some(2), None, Some(2)], false);
            }
        }
    }
    // Impost courses along the walls under a vault.
    for (k, pc) in ps.iter().enumerate() {
        if !vaulted[k] {
            continue;
        }
        let axis_x = pc.conn[Dir::XP.index()].is_some();
        let along = if axis_x { Vec3::X } else { Vec3::Z };
        let across = if axis_x { Vec3::Z } else { Vec3::X };
        let c = Vec3::new((pc.x as f32 + 0.5) * CELL, pc.y0 + spring - 0.15, (pc.z as f32 + 0.5) * CELL);
        for sg in [-1.0f32, 1.0] {
            let cc = c + across * (sg * (r_v - 0.1));
            kit::block(b, cc, [along, Vec3::Y, across], Vec3::new(CELL * 0.5, 0.15, 0.1), [false, false, true, true, sg > 0.0, sg < 0.0], along, 2.0, [Some(2), None, None], false);
        }
    }
    // Pipes along the ceiling of a duct.
    if style == PassStyle::Duct {
        let offs = [(-0.55f32, 0.16f32), (-0.1, 0.1), (0.5, 0.2)];
        for pc in ps.iter().filter(|pc| pc.flat) {
            let c = Vec3::new((pc.x as f32 + 0.5) * CELL, pc.y1, (pc.z as f32 + 0.5) * CELL);
            let conns: Vec<Dir> = Dir::ALL.into_iter().filter(|d| pc.conn[d.index()].is_some()).collect();
            for d in &conns {
                let along = d.vec();
                let across = along.cross(Vec3::Y).normalize();
                for &(o, rad) in &offs {
                    let y = -0.3 - rad;
                    let a = c + across * o + Vec3::Y * y;
                    pipe(b, a, a + along * (CELL * 0.5), rad);
                }
            }
            if conns.len() != 2 || conns[0].opposite() != conns[1] {
                // A junction box where the pipes turn.
                kit::block(b, c - Vec3::Y * 0.55, [Vec3::X, Vec3::Y, Vec3::Z], Vec3::new(0.9, 0.35, 0.9), [true, true, true, false, true, true], Vec3::Y, 1.0, [Some(2), Some(2), Some(2)], false);
            }
        }
    }
}

/// At each mouth of a vent, a grille: torn off and lying on the floor below the mouth, or
/// hanging from its top edge.
fn vent_grille(p: &Plan, b: &mut MeshBuilder, o: &Portal) {
    let is_vent = |s: Option<usize>| s.is_some_and(|s| p.graph.spaces[s].kind == SpaceKind::Vent);
    if !(is_vent(o.between[0]) ^ is_vent(o.between[1])) {
        return;
    }
    if !o.walk {
        // An intact grille across a look-through mouth, with a collider.
        let pm = if is_vent(o.between[0]) { o.p1 - 0.05 } else { o.p0 + 0.05 };
        let axis = if o.x_plane { Vec3::X } else { Vec3::Z };
        let along = if o.x_plane { Vec3::Z } else { Vec3::X };
        let at = |u: f32, y: f32| if o.x_plane { Vec3::new(pm, y, u) } else { Vec3::new(u, y, pm) };
        let mut u = o.u0 + 0.12;
        while u < o.u1 - 0.05 {
            kit::block(b, at(u, (o.y0 + o.y1) * 0.5), [along, Vec3::Y, axis], Vec3::new(0.02, (o.y1 - o.y0) * 0.5, 0.02), [true, true, false, false, true, true], Vec3::Y, 1.0, [None, Some(3), None], false);
            u += 0.15;
        }
        let c = at((o.u0 + o.u1) * 0.5, (o.y0 + o.y1) * 0.5);
        b.collider(kit::solid(c, [along, Vec3::Y, axis], Vec3::new((o.u1 - o.u0) * 0.5, (o.y1 - o.y0) * 0.5, 0.04)));
        return;
    }
    // The open side (not the vent) and its face.
    let (face, out) = if is_vent(o.between[0]) { (o.p1, 1.0f32) } else { (o.p0, -1.0f32) };
    let axis = if o.x_plane { Vec3::X } else { Vec3::Z };
    let along = if o.x_plane { Vec3::Z } else { Vec3::X };
    let um = (o.u0 + o.u1) * 0.5;
    let w = o.u1 - o.u0;
    let at = |u: f32, y: f32, d: f32| if o.x_plane { Vec3::new(face, y, u) } else { Vec3::new(u, y, face) } + axis * (out * d);
    let mut r = Rng::new(crate::seed::hash_ints(p.seed, &[(um * 10.0) as i64, (o.y0 * 10.0) as i64, (face * 10.0) as i64]));
    let room_floor = o.y0 - VENT_LIP;
    let (c, axes) = if r.chance(0.5) {
        // Lying on the floor in front of the mouth, askew.
        let a = r.range(-0.4, 0.4);
        let u2 = (along * a.cos() + axis * a.sin()).normalize();
        let v2 = Vec3::Y.cross(u2).normalize();
        (at(um, room_floor + 0.03, 0.9), [u2, Vec3::Y, v2])
    } else {
        // Hanging from its top edge, swung out.
        let tilt = r.range(0.5, 1.1);
        let down = (-Vec3::Y * tilt.cos() + axis * (out * tilt.sin())).normalize();
        let n = along.cross(down).normalize();
        (at(um, o.y1, 0.05) + down * (w * 0.5), [along, n, down])
    };
    let half = Vec3::new(w * 0.5, 0.025, w * 0.5);
    kit::block(b, c, axes, half, [true, true, true, true, true, true], axes[0], 1.0, [Some(2), Some(2), Some(2)], false);
    for k in 1..6 {
        let s = -w * 0.5 + w * k as f32 / 6.0;
        b.stroke(c + axes[0] * s - axes[2] * (w * 0.5) + axes[1] * 0.03, c + axes[0] * s + axes[2] * (w * 0.5) + axes[1] * 0.03, axes[1], axes[1], StrokeKind::Seam, 2);
    }
}

/// A side room's door leaf standing ajar, swung into the room on its hinge.
fn ajar_leaf(p: &Plan, b: &mut MeshBuilder, sd: &SideDoor) {
    let Some(o) = p.portals.iter().find(|o| o.walk && o.between.contains(&Some(sd.room)) && o.between.contains(&Some(sd.passage))) else { return };
    let room_face = if sd.dir.sign() > 0.0 { o.p1 } else { o.p0 };
    let into = sd.dir.vec();
    let along = if o.x_plane { Vec3::Z } else { Vec3::X };
    let w = o.u1 - o.u0;
    let h = o.y1 - o.y0;
    let hinge = if o.x_plane { Vec3::new(room_face, o.y0, o.u0) } else { Vec3::new(o.u0, o.y0, room_face) };
    let ang: f32 = 1.5;
    let dir = (along * ang.cos() + into * ang.sin()).normalize();
    let n = Vec3::Y.cross(dir).normalize();
    let c = hinge + dir * (w * 0.5) + Vec3::Y * (h * 0.5) + n * 0.03;
    kit::block(b, c, [dir, Vec3::Y, n], Vec3::new(w * 0.5, h * 0.5 - 0.02, 0.04), [true, true, true, true, true, true], Vec3::Y, 1.0, [Some(2), Some(2), Some(2)], true);
}

/// A rectangular duct seen from outside, from `a` to `c`: the casing, a flange every 2.5 m,
/// seams along it. Solid (it can be stood on); not crawlable.
fn exposed_duct(b: &mut MeshBuilder, a: Vec3, c: Vec3, w: f32, h: f32) {
    let along = (c - a).normalize();
    let across = along.cross(Vec3::Y).normalize();
    let len = (c - a).length();
    kit::block(b, (a + c) * 0.5, [along, Vec3::Y, across], Vec3::new(len * 0.5, h * 0.5, w * 0.5), [false, false, true, true, true, true], along, 2.0, [Some(1), None, None], true);
    let n = (len / 2.5).floor() as usize;
    for k in 1..n {
        let p = a + along * (len * k as f32 / n as f32);
        kit::block(b, p, [along, Vec3::Y, across], Vec3::new(0.06, h * 0.5 + 0.07, w * 0.5 + 0.07), [true, true, true, true, true, true], Vec3::Y, 1.0, [None, Some(3), Some(3)], false);
    }
    for sg in [-1.0f32, 1.0] {
        let o = across * (sg * (w * 0.5 + 0.002));
        b.stroke(a + o + Vec3::Y * (h * 0.15), c + o + Vec3::Y * (h * 0.15), across * sg, across * sg, StrokeKind::Seam, 3);
    }
}

/// A grating over a dark pit 1.2 m deep, in the floor rectangle given (the floor's collider
/// stays, so it is walked on).
fn grating(b: &mut MeshBuilder, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, coarse: u32) {
    // Within the floor slab, so the pit never shows in the space below.
    let d = 0.5;
    let yb = y - d;
    b.rect(Vec3::new(x0, yb, z0), Vec3::X * (x1 - x0), Vec3::Z * (z1 - z0), Vec3::Y, Vec3::X, 2.0, coarse);
    b.rect(Vec3::new(x0, yb, z0), Vec3::Z * (z1 - z0), Vec3::Y * d, Vec3::X, Vec3::Y, 2.0, coarse);
    b.rect(Vec3::new(x1, yb, z0), Vec3::Z * (z1 - z0), Vec3::Y * d, -Vec3::X, Vec3::Y, 2.0, coarse);
    b.rect(Vec3::new(x0, yb, z0), Vec3::X * (x1 - x0), Vec3::Y * d, Vec3::Z, Vec3::Y, 2.0, coarse);
    b.rect(Vec3::new(x0, yb, z1), Vec3::X * (x1 - x0), Vec3::Y * d, -Vec3::Z, Vec3::Y, 2.0, coarse);
    // Bars one way, a frame round.
    let mut x = x0 + 0.2;
    while x < x1 - 0.1 {
        b.stroke(Vec3::new(x, y + 0.01, z0), Vec3::new(x, y + 0.01, z1), Vec3::Y, Vec3::Y, StrokeKind::Seam, 2);
        x += 0.2;
    }
    for (a, c) in [((x0, z0), (x1, z0)), ((x1, z0), (x1, z1)), ((x1, z1), (x0, z1)), ((x0, z1), (x0, z0))] {
        b.stroke(Vec3::new(a.0, y + 0.01, a.1), Vec3::new(c.0, y + 0.01, c.1), Vec3::Y, Vec3::Y, StrokeKind::Crease, 1);
    }
}

/// A barrel vault over a straight passage piece, smooth shaded, with ribs at its ends.
fn vault(b: &mut MeshBuilder, pc: &Piece, axis_x: bool, spring: f32, r: f32, coarse: u32) {
    let (along, across) = if axis_x { (Vec3::X, Vec3::Z) } else { (Vec3::Z, Vec3::X) };
    let c = Vec3::new((pc.x as f32 + 0.5) * CELL, spring, (pc.z as f32 + 0.5) * CELL);
    let start = c - along * (CELL * 0.5);
    let n = 12;
    let fine = b.panel();
    let at = |i: usize, j: usize| {
        let th = std::f32::consts::PI * i as f32 / n as f32;
        let p = start + along * (j as f32 * CELL * 0.5) + across * (r * th.cos()) + Vec3::Y * (r * th.sin());
        (p, -(across * th.cos() + Vec3::Y * th.sin()), along)
    };
    b.smooth_grid(n + 1, 3, &at, fine, coarse);
    for i in 0..n {
        let (p0, n0, _) = at(i, 0);
        let (p1, _, _) = at(i + 1, 0);
        b.stroke(p0, p1, n0, along, StrokeKind::Seam, 2);
    }
}

/// The face closing the space above a vault where it meets a taller piece: between the
/// vault's arch and the rectangle up to `top`, on the boundary on side `d`, facing out.
fn spandrel(b: &mut MeshBuilder, pc: &Piece, d: Dir, spring: f32, r: f32, top: f32, coarse: u32) {
    let across = if d.is_x() { Vec3::Z } else { Vec3::X };
    let c = Vec3::new((pc.x as f32 + 0.5) * CELL, spring, (pc.z as f32 + 0.5) * CELL) + d.vec() * (CELL * 0.5);
    let n = 12;
    let fine = b.panel();
    let arc = |i: usize| {
        let th = std::f32::consts::PI * i as f32 / n as f32;
        c + across * (r * th.cos()) + Vec3::Y * (r * th.sin())
    };
    let normal = d.vec();
    for half in [0, 1] {
        let corner = c + across * (if half == 0 { r } else { -r }) + Vec3::Y * (top - spring);
        let range = if half == 0 { 0..n / 2 } else { n / 2..n };
        for i in range {
            let (a, cc) = (arc(i), arc(i + 1));
            if (cc - a).cross(corner - a).length() > 1e-5 {
                b.polygon(&[corner, a, cc], normal, Vec3::Y, fine, coarse);
            }
        }
    }
    // The flat band above the crown, if the taller piece goes higher.
    if top > spring + r + 0.01 {
        let _ = top;
    }
    for i in 0..n {
        b.stroke(arc(i), arc(i + 1), normal, normal, StrokeKind::Crease, 2);
    }
}

/// A round pipe from `a` to `c`, smooth shaded, a ring at each end.
fn pipe(b: &mut MeshBuilder, a: Vec3, c: Vec3, r: f32) {
    let coarse = b.new_coarse();
    let fine = b.panel();
    let along = (c - a).normalize();
    let refv = if along.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let u = along.cross(refv).normalize();
    let v = along.cross(u).normalize();
    let n = 6;
    let at = |i: usize, j: usize| {
        let th = 2.0 * std::f32::consts::PI * i as f32 / n as f32;
        let dir = u * th.cos() + v * th.sin();
        (if j == 0 { a } else { c } + dir * r, dir, along)
    };
    b.smooth_grid(n + 1, 2, &at, fine, coarse);
    for i in 0..n {
        let (p0, n0, _) = at(i, 0);
        let (p1, _, _) = at(i + 1, 0);
        b.stroke(p0, p1, n0, along, StrokeKind::Seam, 3);
    }
    b.stroke(at(n / 4, 0).0, at(n / 4, 1).0, v, v, StrokeKind::Crease, 3);
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
        b.set_group(p.volume(p.ledges[br.a].space));
        let (la, lb) = (&p.ledges[br.a], &p.ledges[br.b]);
        let y = la.level as f32 * LEVEL;
        let a = wall_point(la.side, p.rooms[la.room].face(la.side), la.depth, br.u, y);
        let c = wall_point(lb.side, p.rooms[lb.room].face(lb.side), lb.depth, br.u, y);
        kit::bridge(b, &kit::Bridge { a, c, width: br.width, deck: 0.8, girder: if br.girder { Some((2.4, 1.8)) } else { None }, ribs: Some(4.0), railings: br.railings, rank: 0 });
    }
    for ws in &p.wall_stairs {
        b.set_group(p.volume(p.ledges[ws.lower].space));
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

/// Room for a frame beside a door on one face: half the free width of that face around
/// the door's centre, and the height of the space (passages are narrow and may be low).
fn face_room(p: &Plan, space: Option<usize>) -> (f32, f32) {
    let Some(s) = space else { return (0.0, 0.0) };
    if let Some(pass) = p.passages.iter().find(|q| q.space == s) {
        return (CELL * 0.5 - pass.inset(), pass.height());
    }
    (f32::MAX, f32::MAX)
}

/// A door frame on both faces of a walkable door, its weight by what the door leads into:
/// jambs and a lintel standing proud of the wall, and for the heaviest a raised threshold.
fn door_frame(p: &Plan, b: &mut MeshBuilder, o: &Portal) {
    if o.kind != PortalKind::Door || o.frame == 0 {
        return;
    }
    let (fw, fd, lh): (f32, f32, f32) = match o.frame {
        1 => (0.22, 0.08, 0.25),
        2 => (0.45, 0.18, 0.55),
        _ => (0.8, 0.35, 1.1),
    };
    let w = o.u1 - o.u0;
    let um = (o.u0 + o.u1) * 0.5;
    let axis = if o.x_plane { Vec3::X } else { Vec3::Z };
    let along = if o.x_plane { Vec3::Z } else { Vec3::X };
    for (side, plane, nsign) in [(0, o.p0, -1.0f32), (1, o.p1, 1.0)] {
        let (half_room, height) = face_room(p, o.between[side]);
        let fw = fw.min(half_room - w * 0.5 - 0.02);
        if fw < 0.08 {
            continue;
        }
        let lh = lh.min(height - (o.y1 - o.y0) - 0.02);
        let n = axis * nsign;
        let at = |u: f32, y: f32| if o.x_plane { Vec3::new(plane, y, u) } else { Vec3::new(u, y, plane) } + n * (fd * 0.5);
        let jh = o.y1 - o.y0 + lh.max(0.0);
        for sg in [-1.0f32, 1.0] {
            let c = at(um + sg * (w * 0.5 + fw * 0.5), o.y0 + jh * 0.5);
            kit::block(b, c, [along, Vec3::Y, axis], Vec3::new(fw * 0.5, jh * 0.5, fd * 0.5), [true, true, false, true, nsign < 0.0, nsign > 0.0], Vec3::Y, 1.5, [Some(2), Some(1), Some(2)], true);
        }
        if lh > 0.08 {
            let c = at(um, o.y1 + lh * 0.5);
            kit::block(b, c, [along, Vec3::Y, axis], Vec3::new(w * 0.5, lh * 0.5, fd * 0.5), [false, false, true, true, nsign < 0.0, nsign > 0.0], along, 1.5, [Some(1), None, Some(2)], false);
        }
    }
    if o.frame == 3 {
        let c = if o.x_plane { Vec3::new((o.p0 + o.p1) * 0.5, o.y0 + 0.06, um) } else { Vec3::new(um, o.y0 + 0.06, (o.p0 + o.p1) * 0.5) };
        kit::block(b, c, [along, Vec3::Y, axis], Vec3::new(w * 0.5, 0.06, (o.p1 - o.p0) * 0.5 + 0.25), [false, false, false, true, true, true], along, 1.5, [Some(3), None, Some(2)], true);
    }
}

/// The outside of the region: four walls and a roof, with holes where windows, loggias and
/// the voids open. No colliders (nothing walks out there).
fn skin(p: &Plan, b: &mut MeshBuilder) {
    let (x0, x1, z0, z1) = (-SKIN, nx() as f32 * CELL + SKIN, -SKIN, nz() as f32 * CELL + SKIN);
    let (ybot, ytop) = (-60.0, ny() as f32 * LEVEL);
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
