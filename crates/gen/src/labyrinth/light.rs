//! Openness: how much sky light reaches a place, at the scale of whole spaces. The shaft
//! used an analytic function of depth; here light is propagated on the cell grid from where
//! the sky gets in (the open tops of the voids, windows and loggias) through the spaces and
//! the openings between them, losing a fraction per cell that depends on the kind of space,
//! and a larger fraction through each door. Deep enclosed places end up near black, the void
//! tops near white. It is the "light through the coarse occupancy grid" idea parked in
//! `docs/PLAN.md` section 7a, at the scale of one region.

use super::graph::SpaceKind;
use super::grid::*;
use super::plan::{Plan, PortalKind};
use super::*;
use glam::Vec3;
use std::collections::BinaryHeap;

/// Darkest openness anywhere.
pub const OPEN_FLOOR: f32 = 0.03;

pub struct LightField {
    /// Per cell: openness, and the space it belongs to (u32::MAX for mass).
    value: Vec<f32>,
    owner: Vec<u32>,
}

fn idx(c: Cell) -> usize {
    ((c.y * NZ + c.z) * NX + c.x) as usize
}

/// Fraction kept per cell moved inside a space of this kind (horizontally, vertically).
fn keep(kind: SpaceKind) -> (f32, f32) {
    match kind {
        SpaceKind::Void | SpaceKind::LightWell => (0.985, 0.962),
        SpaceKind::Shaft => (0.97, 0.985),
        SpaceKind::StairHall => (0.95, 0.94),
        SpaceKind::Cistern => (0.85, 0.85),
        SpaceKind::Antechamber => (0.85, 0.85),
        SpaceKind::GreatHall => (0.955, 0.94),
        SpaceKind::Loggia => (0.9, 0.9),
        SpaceKind::StairTower => (0.9, 0.93),
        SpaceKind::Undercroft => (0.7, 0.75),
        SpaceKind::Corridor => (0.9, 0.9),
        SpaceKind::ServicePassage => (0.8, 0.8),
        _ => (0.82, 0.82),
    }
}

/// The darkest openness a kind of space falls to.
fn darkest(kind: SpaceKind) -> f32 {
    // The shading closes to black below a light of about 0.26 (`black_point` plus
    // `black_range` in `nihei_ink`, E066), so the passages are kept above it and only the
    // deepest places may fall below.
    match kind {
        SpaceKind::Corridor | SpaceKind::Junction | SpaceKind::Antechamber | SpaceKind::StairLanding | SpaceKind::Loggia => 0.46,
        SpaceKind::StairTower | SpaceKind::ServicePassage | SpaceKind::StairHall | SpaceKind::GreatHall => 0.36,
        SpaceKind::Undercroft | SpaceKind::Chamber | SpaceKind::Cistern => OPEN_FLOOR,
        _ => 0.2,
    }
}

impl LightField {
    pub fn new(p: &Plan) -> Self {
        let n = (NX * NY * NZ) as usize;
        let mut owner = vec![u32::MAX; n];
        for y in 0..NY {
            for z in 0..NZ {
                for x in 0..NX {
                    let c = Cell::new(x, y, z);
                    if let Occ::Space(s) = p.grid.get(c) {
                        owner[idx(c)] = s;
                    }
                }
            }
        }
        let kind = |s: u32| p.graph.spaces[s as usize].kind;
        let mut value = vec![0.0f32; n];
        // Sources: the top storey of open voids, and the cells behind windows to the outside.
        let mut src: Vec<(Cell, f32)> = Vec::new();
        for r in p.rooms.iter().filter(|r| r.open_top) {
            for z in r.b.z0..r.b.z1 {
                for x in r.b.x0..r.b.x1 {
                    src.push((Cell::new(x, NY - 1, z), 1.0));
                }
            }
        }
        // Links across portals: pairs of cells either side, with a factor.
        let mut links: Vec<(Cell, Cell, f32)> = Vec::new();
        let cell_at = |v: Vec3| Cell::new((v.x / CELL).floor() as i32, (v.y / LEVEL).floor() as i32, (v.z / CELL).floor() as i32);
        for o in &p.portals {
            let um = (o.u0 + o.u1) * 0.5;
            let ym = (o.y0 + 0.2).min(o.y1);
            let pt = |a: f32| if o.x_plane { Vec3::new(a, ym, um) } else { Vec3::new(um, ym, a) };
            let (ca, cb) = (cell_at(pt(o.p0 - 0.5)), cell_at(pt(o.p1 + 0.5)));
            let area = (o.u1 - o.u0) * (o.y1 - o.y0);
            match o.kind {
                PortalKind::Exterior => {
                    let inside = if o.between[0].is_some() { ca } else { cb };
                    if inside.in_bounds() {
                        let s = (0.35 + area / 60.0).min(0.95);
                        src.push((inside, s));
                    }
                }
                PortalKind::Door | PortalKind::Window => {
                    if ca.in_bounds() && cb.in_bounds() {
                        let f = (0.35 + area / 40.0).min(0.8);
                        links.push((ca, cb, f));
                    }
                }
            }
        }
        // The giant arch.
        {
            let a = &p.arch;
            let hall = &p.rooms[a.hall];
            let y = hall.floor_y() + 3.0;
            for dx in [-8.0f32, -4.0, 0.0, 4.0, 8.0] {
                for dy in [0.0f32, 6.0, 12.0, 18.0] {
                    let ca = cell_at(Vec3::new(a.x + dx, y + dy, hall.face(Dir::ZP) - 1.0));
                    let cb = cell_at(Vec3::new(a.x + dx, y + dy, p.rooms[a.void].face(Dir::ZN) + 1.0));
                    if ca.in_bounds() && cb.in_bounds() {
                        links.push((ca, cb, 0.97));
                    }
                }
            }
        }
        // Light shafts pour into the room below them.
        for r in p.rooms.iter().filter(|r| r.kind == SpaceKind::Shaft) {
            for z in r.b.z0..r.b.z1 {
                for x in r.b.x0..r.b.x1 {
                    let (a, c) = (Cell::new(x, r.b.y0, z), Cell::new(x, r.b.y0 - 1, z));
                    if c.in_bounds() {
                        links.push((a, c, 0.92));
                    }
                }
            }
        }
        let mut by_cell: std::collections::HashMap<Cell, Vec<(Cell, f32)>> = std::collections::HashMap::new();
        for &(a, c, f) in &links {
            by_cell.entry(a).or_default().push((c, f));
            by_cell.entry(c).or_default().push((a, f));
        }
        // Max-product propagation, brightest first (a Dijkstra on -log of the light).
        #[derive(PartialEq)]
        struct Item(f32, i32, i32, i32);
        impl Eq for Item {}
        impl PartialOrd for Item {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Item {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                self.0.total_cmp(&o.0).then((self.1, self.2, self.3).cmp(&(o.1, o.2, o.3)))
            }
        }
        let mut heap = BinaryHeap::new();
        for &(c, v) in &src {
            if owner[idx(c)] != u32::MAX && v > value[idx(c)] {
                value[idx(c)] = v;
                heap.push(Item(v, c.x, c.y, c.z));
            }
        }
        while let Some(Item(v, x, y, z)) = heap.pop() {
            let c = Cell::new(x, y, z);
            if v < value[idx(c)] {
                continue;
            }
            let s = owner[idx(c)];
            let (kh, kv) = keep(kind(s));
            let push = |nc: Cell, nv: f32, value: &mut Vec<f32>, heap: &mut BinaryHeap<Item>| {
                if nc.in_bounds() && owner[idx(nc)] != u32::MAX && nv > value[idx(nc)] + 1e-5 {
                    value[idx(nc)] = nv;
                    heap.push(Item(nv, nc.x, nc.y, nc.z));
                }
            };
            for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)] {
                let nc = Cell::new(x + dx, y + dy, z + dz);
                if !nc.in_bounds() || owner[idx(nc)] != s {
                    continue;
                }
                let k = if dy != 0 { kv } else { kh };
                push(nc, v * k, &mut value, &mut heap);
            }
            if let Some(l) = by_cell.get(&c) {
                for &(nc, f) in l {
                    push(nc, v * f, &mut value, &mut heap);
                }
            }
        }
        // Smooth within each space so tone does not kink at cell centres.
        for _ in 0..2 {
            let prev = value.clone();
            for y in 0..NY {
                for z in 0..NZ {
                    for x in 0..NX {
                        let c = Cell::new(x, y, z);
                        let s = owner[idx(c)];
                        if s == u32::MAX {
                            continue;
                        }
                        let (mut sum, mut w) = (prev[idx(c)] * 2.0, 2.0);
                        for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)] {
                            let nc = Cell::new(x + dx, y + dy, z + dz);
                            if nc.in_bounds() && owner[idx(nc)] == s {
                                sum += prev[idx(nc)];
                                w += 1.0;
                            }
                        }
                        value[idx(c)] = sum / w;
                    }
                }
            }
        }
        // Each kind of space has its own darkest value: passages and small rooms stay in grey
        // (pencil in shade), the undercroft, the chamber and the cistern's basin go to black.
        for (i, v) in value.iter_mut().enumerate() {
            let floor = if owner[i] == u32::MAX { OPEN_FLOOR } else { darkest(kind(owner[i])) };
            *v = v.max(floor);
        }
        Self { value, owner }
    }

    /// Openness of the cell, if it belongs to a space.
    pub fn cell(&self, c: Cell) -> Option<f32> {
        if !c.in_bounds() || self.owner[idx(c)] == u32::MAX {
            return None;
        }
        Some(self.value[idx(c)])
    }

    /// Openness at a surface point with normal `n`: the field just off the surface,
    /// interpolated between cell centres of the same space, times a facing term (light comes
    /// mostly from above). Outside the region it is open sky.
    pub fn openness(&self, p: Vec3, n: Vec3) -> f32 {
        let q = p + n * 0.45;
        let facing = 0.86 + 0.14 * n.y;
        let ext = NX as f32 * CELL;
        if q.x < 0.0 || q.z < 0.0 || q.x > ext || q.z > NZ as f32 * CELL || q.y > NY as f32 * LEVEL {
            return facing.min(1.0);
        }
        let g = Vec3::new(q.x / CELL - 0.5, q.y / LEVEL - 0.5, q.z / CELL - 0.5);
        let home = Cell::new((q.x / CELL).floor() as i32, (q.y / LEVEL).floor() as i32, (q.z / CELL).floor() as i32);
        let base = Cell::new(g.x.floor() as i32, g.y.floor() as i32, g.z.floor() as i32);
        let f = g - Vec3::new(base.x as f32, base.y as f32, base.z as f32);
        let own = if home.in_bounds() { self.owner[idx(home)] } else { u32::MAX };
        let (mut sum, mut wsum) = (0.0, 0.0);
        for dz in 0..2 {
            for dy in 0..2 {
                for dx in 0..2 {
                    let c = Cell::new(base.x + dx, base.y + dy, base.z + dz);
                    if !c.in_bounds() {
                        continue;
                    }
                    let o = self.owner[idx(c)];
                    if o == u32::MAX || (own != u32::MAX && o != own) {
                        continue;
                    }
                    let w = (if dx == 1 { f.x } else { 1.0 - f.x }) * (if dy == 1 { f.y } else { 1.0 - f.y }) * (if dz == 1 { f.z } else { 1.0 - f.z });
                    sum += self.value[idx(c)] * w;
                    wsum += w;
                }
            }
        }
        let v = if wsum > 1e-4 {
            sum / wsum
        } else if let Some(v) = self.cell(home) {
            v
        } else {
            // In the mass (a reveal, the arch's thickness): the brightest neighbouring space.
            let mut best: f32 = OPEN_FLOOR;
            for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)] {
                if let Some(v) = self.cell(Cell::new(home.x + dx, home.y + dy, home.z + dz)) {
                    best = best.max(v);
                }
            }
            best
        };
        (v * facing).clamp(0.0, 1.0)
    }
}
