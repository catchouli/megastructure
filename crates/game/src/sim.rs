//! The player simulation: the fly camera (debug mode) and the walking controller.
//! No window, no GPU.

use crate::input::InputState;
use crate::player::{FIXED_DT, MAX_TICKS_PER_FRAME};
use crate::walker::{WalkInput, Walker};
use crate::world::World;
use glam::Vec3;
use mega_gen::scenes::Scene;
use mega_render::Camera;
use std::sync::Arc;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveMode {
    /// First-person walking with collision.
    Walk,
    /// The free debug camera.
    Fly,
}

impl MoveMode {
    pub fn name(self) -> &'static str {
        match self {
            MoveMode::Walk => "walk",
            MoveMode::Fly => "fly",
        }
    }
}

/// The collision world and the walker; present when the scene has colliders.
#[derive(Clone)]
struct Walking {
    world: Arc<World>,
    walker: Walker,
}

#[derive(Clone)]
pub struct Sim {
    pub camera: Camera,
    /// Base fly speed in m/s, changed by the scroll wheel.
    pub speed: f32,
    pub mode: MoveMode,
    walking: Option<Walking>,
    /// Real time not yet consumed by fixed walking steps, seconds.
    accumulator: f64,
    prev_toggle: bool,
    prev_reset: bool,
}

impl Sim {
    /// A fly camera at `position`, with nothing to collide with.
    pub fn new(position: Vec3, yaw: f32) -> Self {
        Self { camera: Camera::new(position, yaw, 0.0), speed: BASE_SPEED, mode: MoveMode::Fly, walking: None, accumulator: 0.0, prev_toggle: false, prev_reset: false }
    }

    /// The simulation for a scene: walking from its spawn if it has colliders (unless `mode`
    /// says fly), otherwise the fly camera.
    pub fn for_scene(scene: &Scene, mode: Option<MoveMode>) -> Self {
        let mut sim = Sim::new(Vec3::from(scene.spawn), scene.spawn_yaw);
        if !scene.colliders.is_empty() {
            sim.walking = Some(Walking { world: Arc::new(World::new(&scene.colliders)), walker: Walker::new(sim.camera.position) });
            sim.mode = mode.unwrap_or(MoveMode::Walk);
        }
        sim
    }

    pub fn scale_speed(&mut self, factor: f32) {
        self.speed = (self.speed * factor).clamp(MIN_SPEED, MAX_SPEED);
    }

    pub fn walker(&self) -> Option<&Walker> {
        self.walking.as_ref().map(|w| &w.walker)
    }

    /// Mutable access to the walker, for tests that stage a situation.
    #[cfg(test)]
    pub fn walker_mut(&mut self) -> Option<&mut Walker> {
        self.walking.as_mut().map(|w| &mut w.walker)
    }

    /// Whether this scene can be walked.
    pub fn can_walk(&self) -> bool {
        self.walking.is_some()
    }

    fn set_mode(&mut self, mode: MoveMode) {
        match (mode, self.walking.as_mut()) {
            (MoveMode::Walk, Some(w)) => {
                w.walker.place_eye(self.camera.position);
                self.accumulator = 0.0;
                self.mode = MoveMode::Walk;
            }
            (MoveMode::Fly, _) => self.mode = MoveMode::Fly,
            _ => {}
        }
    }

    /// Advance by `dt` seconds under `input`. Looking follows `dt`; walking runs in fixed steps.
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

        // F toggles on the rising edge, so a held key (a script segment) toggles once.
        let toggle = input.toggle_fly && !self.prev_toggle;
        self.prev_toggle = input.toggle_fly;
        let reset = input.reset && !self.prev_reset;
        self.prev_reset = input.reset;
        if toggle {
            self.set_mode(if self.mode == MoveMode::Walk { MoveMode::Fly } else { MoveMode::Walk });
        }

        match self.mode {
            MoveMode::Fly => self.fly(dt, input),
            MoveMode::Walk => self.walk(dt, input, reset),
        }
    }

    fn fly(&mut self, dt: f32, input: &InputState) {
        let cam = &mut self.camera;
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
        if input.up || input.jump {
            m += Vec3::Y;
        }
        if input.down {
            m -= Vec3::Y;
        }
        let fast = if input.fast { FAST_MULTIPLIER } else { 1.0 };
        cam.position += m.normalize_or_zero() * self.speed * fast * dt;
    }

    fn walk(&mut self, dt: f32, input: &InputState, reset: bool) {
        let Some(w) = self.walking.as_mut() else { return };
        if reset {
            w.walker.reset();
        }
        let axis = |plus: bool, minus: bool| plus as i32 as f32 - minus as i32 as f32;
        let walk = WalkInput {
            forward: axis(input.forward, input.back),
            right: axis(input.right, input.left),
            yaw: self.camera.yaw,
            run: input.fast,
            jump: input.jump,
        };
        let step = FIXED_DT as f64;
        self.accumulator += dt as f64;
        let mut ticks = 0;
        while self.accumulator >= step - 1e-9 {
            if ticks == MAX_TICKS_PER_FRAME {
                // Too far behind to catch up: drop the backlog instead of spiralling.
                self.accumulator = 0.0;
                break;
            }
            w.walker.tick(&w.world, &walk);
            self.accumulator -= step;
            ticks += 1;
        }
        self.camera.position = w.walker.eye();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::EYE_HEIGHT;

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

    // --- walking mode ---------------------------------------------------------------------

    use mega_gen::{Collider, MeshData};

    /// A floor with its top at y = 0 and a spawn on it, eye 1.65 m up, facing +Z.
    fn floor_scene() -> Scene {
        Scene {
            name: "floor",
            mesh: MeshData::default(),
            spawn: [0.0, 1.65, 0.0],
            spawn_yaw: 0.0,
            colliders: vec![Collider::Aabb { min: [-50.0, -2.0, -50.0], max: [50.0, 0.0, 50.0] }],
        }
    }

    fn held(f: impl FnOnce(&mut InputState)) -> InputState {
        let mut i = InputState::default();
        f(&mut i);
        i
    }

    fn frames(sim: &mut Sim, input: &InputState, n: usize) {
        for _ in 0..n {
            sim.update(1.0 / 60.0, input);
        }
    }

    #[test]
    fn scenes_with_colliders_start_walking_with_the_eye_at_the_spawn_and_others_fly() {
        let sim = Sim::for_scene(&floor_scene(), None);
        assert_eq!(sim.mode, MoveMode::Walk);
        assert!((sim.walker().unwrap().eye() - Vec3::new(0.0, 1.65, 0.0)).length() < 1e-5);
        assert_eq!(Sim::for_scene(&floor_scene(), Some(MoveMode::Fly)).mode, MoveMode::Fly);
        let mut empty = floor_scene();
        empty.colliders.clear();
        let mut sim = Sim::for_scene(&empty, Some(MoveMode::Walk));
        assert_eq!(sim.mode, MoveMode::Fly);
        assert!(!sim.can_walk());
        // F does nothing without colliders.
        sim.update(1.0 / 60.0, &held(|i| i.toggle_fly = true));
        assert_eq!(sim.mode, MoveMode::Fly);
    }

    #[test]
    fn f_toggles_once_per_press_and_keeps_the_position() {
        let mut sim = Sim::for_scene(&floor_scene(), None);
        frames(&mut sim, &held(|i| i.forward = true), 60);
        let at = sim.camera.position;
        assert!(at.z > 1.0 && (at.y - 1.65).abs() < 0.01, "{at:?}");
        // Held for five frames: one toggle.
        frames(&mut sim, &held(|i| i.toggle_fly = true), 5);
        assert_eq!(sim.mode, MoveMode::Fly);
        assert!((sim.camera.position - at).length() < 1e-5);
        frames(&mut sim, &InputState::default(), 3);
        frames(&mut sim, &held(|i| i.toggle_fly = true), 1);
        assert_eq!(sim.mode, MoveMode::Walk);
        assert!((sim.camera.position - at).length() < 1e-3, "eye {:?} vs {at:?}", sim.camera.position);
    }

    #[test]
    fn in_walk_mode_space_jumps_and_e_q_ctrl_do_nothing_and_in_fly_mode_they_fly() {
        let mut sim = Sim::for_scene(&floor_scene(), None);
        frames(&mut sim, &InputState::default(), 30);
        let rest = sim.camera.position;
        frames(&mut sim, &held(|i| {
            i.up = true;
            i.down = true;
        }), 30);
        assert_eq!(sim.camera.position, rest, "E, Q and Ctrl moved the walker");
        frames(&mut sim, &held(|i| i.down = true), 30);
        assert_eq!(sim.camera.position, rest);
        let mut apex = 0.0f32;
        frames(&mut sim, &held(|i| i.jump = true), 1);
        for _ in 0..60 {
            sim.update(1.0 / 60.0, &InputState::default());
            apex = apex.max(sim.camera.position.y - rest.y);
        }
        assert!((0.45..0.6).contains(&apex), "jump apex {apex}");
        // Fly mode: E and Space go up, Q and Ctrl down, as before.
        let mut fly = Sim::for_scene(&floor_scene(), Some(MoveMode::Fly));
        let y0 = fly.camera.position.y;
        frames(&mut fly, &held(|i| i.up = true), 60);
        assert!((fly.camera.position.y - y0 - BASE_SPEED).abs() < 1e-3);
        frames(&mut fly, &held(|i| i.jump = true), 60);
        assert!((fly.camera.position.y - y0 - 2.0 * BASE_SPEED).abs() < 1e-3);
        frames(&mut fly, &held(|i| i.down = true), 120);
        assert!((fly.camera.position.y - y0).abs() < 1e-3);
    }

    #[test]
    fn r_returns_to_the_stable_spot_once_per_press() {
        let mut sim = Sim::for_scene(&floor_scene(), None);
        frames(&mut sim, &InputState::default(), 60);
        let home = sim.walker().unwrap().pos;
        // Throw the player into the air far from home; R, held for ten frames, resets once.
        sim.walker_mut().unwrap().place_eye(Vec3::new(20.0, 30.0, 20.0));
        frames(&mut sim, &InputState::default(), 10);
        assert!(sim.walker().unwrap().pos.y < 30.0 && !sim.walker().unwrap().grounded);
        frames(&mut sim, &held(|i| i.reset = true), 10);
        let w = sim.walker().unwrap();
        assert_eq!(w.resets, 1);
        assert!(w.grounded && (w.pos - home).length() < 1e-4, "{:?} vs {home:?}", w.pos);
        assert!((sim.camera.position.y - (home.y + EYE_HEIGHT)).abs() < 1e-3);
        // Pressed again after a release: a second reset.
        frames(&mut sim, &InputState::default(), 2);
        frames(&mut sim, &held(|i| i.reset = true), 1);
        assert_eq!(sim.walker().unwrap().resets, 2);
        // Run is faster than walk.
        let mut a = Sim::for_scene(&floor_scene(), None);
        let mut b = a.clone();
        frames(&mut a, &held(|i| i.forward = true), 60);
        frames(&mut b, &held(|i| {
            i.forward = true;
            i.fast = true;
        }), 60);
        assert!((a.camera.position.z - 2.5 * (1.0 - 0.04)).abs() < 0.2 && (b.camera.position.z - 6.0 * (1.0 - 0.1)).abs() < 0.4, "{} {}", a.camera.position.z, b.camera.position.z);
    }

    #[test]
    fn walking_uses_fixed_steps_whatever_the_frame_rate() {
        let input = held(|i| i.forward = true);
        let mut a = Sim::for_scene(&floor_scene(), None);
        let mut b = a.clone();
        let mut c = a.clone();
        for _ in 0..120 {
            a.update(1.0 / 60.0, &input);
        }
        for _ in 0..60 {
            b.update(1.0 / 30.0, &input);
        }
        for _ in 0..240 {
            c.update(1.0 / 120.0, &input);
        }
        assert_eq!(a.camera.position, b.camera.position);
        assert!((a.camera.position - c.camera.position).length() < 0.05, "{:?} {:?}", a.camera.position, c.camera.position);
        // A stall does not run away: at most MAX_TICKS_PER_FRAME steps, the rest dropped.
        let mut d = Sim::for_scene(&floor_scene(), None);
        d.update(10.0, &input);
        assert!(d.camera.position.z < 8.0 * (1.0 / 60.0) * 2.5 + 0.01);
    }

    #[test]
    fn looking_works_the_same_in_both_modes() {
        let mut sim = Sim::for_scene(&floor_scene(), None);
        frames(&mut sim, &held(|i| i.look_left = true), 60);
        assert!((sim.camera.yaw - ARROW_TURN_RATE_DEG.to_radians()).abs() < 1e-4);
        sim.update(0.0, &InputState { mouse_delta: (100.0, 40.0), ..Default::default() });
        assert!((sim.camera.pitch + 40.0 * MOUSE_SENSITIVITY).abs() < 1e-6);
    }
}
