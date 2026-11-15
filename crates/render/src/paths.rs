//! Named deterministic camera paths: functions of `t` in 0..1 (clamped).
//!
//! These are tuned to `mega_gen::test_scene`. Add new paths to [`NAMES`] and [`by_name`].

use crate::Camera;
use glam::Vec3;
use std::f32::consts::TAU;

pub const EYE_HEIGHT: f32 = 1.65;

pub const NAMES: &[&str] = &["corridor_walk", "corridor_strafe", "hall_orbit", "still_hall"];

pub fn by_name(name: &str) -> Option<fn(f32) -> Camera> {
    Some(match name {
        "corridor_walk" => corridor_walk,
        "corridor_strafe" => corridor_strafe,
        "hall_orbit" => hall_orbit,
        "still_hall" => still_hall,
        _ => return None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_resolve_and_are_finite() {
        for name in NAMES {
            let f = by_name(name).unwrap();
            for t in [0.0, 0.5, 1.0, 2.0] {
                let c = f(t);
                assert!(c.position.is_finite() && c.yaw.is_finite() && c.pitch.is_finite());
            }
        }
    }
}
