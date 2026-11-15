//! M1 "One Shaft": a square shaft 60 m across and 600 m deep, open to the sky.
//!
//! Coordinates in metres, Y up. The shaft is `x, z` in `-30..30`, its floor at `y = 0` and its
//! rim at `y = 600`. Walls are named by the side they stand on:
//!
//! - **East** (`x = +30`) is the blank wall. One long thin stair climbs it from the end of
//!   bridge B1 (`y = 400`) south to a ledge on the south wall. Nothing else above
//!   `y = 250`, and a single horizontal joint.
//! - **West** (`x = -30`) carries three gallery stacks; the spawn gallery (levels 380 to 420)
//!   is one of them.
//! - **South** (`z = -30`) carries the giant arch into the hall at the top of the stair, its
//!   ledge, and two gallery stacks.
//! - **North** (`z = +30`) carries two gallery stacks and the portals that bridges run into.
//!
//! The walkable route: spawn on the west gallery at `y = 400`, bridge B1 east, the stair up
//! the east wall, the south ledge, the arch, the hall. See docs/features/one-shaft.md.
//!
//! The seed varies panel joints, door patterns, gallery runs and bridge proportions; the
//! route and the large composition are fixed.

use crate::ao::Occluders;
use crate::builder::MeshBuilder;
use crate::kit::*;
use crate::mesh::{Collider, MeshData};
use glam::Vec3;

pub const HALF: f32 = 30.0;
pub const TOP: f32 = 600.0;
pub const WALL_THICKNESS: f32 = 8.0;
/// Floor of the spawn gallery level and of bridge B1.
pub const SPAWN_LEVEL: f32 = 400.0;
/// Bridge B1's centre line (z).
pub const SPAWN_Z: f32 = 21.0;
/// Where the player starts on the spawn gallery (z), clear of the bridge so the void shows.
pub const SPAWN_AT_Z: f32 = 12.0;
/// Bridge B1 lands on the east wall here (x 26..30, z 14..26).
pub const B1_LANDING: (Vec3, Vec3) = (Vec3::new(26.0, SPAWN_LEVEL - 1.0, 14.0), Vec3::new(HALF, SPAWN_LEVEL, 26.0));
/// The stair's foot (centre of its first riser) and lane.
pub const STAIR_X: f32 = 29.25;
pub const STAIR_WIDTH: f32 = 1.5;
pub const STAIR_FOOT_Z: f32 = 14.0;
/// The south ledge's front edge (z) and the stair's top end.
pub const LEDGE_EDGE_Z: f32 = -26.5;
pub const STAIR_MAX_RISERS: usize = 48;
/// Hall: behind the south wall, x -30..30, z -138..-38.
pub const HALL_DEPTH: f32 = 100.0;
pub const HALL_HEIGHT: f32 = 52.0;
pub const ARCH_SPAN: f32 = 26.0;
pub const ARCH_SPRING: f32 = 17.0;
pub const ARCH_W: f32 = 40.0;
pub const ARCH_H: f32 = 36.0;

/// The fixed default seed used by the scene registry.
pub const DEFAULT_SEED: u64 = 0x5eed_0001;

/// Risers and landing length of the blank-wall stair: whole treads of 0.30 m plus landings
/// of 1.2 to 2.5 m fill the run from the foot to the south ledge exactly.
pub fn stair_layout() -> (usize, f32) {
    let run = STAIR_FOOT_Z - LEDGE_EDGE_Z;
    let mut n = (run / STAIR_RUN) as usize;
    loop {
        let flights = n.div_ceil(STAIR_MAX_RISERS);
        if flights >= 2 {
            let landing = (run - n as f32 * STAIR_RUN) / (flights - 1) as f32;
            if (1.2..=2.5).contains(&landing) {
                return (n, landing);
            }
        }
        n -= 1;
    }
}

/// Height climbed by the stair (risers of 0.17 m), and so the floor of the hall above the
/// spawn level.
pub fn stair_height() -> f32 {
    stair_layout().0 as f32 * 0.17
}

/// Height of the blank-wall stair's walking surface (its ramp, or a landing) at `z`, for
/// camera paths. Clamped to the stair's ends.
pub fn stair_surface(z: f32) -> f32 {
    let (n, landing) = stair_layout();
    let flights = n.div_ceil(STAIR_MAX_RISERS);
    let per = n.div_ceil(flights);
    let rise = 0.17;
    let mut s = (STAIR_FOOT_Z - z).max(0.0);
    let mut y = SPAWN_LEVEL;
    let mut left = n;
    while left > 0 {
        let m = per.min(left);
        left -= m;
        let len = m as f32 * STAIR_RUN;
        if s <= len {
            return y + s * rise / STAIR_RUN;
        }
        s -= len;
        y += m as f32 * rise;
        if left > 0 {
            if s <= landing {
                return y;
            }
            s -= landing;
        }
    }
    y
}

pub fn hall_floor() -> f32 {
    SPAWN_LEVEL + stair_height()
}

/// Spawn eye position and yaw (looking east across the void at the stair wall).
pub fn spawn() -> ([f32; 3], f32) {
    ([-31.0, SPAWN_LEVEL + 1.65, SPAWN_AT_Z], std::f32::consts::FRAC_PI_2)
}

/// Built scene parts.
pub struct Shaft {
    pub mesh: MeshData,
    pub colliders: Vec<Collider>,
    /// Waypoints of the walkable route, on walking surfaces: spawn, bridge, stair, ledge,
    /// arch, hall.
    pub route: Vec<Vec3>,
}

/// Deterministic generator (splitmix64).
struct Rng(u64);

impl Rng {
    fn new(seed: u64, salt: u64) -> Self {
        Rng(seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut x = self.0;
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^ (x >> 31)
    }
    fn f(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
}

#[derive(Clone, Copy)]
enum Side {
    West,
    East,
    South,
    North,
}

/// Frame of a shaft wall's inner face: u runs along it, v is y, n points into the shaft.
fn wall_frame(s: Side) -> Frame {
    let h = HALF;
    match s {
        Side::West => Frame::new(Vec3::new(-h, 0.0, -h), Vec3::Z, Vec3::Y, Vec3::X),
        Side::East => Frame::new(Vec3::new(h, 0.0, h), -Vec3::Z, Vec3::Y, -Vec3::X),
        Side::South => Frame::new(Vec3::new(h, 0.0, -h), -Vec3::X, Vec3::Y, Vec3::Z),
        Side::North => Frame::new(Vec3::new(-h, 0.0, h), Vec3::X, Vec3::Y, -Vec3::Z),
    }
}

/// A gallery stack's layout in wall coordinates.
struct Stack {
    u0: f32,
    u1: f32,
    levels: Vec<f32>,
    piers: bool,
    gaps: Vec<(usize, f32, f32)>,
}

const STOREY: f32 = 5.0;
const SLAB: f32 = 0.7;
const GALLERY_DEPTH: f32 = 4.5;
const PROJECTION: f32 = 0.8;

fn levels(from: f32, count: usize) -> Vec<f32> {
    (0..count).map(|k| from + k as f32 * STOREY).collect()
}

pub fn build(seed: u64) -> Shaft {
    let mut b = MeshBuilder::new();
    let mut rng = Rng::new(seed, 1);
    let hall_y = hall_floor();

    // --- Gallery stacks, per wall. Runs with gaps; some walls bare. -----------------------
    let jit = |rng: &mut Rng, x: f32, a: f32| x + rng.range(-a, a);
    let mut west = vec![
        // Spawn stack: z 0..28, levels 380..420; bridge B1 meets the 400 level at z 21.
        Stack { u0: 30.0 - rng.range(0.0, 6.0).round(), u1: 58.0, levels: levels(380.0, 9), piers: false, gaps: vec![(4, 30.0 + SPAWN_Z - 2.2, 30.0 + SPAWN_Z + 2.2)] },
        Stack { u0: jit(&mut rng, 6.0, 2.0).round(), u1: jit(&mut rng, 32.0, 3.0).round(), levels: levels(520.0, 6 + (rng.f() * 3.0) as usize), piers: false, gaps: vec![] },
        Stack { u0: jit(&mut rng, 16.0, 3.0).round(), u1: jit(&mut rng, 52.0, 3.0).round(), levels: levels(155.0, 8), piers: rng.chance(0.5), gaps: vec![] },
    ];
    let south_arch_u = HALF; // arch centred on x = 0
    let mut south = vec![
        // Under the hall ledge, beside nothing: bridge B3 lands on its 330 level at x 14.
        Stack { u0: 4.0, u1: jit(&mut rng, 34.0, 2.0).round(), levels: levels(300.0, 9), piers: true, gaps: vec![(6, 30.0 - 14.0 - 2.5, 30.0 - 14.0 + 2.5)] },
        // High stack in the south-west; bridge B2 lands on its 470 level at x -12.
        Stack { u0: 36.0, u1: 58.0, levels: levels(460.0, 5), piers: false, gaps: vec![(2, 30.0 + 12.0 - 2.0, 30.0 + 12.0 + 2.0)] },
    ];
    let mut north = vec![
        // Bridge B2's north end at 470, x -12; bridge B3's north end at 330, x 14.
        Stack { u0: 2.0, u1: jit(&mut rng, 28.0, 2.0).round(), levels: levels(455.0, 6), piers: rng.chance(0.5), gaps: vec![(3, 30.0 - 12.0 - 2.0, 30.0 - 12.0 + 2.0)] },
        Stack { u0: 33.0, u1: 55.0, levels: levels(315.0, 7), piers: true, gaps: vec![(3, 30.0 + 14.0 - 2.5, 30.0 + 14.0 + 2.5)] },
    ];
    // Keep stacks clear of the corners and of each other.
    for s in west.iter_mut().chain(south.iter_mut()).chain(north.iter_mut()) {
        s.u0 = s.u0.clamp(2.0, 56.0);
        s.u1 = s.u1.clamp(s.u0 + 8.0, 58.0);
    }

    // --- Portals where bridges run into a wall without a gallery. ------------------------
    struct Portal {
        side: Side,
        rect: Rect2,
    }
    let portals = [
        // B4 (y 250, along z at x -4) both ends; B6 (y 545, along z at x 18) both ends.
        Portal { side: Side::South, rect: Rect2::new(30.0 + 4.0 - 6.0, 30.0 + 4.0 + 6.0, 250.0, 262.0) },
        Portal { side: Side::North, rect: Rect2::new(30.0 - 4.0 - 6.0, 30.0 - 4.0 + 6.0, 250.0, 262.0) },
        Portal { side: Side::South, rect: Rect2::new(30.0 - 18.0 - 1.6, 30.0 - 18.0 + 1.6, 545.0, 548.4) },
        Portal { side: Side::North, rect: Rect2::new(30.0 + 18.0 - 1.6, 30.0 + 18.0 + 1.6, 545.0, 548.4) },
        // B5 (y 120, along x at z 0) into the west wall.
        Portal { side: Side::West, rect: Rect2::new(30.0 - 4.0, 30.0 + 4.0, 120.0, 128.0) },
    ];

    // --- The hall arch on the south wall. ------------------------------------------------
    let sf = wall_frame(Side::South);
    let arch_frame = Frame::new(sf.at(south_arch_u, hall_y, 0.0), sf.u, sf.v, sf.n);
    let arch_spec = Arch { frame: arch_frame, w: ARCH_W, height: ARCH_H, span: ARCH_SPAN, spring: ARCH_SPRING, ring: 2.2, voussoirs: 19, thickness: WALL_THICKNESS, facets: 3 };
    let arch_rect = {
        let o = arch_spec.opening();
        Rect2::new(south_arch_u + o.u0, south_arch_u + o.u1, hall_y + o.v0, hall_y + o.v1)
    };

    // --- Shaft walls. --------------------------------------------------------------------
    let door_rng_seed = seed;
    let seams = |rng: &mut Rng, count: usize, lo: f32, hi: f32| -> Vec<f32> {
        let mut v: Vec<f32> = (0..count).map(|_| rng.range(lo, hi).round()).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.dedup();
        v
    };
    for side in [Side::West, Side::East, Side::South, Side::North] {
        let f = wall_frame(side);
        let mut openings: Vec<Rect2> = Vec::new();
        let stacks: &[Stack] = match side {
            Side::West => &west,
            Side::South => &south,
            Side::North => &north,
            Side::East => &[],
        };
        let door_fn = |s: u64| move |l: usize, k: usize| {
            let mut r = Rng::new(door_rng_seed ^ s, (l * 131 + k) as u64);
            r.chance(0.55)
        };
        for (i, st) in stacks.iter().enumerate() {
            let df = door_fn((side as u64) * 977 + i as u64);
            let g = Gallery { frame: f, u0: st.u0, u1: st.u1, levels: &st.levels, storey: STOREY, depth: GALLERY_DEPTH, slab: SLAB, projection: PROJECTION, bay: 6.0, piers: st.piers, door: &df, gaps: &st.gaps };
            openings.push(g.opening());
            gallery(&mut b, &g);
        }
        for p in portals.iter().filter(|p| p.side as u8 == side as u8) {
            openings.push(p.rect);
            recess(&mut b, f, p.rect, 10.0, 2.0, Some(2));
        }
        if let Side::South = side {
            openings.push(arch_rect);
            // Two ordinary doors on the ledge beside the giant arch: the scale anchor.
            for x in [23.0f32, -17.5] {
                let u = HALF - x;
                let d = Rect2::new(u - DOOR_WIDTH * 0.5, u + DOOR_WIDTH * 0.5, hall_y, hall_y + DOOR_HEIGHT);
                openings.push(d);
                recess(&mut b, f, d, 3.0, 1.0, None);
            }
        }
        // Joints: few, irregular, and none on the blank wall but one low string course.
        let (su, sv) = match side {
            Side::East => (vec![], vec![240.0]),
            _ => (seams(&mut rng, 2, 8.0, 52.0), seams(&mut rng, 7, 20.0, 590.0)),
        };
        wall(&mut b, &Wall { frame: f, size: (2.0 * HALF, TOP), thickness: WALL_THICKNESS, seams_u: &su, seams_v: &sv, openings: &openings, hatch: Vec3::Y, step: 1.5, opening_rank: Some(1), seam_rank: 2 });
    }
    // Shaft corners and rim: the massing outline.
    for (x, z, n0, n1) in [(-HALF, -HALF, Vec3::X, Vec3::Z), (HALF, -HALF, -Vec3::X, Vec3::Z), (HALF, HALF, -Vec3::X, -Vec3::Z), (-HALF, HALF, Vec3::X, -Vec3::Z)] {
        b.stroke(Vec3::new(x, 0.0, z), Vec3::new(x, TOP, z), n0, n1, crate::StrokeKind::Crease, 0);
    }
    for (a, c, n) in [
        (Vec3::new(-HALF, TOP, -HALF), Vec3::new(HALF, TOP, -HALF), Vec3::Z),
        (Vec3::new(HALF, TOP, -HALF), Vec3::new(HALF, TOP, HALF), -Vec3::X),
        (Vec3::new(HALF, TOP, HALF), Vec3::new(-HALF, TOP, HALF), -Vec3::Z),
        (Vec3::new(-HALF, TOP, HALF), Vec3::new(-HALF, TOP, -HALF), Vec3::X),
    ] {
        b.stroke(a, c, n, Vec3::Y, crate::StrokeKind::Crease, 0);
    }
    // Rim: the tops of the walls, so the edge has thickness against the sky.
    {
        let o = HALF + WALL_THICKNESS;
        let c = b.new_coarse();
        for (min, max) in [
            (Vec3::new(-o, TOP, -o), Vec3::new(o, TOP, -HALF)),
            (Vec3::new(-o, TOP, HALF), Vec3::new(o, TOP, o)),
            (Vec3::new(-o, TOP, -HALF), Vec3::new(-HALF, TOP, HALF)),
            (Vec3::new(HALF, TOP, -HALF), Vec3::new(o, TOP, HALF)),
        ] {
            b.rect(min, Vec3::X * (max.x - min.x), Vec3::Z * (max.z - min.z), Vec3::Y, Vec3::X, 4.0, c);
        }
    }
    // Floor of the shaft: big slabs, a few blocks fallen on it.
    {
        let f = Frame::new(Vec3::new(-HALF, 0.0, HALF), Vec3::X, -Vec3::Z, Vec3::Y);
        wall(&mut b, &Wall { frame: f, size: (2.0 * HALF, 2.0 * HALF), thickness: 2.0, seams_u: &[12.0, 31.0, 47.0], seams_v: &[9.0, 26.0, 44.0], openings: &[], hatch: Vec3::X, step: 2.0, opening_rank: None, seam_rank: 2 });
        let mut r = Rng::new(seed, 7);
        for _ in 0..5 {
            let (cx, cz) = (r.range(-24.0, 24.0), r.range(-24.0, 24.0));
            let (sx, sy, sz) = (r.range(2.0, 6.0), r.range(1.5, 4.0), r.range(2.0, 6.0));
            aa_block(&mut b, Vec3::new(cx - sx, 0.0, cz - sz), Vec3::new(cx + sx, sy, cz + sz), [true, true, false, true, true, true], Vec3::Y, 1.5, [Some(1), Some(1), Some(1)], true);
        }
    }

    // --- Bridges. ------------------------------------------------------------------------
    let wf = wall_frame(Side::West);
    let b1_a = Vec3::new(-HALF + PROJECTION, SPAWN_LEVEL, SPAWN_Z);
    let b1_c = Vec3::new(B1_LANDING.0.x, SPAWN_LEVEL, SPAWN_Z);
    bridge(&mut b, &Bridge { a: b1_a, c: b1_c, width: 3.6, deck: 0.9, girder: Some((1.4, 1.6)), ribs: Some(4.0), railings: true, rank: 0 });
    let _ = wf;
    // B1's landing on the blank wall.
    aa_block(&mut b, B1_LANDING.0, B1_LANDING.1, [true, false, true, true, true, true], Vec3::Z, 1.5, [Some(1), Some(2), Some(1)], true);
    {
        let (lo, hi) = B1_LANDING;
        let y = hi.y;
        railing(&mut b, Vec3::new(lo.x + 0.08, y, hi.z - 0.08), Vec3::new(hi.x, y, hi.z - 0.08), Vec3::Z, 1.5, 1);
        railing(&mut b, Vec3::new(lo.x + 0.08, y, lo.z + 0.08), Vec3::new(lo.x + 0.08, y, SPAWN_Z - 1.9), -Vec3::X, 1.5, 1);
        railing(&mut b, Vec3::new(lo.x + 0.08, y, SPAWN_Z + 1.9), Vec3::new(lo.x + 0.08, y, hi.z - 0.08), -Vec3::X, 1.5, 1);
        railing(&mut b, Vec3::new(lo.x + 0.08, y, lo.z + 0.08), Vec3::new(STAIR_X - STAIR_WIDTH * 0.5, y, lo.z + 0.08), -Vec3::Z, 1.5, 1);
    }
    // B2: y 470 along z at x -12 (south stack to north stack).
    let w2 = rng.range(3.5, 4.5);
    bridge(&mut b, &Bridge { a: Vec3::new(-12.0, 470.0, -HALF + PROJECTION), c: Vec3::new(-12.0, 470.0, HALF - PROJECTION), width: w2, deck: 0.8, girder: Some((2.2, 1.8)), ribs: Some(5.0), railings: true, rank: 0 });
    // B3: y 330 along z at x 14, heavy.
    bridge(&mut b, &Bridge { a: Vec3::new(14.0, 330.0, -HALF + PROJECTION), c: Vec3::new(14.0, 330.0, HALF - PROJECTION), width: 5.0, deck: 1.2, girder: Some((4.0, 3.0)), ribs: Some(6.0), railings: true, rank: 0 });
    // B4: y 250 along z at x -4, massive, into portals.
    bridge(&mut b, &Bridge { a: Vec3::new(-4.0, 250.0, -HALF), c: Vec3::new(-4.0, 250.0, HALF), width: 11.0, deck: 2.0, girder: Some((6.0, 7.0)), ribs: Some(7.5), railings: true, rank: 0 });
    // B5: y 120 along x at z 0, from the west portal into the east wall.
    bridge(&mut b, &Bridge { a: Vec3::new(-HALF, 120.0, 0.0), c: Vec3::new(HALF, 120.0, 0.0), width: 7.0, deck: 1.5, girder: Some((3.0, 4.0)), ribs: Some(6.0), railings: true, rank: 0 });
    // B6: y 545 along z at x 18, thin.
    bridge(&mut b, &Bridge { a: Vec3::new(18.0, 545.0, -HALF), c: Vec3::new(18.0, 545.0, HALF), width: 3.0, deck: 0.6, girder: None, ribs: Some(3.0), railings: true, rank: 0 });

    // --- Beams and struts. ---------------------------------------------------------------
    beam(&mut b, Vec3::new(-20.0, 366.0, -HALF), Vec3::new(-20.0, 366.0, HALF), 2.4, 3.0, 0, 2.0);
    beam(&mut b, Vec3::new(-HALF, 205.0, 18.0), Vec3::new(HALF, 205.0, 18.0), 2.0, 2.4, 0, 2.0);
    beam(&mut b, Vec3::new(6.0, 586.0, -HALF), Vec3::new(6.0, 586.0, HALF), 1.6, 2.0, 0, 2.0);
    // An inclined strut across the south-west corner.
    {
        let (a, c) = (Vec3::new(-HALF, 284.0, -6.0), Vec3::new(-6.0, 268.0, -HALF));
        let d = (c - a).normalize() * 1.5;
        beam(&mut b, a - d, c + d, 1.8, 1.8, 1, 2.0);
    }

    // --- The stair on the blank wall. ----------------------------------------------------
    let info = stair(&mut b, &Stair {
        foot: Vec3::new(STAIR_X, SPAWN_LEVEL, STAIR_FOOT_Z),
        dir: -Vec3::Z,
        width: STAIR_WIDTH,
        height: stair_height(),
        risers: Some(stair_layout().0),
        max_risers: STAIR_MAX_RISERS,
        landing: stair_layout().1,
        waist: 0.35,
        wall: Some(Vec3::X),
        railing: true,
    });
    debug_assert!((info.top.y - hall_y).abs() < 1e-3 && (info.top.z - LEDGE_EDGE_Z).abs() < 1e-3, "{info:?}");

    // --- South ledge from the stair head to the arch. ------------------------------------
    let ledge_x0 = -ARCH_W * 0.5 - 2.0;
    aa_block(&mut b, Vec3::new(ledge_x0, hall_y - 0.9, -HALF), Vec3::new(HALF, hall_y, LEDGE_EDGE_Z), [true, false, true, true, false, true], Vec3::X, 1.5, [Some(0), Some(2), Some(2)], true);
    {
        let z = LEDGE_EDGE_Z - 0.08;
        railing(&mut b, Vec3::new(ledge_x0 + 0.08, hall_y, z), Vec3::new(STAIR_X - STAIR_WIDTH * 0.5 - 0.05, hall_y, z), Vec3::Z, 1.5, 1);
        railing(&mut b, Vec3::new(ledge_x0 + 0.08, hall_y, -HALF), Vec3::new(ledge_x0 + 0.08, hall_y, LEDGE_EDGE_Z), -Vec3::X, 1.5, 1);
    }

    // --- The arch and the hall behind it. ------------------------------------------------
    arch(&mut b, &arch_spec);
    hall(&mut b, &mut Rng::new(seed, 11), hall_y, &arch_spec);

    // --- AO: openness of the shaft times local occlusion from the solids. ----------------
    // Thin parts (railings, stair flights and landings) do not occlude: they are narrower
    // than the 1.5 m wall tessellation, so their AO on a wall would be a row of triangles.
    let occluders: Vec<Collider> = b.colliders.iter().copied().filter(|c| {
        let (_, _, half) = c.to_obb();
        half.iter().all(|&h| h > 0.25)
    }).collect();
    let occ = Occluders::new(&occluders, 4.0, 32, 3.0);
    b.bake_ao(&|p, n| openness(p, n) * (1.0 - LOCAL_AO * (1.0 - occ.local(p, n))));

    let (spawn_eye, _) = spawn();
    let route = vec![
        Vec3::new(spawn_eye[0], SPAWN_LEVEL, spawn_eye[2]),
        Vec3::new(spawn_eye[0], SPAWN_LEVEL, SPAWN_Z),
        Vec3::new(-HALF + 0.5, SPAWN_LEVEL, SPAWN_Z),
        Vec3::new(0.0, SPAWN_LEVEL, SPAWN_Z),
        Vec3::new(STAIR_X, SPAWN_LEVEL, SPAWN_Z),
        Vec3::new(STAIR_X, SPAWN_LEVEL, STAIR_FOOT_Z + 0.5),
        Vec3::new(STAIR_X, hall_y, LEDGE_EDGE_Z - 0.6),
        Vec3::new(0.0, hall_y, LEDGE_EDGE_Z - 0.8),
        Vec3::new(0.0, hall_y, -HALF - WALL_THICKNESS * 0.5),
        Vec3::new(0.0, hall_y, -HALF - WALL_THICKNESS - 30.0),
    ];
    let (mesh, colliders) = b.finish_with_colliders();
    Shaft { mesh, colliders, route }
}

/// Openness parameters: sky light falls off with depth below the rim towards `OPEN_MIN`
/// over `OPEN_DEPTH` metres, and with distance into the walls over `OPEN_INTO_WALL`.
pub const OPEN_MIN: f32 = 0.45;
pub const OPEN_DEPTH: f32 = 250.0;
pub const OPEN_INTO_WALL: f32 = 160.0;
/// How strongly local occlusion (corners, recesses) darkens, 0..1.
pub const LOCAL_AO: f32 = 0.7;

/// Analytic large-scale openness: how much of the sky's (multi-bounce) light reaches `p`.
/// Smooth everywhere, so it can be interpolated over large triangles.
pub fn openness(p: Vec3, n: Vec3) -> f32 {
    let depth = (TOP - p.y).max(0.0);
    let sky = OPEN_MIN + (1.0 - OPEN_MIN) * (-depth / OPEN_DEPTH).exp();
    let into = (p.x.abs() - HALF).max(p.z.abs() - HALF).max(0.0);
    let inner = (-into / OPEN_INTO_WALL).exp();
    // In the shaft light comes from above; inside the walls (galleries, the hall) it comes
    // from the shaft, so faces turned towards the shaft are lighter.
    let up = 1.0 + 0.25 * n.y;
    let to_shaft = Vec3::new(p.x.clamp(-HALF, HALF) - p.x, 0.0, p.z.clamp(-HALF, HALF) - p.z);
    let facing = if to_shaft.length() > 1e-3 {
        let w = (into / 6.0).min(1.0);
        up * (1.0 - w) + (0.8 + 0.3 * n.dot(to_shaft.normalize()) + 0.1 * n.y) * w
    } else {
        up
    };
    sky * inner * facing
}

fn hall(b: &mut MeshBuilder, rng: &mut Rng, y0: f32, arch: &Arch) {
    let z0 = -HALF - WALL_THICKNESS; // inner face of the front wall
    let z1 = z0 - HALL_DEPTH;
    let (x0, x1) = (-HALF, HALF);
    let h = HALL_HEIGHT;
    let y1 = y0 + h;
    let step = 2.0;
    // Floor, inward-facing walls and ceiling, as walls with seams.
    let floor = Frame::new(Vec3::new(x0, y0, z0), Vec3::X, -Vec3::Z, Vec3::Y);
    wall(b, &Wall { frame: floor, size: (x1 - x0, HALL_DEPTH), thickness: 1.0, seams_u: &[6.0, 18.0, 30.0, 42.0, 54.0], seams_v: &[12.0, 28.0, 44.0, 60.0, 76.0, 92.0], openings: &[], hatch: Vec3::Z, step, opening_rank: None, seam_rank: 2 });
    let ceil = Frame::new(Vec3::new(x0, y1, z1), Vec3::X, Vec3::Z, -Vec3::Y);
    wall(b, &Wall { frame: ceil, size: (x1 - x0, HALL_DEPTH), thickness: 1.0, seams_u: &[], seams_v: &[], openings: &[], hatch: Vec3::Z, step: 3.0, opening_rank: None, seam_rank: 2 });
    let tall: Vec<f32> = (1..8).map(|k| k as f32 * 12.5 + rng.range(-0.5, 0.5)).collect();
    let rows = [h * 0.42];
    for (f, len) in [
        (Frame::new(Vec3::new(x0, y0, z0), -Vec3::Z, Vec3::Y, Vec3::X), HALL_DEPTH),
        (Frame::new(Vec3::new(x1, y0, z1), Vec3::Z, Vec3::Y, -Vec3::X), HALL_DEPTH),
    ] {
        wall(b, &Wall { frame: f, size: (len, h), thickness: 1.0, seams_u: &tall, seams_v: &rows, openings: &[], hatch: Vec3::Y, step, opening_rank: None, seam_rank: 2 });
    }
    // Back wall with one small door at human scale, far from the arch.
    let back = Frame::new(Vec3::new(x0, y0, z1), Vec3::X, Vec3::Y, Vec3::Z);
    let door = Rect2::new(30.0 - DOOR_WIDTH * 0.5 + 3.0, 30.0 + DOOR_WIDTH * 0.5 + 3.0, 0.0, DOOR_HEIGHT);
    wall(b, &Wall { frame: back, size: (x1 - x0, h), thickness: 1.0, seams_u: &[15.0, 45.0], seams_v: &rows, openings: &[door], hatch: Vec3::Y, step, opening_rank: Some(2), seam_rank: 2 });
    recess(b, back, door, 3.0, 1.0, None);
    // Front wall, inside face, with the arch's rectangle left open (the arch emits its back face).
    let front = Frame::new(Vec3::new(x1, y0, z0), -Vec3::X, Vec3::Y, -Vec3::Z);
    let o = arch.opening();
    let open = Rect2::new(HALF + o.u0, HALF + o.u1, o.v0, o.v1);
    wall(b, &Wall { frame: front, size: (x1 - x0, h), thickness: 0.0, seams_u: &[], seams_v: &rows, openings: &[open], hatch: Vec3::Y, step, opening_rank: None, seam_rank: 2 });
    // Hall box creases (inward), rank 1.
    for (a, c) in [
        (Vec3::new(x0, y0, z0), Vec3::new(x0, y0, z1)),
        (Vec3::new(x1, y0, z0), Vec3::new(x1, y0, z1)),
        (Vec3::new(x0, y1, z0), Vec3::new(x0, y1, z1)),
        (Vec3::new(x1, y1, z0), Vec3::new(x1, y1, z1)),
        (Vec3::new(x0, y0, z1), Vec3::new(x1, y0, z1)),
        (Vec3::new(x0, y1, z1), Vec3::new(x1, y1, z1)),
        (Vec3::new(x0, y0, z1), Vec3::new(x0, y1, z1)),
        (Vec3::new(x1, y0, z1), Vec3::new(x1, y1, z1)),
        (Vec3::new(x0, y1, z0), Vec3::new(x1, y1, z0)),
        (Vec3::new(x0, y0, z0), Vec3::new(x0, y1, z0)),
        (Vec3::new(x1, y0, z0), Vec3::new(x1, y1, z0)),
    ] {
        b.stroke(a, c, Vec3::Y, Vec3::Y, crate::StrokeKind::Crease, 1);
    }
    for (a, c) in [(Vec3::new(x0, y0, z0), Vec3::new(-o.u1, y0, z0)), (Vec3::new(o.u1, y0, z0), Vec3::new(x1, y0, z0))] {
        b.stroke(a, c, Vec3::Y, -Vec3::Z, crate::StrokeKind::Crease, 1);
    }
    // Floor through the arch.
    aa_block(b, Vec3::new(-ARCH_SPAN * 0.5, y0 - 1.0, z0), Vec3::new(ARCH_SPAN * 0.5, y0, -HALF), [false, false, false, true, false, false], Vec3::Z, 1.5, [None, None, None], true);
    // Two rows of round columns, one missing; beams along each row under the ceiling.
    let missing = (1usize, 3usize);
    for (ri, &cx) in [-12.0f32, 12.0].iter().enumerate() {
        for k in 0..6 {
            if (ri, k) == missing {
                // Its plinth remains.
                let cz = z0 - 12.0 - 16.0 * k as f32;
                aa_block(b, Vec3::new(cx - 2.1, y0, cz - 2.1), Vec3::new(cx + 2.1, y0 + 0.9, cz + 2.1), [true, true, false, true, true, true], Vec3::Y, 1.0, [Some(1), Some(1), Some(1)], true);
                continue;
            }
            let cz = z0 - 12.0 - 16.0 * k as f32;
            column_round(b, Vec3::new(cx, y0, cz), 1.7, h - 3.0, 20, 6.5, 0.4, 1.5);
        }
        beam(b, Vec3::new(cx, y1 - 1.5, z0), Vec3::new(cx, y1 - 1.5, z1), 2.6, 3.0, 1, 2.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_a_seed() {
        let a = build(DEFAULT_SEED).mesh.content_hash();
        let b = build(DEFAULT_SEED).mesh.content_hash();
        assert_eq!(a, b);
    }

    #[test]
    fn seeds_differ() {
        let a = build(1).mesh.content_hash();
        let b = build(2).mesh.content_hash();
        assert_ne!(a, b);
    }

    #[test]
    fn invariants_hold() {
        for seed in [DEFAULT_SEED, 1, 2, 3] {
            let s = build(seed);
            s.mesh.check_invariants().unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            assert!(s.mesh.strokes.iter().any(|s| s.rank == 0));
            let (lo, hi) = s.mesh.bounds().unwrap();
            assert!(hi[1] - lo[1] >= TOP - 1.0 && hi[0] - lo[0] >= 2.0 * HALF);
        }
    }

    #[test]
    fn stairs_meet_rise_and_run_limits() {
        let s = build(DEFAULT_SEED);
        let ramps: Vec<(Vec3, Vec3)> = s.colliders.iter().filter_map(|c| if let Collider::Ramp { start, end, .. } = c { Some((Vec3::from(*start), Vec3::from(*end))) } else { None }).collect();
        assert!(!ramps.is_empty());
        for (a, c) in ramps {
            let rise = c.y - a.y;
            let run = (c - a).with_y(0.0).length();
            let steps = (run / STAIR_RUN).round();
            assert!((steps * STAIR_RUN - run).abs() < 1e-3, "run {run} is not whole treads");
            let r = rise / steps;
            assert!((STAIR_RISE_MIN..=STAIR_RISE_MAX).contains(&r), "rise {r}");
            assert!((0.28..=0.32).contains(&STAIR_RUN));
            assert!((rise / run).atan().to_degrees() < 40.0);
        }
        // And from the geometry: risers (faces with horizontal normals on the stair lane)
        // are 0.15..0.18 tall.
        let m = &s.mesh;
        let mut checked = 0;
        let mut by_fine: std::collections::BTreeMap<u32, (f32, f32, usize)> = Default::default();
        for v in &m.vertices {
            let p = Vec3::from(v.position);
            if v.normal == [0.0, 0.0, 1.0] && (p.x - STAIR_X).abs() < STAIR_WIDTH * 0.5 + 1e-3 && p.y > SPAWN_LEVEL && p.y < hall_floor() + 0.01 {
                let e = by_fine.entry(v.id_fine).or_insert((f32::MAX, f32::MIN, 0));
                e.0 = e.0.min(p.y);
                e.1 = e.1.max(p.y);
                e.2 += 1;
            }
        }
        for (_, (lo, hi, n)) in by_fine {
            // Four-vertex faces lower than 0.25 m are risers (landing ends are 0.35 m).
            if n == 4 && hi - lo < 0.25 {
                let r = hi - lo;
                assert!((STAIR_RISE_MIN - 1e-3..=STAIR_RISE_MAX + 1e-3).contains(&r), "riser height {r}");
                checked += 1;
            }
        }
        assert!(checked > 100, "only {checked} risers found");
    }

    /// Height of the walkable surface under (x, z) at or below `y + step`, from the colliders.
    fn ground(cols: &[Collider], x: f32, z: f32, y: f32, step: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for c in cols {
            let top = match *c {
                Collider::Aabb { min, max } => {
                    if x >= min[0] && x <= max[0] && z >= min[2] && z <= max[2] {
                        Some(max[1])
                    } else {
                        None
                    }
                }
                Collider::Ramp { start, end, half_width, .. } => {
                    let (a, e) = (Vec3::from(start), Vec3::from(end));
                    let d = (e - a).with_y(0.0);
                    let len = d.length();
                    let dir = d / len;
                    let rel = Vec3::new(x, 0.0, z) - a.with_y(0.0);
                    let s = rel.dot(dir);
                    let w = (rel - dir * s).length();
                    if s >= 0.0 && s <= len && w <= half_width {
                        Some(a.y + (e.y - a.y) * s / len)
                    } else {
                        None
                    }
                }
                Collider::Obb { .. } => None,
            };
            if let Some(t) = top {
                if t <= y + step + 1e-3 && best.map_or(true, |b| t > b) {
                    best = Some(t);
                }
            }
        }
        best
    }

    #[test]
    fn colliders_are_sane() {
        let s = build(DEFAULT_SEED);
        assert!(s.colliders.len() > 100);
        for c in &s.colliders {
            let (lo, hi) = c.bounds();
            assert!((0..3).all(|i| lo[i].is_finite() && hi[i].is_finite() && lo[i] < hi[i]), "{c:?}");
            if let Collider::Obb { axes, .. } = c {
                for i in 0..3 {
                    let a = Vec3::from(axes[i]);
                    assert!((a.length() - 1.0).abs() < 1e-4);
                    for j in (i + 1)..3 {
                        assert!(a.dot(Vec3::from(axes[j])).abs() < 1e-4);
                    }
                }
            }
        }
        // The spawn stands on something, with headroom.
        let (eye, _) = spawn();
        let g = ground(&s.colliders, eye[0], eye[2], eye[1] - 1.65, 0.3).expect("nothing under the spawn");
        assert!((eye[1] - 1.65 - g).abs() < 0.05, "spawn floor {g}");
        for y in [0.3, 1.0, 1.7] {
            let p = [eye[0], g + y, eye[2]];
            assert!(!s.colliders.iter().any(|c| c.contains(p)), "spawn inside a collider at {p:?}");
        }
    }

    /// Walk the route in 0.2 m steps: there must always be ground within the step-up height,
    /// slopes under 40 degrees, and a 1.8 m by 0.3 m capsule must fit above it (ignoring the
    /// solid it stands on).
    #[test]
    fn route_is_walkable_in_collider_space() {
        let s = build(DEFAULT_SEED);
        let mut y = s.route[0].y;
        for w in s.route.windows(2) {
            let (a, c) = (w[0], w[1]);
            let d = (c - a).with_y(0.0);
            let n = (d.length() / 0.2).ceil().max(1.0) as usize;
            for i in 0..=n {
                let p = a + d * (i as f32 / n as f32);
                let g = ground(&s.colliders, p.x, p.z, y, 0.3).unwrap_or_else(|| panic!("no ground at {p:?} (from y {y})"));
                assert!((g - y).abs() <= 0.3, "height change from {y} to {g} at {p:?}");
                y = g;
                for dy in [0.35f32, 0.9, 1.5, 1.75] {
                    for (ox, oz) in [(0.0, 0.0), (0.25, 0.0), (-0.25, 0.0), (0.0, 0.25), (0.0, -0.25)] {
                        let q = [p.x + ox, y + dy, p.z + oz];
                        if let Some(c) = s.colliders.iter().find(|c| c.contains(q)) {
                            // Standing on a ramp, the slope ahead is not an obstacle at shin height.
                            let ramp = matches!(c, Collider::Ramp { .. });
                            assert!(ramp && dy < 0.5, "blocked at {q:?} by {c:?}");
                        }
                    }
                }
            }
            assert!((y - c.y).abs() < 0.31, "route waypoint {c:?} not reached (at y {y})");
        }
    }
}
