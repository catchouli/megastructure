//! Director, graph grammar and embedding: from a seed to a [`Plan`], which says where every
//! space is and how they connect, with no triangles yet.
//!
//! 1. **Director** (`Director::new`): places the set pieces in the region as cell boxes: the
//!    deep void, the great hall against it (its giant arch on the void's axis, its far end on
//!    the sunlit exterior face), the undercroft at the bottom on one flank, the light well on
//!    the other, and picks the storeys of the hall, the spawn ledge and the light well's
//!    bridge.
//! 2. **Grammar** (`Builder::arc` and the chord and spur rules in `plan`): the core cycle
//!    through the set pieces is a list of arcs between ledges and rooms at given storeys.
//!    Each arc is rewritten into a chain: a stair tower if it changes many storeys, a
//!    junction if it is long, else one passage (corridor or service passage). Chords add a
//!    second and third cycle (through a stair climbing a void wall, and through the hall's
//!    gallery); spurs hang dead ends (loggias, an overlook, a dark chamber).
//! 3. **Embedding**: rooms the grammar creates are placed near the middle of their arc in
//!    free cells; then each passage is routed by A* through free cells from a door of one
//!    space to a door of the other, climbing or descending a storey with a 12 m stair run.
//!
//! Every choice draws from a generator seeded by `hash64(seed, stable id, layer)`.

use super::graph::*;
use super::grid::*;
use super::*;
use crate::seed::{hash64, hash_ints, layer, Rng};
use glam::Vec3;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// A space that is a box of cells (everything but passages and ledges).
#[derive(Clone, Copy, Debug)]
pub struct Room {
    pub space: usize,
    pub kind: SpaceKind,
    pub b: CBox,
    /// Distance from the cell boundary to the inner wall face.
    pub inset: f32,
    pub open_top: bool,
}

impl Room {
    pub fn floor_y(&self) -> f32 {
        self.b.y0 as f32 * LEVEL
    }

    pub fn ceiling_y(&self) -> f32 {
        if self.open_top {
            self.b.y1 as f32 * LEVEL
        } else {
            self.b.y1 as f32 * LEVEL - SLAB
        }
    }

    /// World coordinate of the inner wall face on side `d`.
    pub fn face(&self, d: Dir) -> f32 {
        self.b.plane(d) - d.sign() * self.inset
    }

    pub fn inner_min(&self) -> Vec3 {
        Vec3::new(self.face(Dir::XN), self.floor_y(), self.face(Dir::ZN))
    }

    pub fn inner_max(&self) -> Vec3 {
        Vec3::new(self.face(Dir::XP), self.ceiling_y(), self.face(Dir::ZP))
    }

    /// Inner extent along side `d`: (start, end) in world u (z for x sides, x for z sides).
    pub fn side_span(&self, d: Dir) -> (f32, f32) {
        if d.is_x() {
            (self.face(Dir::ZN), self.face(Dir::ZP))
        } else {
            (self.face(Dir::XN), self.face(Dir::XP))
        }
    }
}

/// One move of a routed passage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// A flat cell at its storey.
    Flat(Cell),
    /// A stair run leaving flat cell `from` in `dir`: three stair cells spanning two storeys,
    /// then the next flat cell, one storey up or down.
    Stair { from: Cell, dir: Dir, up: bool },
}

/// Where a passage meets another space.
#[derive(Clone, Copy, Debug)]
pub struct PortEnd {
    /// The passage's end cell.
    pub cell: Cell,
    /// From that cell into the other space.
    pub dir: Dir,
    pub target: usize,
    pub link: LinkKind,
}

#[derive(Clone, Debug)]
pub struct Passage {
    pub space: usize,
    pub kind: SpaceKind,
    pub path: Vec<Step>,
    pub ends: [PortEnd; 2],
}

impl Passage {
    pub fn inset(&self) -> f32 {
        passage_inset(self.kind)
    }

    pub fn height(&self) -> f32 {
        passage_height(self.kind)
    }

    /// Flat cells in order.
    pub fn flats(&self) -> impl Iterator<Item = Cell> + '_ {
        self.path.iter().filter_map(|s| if let Step::Flat(c) = s { Some(*c) } else { None })
    }

    pub fn stairs(&self) -> usize {
        self.path.iter().filter(|s| matches!(s, Step::Stair { .. })).count()
    }

    /// Horizontal length walked, metres.
    pub fn length(&self) -> f32 {
        self.path.iter().map(|s| if let Step::Stair { .. } = s { 3.0 * CELL } else { CELL }).sum()
    }
}

pub fn passage_inset(kind: SpaceKind) -> f32 {
    if kind == SpaceKind::ServicePassage {
        1.0
    } else {
        0.4
    }
}

pub fn passage_height(kind: SpaceKind) -> f32 {
    if kind == SpaceKind::ServicePassage {
        2.8
    } else {
        LEVEL - SLAB
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rail {
    Full,
    /// Railing with a missing stretch.
    Broken,
    None,
}

/// A ledge on a void wall.
#[derive(Clone, Debug)]
pub struct Ledge {
    pub space: usize,
    /// Index into `Plan::rooms` of the void.
    pub room: usize,
    /// The wall it stands against (the void's side).
    pub side: Dir,
    pub level: i32,
    pub depth: f32,
    /// World u extent (z for x walls, x for z walls), set after routing.
    pub span: (f32, f32),
    /// Extents it must cover (ports, bridge ends, the arch, stair ends).
    pub needs: Vec<(f32, f32)>,
    /// u ranges where no door may open (behind a wall stair).
    pub forbid: Vec<(f32, f32)>,
    /// Only this cell (u index) may hold a door.
    pub forced: Option<i32>,
    pub rail: Rail,
    pub rail_gaps: Vec<(f32, f32)>,
    /// No railing across the low and high u ends (a wall stair attaches there).
    pub open_end: [bool; 2],
}

#[derive(Clone, Copy, Debug)]
pub struct BridgeSpec {
    pub a: usize,
    pub b: usize,
    /// World coordinate along the two walls of the bridge's centre line.
    pub u: f32,
    pub width: f32,
    pub railings: bool,
    pub girder: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct WallStairSpec {
    pub lower: usize,
    pub upper: usize,
    /// World u where it leaves the lower ledge.
    pub u0: f32,
    /// +1 or -1 along u.
    pub dir: f32,
    pub levels: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct ArchSpec {
    pub hall: usize,
    pub void: usize,
    /// World x of the centre.
    pub x: f32,
    pub w: f32,
    pub height: f32,
    pub span: f32,
    pub spring: f32,
    pub ring: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct GallerySpec {
    pub space: usize,
    pub hall: usize,
    pub side: Dir,
    pub level: i32,
    /// World z extent of the gallery.
    pub z0: f32,
    pub z1: f32,
    pub depth: f32,
    /// The grand stair's foot (world z) and length; it climbs towards +z to `z0`.
    pub stair_foot: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalKind {
    /// A walkable door or passage mouth.
    Door,
    /// A window between two spaces.
    Window,
    /// A window or opening to the outside.
    Exterior,
}

/// An opening through the wall between two parallel faces.
#[derive(Clone, Copy, Debug)]
pub struct Portal {
    pub kind: PortalKind,
    /// The faces are planes of constant x (true) or z (false).
    pub x_plane: bool,
    pub p0: f32,
    pub p1: f32,
    pub u0: f32,
    pub u1: f32,
    pub y0: f32,
    pub y1: f32,
    /// The spaces on the low and high side (None outside the structure).
    pub between: [Option<usize>; 2],
    /// Whether a walker passes through (a floor is needed in the reveal).
    pub walk: bool,
}

/// Everything about the layout, before triangles.
pub struct Plan {
    pub seed: u64,
    /// Attempts the embedding needed (1 when the first routing succeeded).
    pub attempts: u32,
    pub graph: SpaceGraph,
    pub grid: Grid,
    pub rooms: Vec<Room>,
    pub room_of: Vec<Option<usize>>,
    pub passages: Vec<Passage>,
    pub ledges: Vec<Ledge>,
    pub bridges: Vec<BridgeSpec>,
    pub wall_stairs: Vec<WallStairSpec>,
    pub arch: ArchSpec,
    pub gallery: GallerySpec,
    pub portals: Vec<Portal>,
    /// Eye position and yaw at the start.
    pub spawn: (Vec3, f32),
    /// Which ledge the spawn stands on.
    pub spawn_space: usize,
}

/// Set pieces and the storeys they fix.
struct Director {
    void: CBox,
    hall: CBox,
    under: CBox,
    well: CBox,
    /// Hall floor storey.
    fh: i32,
    /// Spawn ledge storey.
    ks: i32,
    /// Light well bridge storey.
    kw: i32,
    /// Arch and bridge axis: cell column x.
    arch_cx: i32,
    /// Void wall carrying the wall stair, and its lower storey.
    stair_side: Dir,
    k1: i32,
    /// Overlook ledge side and storey.
    look_side: Dir,
    ko: i32,
    gallery_side: Dir,
}

impl Director {
    fn new(seed: u64) -> Self {
        let mut r = Rng::for_id(seed, "director", layer::DIRECTOR);
        let vw = r.int(10, 12);
        let vd = r.int(11, 12);
        let vx0 = r.int(27, 32);
        let vz0 = r.int(22, 26);
        let void = CBox::new(vx0, vx0 + vw, 0, NY, vz0, vz0 + vd);
        let fh = r.int(7, 9);
        let hh = r.int(6, 7);
        let hw = r.int(13, 15);
        let hx0 = (vx0 + vw / 2 - hw / 2 + r.int(-1, 1)).max(2);
        let hall = CBox::new(hx0, hx0 + hw, fh, fh + hh, 0, vz0 - 1);
        // The arch on a cell column inside both the hall and the void, near their middle.
        let lo = hx0.max(vx0) + 3;
        let hi = (hx0 + hw).min(vx0 + vw) - 4;
        let arch_cx = ((lo + hi) / 2 + r.int(-1, 1)).clamp(lo, hi.max(lo));
        let east = r.chance(0.5);
        let uw = r.int(12, 15);
        let ud = r.int(10, 13);
        let ux0 = if east { r.int(vx0 + vw + 4, NX - 2 - uw) } else { r.int(2, vx0 - 4 - uw) };
        let uz0 = r.int(vz0, NZ - 3 - ud);
        let under = CBox::new(ux0, ux0 + uw, 0, 2, uz0, uz0 + ud);
        let ws = r.int(6, 7);
        let kw0 = r.int(3, 5);
        let wx0 = if east { r.int(3, (hx0 - ws - 3).max(3)) } else { r.int((hx0 + hw + 3).min(NX - 3 - ws), NX - 3 - ws) };
        let wz0 = r.int(6, 16);
        let well = CBox::new(wx0, wx0 + ws, kw0, NY, wz0, wz0 + ws);
        let ks = fh + r.int(4, 6);
        let kw = (kw0 + r.int(5, 8)).min(NY - 6);
        let stair_side = if r.chance(0.5) { Dir::XN } else { Dir::XP };
        let k1 = r.int(2, 4);
        let look_side = stair_side.opposite();
        let ko = r.int(15, 20);
        let gallery_side = if east { Dir::XN } else { Dir::XP };
        Self { void, hall, under, well, fh, ks, kw, arch_cx, stair_side, k1, look_side, ko, gallery_side }
    }
}

/// One end of an arc or passage: a space at a storey.
#[derive(Clone, Copy, Debug)]
struct End {
    space: usize,
    level: i32,
}

struct Job {
    space: usize,
    a: End,
    b: End,
}

struct Builder {
    seed: u64,
    g: SpaceGraph,
    grid: Grid,
    rooms: Vec<Room>,
    room_of: Vec<Option<usize>>,
    ledges: Vec<Ledge>,
    ledge_of: Vec<Option<usize>>,
    jobs: Vec<Job>,
    passages: Vec<Passage>,
    gallery: Option<GallerySpec>,
}

impl Builder {
    fn space(&mut self, sid: &str, kind: SpaceKind, role: Role, level: i32) -> usize {
        let i = self.g.add(sid, kind, role, level);
        self.room_of.push(None);
        self.ledge_of.push(None);
        i
    }

    fn add_room(&mut self, space: usize, b: CBox, inset: f32, open_top: bool) -> usize {
        let kind = self.g.spaces[space].kind;
        self.grid.fill(&b, Occ::Space(space as u32));
        self.rooms.push(Room { space, kind, b, inset, open_top });
        self.room_of[space] = Some(self.rooms.len() - 1);
        self.rooms.len() - 1
    }

    fn add_ledge(&mut self, sid: &str, role: Role, void_space: usize, side: Dir, level: i32, depth: f32) -> usize {
        let s = self.space(sid, SpaceKind::Ledge, role, level);
        self.g.spaces[s].parent = Some(void_space);
        let room = self.room_of[void_space].unwrap();
        self.ledges.push(Ledge { space: s, room, side, level, depth, span: (0.0, 0.0), needs: Vec::new(), forbid: Vec::new(), forced: None, rail: Rail::Full, rail_gaps: Vec::new(), open_end: [false, false] });
        self.ledge_of[s] = Some(self.ledges.len() - 1);
        s
    }

    /// Plan position (cell units) of a space, for placing things between spaces.
    fn pos(&self, s: usize) -> (f32, f32) {
        if let Some(li) = self.ledge_of[s] {
            let l = &self.ledges[li];
            let r = &self.rooms[l.room];
            let (cx, cz) = r.b.centre_xz();
            let (dx, dz) = l.side.d();
            return (cx + dx as f32 * r.b.width() as f32 * 0.5, cz + dz as f32 * r.b.depth() as f32 * 0.5);
        }
        if let Some(ri) = self.room_of[s] {
            return self.rooms[ri].b.centre_xz();
        }
        if let Some(g) = self.gallery.filter(|g| g.space == s) {
            let r = &self.rooms[g.hall];
            let x = if g.side == Dir::XN { r.b.x0 as f32 } else { r.b.x1 as f32 };
            return (x, (g.z0 + g.z1) * 0.5 / CELL);
        }
        (NX as f32 * 0.5, NZ as f32 * 0.5)
    }

    /// Find a free box of the given size near `(tx, tz)`, searching outwards.
    #[allow(clippy::too_many_arguments)]
    fn place_near(&self, tx: f32, tz: f32, w: i32, d: i32, y0: i32, y1: i32, touch_edge: bool) -> Option<CBox> {
        let cx = (tx - w as f32 * 0.5).round() as i32;
        let cz = (tz - d as f32 * 0.5).round() as i32;
        for r in 0i32..28 {
            let mut cands: Vec<(i32, i32)> = Vec::new();
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dz.abs()) == r {
                        cands.push((dx, dz));
                    }
                }
            }
            cands.sort_by_key(|&(dx, dz)| (dx * dx + dz * dz, dz, dx));
            for (dx, dz) in cands {
                let b = CBox::new(cx + dx, cx + dx + w, y0, y1, cz + dz, cz + dz + d);
                if self.grid.box_free(&b, 1, touch_edge) {
                    return Some(b);
                }
            }
        }
        None
    }

    /// Rewrite an arc between two ends into spaces and passages. Returns the rooms it made.
    fn arc(&mut self, a: End, b: End, role: Role, sid: &str, depth: u32) -> Vec<usize> {
        let mut r = Rng::for_id(self.seed, sid, layer::GRAPH);
        let (ax, az) = self.pos(a.space);
        let (bx, bz) = self.pos(b.space);
        let dist = ((bx - ax).powi(2) + (bz - az).powi(2)).sqrt();
        let dk = (b.level - a.level).abs();
        let t = r.range(0.35, 0.65);
        let (mx, mz) = (ax + (bx - ax) * t + r.range(-5.0, 5.0), az + (bz - az) * t + r.range(-5.0, 5.0));
        if dk >= 4 && depth < 2 && r.chance(0.85) {
            let lo = a.level.min(b.level);
            let hi = a.level.max(b.level);
            let y0 = (lo - r.int(0, 1)).max(0);
            let y1 = (hi + 1 + r.int(0, 1)).min(NY);
            let edge = r.chance(0.5);
            if let Some(bx) = self.place_near(mx, mz, 3, 3, y0, y1, edge) {
                let s = self.space(&format!("{sid}/tower"), SpaceKind::StairTower, role, y0);
                self.add_room(s, bx, 0.5, false);
                let mut out = vec![s];
                out.extend(self.arc(a, End { space: s, level: a.level }, role, &format!("{sid}/0"), depth + 1));
                out.extend(self.arc(End { space: s, level: b.level }, b, role, &format!("{sid}/1"), depth + 1));
                return out;
            }
        }
        if dist > 16.0 && depth < 2 && r.chance(0.75) {
            let level = ((a.level + b.level) as f32 * 0.5).round() as i32 + r.int(-1, 1);
            let level = level.clamp(0, NY - 3);
            let (w, d) = (r.int(2, 4), r.int(2, 4));
            let h = if r.chance(0.4) { 2 } else { 1 };
            if let Some(bx) = self.place_near(mx, mz, w, d, level, level + h, false) {
                let s = self.space(&format!("{sid}/junction"), SpaceKind::Junction, role, level);
                self.add_room(s, bx, 0.4, false);
                let mut out = vec![s];
                out.extend(self.arc(a, End { space: s, level }, role, &format!("{sid}/0"), depth + 1));
                out.extend(self.arc(End { space: s, level }, b, role, &format!("{sid}/1"), depth + 1));
                return out;
            }
        }
        let kind = if r.chance(0.6) { SpaceKind::Corridor } else { SpaceKind::ServicePassage };
        self.passage(a, b, kind, role, sid);
        Vec::new()
    }

    fn passage(&mut self, a: End, b: End, kind: SpaceKind, role: Role, sid: &str) -> usize {
        let s = self.space(&format!("{sid}/passage"), kind, role, a.level.min(b.level));
        self.jobs.push(Job { space: s, a, b });
        s
    }

    /// Door candidates of an end: (cell outside the space, direction into it).
    fn candidates(&self, e: End) -> Vec<(Cell, Dir)> {
        let mut out = Vec::new();
        let sp = &self.g.spaces[e.space];
        if let Some(li) = self.ledge_of[e.space] {
            let l = &self.ledges[li];
            let r = &self.rooms[l.room];
            let (u0, u1) = r.b.side_cells(l.side);
            let corners = l.open_end[0] || l.open_end[1];
            let (ua, ub) = if corners { (u0, u1) } else { (u0 + 1, u1 - 1) };
            for u in ua..ub {
                if l.forced.is_some_and(|f| f != u) {
                    continue;
                }
                let (c0, c1) = (u as f32 * CELL, (u + 1) as f32 * CELL);
                if l.forbid.iter().any(|&(f0, f1)| c1 > f0 - 0.5 && c0 < f1 + 0.5) {
                    continue;
                }
                out.push((r.b.outside(l.side, l.level, u), l.side.opposite()));
            }
        } else if let Some(g) = self.gallery.filter(|g| g.space == e.space) {
            let r = &self.rooms[g.hall];
            let (u0, u1) = r.b.side_cells(g.side);
            for u in u0..u1 {
                let (c0, c1) = (u as f32 * CELL, (u + 1) as f32 * CELL);
                if c0 >= g.z0 + 1.0 && c1 <= g.z1 - 1.0 {
                    out.push((r.b.outside(g.side, g.level, u), g.side.opposite()));
                }
            }
        } else if let Some(ri) = self.room_of[e.space] {
            let r = &self.rooms[ri];
            if sp.kind == SpaceKind::StairTower {
                // Doors open on the landing of that storey; if those cells are taken, the
                // nearest storey that has one free will do (the passage takes up the
                // difference with stairs).
                let mut levels: Vec<i32> = (r.b.y0..r.b.y1).collect();
                levels.sort_by_key(|&l| ((l - e.level).abs(), l));
                for level in levels {
                    let m = level - r.b.y0;
                    let (cx, cz, sides) = if m % 2 == 0 { (r.b.x0, r.b.z0, [Dir::XN, Dir::ZN]) } else { (r.b.x1 - 1, r.b.z1 - 1, [Dir::XP, Dir::ZP]) };
                    let mut here = Vec::new();
                    for d in sides {
                        let u = if d.is_x() { cz } else { cx };
                        here.push((r.b.outside(d, level, u), d.opposite()));
                    }
                    here.retain(|&(c, d)| self.door_ok(c, d));
                    if !here.is_empty() {
                        return here;
                    }
                }
            } else {
                for d in Dir::ALL {
                    if sp.kind == SpaceKind::GreatHall && (d == Dir::ZP || d == Dir::ZN || Some(d) == self.gallery.map(|g| g.side)) {
                        continue;
                    }
                    let (u0, u1) = r.b.side_cells(d);
                    let (a, b) = if u1 - u0 >= 3 { (u0 + 1, u1 - 1) } else { (u0, u1) };
                    for u in a..b {
                        out.push((r.b.outside(d, e.level, u), d.opposite()));
                    }
                }
            }
        }
        out.retain(|&(c, d)| self.door_ok(c, d));
        out
    }

    /// A door cell must be free, in the region, and not right beside another door in the
    /// same wall.
    fn door_ok(&self, c: Cell, d: Dir) -> bool {
        c.in_bounds()
            && self.grid.is_free(c)
            && !self.passages.iter().any(|p| {
                p.ends.iter().any(|pe| {
                    pe.cell.y == c.y && pe.dir == d && if d.is_x() { pe.cell.x == c.x && (pe.cell.z - c.z).abs() <= 1 } else { pe.cell.z == c.z && (pe.cell.x - c.x).abs() <= 1 }
                })
            })
    }

    fn link_kind(&self, passage: SpaceKind, target: usize) -> LinkKind {
        match self.g.spaces[target].kind {
            SpaceKind::GreatHall | SpaceKind::HallGallery | SpaceKind::StairTower => LinkKind::Doorway,
            _ if passage == SpaceKind::ServicePassage => LinkKind::Doorway,
            _ => LinkKind::Opening,
        }
    }

    fn route_job(&mut self, k: usize) -> Result<(), String> {
        let (space, a, b) = {
            let j = &self.jobs[k];
            (j.space, j.a, j.b)
        };
        let kind = self.g.spaces[space].kind;
        let starts = self.candidates(a);
        let goals = self.candidates(b);
        let sid = self.g.spaces[space].sid.clone();
        if starts.is_empty() || goals.is_empty() {
            return Err(format!("{sid}: no door candidates ({} {} {} / {} {} {})", self.g.spaces[a.space].sid, a.level, starts.len(), self.g.spaces[b.space].sid, b.level, goals.len()));
        }
        let noise_seed = hash64(self.seed, &sid, layer::LAYOUT);
        // Door cells that later passages will need are expensive to pass through.
        let mut precious: Vec<Cell> = Vec::new();
        for j in &self.jobs[k + 1..] {
            for e in [j.a, j.b] {
                precious.extend(self.candidates(e).into_iter().map(|c| c.0));
            }
        }
        precious.retain(|c| !starts.iter().any(|s| s.0 == *c) && !goals.iter().any(|g| g.0 == *c));
        precious.sort();
        precious.dedup();
        let path = route(&self.grid, &starts, &goals, kind, noise_seed, &precious).ok_or_else(|| format!("{sid}: no route"))?;
        // Occupy, checking the path does not cross itself.
        let mut cells: Vec<Cell> = Vec::new();
        for s in &path {
            match *s {
                Step::Flat(c) => cells.push(c),
                Step::Stair { from, dir, up } => {
                    let lo = if up { from.y } else { from.y - 1 };
                    for i in 1..=3 {
                        let c = from.step(dir, i);
                        cells.push(Cell { y: lo, ..c });
                        cells.push(Cell { y: lo + 1, ..c });
                    }
                }
            }
        }
        let mut sorted = cells.clone();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != cells.len() {
            return Err(format!("{sid}: route crosses itself"));
        }
        for c in &cells {
            self.grid.set(*c, Occ::Space(space as u32));
        }
        let first = match path[0] {
            Step::Flat(c) => c,
            _ => unreachable!(),
        };
        let last = match *path.last().unwrap() {
            Step::Flat(c) => c,
            _ => unreachable!(),
        };
        let da = starts.iter().find(|(c, _)| *c == first).unwrap().1;
        let db = goals.iter().find(|(c, _)| *c == last).unwrap().1;
        let ends = [
            PortEnd { cell: first, dir: da, target: a.space, link: self.link_kind(kind, a.space) },
            PortEnd { cell: last, dir: db, target: b.space, link: self.link_kind(kind, b.space) },
        ];
        self.g.link(space, a.space, ends[0].link, self.g.spaces[space].role);
        self.g.link(space, b.space, ends[1].link, self.g.spaces[space].role);
        self.passages.push(Passage { space, kind, path, ends });
        Ok(())
    }
}

/// A* over the cell grid from any start door cell to any goal door cell. Flat moves cost one
/// cell; a stair move (three cells of stair over two storeys, then a flat cell one storey up
/// or down) costs five. Turns cost extra (more in corridors, which run straight), and a
/// seeded noise per cell bends routes so they are not all shortest paths.
fn route(grid: &Grid, starts: &[(Cell, Dir)], goals: &[(Cell, Dir)], kind: SpaceKind, seed: u64, precious: &[Cell]) -> Option<Vec<Step>> {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
    struct St {
        c: Cell,
        h: u8,
    }
    let turn: u32 = if kind == SpaceKind::Corridor { 160 } else { 40 };
    let noise_amp: u64 = if kind == SpaceKind::Corridor { 50 } else { 90 };
    let noise = |c: Cell| (hash_ints(seed, &[c.x as i64, c.y as i64, c.z as i64]) % noise_amp) as u32 + if precious.binary_search(&c).is_ok() { 1500 } else { 0 };
    let goal_set: Vec<Cell> = goals.iter().map(|g| g.0).collect();
    let heur = |c: Cell| -> u32 {
        goal_set
            .iter()
            .map(|g| {
                let hd = ((g.x - c.x).abs() + (g.z - c.z).abs()) as u32;
                let vd = (g.y - c.y).unsigned_abs() * 4;
                hd.max(vd) * 100
            })
            .min()
            .unwrap_or(0)
    };
    let mut open: BinaryHeap<Reverse<(u32, u32, u32, St)>> = BinaryHeap::new();
    let mut best: HashMap<St, u32> = HashMap::new();
    let mut parent: HashMap<St, (St, Option<(Dir, bool)>)> = HashMap::new();
    let mut counter = 0u32;
    for &(c, _) in starts {
        let s = St { c, h: 4 };
        best.insert(s, 0);
        open.push(Reverse((heur(c), 0, counter, s)));
        counter += 1;
    }
    let mut expansions = 0;
    while let Some(Reverse((_, g, _, s))) = open.pop() {
        if best.get(&s).is_some_and(|&b| b < g) {
            continue;
        }
        if goal_set.contains(&s.c) {
            // Rebuild.
            let mut steps = Vec::new();
            let mut cur = s;
            loop {
                match parent.get(&cur) {
                    None => {
                        steps.push(Step::Flat(cur.c));
                        break;
                    }
                    Some(&(prev, mv)) => {
                        steps.push(Step::Flat(cur.c));
                        if let Some((dir, up)) = mv {
                            steps.push(Step::Stair { from: prev.c, dir, up });
                        }
                        cur = prev;
                    }
                }
            }
            steps.reverse();
            return Some(steps);
        }
        expansions += 1;
        if expansions > 400_000 {
            return None;
        }
        for d in Dir::ALL {
            let h = d.index() as u8;
            if s.h != 4 && Dir::ALL[s.h as usize].opposite() == d {
                continue;
            }
            let tp = if s.h != 4 && s.h != h { turn } else { 0 };
            // Flat step.
            let n = s.c.step(d, 1);
            if n.in_bounds() && grid.is_free(n) {
                let ng = g + 100 + tp + noise(n);
                let ns = St { c: n, h };
                if best.get(&ns).is_none_or(|&b| ng < b) {
                    best.insert(ns, ng);
                    parent.insert(ns, (s, None));
                    open.push(Reverse((ng + heur(n), ng, counter, ns)));
                    counter += 1;
                }
            }
            // Stair runs, up and down.
            for up in [true, false] {
                let lo = if up { s.c.y } else { s.c.y - 1 };
                if lo < 0 || lo + 1 >= NY {
                    continue;
                }
                let ok = (1..=3).all(|i| {
                    let c = s.c.step(d, i);
                    grid.is_free(Cell { y: lo, ..c }) && grid.is_free(Cell { y: lo + 1, ..c })
                });
                let e = Cell { y: if up { s.c.y + 1 } else { s.c.y - 1 }, ..s.c.step(d, 4) };
                if !ok || !e.in_bounds() || !grid.is_free(e) {
                    continue;
                }
                let pen: u32 = (1..=3).map(|i| {
                    let c = s.c.step(d, i);
                    [lo, lo + 1].iter().filter(|&&y| precious.binary_search(&Cell { y, ..c }).is_ok()).count() as u32 * 1500
                }).sum();
                let ng = g + 520 + tp + noise(e) + pen;
                let ns = St { c: e, h };
                if best.get(&ns).is_none_or(|&b| ng < b) {
                    best.insert(ns, ng);
                    parent.insert(ns, (s, Some((d, up))));
                    open.push(Reverse((ng + heur(e), ng, counter, ns)));
                    counter += 1;
                }
            }
        }
    }
    None
}

/// Build the plan for a seed, retrying the embedding with derived seeds if a route fails.
pub fn plan(seed: u64) -> Plan {
    let mut errors = Vec::new();
    for attempt in 0..24u32 {
        let s = if attempt == 0 { seed } else { hash64(seed, &format!("attempt/{attempt}"), layer::LAYOUT) };
        match try_plan(s) {
            Ok(mut p) => {
                p.seed = seed;
                p.attempts = attempt + 1;
                return p;
            }
            Err(e) => {
                if std::env::var("LABYRINTH_DEBUG").is_ok() {
                    eprintln!("attempt {attempt}: {e}");
                }
                errors.push(e)
            }
        }
    }
    panic!("labyrinth seed {seed:#x}: no embedding in 24 attempts: {errors:?}");
}

fn try_plan(seed: u64) -> Result<Plan, String> {
    let d = Director::new(seed);
    let mut b = Builder { seed, g: SpaceGraph::default(), grid: Grid::new(), rooms: Vec::new(), room_of: Vec::new(), ledges: Vec::new(), ledge_of: Vec::new(), jobs: Vec::new(), passages: Vec::new(), gallery: None };

    // --- Set pieces. -------------------------------------------------------------------------
    let void = b.space("void", SpaceKind::Void, Role::Container, 0);
    b.g.spaces[void].set_piece = Some("the deepest void");
    let void_room = b.add_room(void, d.void, 0.6, true);
    let hall = b.space("hall", SpaceKind::GreatHall, Role::Core, d.fh);
    b.g.spaces[hall].set_piece = Some("the biggest hall");
    let hall_room = b.add_room(hall, d.hall, 1.0, false);
    let under = b.space("undercroft", SpaceKind::Undercroft, Role::Core, 0);
    b.g.spaces[under].set_piece = Some("the darkest place");
    b.add_room(under, d.under, 0.5, false);
    let well = b.space("well", SpaceKind::LightWell, Role::Container, d.well.y0);
    b.add_room(well, d.well, 0.5, true);
    // The thickness of the arch: the row of cells between the hall and the void.
    b.grid.fill(&CBox::new(d.hall.x0, d.hall.x1, d.hall.y0, d.hall.y1, d.void.z0 - 1, d.void.z0), Occ::Reserved);

    // The arch, the bridge on its axis and the ledges at either end.
    let hr = b.rooms[hall_room];
    let arch_x = (d.arch_cx as f32 + 0.5) * CELL;
    let inner_w = hr.face(Dir::XP) - hr.face(Dir::XN);
    let inner_h = hr.ceiling_y() - hr.floor_y();
    let aw = (inner_w - 10.0).min(40.0).min(2.0 * (arch_x - hr.face(Dir::XN) - 2.0)).min(2.0 * (hr.face(Dir::XP) - arch_x - 2.0));
    let ah = (inner_h - 3.0).min(36.0);
    let ring = 2.4;
    let span = (aw * 0.62).min(2.0 * (ah - 10.0 - ring));
    let spring = ah - span * 0.5 - ring - 2.5;
    let arch = ArchSpec { hall: hall_room, void: void_room, x: arch_x, w: aw, height: ah, span, spring, ring };
    let la = b.add_ledge("void/ledge/arch", Role::Core, void, Dir::ZN, d.fh, 6.0);
    let lf = b.add_ledge("void/ledge/far", Role::Core, void, Dir::ZP, d.fh, 5.0);
    let ls = b.add_ledge("void/ledge/spawn", Role::Core, void, Dir::ZP, d.ks, 4.0);
    b.g.link(hall, la, LinkKind::Archway, Role::Core);
    b.g.link(la, lf, LinkKind::Bridge, Role::Core);
    {
        let l = &mut b.ledges[b.ledge_of[la].unwrap()];
        l.needs.push((arch_x - aw * 0.5 - 1.0, arch_x + aw * 0.5 + 1.0));
        let l = &mut b.ledges[b.ledge_of[lf].unwrap()];
        l.forced = Some(d.arch_cx);
    }
    let bridges = vec![BridgeSpec { a: b.ledge_of[la].unwrap(), b: b.ledge_of[lf].unwrap(), u: arch_x, width: 4.6, railings: true, girder: true }];
    let mut bridges = bridges;

    // Light well: two ledges facing each other and a bridge with no railings.
    let wa = b.add_ledge("well/ledge/a", Role::Core, well, Dir::XN, d.kw, 3.0);
    let wb = b.add_ledge("well/ledge/b", Role::Core, well, Dir::XP, d.kw, 3.0);
    b.g.link(wa, wb, LinkKind::Bridge, Role::Core);
    let wz = (d.well.z0 as f32 + d.well.depth() as f32 * 0.5) * CELL;
    bridges.push(BridgeSpec { a: b.ledge_of[wa].unwrap(), b: b.ledge_of[wb].unwrap(), u: wz, width: 2.4, railings: false, girder: false });
    for l in [wa, wb] {
        let li = b.ledge_of[l].unwrap();
        b.ledges[li].rail = Rail::None;
    }

    // The wall stair: two ledges on one void wall, two storeys apart.
    let vr = b.rooms[void_room];
    let (s0, _s1) = vr.side_span(d.stair_side);
    let stair_run = wall_stair_run(2);
    let u_s = s0 + 2.0 * CELL + 1.0;
    let l1 = b.add_ledge("void/ledge/stair-low", Role::Loop, void, d.stair_side, d.k1, 3.5);
    let l2 = b.add_ledge("void/ledge/stair-high", Role::Loop, void, d.stair_side, d.k1 + 2, 3.5);
    b.g.link(l1, l2, LinkKind::WallStair, Role::Loop);
    {
        let li1 = b.ledge_of[l1].unwrap();
        let li2 = b.ledge_of[l2].unwrap();
        // Doors only on the far side of each ledge from the stair.
        b.ledges[li1].needs.push((u_s - 1.0, u_s));
        b.ledges[li1].forbid.push((u_s - 0.5, 1e6));
        b.ledges[li1].open_end[1] = true;
        b.ledges[li2].needs.push((u_s + stair_run, u_s + stair_run + 1.0));
        b.ledges[li2].forbid.push((-1e6, u_s + stair_run + 0.5));
        b.ledges[li2].open_end[0] = true;
    }
    let wall_stairs = vec![WallStairSpec { lower: b.ledge_of[l1].unwrap(), upper: b.ledge_of[l2].unwrap(), u0: u_s, dir: 1.0, levels: 2 }];

    // The hall's gallery and grand stair.
    let gal = b.space("hall/gallery", SpaceKind::HallGallery, Role::Loop, d.fh + 3);
    b.g.spaces[gal].parent = Some(hall);
    b.g.link(hall, gal, LinkKind::HallStair, Role::Loop);
    {
        let z1 = hr.face(Dir::ZP);
        let z0 = z1 - (hr.face(Dir::ZP) - hr.face(Dir::ZN)) * 0.42;
        let stair_len = hall_stair_run();
        b.gallery = Some(GallerySpec { space: gal, hall: hall_room, side: d.gallery_side, level: d.fh + 3, z0, z1, depth: 5.0, stair_foot: z0 - stair_len });
    }

    // --- The core cycle, rewritten arc by arc. --------------------------------------------
    let e = |s: usize, l: i32| End { space: s, level: l };
    let a1 = b.arc(e(ls, d.ks), e(hall, d.fh), Role::Core, "core/a1", 0);
    let a2 = b.arc(e(lf, d.fh), e(under, 0), Role::Core, "core/a2", 0);
    let a3 = b.arc(e(under, 0), e(wa, d.kw), Role::Core, "core/a3", 0);
    let a4 = b.arc(e(wb, d.kw), e(ls, d.ks), Role::Core, "core/a4", 0);

    // --- Loops. ----------------------------------------------------------------------------
    let mut r = Rng::for_id(seed, "loops", layer::GRAPH);
    let pick_end = |b: &Builder, r: &mut Rng, list: &[usize], fallback: End, level_near: i32| -> End {
        if list.is_empty() {
            return fallback;
        }
        let s = *r.pick(list);
        let sp = &b.g.spaces[s];
        let level = if sp.kind == SpaceKind::StairTower {
            let rb = b.rooms[b.room_of[s].unwrap()].b;
            level_near.clamp(rb.y0, rb.y1 - 1)
        } else {
            sp.level
        };
        End { space: s, level }
    };
    // Loop 1: from the descent towards the undercroft, up the void wall stair, back to the
    // climb from the light well.
    let x = pick_end(&b, &mut r, &[a2.as_slice(), a3.as_slice()].concat(), e(under, 0), d.k1);
    b.arc(x, e(l1, d.k1), Role::Loop, "loop/stair-low", 1);
    let y = pick_end(&b, &mut r, &[a4.as_slice(), a1.as_slice()].concat(), e(ls, d.ks), d.k1 + 2);
    b.arc(e(l2, d.k1 + 2), y, Role::Loop, "loop/stair-high", 1);
    // Loop 2: the hall's gallery out to the climb or the descent.
    let z = pick_end(&b, &mut r, &[a4.as_slice(), a1.as_slice(), a3.as_slice()].concat(), e(ls, d.ks), d.fh + 3);
    b.arc(e(gal, d.fh + 3), z, Role::Loop, "loop/gallery", 1);

    // --- Spurs. ----------------------------------------------------------------------------
    let all_rooms: Vec<usize> = [a1.as_slice(), a2.as_slice(), a3.as_slice(), a4.as_slice()].concat();
    // The overlook: a small ledge high on the void, railing broken.
    let lo = b.add_ledge("void/ledge/overlook", Role::Spur, void, d.look_side, d.ko, 3.0);
    {
        let li = b.ledge_of[lo].unwrap();
        b.ledges[li].rail = Rail::Broken;
    }
    let o = pick_end(&b, &mut r, &all_rooms, e(ls, d.ks), d.ko);
    b.arc(o, e(lo, d.ko), Role::Spur, "spur/overlook", 1);
    // A dark chamber off the undercroft.
    {
        let ur = b.rooms[b.room_of[under].unwrap()].b;
        let (cx, cz) = ur.centre_xz();
        let tx = if cx < NX as f32 * 0.5 { ur.x0 as f32 - 3.0 } else { ur.x1 as f32 + 3.0 };
        if let Some(bx) = b.place_near(tx, cz, 3, 2, 0, 1, false) {
            let ch = b.space("spur/chamber", SpaceKind::Chamber, Role::Spur, 0);
            b.add_room(ch, bx, 0.4, false);
            b.passage(e(under, 0), e(ch, 0), SpaceKind::ServicePassage, Role::Spur, "spur/chamber");
        }
    }
    // Loggias on the outside: one on a sunlit face, one in shade.
    for (i, face) in [Dir::ZN, Dir::XP, Dir::ZP, Dir::XN].into_iter().enumerate() {
        let n_logg = b.g.spaces.iter().filter(|s| s.kind == SpaceKind::Loggia).count();
        if n_logg >= 2 {
            break;
        }
        let sid = format!("spur/loggia/{i}");
        let mut rr = Rng::for_id(seed, &sid, layer::GRAPH);
        if i == 0 && rr.chance(0.3) {
            continue;
        }
        let from = pick_end(&b, &mut rr, &all_rooms, e(hall, d.fh), d.fh);
        let level = from.level.clamp(1, NY - 4);
        let (fx, fz) = b.pos(from.space);
        let (w, dep) = (rr.int(4, 6), 2);
        let (bw, bd) = if face.is_x() { (dep, w) } else { (w, dep) };
        let (tx, tz) = match face {
            Dir::XN => (0.0, fz),
            Dir::XP => (NX as f32, fz),
            Dir::ZN => (fx, 0.0),
            Dir::ZP => (fx, NZ as f32),
        };
        // Search along the face for a free spot touching it.
        let mut placed = None;
        for off in 0..40 {
            for sgn in [1, -1] {
                let o = off * sgn;
                let bx = match face {
                    Dir::XN => CBox::new(0, bw, level, level + 2, (tz as i32 - bd / 2 + o).max(1), (tz as i32 - bd / 2 + o).max(1) + bd),
                    Dir::XP => CBox::new(NX - bw, NX, level, level + 2, (tz as i32 - bd / 2 + o).max(1), (tz as i32 - bd / 2 + o).max(1) + bd),
                    Dir::ZN => CBox::new((tx as i32 - bw / 2 + o).max(1), (tx as i32 - bw / 2 + o).max(1) + bw, level, level + 2, 0, bd),
                    Dir::ZP => CBox::new((tx as i32 - bw / 2 + o).max(1), (tx as i32 - bw / 2 + o).max(1) + bw, level, level + 2, NZ - bd, NZ),
                };
                if placed.is_none() && b.grid.box_free(&bx, 1, true) {
                    placed = Some(bx);
                }
            }
            if placed.is_some() {
                break;
            }
        }
        if let Some(bx) = placed {
            let lg = b.space(&sid, SpaceKind::Loggia, Role::Spur, level);
            b.add_room(lg, bx, 0.5, false);
            b.arc(e(from.space, from.level.max(b.g.spaces[from.space].level)), e(lg, level), Role::Spur, &sid, 2);
        }
    }
    // Fix the ends of spur arcs that start in a tower: the end level must be one the tower has.
    for j in b.jobs.iter_mut() {
        for end in [&mut j.a, &mut j.b] {
            if let Some(ri) = b.room_of[end.space] {
                let rb = b.rooms[ri].b;
                if b.g.spaces[end.space].kind == SpaceKind::StairTower {
                    end.level = end.level.clamp(rb.y0, rb.y1 - 1);
                }
            }
        }
    }

    // --- Route every passage. ------------------------------------------------------------
    for k in 0..b.jobs.len() {
        b.route_job(k)?;
    }

    // --- Ledge extents, now that the doors are known. ------------------------------------
    for p in &b.passages {
        for pe in &p.ends {
            if let Some(li) = b.ledge_of[pe.target] {
                let l = &mut b.ledges[li];
                let u = if l.side.is_x() { pe.cell.z } else { pe.cell.x } as f32 * CELL;
                l.needs.push((u - 1.0, u + CELL + 1.0));
            }
        }
    }
    for br in &bridges {
        for li in [br.a, br.b] {
            b.ledges[li].needs.push((br.u - br.width * 0.5 - 1.5, br.u + br.width * 0.5 + 1.5));
            b.ledges[li].rail_gaps.push((br.u - br.width * 0.5, br.u + br.width * 0.5));
        }
    }
    for l in b.ledges.iter_mut() {
        let r = &b.rooms[l.room];
        let (s0, s1) = r.side_span(l.side);
        let lo = l.needs.iter().map(|n| n.0).fold(f32::MAX, f32::min);
        let hi = l.needs.iter().map(|n| n.1).fold(f32::MIN, f32::max);
        let (mut a, mut c) = if lo < hi { (lo, hi) } else { (s0, s0 + 8.0) };
        if c - a < 9.0 {
            if l.open_end[0] {
                c = a + 9.0;
            } else if l.open_end[1] {
                a = c - 9.0;
            } else {
                let m = (a + c) * 0.5;
                a = m - 4.5;
                c = m + 4.5;
            }
        }
        l.span = (a.max(s0), c.min(s1));
    }
    // Ledges on neighbouring walls at the same storey must not overlap in the corner.
    for i in 0..b.ledges.len() {
        for j in 0..b.ledges.len() {
            let (li, lj) = (&b.ledges[i], &b.ledges[j]);
            if i == j || li.room != lj.room || li.level != lj.level || li.side.is_x() == lj.side.is_x() {
                continue;
            }
            // lj's wall plane, in li's u coordinate; trim li to stay clear of lj's depth.
            let r = &b.rooms[li.room];
            let plane = r.face(lj.side);
            let edge = plane - lj.side.sign() * lj.depth;
            let (a, c) = li.span;
            let (a, c) = if lj.side.sign() < 0.0 { (a.max(edge + 0.01), c) } else { (a, c.min(edge - 0.01)) };
            let own = (b.ledges[j].span.0, b.ledges[j].span.1);
            let _ = own;
            if j > i {
                b.ledges[i].span = (a, c);
            }
        }
    }

    // --- Anchors, bounds, portals. --------------------------------------------------------
    let gallery = b.gallery.unwrap();
    let spawn_li = b.ledge_of[ls].unwrap();
    let portals = make_portals(&b, &arch);
    let mut plan = Plan {
        seed,
        attempts: 1,
        graph: b.g,
        grid: b.grid,
        rooms: b.rooms,
        room_of: b.room_of,
        passages: b.passages,
        ledges: b.ledges,
        bridges,
        wall_stairs,
        arch,
        gallery,
        portals,
        spawn: (Vec3::ZERO, 0.0),
        spawn_space: ls,
    };
    set_anchors(&mut plan);
    // Spawn: on the spawn ledge in front of its door, looking across the void (-z).
    let l = &plan.ledges[spawn_li];
    let r = &plan.rooms[l.room];
    let door = plan.passages.iter().flat_map(|p| p.ends.iter()).find(|pe| pe.target == ls).map(|pe| pe.cell).unwrap();
    let face = r.face(l.side);
    let eye = Vec3::new((door.x as f32 + 0.5) * CELL, l.level as f32 * LEVEL + 1.65, face - 1.6);
    plan.spawn = (eye, std::f32::consts::PI);
    Ok(plan)
}

/// Horizontal run of a wall stair climbing `levels` storeys: flights of 24 risers with 1.2 m
/// landings.
pub fn wall_stair_run(levels: i32) -> f32 {
    let risers = levels as usize * RISERS_PER_LEVEL;
    let flights = risers.div_ceil(24);
    risers as f32 * crate::kit::STAIR_RUN + (flights - 1) as f32 * 1.2
}

/// Horizontal run of the hall's grand stair (three storeys): flights of 27 risers, 2 m landings.
pub fn hall_stair_run() -> f32 {
    let risers = 3 * RISERS_PER_LEVEL;
    let flights = risers.div_ceil(27);
    risers as f32 * crate::kit::STAIR_RUN + (flights - 1) as f32 * 2.0
}

fn portal_size(link: LinkKind, kind: SpaceKind, target: SpaceKind) -> (f32, f32) {
    match (link, kind) {
        (LinkKind::Opening, _) => (CELL - 2.0 * passage_inset(kind), 4.4),
        (_, SpaceKind::ServicePassage) => (crate::kit::DOOR_WIDTH, crate::kit::DOOR_HEIGHT),
        _ if target == SpaceKind::StairTower => (1.6, 2.8),
        _ => (1.8, 3.2),
    }
}

/// Doors at passage ends, windows from passages into voids, the hall and the outside, the
/// hall's exterior windows, tower windows and the loggias' colonnades.
fn make_portals(b: &Builder, _arch: &ArchSpec) -> Vec<Portal> {
    let mut out = Vec::new();
    let face_of = |target: usize, dir_into: Dir| -> f32 {
        // The target's wall face that a door entering it in `dir_into` passes.
        let side = dir_into.opposite();
        if let Some(li) = b.ledge_of[target] {
            let l = &b.ledges[li];
            return b.rooms[l.room].face(l.side);
        }
        if let Some(g) = b.gallery.filter(|g| g.space == target) {
            return b.rooms[g.hall].face(g.side);
        }
        b.rooms[b.room_of[target].unwrap()].face(side)
    };
    for p in &b.passages {
        let t = p.inset();
        for pe in &p.ends {
            let (w, h) = portal_size(pe.link, p.kind, b.g.spaces[pe.target].kind);
            let c = pe.cell;
            let own = if pe.dir.sign() > 0.0 { if pe.dir.is_x() { (c.x + 1) as f32 * CELL - t } else { (c.z + 1) as f32 * CELL - t } } else if pe.dir.is_x() { c.x as f32 * CELL + t } else { c.z as f32 * CELL + t };
            let other = face_of(pe.target, pe.dir);
            let um = if pe.dir.is_x() { (c.z as f32 + 0.5) * CELL } else { (c.x as f32 + 0.5) * CELL };
            let y0 = c.y as f32 * LEVEL;
            out.push(Portal {
                kind: PortalKind::Door,
                x_plane: pe.dir.is_x(),
                p0: own.min(other),
                p1: own.max(other),
                u0: um - w * 0.5,
                u1: um + w * 0.5,
                y0,
                y1: y0 + h,
                between: if pe.dir.sign() > 0.0 { [Some(p.space), Some(pe.target)] } else { [Some(pe.target), Some(p.space)] },
                walk: true,
            });
        }
        // Windows on unconnected sides of flat cells.
        let flats: Vec<Cell> = p.flats().collect();
        let mut r = Rng::for_id(b.seed, &format!("{}/windows", b.g.spaces[p.space].sid), layer::ELEMENTS);
        for (i, &c) in flats.iter().enumerate() {
            for d in Dir::ALL {
                let n = c.step(d, 1);
                // Connected to the path or a door on this side?
                if p.ends.iter().any(|pe| pe.cell == c && pe.dir == d) {
                    continue;
                }
                if connected_side(p, c, d) {
                    continue;
                }
                let own = if d.sign() > 0.0 { b_plane(c, d) - t } else { b_plane(c, d) + t };
                let um = if d.is_x() { (c.z as f32 + 0.5) * CELL } else { (c.x as f32 + 0.5) * CELL };
                let y0 = c.y as f32 * LEVEL + 1.1;
                let top = c.y as f32 * LEVEL + p.height() - 0.3;
                let roll = r.f();
                let _ = i;
                if !n.in_bounds() && n.y >= 0 && n.y < NY {
                    if roll < 0.5 {
                        let skin = if d.sign() > 0.0 { b_plane(c, d) + SKIN } else { b_plane(c, d) - SKIN };
                        out.push(Portal { kind: PortalKind::Exterior, x_plane: d.is_x(), p0: own.min(skin), p1: own.max(skin), u0: um - 0.7, u1: um + 0.7, y0, y1: (y0 + 3.0).min(top), between: if d.sign() > 0.0 { [Some(p.space), None] } else { [None, Some(p.space)] }, walk: false });
                    }
                    continue;
                }
                if let Occ::Space(s) = b.grid.get(n) {
                    let s = s as usize;
                    let kind = b.g.spaces[s].kind;
                    let Some(ri) = b.room_of[s] else { continue };
                    let room = &b.rooms[ri];
                    let ok = match kind {
                        SpaceKind::Void | SpaceKind::LightWell => roll < 0.75 && !wall_stair_zone(b, ri, d.opposite(), um, c.y),
                        SpaceKind::GreatHall => roll < 0.6 && c.y > room.b.y0 && (c.y as f32 * LEVEL + 3.5) < room.ceiling_y(),
                        _ => false,
                    };
                    if ok {
                        let other = room.face(d.opposite());
                        out.push(Portal { kind: PortalKind::Window, x_plane: d.is_x(), p0: own.min(other), p1: own.max(other), u0: um - 0.5, u1: um + 0.5, y0, y1: (y0 + 2.4).min(top), between: if d.sign() > 0.0 { [Some(p.space), Some(s)] } else { [Some(s), Some(p.space)] }, walk: false });
                    }
                }
            }
        }
    }
    for r in &b.rooms {
        let mut rng = Rng::for_id(b.seed, &format!("{}/windows", b.g.spaces[r.space].sid), layer::ELEMENTS);
        for d in Dir::ALL {
            let on_edge = match d {
                Dir::XN => r.b.x0 == 0,
                Dir::XP => r.b.x1 == NX,
                Dir::ZN => r.b.z0 == 0,
                Dir::ZP => r.b.z1 == NZ,
            };
            if !on_edge {
                continue;
            }
            let face = r.face(d);
            let skin = r.b.plane(d) + d.sign() * SKIN;
            let between = |s: usize| if d.sign() > 0.0 { [Some(s), None] } else { [None, Some(s)] };
            let (u0, u1) = r.side_span(d);
            match r.kind {
                SpaceKind::GreatHall => {
                    // Tall slots in the exterior end, the sun's way in.
                    let n = ((u1 - u0) / 9.0).floor().max(3.0) as usize;
                    let pitch = (u1 - u0) / n as f32;
                    for k in 0..n {
                        let um = u0 + pitch * (k as f32 + 0.5);
                        let w = if k == n / 2 { 4.0 } else { 2.4 };
                        let y0 = r.floor_y() + rng.range(5.0, 7.0);
                        let y1 = r.ceiling_y() - rng.range(3.0, 6.0);
                        out.push(Portal { kind: PortalKind::Exterior, x_plane: d.is_x(), p0: face.min(skin), p1: face.max(skin), u0: um - w * 0.5, u1: um + w * 0.5, y0, y1, between: between(r.space), walk: false });
                    }
                }
                SpaceKind::Loggia => {
                    // A colonnade: openings between piers, from the floor.
                    let cells = ((u1 - u0 + 2.0 * r.inset) / CELL).round() as usize;
                    for k in 0..cells {
                        let a = (u0 - r.inset) + k as f32 * CELL + 0.6;
                        let c = (u0 - r.inset) + (k + 1) as f32 * CELL - 0.6;
                        let (a, c) = (a.max(u0 + 0.2), c.min(u1 - 0.2));
                        out.push(Portal { kind: PortalKind::Exterior, x_plane: d.is_x(), p0: face.min(skin), p1: face.max(skin), u0: a, u1: c, y0: r.floor_y(), y1: r.ceiling_y() - 1.8, between: between(r.space), walk: true });
                    }
                }
                SpaceKind::StairTower | SpaceKind::Junction | SpaceKind::Chamber | SpaceKind::Undercroft => {
                    let um = (u0 + u1) * 0.5;
                    for y in r.b.y0..r.b.y1 {
                        let y0 = y as f32 * LEVEL + 1.6;
                        out.push(Portal { kind: PortalKind::Exterior, x_plane: d.is_x(), p0: face.min(skin), p1: face.max(skin), u0: um - 0.6, u1: um + 0.6, y0, y1: y0 + 3.2, between: between(r.space), walk: false });
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn b_plane(c: Cell, d: Dir) -> f32 {
    match d {
        Dir::XN => c.x as f32 * CELL,
        Dir::XP => (c.x + 1) as f32 * CELL,
        Dir::ZN => c.z as f32 * CELL,
        Dir::ZP => (c.z + 1) as f32 * CELL,
    }
}

/// Whether the flat cell `c` of the passage connects to the next or previous step on side
/// `d` (a flat neighbour or a stair run).
pub fn connected_side(p: &Passage, c: Cell, d: Dir) -> bool {
    let k = p.path.iter().position(|s| *s == Step::Flat(c)).unwrap();
    let toward = |s: &Step| -> Option<Dir> {
        match *s {
            Step::Flat(n) => Dir::ALL.into_iter().find(|&dd| c.step(dd, 1) == n),
            Step::Stair { from, dir, .. } => {
                if from == c {
                    Some(dir)
                } else {
                    None
                }
            }
        }
    };
    let prev = if k > 0 { Some(&p.path[k - 1]) } else { None };
    let next = p.path.get(k + 1);
    let mut sides = Vec::new();
    if let Some(n) = next {
        if let Some(dd) = toward(n) {
            sides.push(dd);
        }
    }
    if let Some(pv) = prev {
        match *pv {
            Step::Flat(n) => {
                if let Some(dd) = Dir::ALL.into_iter().find(|&dd| c.step(dd, 1) == n) {
                    sides.push(dd);
                }
            }
            Step::Stair { dir, .. } => sides.push(dir.opposite()),
        }
    }
    sides.contains(&d)
}

fn wall_stair_zone(b: &Builder, room: usize, side: Dir, u: f32, y: i32) -> bool {
    b.ledges.iter().any(|l| l.room == room && l.side == side && l.forbid.iter().any(|&(f0, f1)| u > f0 - 1.0 && u < f1 + 1.0) && (y - l.level).abs() <= 2)
}

/// Points every walkable space must have reachable, and world bounds.
fn set_anchors(p: &mut Plan) {
    let n = p.graph.spaces.len();
    for i in 0..n {
        let mut anchors = Vec::new();
        let mut bounds = None;
        if let Some(ri) = p.room_of[i] {
            let r = &p.rooms[ri];
            bounds = Some((r.inner_min(), r.inner_max()));
            match r.kind {
                SpaceKind::Void | SpaceKind::LightWell => {}
                SpaceKind::StairTower => {
                    let c = (r.inner_min() + r.inner_max()) * 0.5;
                    anchors.push(Vec3::new(c.x, r.floor_y(), c.z));
                    // The top landing.
                    let top = r.b.y1 - 1;
                    let m = top - r.b.y0;
                    let (x, z) = if m % 2 == 0 { (r.face(Dir::XN) + 1.1, r.face(Dir::ZN) + 1.1) } else { (r.face(Dir::XP) - 1.1, r.face(Dir::ZP) - 1.1) };
                    anchors.push(Vec3::new(x, top as f32 * LEVEL, z));
                }
                _ => {
                    // Just inside each door.
                    for pass in &p.passages {
                        for pe in pass.ends.iter().filter(|pe| pe.target == i) {
                            let into = pe.dir.vec();
                            let face = r.face(pe.dir.opposite());
                            let c = pe.cell.floor_centre();
                            let a = if pe.dir.is_x() { Vec3::new(face, c.y, c.z) } else { Vec3::new(c.x, c.y, face) } + into * 1.2;
                            anchors.push(a);
                        }
                    }
                    if r.kind == SpaceKind::GreatHall {
                        let c = (r.inner_min() + r.inner_max()) * 0.5;
                        anchors.push(Vec3::new(c.x, r.floor_y(), c.z));
                    }
                }
            }
        }
        if let Some(l) = p.ledges.iter().find(|l| l.space == i) {
            let r = &p.rooms[l.room];
            let face = r.face(l.side);
            let um = (l.span.0 + l.span.1) * 0.5;
            let d = -l.side.sign() * (l.depth * 0.5);
            let y = l.level as f32 * LEVEL;
            anchors.push(if l.side.is_x() { Vec3::new(face + d, y, um) } else { Vec3::new(um, y, face + d) });
            let (lo, hi) = (Vec3::new(face, y - 0.8, l.span.0), Vec3::new(face + d * 2.0, y, l.span.1));
            bounds = Some(if l.side.is_x() { (lo.min(hi), lo.max(hi)) } else { (Vec3::new(l.span.0, y - 0.8, face).min(Vec3::new(l.span.1, y, face + d * 2.0)), Vec3::new(l.span.0, y - 0.8, face).max(Vec3::new(l.span.1, y, face + d * 2.0))) });
        }
        if p.gallery.space == i {
            let g = &p.gallery;
            let r = &p.rooms[g.hall];
            let face = r.face(g.side);
            let x = face - g.side.sign() * g.depth * 0.5;
            let y = g.level as f32 * LEVEL;
            anchors.push(Vec3::new(x, y, (g.z0 + g.z1) * 0.5));
        }
        if let Some(pass) = p.passages.iter().find(|q| q.space == i) {
            let flats: Vec<Cell> = pass.flats().collect();
            anchors.push(flats[flats.len() / 2].floor_centre());
            let mut lo = Vec3::splat(f32::MAX);
            let mut hi = Vec3::splat(f32::MIN);
            for c in &flats {
                lo = lo.min(Vec3::new(c.x as f32 * CELL, c.y as f32 * LEVEL, c.z as f32 * CELL));
                hi = hi.max(Vec3::new((c.x + 1) as f32 * CELL, (c.y + 1) as f32 * LEVEL, (c.z + 1) as f32 * CELL));
            }
            bounds = Some((lo, hi));
        }
        p.graph.spaces[i].anchors = anchors;
        p.graph.spaces[i].bounds = bounds;
    }
}
