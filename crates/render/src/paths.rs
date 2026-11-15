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
        _ => return None,
    })
}

/// The scene (a `mega_gen::scenes` name) a path is tuned to, or None for an unknown path.
pub fn scene_of(name: &str) -> Option<&'static str> {
    by_name(name)?;
    Some(if name.starts_with("shaft_") { "shaft" } else { "test" })
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
