//! The coarse cell grid the labyrinth is laid out on: `CELL` metres in plan, `LEVEL` metres a
//! storey. Spaces own boxes or paths of cells; everything not owned is solid mass.

use super::{CELL, LEVEL, NX, NY, NZ};
use glam::Vec3;

/// A horizontal direction on the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dir {
    XN,
    XP,
    ZN,
    ZP,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::XN, Dir::XP, Dir::ZN, Dir::ZP];

    pub fn d(self) -> (i32, i32) {
        match self {
            Dir::XN => (-1, 0),
            Dir::XP => (1, 0),
            Dir::ZN => (0, -1),
            Dir::ZP => (0, 1),
        }
    }

    pub fn vec(self) -> Vec3 {
        let (x, z) = self.d();
        Vec3::new(x as f32, 0.0, z as f32)
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::XN => Dir::XP,
            Dir::XP => Dir::XN,
            Dir::ZN => Dir::ZP,
            Dir::ZP => Dir::ZN,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// Whether the direction runs along x (its walls are planes of constant x).
    pub fn is_x(self) -> bool {
        matches!(self, Dir::XN | Dir::XP)
    }

    /// The horizontal axis along a wall facing this way (+Z for x walls, +X for z walls).
    pub fn along(self) -> Vec3 {
        if self.is_x() {
            Vec3::Z
        } else {
            Vec3::X
        }
    }

    pub fn sign(self) -> f32 {
        match self {
            Dir::XN | Dir::ZN => -1.0,
            _ => 1.0,
        }
    }
}

/// A grid cell: `x`, `z` in plan, `y` the storey.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Cell {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn step(self, d: Dir, n: i32) -> Cell {
        let (dx, dz) = d.d();
        Cell { x: self.x + dx * n, y: self.y, z: self.z + dz * n }
    }

    pub fn up(self, n: i32) -> Cell {
        Cell { y: self.y + n, ..self }
    }

    /// World position of the centre of the cell's floor.
    pub fn floor_centre(self) -> Vec3 {
        Vec3::new((self.x as f32 + 0.5) * CELL, self.y as f32 * LEVEL, (self.z as f32 + 0.5) * CELL)
    }

    pub fn in_bounds(self) -> bool {
        self.x >= 0 && self.x < NX && self.z >= 0 && self.z < NZ && self.y >= 0 && self.y < NY
    }
}

/// A box of cells, half-open: `x0..x1`, `y0..y1` (storeys), `z0..z1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CBox {
    pub x0: i32,
    pub x1: i32,
    pub y0: i32,
    pub y1: i32,
    pub z0: i32,
    pub z1: i32,
}

impl CBox {
    pub fn new(x0: i32, x1: i32, y0: i32, y1: i32, z0: i32, z1: i32) -> Self {
        Self { x0, x1, y0, y1, z0, z1 }
    }

    pub fn contains(&self, c: Cell) -> bool {
        c.x >= self.x0 && c.x < self.x1 && c.y >= self.y0 && c.y < self.y1 && c.z >= self.z0 && c.z < self.z1
    }

    pub fn width(&self) -> i32 {
        self.x1 - self.x0
    }

    pub fn depth(&self) -> i32 {
        self.z1 - self.z0
    }

    pub fn centre_xz(&self) -> (f32, f32) {
        ((self.x0 + self.x1) as f32 * 0.5, (self.z0 + self.z1) as f32 * 0.5)
    }

    /// World coordinate of the boundary plane on side `d` (x for x sides, z for z sides).
    pub fn plane(&self, d: Dir) -> f32 {
        match d {
            Dir::XN => self.x0 as f32 * CELL,
            Dir::XP => self.x1 as f32 * CELL,
            Dir::ZN => self.z0 as f32 * CELL,
            Dir::ZP => self.z1 as f32 * CELL,
        }
    }

    /// Range of cells along side `d` (z cells for x sides, x cells for z sides).
    pub fn side_cells(&self, d: Dir) -> (i32, i32) {
        if d.is_x() {
            (self.z0, self.z1)
        } else {
            (self.x0, self.x1)
        }
    }

    /// The cell outside side `d`, at storey `y` and position `u` along the side (cell index).
    pub fn outside(&self, d: Dir, y: i32, u: i32) -> Cell {
        match d {
            Dir::XN => Cell::new(self.x0 - 1, y, u),
            Dir::XP => Cell::new(self.x1, y, u),
            Dir::ZN => Cell::new(u, y, self.z0 - 1),
            Dir::ZP => Cell::new(u, y, self.z1),
        }
    }

    pub fn overlaps(&self, o: &CBox) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.y0 < o.y1 && o.y0 < self.y1 && self.z0 < o.z1 && o.z0 < self.z1
    }

    pub fn grow(&self, m: i32) -> CBox {
        CBox::new(self.x0 - m, self.x1 + m, self.y0, self.y1, self.z0 - m, self.z1 + m)
    }
}

/// What a cell belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Occ {
    Free,
    /// A space of the graph (its index).
    Space(u32),
    /// Kept clear of passages (the thickness of a giant arch, a stair against a void wall).
    Reserved,
}

pub struct Grid {
    occ: Vec<Occ>,
}

impl Default for Grid {
    fn default() -> Self {
        Self::new()
    }
}

impl Grid {
    pub fn new() -> Self {
        Self { occ: vec![Occ::Free; (NX * NY * NZ) as usize] }
    }

    fn idx(c: Cell) -> usize {
        ((c.y * NZ + c.z) * NX + c.x) as usize
    }

    pub fn get(&self, c: Cell) -> Occ {
        if !c.in_bounds() {
            return Occ::Reserved;
        }
        self.occ[Self::idx(c)]
    }

    pub fn set(&mut self, c: Cell, o: Occ) {
        if c.in_bounds() {
            self.occ[Self::idx(c)] = o;
        }
    }

    pub fn is_free(&self, c: Cell) -> bool {
        self.get(c) == Occ::Free
    }

    pub fn fill(&mut self, b: &CBox, o: Occ) {
        for y in b.y0..b.y1 {
            for z in b.z0..b.z1 {
                for x in b.x0..b.x1 {
                    self.set(Cell::new(x, y, z), o);
                }
            }
        }
    }

    /// Whether the box, grown by `margin` cells in plan, is inside the region and free.
    /// `edge` cells of the region boundary must stay clear unless `touch_edge`.
    pub fn box_free(&self, b: &CBox, margin: i32, touch_edge: bool) -> bool {
        let e = if touch_edge { 0 } else { 1 };
        if b.x0 < e || b.z0 < e || b.x1 > NX - e || b.z1 > NZ - e || b.y0 < 0 || b.y1 > NY {
            return false;
        }
        let g = b.grow(margin);
        for y in g.y0..g.y1 {
            for z in g.z0.max(0)..g.z1.min(NZ) {
                for x in g.x0.max(0)..g.x1.min(NX) {
                    if self.get(Cell::new(x, y, z)) != Occ::Free {
                        return false;
                    }
                }
            }
        }
        true
    }
}
