//! The fly-camera simulation. No window, no GPU.

use crate::input::InputState;
use glam::Vec3;
use mega_render::Camera;

/// Base fly speed in metres per second.
pub const BASE_SPEED: f32 = 8.0;
/// Speed multiplier while the fast key is held.
pub const FAST_MULTIPLIER: f32 = 5.0;
pub const MIN_SPEED: f32 = 0.5;
pub const MAX_SPEED: f32 = 500.0;
/// Radians of turn per pixel of mouse movement.
pub const MOUSE_SENSITIVITY: f32 = 0.0025;
/// Arrow-key turn rate, degrees per second, independent of frame rate.
pub const ARROW_TURN_RATE_DEG: f32 = 90.0;
/// Pitch limit in radians (just under 90 degrees), shared by mouse and arrow keys.
pub const PITCH_LIMIT: f32 = 1.553;

#[derive(Clone, Copy, Debug)]
pub struct Sim {
    pub camera: Camera,
    /// Base speed in m/s, changed by the scroll wheel.
    pub speed: f32,
}

impl Sim {
    pub fn new(position: Vec3, yaw: f32) -> Self {
        Self { camera: Camera::new(position, yaw, 0.0), speed: BASE_SPEED }
    }

    pub fn scale_speed(&mut self, factor: f32) {
        self.speed = (self.speed * factor).clamp(MIN_SPEED, MAX_SPEED);
    }

    /// Advance the camera by `dt` seconds under `input`.
    pub fn update(&mut self, dt: f32, input: &InputState) {
        let cam = &mut self.camera;
        // Positive yaw turns towards +X, which is to the left when facing +Z, so moving the
        // mouse right must decrease yaw.
        cam.yaw -= input.mouse_delta.0 * MOUSE_SENSITIVITY;
        cam.pitch -= input.mouse_delta.1 * MOUSE_SENSITIVITY;

        let turn = ARROW_TURN_RATE_DEG.to_radians() * dt;
        if input.look_left {
            cam.yaw += turn;
        }
        if input.look_right {
            cam.yaw -= turn;
        }
        if input.look_up {
            cam.pitch += turn;
        }
        if input.look_down {
            cam.pitch -= turn;
        }
        cam.pitch = cam.pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);

        let (fwd, right) = (cam.forward(), cam.right());
        let mut m = Vec3::ZERO;
        if input.forward {
            m += fwd;
        }
        if input.back {
            m -= fwd;
        }
        if input.right {
            m += right;
        }
        if input.left {
            m -= right;
        }
        if input.up {
            m += Vec3::Y;
        }
        if input.down {
            m -= Vec3::Y;
        }
        let fast = if input.fast { FAST_MULTIPLIER } else { 1.0 };
        cam.position += m.normalize_or_zero() * self.speed * fast * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim() -> Sim {
        Sim::new(Vec3::new(1.0, 2.0, 3.0), 0.0)
    }

    fn run(mut s: Sim, input: &InputState, seconds: f32, steps: u32) -> Sim {
        for _ in 0..steps {
            s.update(seconds / steps as f32, input);
        }
        s
    }

    #[test]
    fn forward_two_seconds_moves_sixteen_metres_along_facing() {
        let input = InputState { forward: true, ..Default::default() };
        let s = run(sim(), &input, 2.0, 120);
        let moved = s.camera.position - Vec3::new(1.0, 2.0, 3.0);
        assert!((moved - Vec3::new(0.0, 0.0, 2.0 * BASE_SPEED)).length() < 1e-3, "{moved:?}");
    }

    #[test]
    fn forward_follows_yaw() {
        let mut s = Sim::new(Vec3::ZERO, std::f32::consts::FRAC_PI_2);
        s.update(1.0, &InputState { forward: true, ..Default::default() });
        assert!((s.camera.position - Vec3::new(BASE_SPEED, 0.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn fast_and_back_and_strafe() {
        let mut s = Sim::new(Vec3::ZERO, 0.0);
        s.update(1.0, &InputState { back: true, fast: true, ..Default::default() });
        assert!((s.camera.position.z + BASE_SPEED * FAST_MULTIPLIER).abs() < 1e-4);
        let mut s = Sim::new(Vec3::ZERO, 0.0);
        s.update(1.0, &InputState { right: true, ..Default::default() });
        assert!((s.camera.position.x + BASE_SPEED).abs() < 1e-4, "right is -X when facing +Z");
    }

    #[test]
    fn up_down_cancel_and_diagonal_is_not_faster() {
        let mut s = Sim::new(Vec3::ZERO, 0.0);
        s.update(1.0, &InputState { up: true, down: true, ..Default::default() });
        assert_eq!(s.camera.position, Vec3::ZERO);
        s.update(1.0, &InputState { forward: true, up: true, ..Default::default() });
        assert!((s.camera.position.length() - BASE_SPEED).abs() < 1e-4);
    }

    #[test]
    fn look_right_one_second_turns_by_the_rate() {
        let s = run(sim(), &InputState { look_right: true, ..Default::default() }, 1.0, 60);
        assert!((s.camera.yaw + ARROW_TURN_RATE_DEG.to_radians()).abs() < 1e-4);
        let s = run(sim(), &InputState { look_left: true, ..Default::default() }, 1.0, 60);
        assert!((s.camera.yaw - ARROW_TURN_RATE_DEG.to_radians()).abs() < 1e-4);
    }

    #[test]
    fn pitch_clamps_both_ways() {
        let s = run(sim(), &InputState { look_up: true, ..Default::default() }, 5.0, 300);
        assert_eq!(s.camera.pitch, PITCH_LIMIT);
        let s = run(sim(), &InputState { look_down: true, ..Default::default() }, 5.0, 300);
        assert_eq!(s.camera.pitch, -PITCH_LIMIT);
        let mut s = sim();
        s.update(0.0, &InputState { mouse_delta: (0.0, -1.0e6), ..Default::default() });
        assert_eq!(s.camera.pitch, PITCH_LIMIT);
    }

    #[test]
    fn one_big_step_equals_many_small_steps() {
        for input in [
            InputState { forward: true, fast: true, ..Default::default() },
            InputState { look_right: true, look_up: true, ..Default::default() },
            InputState { up: true, left: true, ..Default::default() },
        ] {
            let a = run(sim(), &input, 1.0, 1);
            let b = run(sim(), &input, 1.0, 60);
            assert!((a.camera.position - b.camera.position).length() < 1e-3, "{input:?}");
            assert!((a.camera.yaw - b.camera.yaw).abs() < 1e-4);
            assert!((a.camera.pitch - b.camera.pitch).abs() < 1e-4);
        }
    }

    #[test]
    fn mouse_right_turns_right() {
        let mut s = sim();
        let right_before = s.camera.right();
        s.update(0.0, &InputState { mouse_delta: (100.0, 40.0), ..Default::default() });
        assert!((s.camera.yaw + 100.0 * MOUSE_SENSITIVITY).abs() < 1e-6);
        assert!(s.camera.forward().dot(right_before) > 0.0);
        assert!((s.camera.pitch + 40.0 * MOUSE_SENSITIVITY).abs() < 1e-6);
    }
}
