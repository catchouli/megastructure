//! Named deterministic camera paths: functions of `t` in 0..1 (clamped).
//!
//! Each path belongs to a scene ([`scene_of`]): the `corridor_*`, `hall_orbit` and
//! `still_hall` paths are tuned to `mega_gen::test_scene`, the `shaft_*` paths to
//! `mega_gen::shaft`. Add new paths to [`NAMES`], [`by_name`] and [`scene_of`].

use crate::Camera;
use glam::Vec3;
use mega_gen::shaft;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

pub const EYE_HEIGHT: f32 = 1.65;

pub const NAMES: &[&str] = &[
    "corridor_walk",
    "corridor_strafe",
    "hall_orbit",
    "still_hall",
    "shaft_spawn",
    "shaft_gallery_down",
    "shaft_bridge_walk",
    "shaft_stair_climb",
    "shaft_stair_view",
    "shaft_stair_below",
    "shaft_look_up",
    "shaft_hall_enter",
    "shaft_arch_view",
    "shaft_rim_down",
    "shaft_b2_down",
    "hall_yaw",
    "hall_pitch",
    "hall_walk",
    "shaft_spawn_yaw",
    "shaft_spawn_pitch",
    "shaft_hall_wall",
    "shaft_wall_approach",
    "shaft_hall_walk",
    "shaft_rim_pitch",
    "lab_spawn",
    "lab_far_ledge_axis",
    "lab_hall_from_arch",
    "lab_hall_exterior_end",
    "lab_void_bottom_up",
    "lab_stair_hall_foot",
    "lab_stair_hall_landing",
    "lab_cistern_walkway",
    "lab_cistern_basin",
    "lab_undercroft_shaft",
    "lab_shaft_up",
    "lab_cleft",
    "lab_well_bridge",
    "lab_lantern_crossing",
    "lab_corridor_vaulted",
    "lab_corridor_colonnade",
    "lab_corridor_slot",
    "lab_corridor_low",
    "lab_corridor_gallery",
    "lab_corridor_plain",
    "lab_duct",
    "lab_service",
    "lab_vent",
    "lab_axis_walk",
    "lab_far_ledge_yaw",
    "lab_corridor_walk",
    "lab_passage_ledge",
    "shaft_hall_deep_in",
    "shaft_hall_deep_back",
    "shaft_hall_deep_side",
];

pub fn by_name(name: &str) -> Option<fn(f32) -> Camera> {
    Some(match name {
        "corridor_walk" => corridor_walk,
        "corridor_strafe" => corridor_strafe,
        "hall_orbit" => hall_orbit,
        "still_hall" => still_hall,
        "shaft_spawn" => shaft_spawn,
        "shaft_gallery_down" => shaft_gallery_down,
        "shaft_bridge_walk" => shaft_bridge_walk,
        "shaft_stair_climb" => shaft_stair_climb,
        "shaft_stair_view" => shaft_stair_view,
        "shaft_stair_below" => shaft_stair_below,
        "shaft_look_up" => shaft_look_up,
        "shaft_hall_enter" => shaft_hall_enter,
        "shaft_arch_view" => shaft_arch_view,
        "shaft_rim_down" => shaft_rim_down,
        "shaft_b2_down" => shaft_b2_down,
        "hall_yaw" => hall_yaw,
        "hall_pitch" => hall_pitch,
        "hall_walk" => hall_walk,
        "shaft_spawn_yaw" => shaft_spawn_yaw,
        "shaft_spawn_pitch" => shaft_spawn_pitch,
        "shaft_hall_wall" => shaft_hall_wall,
        "shaft_wall_approach" => shaft_wall_approach,
        "shaft_hall_walk" => shaft_hall_walk,
        "shaft_rim_pitch" => shaft_rim_pitch,
        "lab_spawn" => lab_spawn,
        "lab_far_ledge_axis" => lab_far_ledge_axis,
        "lab_hall_from_arch" => lab_hall_from_arch,
        "lab_hall_exterior_end" => lab_hall_exterior_end,
        "lab_void_bottom_up" => lab_void_bottom_up,
        "lab_stair_hall_foot" => lab_stair_hall_foot,
        "lab_stair_hall_landing" => lab_stair_hall_landing,
        "lab_cistern_walkway" => lab_cistern_walkway,
        "lab_cistern_basin" => lab_cistern_basin,
        "lab_undercroft_shaft" => lab_undercroft_shaft,
        "lab_shaft_up" => lab_shaft_up,
        "lab_cleft" => lab_cleft,
        "lab_well_bridge" => lab_well_bridge,
        "lab_lantern_crossing" => lab_lantern_crossing,
        "lab_corridor_vaulted" => lab_corridor_vaulted,
        "lab_corridor_colonnade" => lab_corridor_colonnade,
        "lab_corridor_slot" => lab_corridor_slot,
        "lab_corridor_low" => lab_corridor_low,
        "lab_corridor_gallery" => lab_corridor_gallery,
        "lab_corridor_plain" => lab_corridor_plain,
        "lab_duct" => lab_duct,
        "lab_service" => lab_service,
        "lab_vent" => lab_vent,
        "lab_axis_walk" => lab_axis_walk,
        "lab_far_ledge_yaw" => lab_far_ledge_yaw,
        "lab_corridor_walk" => lab_corridor_walk,
        "lab_passage_ledge" => lab_passage_ledge,
        "shaft_hall_deep_in" => shaft_hall_deep_in,
        "shaft_hall_deep_back" => shaft_hall_deep_back,
        "shaft_hall_deep_side" => shaft_hall_deep_side,
        _ => return None,
    })
}

/// The scene (a `mega_gen::scenes` name) a path is tuned to, or None for an unknown path.
pub fn scene_of(name: &str) -> Option<&'static str> {
    by_name(name)?;
    Some(if name.starts_with("shaft_") {
        "shaft"
    } else if name.starts_with("lab_") {
        "labyrinth"
    } else {
        "test"
    })
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Straight walk down the corridor axis, through the arch and 40 m into the hall.
pub fn corridor_walk(t: f32) -> Camera {
    Camera::new(Vec3::new(0.0, EYE_HEIGHT, lerp(2.0, 160.0, t)), 0.0, 0.0)
}

/// Sideways motion across the corridor at z=40, looking straight down it.
pub fn corridor_strafe(t: f32) -> Camera {
    Camera::new(Vec3::new(lerp(-1.3, 1.3, t), EYE_HEIGHT, 40.0), 0.0, 0.0)
}

/// One full lap of the hall at radius 22 m around its middle, 9 m up, looking at the centre.
pub fn hall_orbit(t: f32) -> Camera {
    let centre = Vec3::new(0.0, 0.0, 195.0);
    let a = t.clamp(0.0, 1.0) * TAU;
    let eye = centre + Vec3::new(22.0 * a.sin(), 9.0, 22.0 * a.cos());
    Camera::look_at(eye, centre + Vec3::new(0.0, 8.0, 0.0))
}

/// One fixed view from the vantage in the middle of the hall, looking back towards the arched doorway with the stair
/// climbing the left wall.
pub fn still_hall(_t: f32) -> Camera {
    Camera::look_at(Vec3::new(-2.0, 5.0, 172.0), Vec3::new(-16.0, 6.0, 130.0))
}

// --- Rotation and walking-speed probes (E020) ------------------------------------------------
//
// Over t 0..1 the rotation paths turn 90 degrees, so `--frames 61` is a turn at 90 degrees
// per second at 60 fps (1.5 degrees per frame). `hall_walk` moves 6 m over t 0..1, so
// `--frames 61 --t1 0.2333` is 1.4 m/s at 60 fps (2.3 cm per frame). The position is fixed
// in the rotation paths, so reprojection between frames is exact and any change in the ink
// on surfaces is swimming.

/// From the `still_hall` position, turning 90 degrees left to right about the vertical.
pub fn hall_yaw(t: f32) -> Camera {
    let c = still_hall(0.0);
    Camera::new(c.position, c.yaw + lerp(-0.25 * PI, 0.25 * PI, t), c.pitch)
}

/// From the `still_hall` position, tipping from 45 degrees down to 45 degrees up.
pub fn hall_pitch(t: f32) -> Camera {
    let c = still_hall(0.0);
    Camera::new(c.position, c.yaw, lerp(-0.25 * PI, 0.25 * PI, t))
}

/// From the `still_hall` position, walking 6 m along its view direction (level).
pub fn hall_walk(t: f32) -> Camera {
    let c = still_hall(0.0);
    let d = Vec3::new(c.yaw.sin(), 0.0, c.yaw.cos());
    Camera::new(c.position + d * lerp(0.0, 6.0, t), c.yaw, c.pitch)
}

/// At the shaft spawn, turning 90 degrees left to right.
pub fn shaft_spawn_yaw(t: f32) -> Camera {
    let (eye, yaw) = shaft::spawn();
    Camera::new(Vec3::from(eye), yaw + lerp(-0.25 * PI, 0.25 * PI, t), -0.22)
}

/// At the shaft spawn, tipping from 45 degrees down to 45 degrees up.
pub fn shaft_spawn_pitch(t: f32) -> Camera {
    let (eye, yaw) = shaft::spawn();
    Camera::new(Vec3::from(eye), yaw, lerp(-0.25 * PI, 0.25 * PI, t))
}

/// The owner's screenshot 20261116-000630 (facing the far wall of the hall), approached along
/// its view direction from 78 m to 2 m away: t 0.85 is the screenshot's distance (about 10 m).
/// For checking that the hatching keeps its character with distance (E026).
pub fn shaft_hall_wall(t: f32) -> Camera {
    let z = lerp(-60.0, -136.0, t);
    Camera::new(Vec3::new(-0.02, 432.24, z), 3.2532, 0.0387)
}

/// Square on to the back wall of the hall (z = -138) at the screenshot's height, from 160 m
/// to 2 m on a log scale: distance `d = 2 * 80^(1 - t)` metres. For the tone-against-distance
/// checks (E028): t 1, 0.791, 0.633, 0.382, 0.159, 0 are 2, 5, 10, 30, 80 and 160 m.
pub fn shaft_wall_approach(t: f32) -> Camera {
    let d = 2.0 * 80f32.powf(1.0 - t.clamp(0.0, 1.0));
    Camera::new(Vec3::new(-0.02, 432.24, -138.0 + d), PI, 0.0)
}

/// Walking 60 m down the hall at eye height, along its floor and past its columns, eyes
/// slightly down, so the floor recedes at grazing angles through many octaves of the surface
/// screentone. `--frames 61 --t1 0.02333` is 1.4 m/s at 60 fps (2.3 cm per frame); the whole
/// path is 60 m (E036).
pub fn shaft_hall_walk(t: f32) -> Camera {
    let y = shaft::hall_floor() + EYE_HEIGHT;
    Camera::new(Vec3::new(-7.0, y, lerp(-42.0, -102.0, t)), PI, -0.12)
}

/// Just below the rim, tipping from looking across the shaft (pitch -0.4) to straight down
/// it (-1.45): the far walls descend into darkness through the frame (E054). Over t 0..1
/// the pitch changes 60 degrees; `--frames 61` is 1 degree per frame.
pub fn shaft_rim_pitch(t: f32) -> Camera {
    let eye = Vec3::new(-20.0, shaft::TOP - 6.0, 20.0);
    Camera::new(eye, 2.36, lerp(-0.4, -1.45, t))
}

// Deep in the giant hall, the darkest places in the scene, for judging pencil shading in
// shade (E056). Over t the eye moves 4 m, so `--t` picks a slightly different spot.

/// 80 m into the hall, looking further in at the back wall and its door.
pub fn shaft_hall_deep_in(t: f32) -> Camera {
    let y = shaft::hall_floor() + EYE_HEIGHT;
    Camera::new(Vec3::new(lerp(-6.0, -2.0, t), y, -118.0), PI + 0.15, 0.05)
}

/// At the back of the hall, looking back towards the arch and the lit shaft.
pub fn shaft_hall_deep_back(t: f32) -> Camera {
    let y = shaft::hall_floor() + EYE_HEIGHT;
    Camera::new(Vec3::new(lerp(8.0, 4.0, t), y, -130.0), -0.1, 0.08)
}

/// In the back corner of the hall, looking along the side wall towards the arch.
pub fn shaft_hall_deep_side(t: f32) -> Camera {
    let y = shaft::hall_floor() + EYE_HEIGHT;
    Camera::new(Vec3::new(-26.0, y, lerp(-134.0, -130.0, t)), 0.35, 0.12)
}

// --- Labyrinth: cameras from the generator's plan (`Plan::landmarks`, default seed), so they
// follow the layout when it changes (E069). The plan is computed once per process. A name
// the plan does not provide falls back to the spawn camera. ------------------------------

fn lab_landmarks() -> &'static [mega_gen::labyrinth::plan::Landmark] {
    static L: std::sync::OnceLock<Vec<mega_gen::labyrinth::plan::Landmark>> = std::sync::OnceLock::new();
    L.get_or_init(|| mega_gen::labyrinth::plan::plan(mega_gen::labyrinth::DEFAULT_SEED).landmarks())
}

/// The landmark camera `name`, 1.4 rad vertical field of view.
pub fn lab_landmark(name: &str) -> Camera {
    let l = lab_landmarks();
    let m = l.iter().find(|m| m.name == name).or_else(|| l.iter().find(|m| m.name == "spawn"));
    match m {
        Some(m) => Camera { position: m.eye, yaw: m.yaw, pitch: m.pitch, fov_y: 1.4 },
        None => Camera::new(Vec3::ZERO, 0.0, 0.0),
    }
}

macro_rules! lab_paths {
    ($($f:ident => $n:literal),* $(,)?) => {
        $(
            #[doc = concat!("The labyrinth landmark `", $n, "`.")]
            pub fn $f(_t: f32) -> Camera {
                lab_landmark($n)
            }
        )*
    };
}

lab_paths! {
    lab_spawn => "spawn",
    lab_far_ledge_axis => "far_ledge_axis",
    lab_hall_from_arch => "hall_from_arch",
    lab_hall_exterior_end => "hall_exterior_end",
    lab_void_bottom_up => "void_bottom_up",
    lab_stair_hall_foot => "stair_hall_foot",
    lab_stair_hall_landing => "stair_hall_landing",
    lab_cistern_walkway => "cistern_walkway",
    lab_cistern_basin => "cistern_basin",
    lab_undercroft_shaft => "undercroft_shaft",
    lab_shaft_up => "shaft_up",
    lab_cleft => "cleft",
    lab_well_bridge => "well_bridge",
    lab_lantern_crossing => "lantern_crossing",
    lab_corridor_vaulted => "corridor_vaulted",
    lab_corridor_colonnade => "corridor_colonnade",
    lab_corridor_slot => "corridor_slot",
    lab_corridor_low => "corridor_low",
    lab_corridor_gallery => "corridor_gallery",
    lab_corridor_plain => "corridor_plain",
    lab_duct => "duct",
    lab_service => "service",
    lab_vent => "vent",
}

/// From `far_ledge_axis` to `hall_from_arch` (across the bridge and through the arch),
/// looking along the way. `--frames 61 --t1 x` with x = 1.4 m/s * 1 s / path length.
pub fn lab_axis_walk(t: f32) -> Camera {
    let a = lab_landmark("far_ledge_axis");
    let b = lab_landmark("hall_from_arch");
    Camera { position: a.position.lerp(b.position, t.clamp(0.0, 1.0)), pitch: 0.05, ..a }
}

/// From `corridor_slot`, walking 8 m along its view (level), for the player light (E070).
/// `--frames 61 --t1 0.175` is 1.4 m/s at 60 fps.
pub fn lab_corridor_walk(t: f32) -> Camera {
    let c = lab_landmark("corridor_slot");
    let d = Vec3::new(c.yaw.sin(), 0.0, c.yaw.cos());
    Camera { position: c.position + d * lerp(0.0, 8.0, t.clamp(0.0, 1.0)), pitch: 0.0, ..c }
}

/// The walk for exposure adaptation (E073): along a sealed passage out onto a ledge over the
/// void. Of the passages that end on a ledge with a level run of at least 8 m before the door,
/// the darkest kind (service, slot, low, plain, vaulted, in that order; the run up to 14 m); the line runs from the inner end of that
/// run through the door cell to a point 1.6 m out on the ledge, at eye height. Computed once
/// per process from the default seed.
fn ledge_passage_line() -> &'static [Vec3] {
    use mega_gen::labyrinth::{graph::SpaceKind, grid::Cell, plan::PassStyle, CELL};
    static L: std::sync::OnceLock<Vec<Vec3>> = std::sync::OnceLock::new();
    L.get_or_init(|| {
        let plan = mega_gen::labyrinth::plan::plan(mega_gen::labyrinth::DEFAULT_SEED);
        let eye = |c: Cell| c.floor_centre() + Vec3::Y * 1.65;
        let mut best: Vec<Vec3> = Vec::new();
        let mut best_key = (usize::MAX, 0.0f32);
        const DARK_FIRST: [PassStyle; 5] = [PassStyle::Service, PassStyle::Slot, PassStyle::Low, PassStyle::Plain, PassStyle::Vaulted];
        for pass in plan.passages.iter() {
            let Some(rank) = DARK_FIRST.iter().position(|s| *s == pass.style) else { continue };
            for end in pass.ends.iter().filter(|e| plan.graph.spaces[e.target].kind == SpaceKind::Ledge) {
                let mut cells: Vec<Cell> = pass.flats().collect();
                if cells.first() != Some(&end.cell) {
                    cells.reverse();
                }
                if cells.first() != Some(&end.cell) {
                    continue;
                }
                let (dx, dz) = end.dir.d();
                let out = eye(end.cell) + Vec3::new(dx as f32, 0.0, dz as f32) * (CELL * 0.5 + 1.6);
                let mut line = vec![out];
                let mut len = 0.0;
                let mut prev: Option<Cell> = None;
                for c in cells {
                    if let Some(p) = prev {
                        if c.y != p.y || (c.x - p.x).abs() + (c.z - p.z).abs() != 1 || len >= 14.0 {
                            break;
                        }
                    }
                    let q = eye(c);
                    len += q.distance(*line.last().unwrap());
                    line.push(q);
                    prev = Some(c);
                }
                if len >= 8.0 && (rank < best_key.0 || (rank == best_key.0 && len > best_key.1)) {
                    best_key = (rank, len);
                    best = line;
                }
            }
        }
        if best.len() < 2 {
            let sp = lab_landmark("spawn").position;
            best = vec![sp, sp + Vec3::Z * 8.0];
        }
        best.reverse();
        best
    })
}

/// A point at arc length `s` along `line`, extended straight beyond both ends.
fn along(line: &[Vec3], s: f32) -> Vec3 {
    let n = line.len();
    if n < 2 {
        return line.first().copied().unwrap_or(Vec3::ZERO);
    }
    if s <= 0.0 {
        return line[0] + (line[0] - line[1]).normalize_or_zero() * -s;
    }
    let mut s = s;
    for w in line.windows(2) {
        let l = w[0].distance(w[1]);
        if s <= l {
            return w[0].lerp(w[1], s / l.max(1e-6));
        }
        s -= l;
    }
    line[n - 1] + (line[n - 1] - line[n - 2]).normalize_or_zero() * s
}

/// From inside a sealed passage out through its door onto a ledge over the void and back
/// (E073, exposure adaptation), following the passage's cells (`ledge_passage_line`): walking out over t 0..0.4,
/// standing on the ledge looking at the void until 0.5, turning round until 0.55, walking
/// back in until 0.95, standing. The heading looks 1.5 m ahead and behind on the line, so
/// it turns smoothly at corners. Frames for 1.4 m/s at 30 fps: length / 1.4 / 0.4 * 30
/// (`ledge_passage_length` prints the length in tests).
pub fn lab_passage_ledge(t: f32) -> Camera {
    let c = lab_landmark("spawn");
    let line = ledge_passage_line();
    let len: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
    let t = t.clamp(0.0, 1.0);
    let ease = |x: f32| x * x * (3.0 - 2.0 * x);
    let at = |s: f32, back: bool| {
        let d = along(line, s + 1.5) - along(line, s - 1.5);
        let yaw = d.x.atan2(d.z) + if back { PI } else { 0.0 };
        Camera { position: along(line, s), yaw, pitch: 0.0, ..c }
    };
    if t < 0.4 {
        at(len * t / 0.4, false)
    } else if t < 0.5 {
        at(len, false)
    } else if t < 0.55 {
        let a = at(len, false);
        Camera { yaw: a.yaw + PI * ease((t - 0.5) / 0.05), ..a }
    } else {
        at(len * (1.0 - ((t - 0.55) / 0.4).min(1.0)), true)
    }
}

/// At `far_ledge_axis`, turning 90 degrees left to right.
pub fn lab_far_ledge_yaw(t: f32) -> Camera {
    let c = lab_landmark("far_ledge_axis");
    Camera { yaw: c.yaw + lerp(-0.25 * PI, 0.25 * PI, t), ..c }
}

// --- One Shaft -----------------------------------------------------------------------------

/// The spawn view: on the west gallery at y 400, looking east across the void at the stair on
/// the blank wall. Over t the head turns slowly from slightly left to slightly right.
pub fn shaft_spawn(t: f32) -> Camera {
    let (eye, yaw) = shaft::spawn();
    Camera::new(Vec3::from(eye), yaw + lerp(-0.15, 0.15, t), -0.22)
}

/// At the spawn gallery's rail, looking over it: the view tips from across the void to
/// straight down the shaft (400 m to the floor).
pub fn shaft_gallery_down(t: f32) -> Camera {
    let eye = Vec3::new(-29.6, shaft::SPAWN_LEVEL + EYE_HEIGHT, 8.0);
    Camera::new(eye, FRAC_PI_2 - 0.35, lerp(-0.35, -1.45, t))
}

/// Walking bridge B1 from the west gallery to the blank wall, eyes slightly down.
pub fn shaft_bridge_walk(t: f32) -> Camera {
    let eye = Vec3::new(lerp(-28.5, 24.0, t), shaft::SPAWN_LEVEL + EYE_HEIGHT, shaft::SPAWN_Z);
    Camera::new(eye, FRAC_PI_2, -0.08)
}

/// Climbing the stair on the blank wall, looking up the flight with the wall on the left
/// and the void on the right.
pub fn shaft_stair_climb(t: f32) -> Camera {
    let z = lerp(shaft::STAIR_FOOT_Z - 0.3, shaft::LEDGE_EDGE_Z + 1.0, t);
    let eye = Vec3::new(shaft::STAIR_X - 0.1, shaft::stair_surface(z) + EYE_HEIGHT, z);
    Camera::new(eye, PI - 0.25, 0.12)
}

/// The stair as an image: from the south ledge by the stair head, looking down and north
/// along the blank wall, the stair falling away to bridge B1's landing.
pub fn shaft_stair_view(t: f32) -> Camera {
    let y = shaft::hall_floor() + EYE_HEIGHT;
    let eye = Vec3::new(lerp(24.0, 22.0, t), y, shaft::LEDGE_EDGE_Z - 0.6);
    Camera::look_at(eye, Vec3::new(30.0, shaft::SPAWN_LEVEL - 25.0, 22.0))
}

/// The stair from below: from bridge B3 (y 330) looking up at the blank wall.
pub fn shaft_stair_below(t: f32) -> Camera {
    let eye = Vec3::new(13.0, 330.0 + EYE_HEIGHT, lerp(-8.0, 0.0, t));
    Camera::look_at(eye, Vec3::new(30.0, 425.0, -4.0))
}

/// From the floor of the shaft looking up 600 m at the sky, turning slowly.
pub fn shaft_look_up(t: f32) -> Camera {
    let eye = Vec3::new(4.0, EYE_HEIGHT + 4.0, -8.0);
    Camera::new(eye, lerp(0.3, 1.1, t), 1.35)
}

/// On the south ledge in front of the giant arch, looking up at it, then walking through it
/// 50 m into the hall.
pub fn shaft_hall_enter(t: f32) -> Camera {
    let y = shaft::hall_floor() + EYE_HEIGHT;
    let t = t.clamp(0.0, 1.0);
    let k = 0.3; // share of the path spent turning on the ledge
    let ledge_z = shaft::LEDGE_EDGE_Z - 0.7;
    if t < k {
        let u = t / k;
        Camera::new(Vec3::new(lerp(5.0, 0.0, u), y, ledge_z), PI + lerp(-0.55, 0.0, u), lerp(0.75, 0.25, u))
    } else {
        let u = (t - k) / (1.0 - k);
        Camera::new(Vec3::new(0.0, y, lerp(ledge_z, -80.0, u)), PI, lerp(0.25, 0.04, (u * 2.0).min(1.0)))
    }
}

/// The giant arch seen whole from bridge B1, across the shaft: the ledge, the stair head and
/// the hall's columns in the dark beyond.
pub fn shaft_arch_view(t: f32) -> Camera {
    let eye = Vec3::new(lerp(-6.0, 2.0, t), shaft::SPAWN_LEVEL + EYE_HEIGHT, shaft::SPAWN_Z);
    Camera::look_at(eye, Vec3::new(0.0, shaft::hall_floor() + 12.0, -HALF_SHAFT))
}

const HALF_SHAFT: f32 = shaft::HALF;

/// From just below the rim looking straight down the full 600 m.
pub fn shaft_rim_down(t: f32) -> Camera {
    let eye = Vec3::new(lerp(-22.0, -16.0, t), shaft::TOP - 6.0, lerp(22.0, 18.0, t));
    Camera::look_at(eye, Vec3::new(2.0, 0.0, -2.0))
}

/// Leaning over bridge B2's east railing at y 470, looking down past the lower bridges
/// (100 to 470 m below).
pub fn shaft_b2_down(t: f32) -> Camera {
    let eye = Vec3::new(-9.6, 470.0 + EYE_HEIGHT, lerp(-6.0, 4.0, t));
    Camera::new(eye, FRAC_PI_2 + 0.3, -1.15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledge_passage_length() {
        let line = ledge_passage_line();
        let len: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
        println!("ledge passage walk: {} points, {len:.1} m", line.len());
        println!("{line:?}");
        assert!(line.len() >= 2 && len > 3.0);
    }

    #[test]
    fn all_names_resolve_and_are_finite() {
        for name in NAMES {
            let f = by_name(name).unwrap();
            assert!(mega_gen::scenes::NAMES.contains(&scene_of(name).unwrap()));
            for t in [0.0, 0.5, 1.0, 2.0] {
                let c = f(t);
                assert!(c.position.is_finite() && c.yaw.is_finite() && c.pitch.is_finite());
            }
        }
        assert_eq!(scene_of("nope"), None);
    }
}
