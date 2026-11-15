//! The first-person walking controller: a kinematic capsule moved by collide-and-slide against
//! the scene's colliders, with step-up, stick-to-ground, jumping, drag-limited falling, fatal
//! impacts and a return to the last stable ground. No window, no GPU; advanced only in fixed
//! steps of `FIXED_DT`, so the same inputs give the same result.
//!
//! The position is the capsule's feet. See docs/features/player-movement.md.

use crate::player::*;
use crate::world::World;
use glam::Vec3;

/// What the player asks for during one fixed step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WalkInput {
    /// Forward (+1) or back (-1).
    pub forward: f32,
    /// Right (+1) or left (-1).
    pub right: f32,
    /// Facing, radians (0 looks along +Z, positive turns towards +X).
    pub yaw: f32,
    pub run: bool,
    pub jump: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Walker {
    /// Feet position.
    pub pos: Vec3,
    pub vel: Vec3,
    pub grounded: bool,
    /// Normal of the surface stood on (meaningful while grounded).
    pub ground_normal: Vec3,
    /// Seconds spent on the ground without a break.
    pub ground_time: f32,
    /// Last feet position that had been stood on for at least `STABLE_GROUND_TIME`.
    pub stable_pos: Vec3,
    /// Deaths so far (fatal impacts).
    pub deaths: u32,
    /// Manual resets so far.
    pub resets: u32,
    /// Eye height correction (metres, usually zero): after a step up or down the eye lags and
    /// eases back, so a ledge does not pop the picture.
    pub eye_offset: f32,
    /// Speed along the normal of the hardest contact of the latest step (diagnostic).
    pub last_impact: f32,
    /// Seconds left in which movement input is ignored (after a death or a reset).
    pub lockout: f32,
    /// Times the position was set directly (a death, a reset, leaving the fly camera): a renderer
    /// that interpolates must not slide across these.
    pub teleports: u32,
    /// Jump key state of the previous step, for the rising edge.
    jump_prev: bool,
    /// Seconds a jump press stays valid while waiting for the ground.
    jump_buffer: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Standing or walking: walkable surfaces are climbed, anything steeper is a vertical wall.
    Ground,
    /// Every contact is a vertical wall (the raised capsule of a step).
    Wall,
    /// Falling or jumping: contacts are what they are.
    Air,
}

struct Slide {
    pos: Vec3,
    /// Normals of the surfaces met, in order.
    normals: Vec<Vec3>,
    /// A surface too steep to stand on was met.
    wall: bool,
}

fn horizontal(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    horizontal(b - a).length()
}

/// Move the capsule from `start` by `motion`, sliding along what it meets.
fn slide(world: &World, start: Vec3, motion: Vec3, mode: Mode) -> Slide {
    let min_y = min_ground_y();
    let mut out = Slide { pos: start, normals: Vec::new(), wall: false };
    let mut remaining = motion;
    let mut planes: Vec<Vec3> = Vec::new();
    for _ in 0..MAX_SLIDE_ITERATIONS {
        if remaining.length_squared() < 1e-12 {
            break;
        }
        let Some(hit) = world.cast(out.pos, remaining) else {
            out.pos += remaining;
            break;
        };
        let travel = remaining * hit.fraction;
        out.pos += travel;
        remaining -= travel;
        out.normals.push(hit.normal);
        if hit.normal.y < min_y {
            out.wall = true;
        }
        // The normal to slide along. On foot, a surface too steep to stand on is treated as a
        // vertical wall, so it cannot be climbed by sliding up it.
        let flat = horizontal(hit.normal).try_normalize();
        let n = match (mode, flat) {
            (Mode::Wall, Some(f)) => f,
            (Mode::Ground, Some(f)) if hit.normal.y < min_y => f,
            _ => hit.normal,
        };
        let mut r = remaining;
        let d = r.dot(n);
        if d < 0.0 {
            r -= n * d;
        }
        for p in &planes {
            if r.dot(*p) < -1e-5 {
                // Wedged between two planes: slide along their crease, or stop.
                let c = n.cross(*p);
                r = if c.length_squared() > 1e-8 {
                    let c = c.normalize();
                    c * remaining.dot(c)
                } else {
                    Vec3::ZERO
                };
            }
        }
        planes.push(n);
        remaining = r;
    }
    out
}

/// Remove from `v` its components into each surface in `normals`.
fn clip_velocity(mut v: Vec3, normals: &[Vec3]) -> Vec3 {
    for _ in 0..2 {
        for n in normals {
            let d = v.dot(*n);
            if d < 0.0 {
                v -= *n * d;
            }
        }
    }
    v
}

/// Move `v` towards `target` by at most `max_delta`.
fn approach(v: Vec3, target: Vec3, max_delta: f32) -> Vec3 {
    let d = target - v;
    let len = d.length();
    if len <= max_delta {
        target
    } else {
        v + d * (max_delta / len)
    }
}

/// Acceleration of a falling body: gravity and quadratic drag.
fn fall_acceleration(v: Vec3) -> Vec3 {
    Vec3::new(0.0, -GRAVITY, 0.0) - v * (v.length() * DRAG_K)
}

impl Walker {
    /// A walker with its eye at `eye`, not yet on the ground. The spot counts as stable.
    pub fn new(eye: Vec3) -> Walker {
        let pos = eye - Vec3::Y * EYE_HEIGHT;
        Walker {
            pos,
            vel: Vec3::ZERO,
            grounded: false,
            ground_normal: Vec3::Y,
            ground_time: STABLE_GROUND_TIME,
            stable_pos: pos,
            deaths: 0,
            resets: 0,
            eye_offset: 0.0,
            last_impact: 0.0,
            lockout: 0.0,
            teleports: 0,
            jump_prev: false,
            jump_buffer: 0.0,
        }
    }

    /// Eye position.
    pub fn eye(&self) -> Vec3 {
        self.pos + Vec3::Y * (EYE_HEIGHT + self.eye_offset)
    }

    /// Put the player's eye at `eye` at rest (leaving fly mode). Death and reset counts and
    /// the stable spot are kept.
    pub fn place_eye(&mut self, eye: Vec3) {
        self.pos = eye - Vec3::Y * EYE_HEIGHT;
        self.vel = Vec3::ZERO;
        self.grounded = false;
        self.ground_time = 0.0;
        self.eye_offset = 0.0;
        self.teleports += 1;
    }

    /// Manual reset: back to the last stable ground, at rest.
    pub fn reset(&mut self) {
        self.resets += 1;
        self.return_to_stable();
    }

    fn return_to_stable(&mut self) {
        self.pos = self.stable_pos;
        self.vel = Vec3::ZERO;
        self.grounded = true;
        self.ground_normal = Vec3::Y;
        self.ground_time = STABLE_GROUND_TIME;
        self.eye_offset = 0.0;
        self.lockout = RESPAWN_LOCKOUT;
        self.jump_buffer = 0.0;
        self.teleports += 1;
    }

    /// Advance by one fixed step.
    pub fn tick(&mut self, world: &World, input: &WalkInput) {
        let dt = FIXED_DT;
        // Just after a respawn the keys do nothing, so a held key does not carry the player
        // straight back over the edge.
        let raw_jump = input.jump;
        let locked = self.lockout > 0.0;
        self.lockout = (self.lockout - dt).max(0.0);
        let input = &if locked { WalkInput { yaw: input.yaw, ..Default::default() } } else { *input };
        // A jump is a key press, not a held key; a press shortly before landing is kept.
        if input.jump && !self.jump_prev {
            self.jump_buffer = JUMP_BUFFER;
        }
        self.jump_prev = raw_jump;
        self.pos = world.depenetrate(self.pos);
        self.last_impact = 0.0;

        // Wanted velocity from the keys and the facing.
        let (sy, cy) = input.yaw.sin_cos();
        let fwd = Vec3::new(sy, 0.0, cy);
        let right = Vec3::new(-cy, 0.0, sy);
        let mut dir = fwd * input.forward + right * input.right;
        if dir.length_squared() > 1.0 {
            dir = dir.normalize();
        }
        let wish = dir * if input.run { RUN_SPEED } else { WALK_SPEED };

        let mut vh = horizontal(self.vel);
        if self.grounded {
            let rate = if wish == Vec3::ZERO { GROUND_DECEL } else { GROUND_ACCEL };
            vh = approach(vh, wish, rate * dt);
            if self.jump_buffer > 0.0 {
                self.vel.y = JUMP_SPEED;
                self.grounded = false;
                self.jump_buffer = 0.0;
            }
        } else if let Some(wd) = wish.try_normalize() {
            // Air control: push along the wanted direction, but never beyond the wanted speed.
            let add = (wish.length() - vh.dot(wd)).clamp(0.0, AIR_ACCEL * dt);
            vh += wd * add;
        }
        self.vel.x = vh.x;
        self.vel.z = vh.z;

        let died = if self.grounded {
            self.ground_move(world, dt);
            false
        } else {
            self.air_move(world, dt)
        };
        if died {
            self.deaths += 1;
            self.return_to_stable();
            return;
        }

        self.jump_buffer = (self.jump_buffer - dt).max(0.0);
        if self.grounded {
            self.ground_time += dt;
            if self.ground_time >= STABLE_GROUND_TIME && world.supported_ring(self.pos, STABLE_RING_RADIUS) {
                self.stable_pos = self.pos;
            }
        } else {
            self.ground_time = 0.0;
        }
        self.eye_offset *= (-EYE_SMOOTH_RATE * dt).exp();
        if self.eye_offset.abs() < 1e-4 {
            self.eye_offset = 0.0;
        }
    }

    /// Walking: slide along the ground, step up ledges, stay stuck to the ground below.
    fn ground_move(&mut self, world: &World, dt: f32) {
        let motion = horizontal(self.vel) * dt;
        let start = self.pos;
        let a = slide(world, start, motion, Mode::Ground);
        let mut best = a.pos;
        let mut stepped = false;
        if a.wall {
            // Blocked by something steep: see whether it is a ledge that can be stepped onto.
            let progress = horizontal_distance(start, a.pos);
            if let Some(b) = self.try_step(world, start, motion) {
                if horizontal_distance(start, b) > progress + 1e-3 {
                    self.eye_offset -= b.y - start.y;
                    best = b;
                    stepped = true;
                }
            }
        }
        // Stay on the ground: follow slopes and steps down.
        let down = Vec3::NEG_Y * SNAP_DISTANCE;
        match world.cast(best, down) {
            Some(h) if h.normal.y >= min_ground_y() => {
                let p = best + down * h.fraction;
                let drop = best.y - p.y;
                if !stepped && drop > EYE_SMOOTH_MIN_DROP {
                    self.eye_offset += drop;
                }
                self.pos = p;
                self.ground_normal = h.normal;
            }
            _ => {
                self.pos = best;
                self.grounded = false;
                self.vel.y = 0.0;
            }
        }
        self.eye_offset = self.eye_offset.clamp(-STEP_HEIGHT * 1.5, STEP_HEIGHT * 1.5);
    }

    /// Raise by up to the step height, move forward, come down; Some(landing) if that ends on
    /// ground that can be stood on. The forward probe is lengthened (once) when the plain move
    /// would end with the capsule perched on the ledge's corner.
    fn try_step(&self, world: &World, start: Vec3, motion: Vec3) -> Option<Vec3> {
        let want = motion.length();
        let dir = motion.try_normalize()?;
        let up = Vec3::Y * STEP_HEIGHT;
        let rise = world.cast(start, up).map_or(STEP_HEIGHT, |h| STEP_HEIGHT * h.fraction);
        if rise < 0.02 {
            return None;
        }
        let raised = start + Vec3::Y * rise;
        let mut lengths = vec![want];
        if want < STEP_MIN_FORWARD {
            lengths.push(STEP_MIN_FORWARD);
        }
        for len in lengths {
            let s = slide(world, raised, dir * len, Mode::Wall);
            if horizontal_distance(raised, s.pos) < len - 1e-3 {
                // Something at this height is in the way too: a wall, not a step.
                continue;
            }
            let down = Vec3::NEG_Y * (rise + SNAP_DISTANCE);
            match world.cast(s.pos, down) {
                // The ledge's own height counts, not the capsule's: resting on the corner of a
                // ledge the sphere sits lower than the top it is touching.
                Some(h) if h.normal.y >= min_ground_y() && h.point.y - start.y <= STEP_HEIGHT + 0.005 => return Some(s.pos + down * h.fraction),
                _ => continue,
            }
        }
        None
    }

    /// Falling or jumping: integrate gravity and drag, collide, land. True if the impact killed.
    fn air_move(&mut self, world: &World, dt: f32) -> bool {
        let v0 = self.vel;
        let mid = v0 + fall_acceleration(v0) * (dt * 0.5);
        let mut v1 = v0 + fall_acceleration(mid) * dt;
        let speed = v1.length();
        if speed > TERMINAL_VELOCITY {
            v1 *= TERMINAL_VELOCITY / speed;
        }
        let v_avg = (v0 + v1) * 0.5;
        let s = slide(world, self.pos, v_avg * dt, Mode::Air);
        self.pos = s.pos;

        let impact = s.normals.iter().map(|n| -v_avg.dot(*n)).fold(0.0f32, f32::max);
        self.last_impact = impact;
        if impact > FATAL_IMPACT_SPEED {
            return true;
        }

        let floor = s.normals.iter().find(|n| n.y >= min_ground_y());
        if let (Some(n), true) = (floor, v1.y <= 0.0) {
            // Landed: keep the horizontal speed, lose the vertical.
            self.grounded = true;
            self.ground_normal = *n;
            self.vel = Vec3::new(v1.x, 0.0, v1.z);
        } else {
            self.vel = clip_velocity(v1, &s.normals);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mega_gen::Collider;

    pub fn aabb(min: [f32; 3], max: [f32; 3]) -> Collider {
        Collider::Aabb { min, max }
    }

    /// A big floor with its top at y = 0.
    fn floor() -> Collider {
        aabb([-100.0, -2.0, -100.0], [100.0, 0.0, 100.0])
    }

    fn walker_at(feet: Vec3) -> Walker {
        Walker::new(feet + Vec3::Y * EYE_HEIGHT)
    }

    /// Facing +X.
    const EAST: f32 = std::f32::consts::FRAC_PI_2;

    fn forward(yaw: f32) -> WalkInput {
        WalkInput { forward: 1.0, yaw, ..Default::default() }
    }

    fn run(w: &mut Walker, world: &World, input: WalkInput, seconds: f32) {
        for _ in 0..(seconds / FIXED_DT).round() as usize {
            w.tick(world, &input);
        }
    }

    fn settle(w: &mut Walker, world: &World) {
        run(w, world, WalkInput::default(), 1.0);
    }

    #[test]
    fn standing_on_a_box_for_ten_seconds_does_not_move() {
        let world = World::new(&[floor()]);
        let mut w = walker_at(Vec3::new(1.0, 0.0, 2.0));
        settle(&mut w, &world);
        assert!(w.grounded);
        let rest = w.pos;
        run(&mut w, &world, WalkInput::default(), 10.0);
        assert_eq!(w.pos, rest, "drifted");
        assert!(w.grounded && w.vel == Vec3::ZERO);
        assert!(rest.y.abs() < 0.01, "standing at {}", rest.y);
        // Dropped from slightly above, it settles a skin above the floor and stays.
        let mut w = walker_at(Vec3::new(1.0, 0.1, 2.0));
        settle(&mut w, &world);
        let rest = w.pos;
        run(&mut w, &world, WalkInput::default(), 10.0);
        assert_eq!(w.pos, rest);
        assert!(w.grounded && (rest.y - SKIN).abs() < 1e-3, "{rest:?}");
    }

    #[test]
    fn a_wall_stops_a_walker_and_is_slid_along_when_approached_at_an_angle() {
        let wall = aabb([3.0, 0.0, -50.0], [4.0, 5.0, 50.0]);
        let world = World::new(&[floor(), wall]);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST), 3.0);
        assert!((w.pos.x - (3.0 - CAPSULE_RADIUS)).abs() < 0.02, "stopped at x {}", w.pos.x);
        assert!(w.pos.z.abs() < 1e-3 && w.grounded);

        // 45 degrees into the wall: x stays at the wall, z carries on.
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST / 2.0), 3.0);
        assert!(w.pos.x < 3.0 - CAPSULE_RADIUS + 0.01 && w.pos.x > 3.0 - CAPSULE_RADIUS - 0.02, "x {}", w.pos.x);
        assert!(w.pos.z > 3.0, "slid only to z {}", w.pos.z);
        assert!(w.grounded);
    }

    fn ledge_test(height: f32) -> Walker {
        let ledge = aabb([2.0, -1.0, -50.0], [50.0, height, 50.0]);
        let world = World::new(&[floor(), ledge]);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST), 3.0);
        w
    }

    #[test]
    fn a_quarter_metre_ledge_is_stepped_up_and_a_35_cm_ledge_is_not() {
        let w = ledge_test(0.25);
        assert!(w.pos.x > 3.5 && (w.pos.y - 0.25).abs() < 0.02 && w.grounded, "{:?}", w.pos);
        let w = ledge_test(0.35);
        assert!(w.pos.x < 2.0 - CAPSULE_RADIUS + 0.02 && w.pos.y < 0.02 && w.grounded, "{:?}", w.pos);
        // Also at a run, and a 0.3 m ledge.
        let ledge = aabb([2.0, -1.0, -50.0], [50.0, 0.29, 50.0]);
        let world = World::new(&[floor(), ledge]);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, WalkInput { run: true, ..forward(EAST) }, 2.0);
        assert!(w.pos.x > 5.0 && (w.pos.y - 0.29).abs() < 0.02, "{:?}", w.pos);
    }

    /// A floor, a ramp rising at `deg` degrees from x = 2 over `len` metres along the slope,
    /// and a landing at the top.
    fn ramp_world(deg: f32, len: f32) -> (World, Vec3) {
        let (s, c) = deg.to_radians().sin_cos();
        let start = Vec3::new(2.0, 0.0, 0.0);
        let end = start + Vec3::new(len * c, len * s, 0.0);
        let ramp = Collider::Ramp { start: start.to_array(), end: end.to_array(), half_width: 2.0, thickness: 0.4 };
        let landing = aabb([end.x, end.y - 2.0, -50.0], [end.x + 50.0, end.y, 50.0]);
        (World::new(&[floor(), ramp, landing]), end)
    }

    #[test]
    fn a_35_degree_ramp_can_be_climbed_and_a_45_degree_ramp_cannot() {
        let (world, end) = ramp_world(35.0, 6.0);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST), 6.0);
        assert!(w.grounded && (w.pos.y - end.y).abs() < 0.05 && w.pos.x > end.x + 1.0, "{:?} vs {end:?}", w.pos);

        let (world, end) = ramp_world(45.0, 6.0);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST), 6.0);
        assert!(w.grounded && w.pos.y < 0.05 && w.pos.x < 2.0, "climbed a 45 degree ramp: {:?} (top {end:?})", w.pos);
        run(&mut w, &world, WalkInput { run: true, ..forward(EAST) }, 4.0);
        assert!(w.pos.y < 0.05 && w.pos.x < 2.0, "ran up a 45 degree ramp: {:?}", w.pos);
        // A 39 degree ramp is fine, 41 is not.
        let (world, end) = ramp_world(39.0, 6.0);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST), 8.0);
        assert!(w.pos.y > end.y - 0.05, "39 degrees: {:?}", w.pos);
        let (world, _) = ramp_world(41.0, 6.0);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST), 8.0);
        assert!(w.pos.y < 0.05, "41 degrees: {:?}", w.pos);
    }

    #[test]
    fn a_stair_ramp_is_climbed_and_descended_smoothly_without_leaving_the_ground() {
        // 40 treads, rise 0.17, run 0.30, as the generator emits for a stair flight.
        let (rise, run_len, n) = (0.17f32, 0.30f32, 40);
        let start = Vec3::new(2.0, 0.0, 0.0);
        let end = start + Vec3::new(n as f32 * run_len, n as f32 * rise, 0.0);
        let cos = run_len / (rise * rise + run_len * run_len).sqrt();
        let ramp = Collider::Ramp { start: start.to_array(), end: end.to_array(), half_width: 0.75, thickness: 0.35 * cos };
        let landing = aabb([end.x, end.y - 1.0, -0.75], [end.x + 10.0, end.y, 0.75]);
        let foot = aabb([-10.0, -1.0, -0.75], [2.0, 0.0, 0.75]);
        let world = World::new(&[foot, ramp, landing]);
        for (speed_run, name) in [(false, "walk"), (true, "run")] {
            let dt_speed = if speed_run { RUN_SPEED } else { WALK_SPEED };
            let mut w = walker_at(Vec3::ZERO);
            settle(&mut w, &world);
            let input = WalkInput { run: speed_run, ..forward(EAST) };
            let mut eye = vec![w.eye().y];
            let mut airborne = 0;
            for _ in 0..(40 * 60) {
                if w.pos.x > end.x + 0.5 {
                    break;
                }
                w.tick(&world, &input);
                eye.push(w.eye().y);
                airborne += !w.grounded as usize;
            }
            settle(&mut w, &world);
            assert!(w.pos.x > end.x + 0.5 && (w.pos.y - end.y).abs() < 0.02, "{name}: ended at {:?}, top {end:?}", w.pos);
            assert_eq!(airborne, 0, "{name}: left the ground {airborne} times going up");
            check_smooth(&eye, 1, dt_speed, &format!("{name} up"));

            // And back down.
            let back = WalkInput { run: speed_run, ..forward(-EAST) };
            let mut eye = vec![w.eye().y];
            let mut airborne = 0;
            for _ in 0..(40 * 60) {
                if w.pos.x < 1.0 {
                    break;
                }
                w.tick(&world, &back);
                eye.push(w.eye().y);
                airborne += !w.grounded as usize;
            }
            assert!(w.pos.x < 2.0 && w.pos.y.abs() < 0.02, "{name}: ended at {:?}", w.pos);
            assert_eq!(airborne, 0, "{name}: skipped off the stairs {airborne} times going down");
            check_smooth(&eye, -1, dt_speed, &format!("{name} down"));
        }
    }

    /// The eye moves one way only (`dir`), by at most the slope's worth per step, and never by a jump.
    fn check_smooth(eye: &[f32], dir: i32, speed: f32, what: &str) {
        let slope = 0.17 / 0.30;
        let max_step = speed * slope * FIXED_DT * 1.25 + 0.002;
        for (i, p) in eye.windows(2).enumerate() {
            let d = (p[1] - p[0]) * dir as f32;
            assert!(d > -0.001, "{what}: eye went the wrong way by {d} at step {i}");
            assert!(d < max_step, "{what}: eye jumped {d} (max {max_step}) at step {i}");
        }
        // Mid-flight the eye moves every step: no stalls longer than a few steps except at the ends.
        let moving: Vec<usize> = eye.windows(2).enumerate().filter(|(_, p)| ((p[1] - p[0]) * dir as f32).abs() > 1e-4).map(|(i, _)| i).collect();
        for p in moving.windows(2) {
            assert!(p[1] - p[0] <= 3, "{what}: eye stalled between steps {} and {}", p[0], p[1]);
        }
    }

    #[test]
    fn a_jump_peaks_about_half_a_metre_high_and_lands_without_harm() {
        let world = World::new(&[floor()]);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        let base = w.pos.y;
        w.tick(&world, &WalkInput { jump: true, ..Default::default() });
        assert!(!w.grounded);
        let mut apex = 0.0f32;
        let mut air = 1;
        while !w.grounded && air < 200 {
            w.tick(&world, &WalkInput::default());
            apex = apex.max(w.pos.y - base);
            air += 1;
        }
        assert!((0.47..0.56).contains(&apex), "apex {apex}");
        assert!((35..45).contains(&air), "{air} steps in the air");
        assert!(w.grounded && w.deaths == 0);
        // Not while airborne: holding jump does not add speed in the air.
        let mut w = walker_at(Vec3::new(0.0, 5.0, 0.0));
        for _ in 0..10 {
            w.tick(&world, &WalkInput { jump: true, ..Default::default() });
        }
        assert!(w.vel.y < 0.0);
    }

    #[test]
    fn free_fall_approaches_terminal_velocity_and_never_exceeds_it() {
        let world = World::new(&[]);
        let mut w = walker_at(Vec3::new(0.0, 1.0e5, 0.0));
        let mut v8 = 0.0;
        let mut top = 0.0f32;
        for i in 0..(60 * 60) {
            w.tick(&world, &WalkInput::default());
            top = top.max(-w.vel.y);
            if i + 1 == 8 * 60 {
                v8 = -w.vel.y;
            }
        }
        assert!(top <= TERMINAL_VELOCITY, "exceeded terminal: {top}");
        assert!(top > TERMINAL_VELOCITY - 0.01, "never got there: {top}");
        assert!((v8 / TERMINAL_VELOCITY - 0.89).abs() < 0.015, "after 8 s: {v8} = {:.3} of terminal", v8 / TERMINAL_VELOCITY);
        assert!(!w.grounded && w.deaths == 0);
        // Analytic: v = vt tanh(g t / vt).
        let exact = TERMINAL_VELOCITY * (GRAVITY * 8.0 / TERMINAL_VELOCITY).tanh();
        assert!((v8 - exact).abs() < 0.1, "{v8} vs {exact}");
    }

    /// A platform at `top` (x -6..3) over a floor at y = 0.
    fn platform_world(top: f32) -> World {
        World::new(&[floor(), aabb([-6.0, top - 1.0, -4.0], [3.0, top, 4.0])])
    }

    #[test]
    fn a_five_metre_drop_is_survived_and_a_fifteen_metre_drop_kills() {
        for (top, dies) in [(5.0f32, false), (8.0, false), (15.0, true), (40.0, true)] {
            let world = platform_world(top);
            let mut w = walker_at(Vec3::new(-2.0, top, 0.0));
            run(&mut w, &world, WalkInput::default(), 1.0);
            // Walk 5 m to the edge at x = 3 and a little over (2 s), then let go and wait.
            run(&mut w, &world, forward(EAST), 2.2);
            run(&mut w, &world, WalkInput::default(), 8.0);
            assert_eq!(w.deaths, if dies { 1 } else { 0 }, "drop {top}: deaths {}, pos {:?}", w.deaths, w.pos);
            assert!(w.grounded);
            if dies {
                assert!(w.pos.y > top - 0.01, "drop {top}: respawned at {:?}", w.pos);
            } else {
                assert!(w.pos.y < 0.01, "drop {top}: ended at {:?}", w.pos);
            }
        }
    }

    #[test]
    fn death_returns_to_the_last_stable_ground_and_so_does_the_reset_key() {
        let world = platform_world(20.0);
        let mut w = walker_at(Vec3::new(-5.0, 20.0, 0.0));
        run(&mut w, &world, WalkInput::default(), 1.0);
        // Walk 2 m (0.8 s), stand 1 s: that spot is stable.
        run(&mut w, &world, forward(EAST), 0.8);
        run(&mut w, &world, WalkInput::default(), 1.0);
        let spot = w.pos;
        assert_eq!(w.stable_pos, spot);
        // Walk off the edge at x = 3 (it takes 2.4 s) and let go.
        run(&mut w, &world, forward(EAST), 2.8);
        assert!(!w.grounded, "still on the platform at {:?}", w.pos);
        let leaving = w.stable_pos;
        assert!(leaving.x > spot.x + 1.0 && leaving.x < 3.0 && (leaving.y - 20.0).abs() < 0.01, "stable spot {leaving:?}");
        run(&mut w, &world, WalkInput::default(), 6.0);
        // Dead after the 20 m fall, and back on the platform at the last spot stood on while
        // walking, a step short of the edge.
        assert_eq!(w.deaths, 1);
        assert!(w.grounded && w.vel == Vec3::ZERO);
        assert!((w.pos - leaving).length() < 0.01, "{:?} vs {leaving:?}", w.pos);
        // Walk back 1 s and stand; R puts the player back at the stable spot.
        run(&mut w, &world, forward(-EAST), 1.0);
        run(&mut w, &world, WalkInput::default(), 1.0);
        assert!(w.pos.x < leaving.x - 1.0);
        let here = w.stable_pos;
        w.reset();
        assert_eq!((w.resets, w.deaths), (1, 1));
        assert!(w.grounded && w.vel == Vec3::ZERO && w.pos == here);
        assert!(here.x < leaving.x - 1.0, "stable spot follows the player while standing");
    }

    #[test]
    fn standing_less_than_half_a_second_does_not_make_a_stable_spot() {
        let world = platform_world(20.0);
        let mut w = walker_at(Vec3::new(-5.0, 20.0, 0.0));
        run(&mut w, &world, WalkInput::default(), 1.0);
        let origin = w.stable_pos;
        // Jump at once and keep hopping: never on the ground for half a second.
        for _ in 0..4 {
            w.tick(&world, &WalkInput { jump: true, forward: 1.0, yaw: EAST, ..Default::default() });
            for _ in 0..30 {
                w.tick(&world, &WalkInput { forward: 1.0, yaw: EAST, ..Default::default() });
            }
        }
        assert!(w.ground_time < STABLE_GROUND_TIME);
        // The spot is whatever was stable before the hops began: still on the platform's -X side.
        assert!(w.stable_pos.x >= origin.x);
        assert!(w.stable_pos.x < w.pos.x + 1e-3 || w.stable_pos == origin);
    }

    #[test]
    fn stepping_off_into_the_void_keeps_falling_and_does_not_reset() {
        let world = World::new(&[aabb([-6.0, -1.0, -4.0], [3.0, 0.0, 4.0])]);
        let mut w = walker_at(Vec3::new(-2.0, 0.0, 0.0));
        run(&mut w, &world, WalkInput::default(), 1.0);
        run(&mut w, &world, forward(EAST), 3.0);
        let mut lowest = w.pos.y;
        for _ in 0..(15 * 60) {
            w.tick(&world, &WalkInput::default());
            assert!(w.pos.y <= lowest + 1e-4, "went up");
            lowest = w.pos.y;
        }
        assert!(!w.grounded && w.deaths == 0 && w.resets == 0);
        assert!(w.pos.y < -400.0, "only fell to {}", w.pos.y);
        assert!(w.vel.y < -50.0);
    }

    #[test]
    fn the_same_script_gives_bit_identical_results() {
        let ledge = aabb([8.0, -1.0, -50.0], [50.0, 0.25, 50.0]);
        let (ramp_world, _) = ramp_world(30.0, 6.0);
        let _ = ramp_world;
        let world = World::new(&[floor(), ledge, aabb([-20.0, 9.0, -50.0], [-10.0, 10.0, 50.0])]);
        let script = |w: &mut Walker| {
            run(w, &world, WalkInput::default(), 0.5);
            run(w, &world, WalkInput { run: true, ..forward(EAST) }, 1.0);
            w.tick(&world, &WalkInput { jump: true, ..forward(EAST) });
            run(w, &world, forward(EAST / 2.0), 2.0);
            run(w, &world, WalkInput { right: 1.0, yaw: 1.0, ..Default::default() }, 1.0);
            run(w, &world, forward(-EAST), 3.0);
        };
        let (mut a, mut b) = (walker_at(Vec3::new(0.0, 0.0, 0.0)), walker_at(Vec3::new(0.0, 0.0, 0.0)));
        script(&mut a);
        script(&mut b);
        assert_eq!(a.pos.to_array().map(f32::to_bits), b.pos.to_array().map(f32::to_bits));
        assert_eq!(a.vel.to_array().map(f32::to_bits), b.vel.to_array().map(f32::to_bits));
        assert!(a.pos.x != 0.0, "the script did not move");
    }

    #[test]
    fn air_control_is_limited() {
        let world = World::new(&[floor()]);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        w.tick(&world, &WalkInput { jump: true, ..Default::default() });
        let x0 = w.pos.x;
        let mut input = forward(EAST);
        input.run = true;
        for _ in 0..30 {
            w.tick(&world, &input);
        }
        // Half a second of air at 6 m/s^2 gives at most 3 m/s sideways, not 6.
        assert!(w.vel.x > 1.0 && w.vel.x < 3.1, "air speed {}", w.vel.x);
        assert!(w.pos.x - x0 < 1.0);
    }

    #[test]
    fn stairs_against_a_railing_do_not_trap_the_walker() {
        // A thin wall (a railing) beside the stair lane: walking along it slides.
        let rail = aabb([-50.0, 0.0, 0.5], [50.0, 1.1, 0.56]);
        let world = World::new(&[floor(), rail]);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST * 0.8), 4.0);
        assert!(w.pos.z < 0.2 + 1e-3 && w.pos.z > 0.1, "z {}", w.pos.z);
        assert!(w.pos.x > 5.0);
    }

    #[test]
    fn respawn_is_well_back_from_the_edge_and_held_keys_are_ignored_for_a_moment() {
        let world = platform_world(20.0);
        let mut w = walker_at(Vec3::new(-5.0, 20.0, 0.0));
        run(&mut w, &world, WalkInput::default(), 1.0);
        // Forward stays held the whole time, running: off the edge at x = 3, fall, die.
        let input = WalkInput { run: true, ..forward(EAST) };
        let mut ticks = 0;
        while w.deaths == 0 && ticks < 60 * 20 {
            w.tick(&world, &input);
            ticks += 1;
        }
        assert_eq!(w.deaths, 1, "never died");
        let back = w.pos;
        assert!((back.y - 20.0).abs() < 0.01 && w.grounded && w.vel == Vec3::ZERO, "{back:?}");
        assert!(3.0 - back.x >= 0.6, "returned {} m from the edge", 3.0 - back.x);
        // The lockout: for 0.3 s the held key does nothing.
        for _ in 0..(RESPAWN_LOCKOUT / FIXED_DT) as usize - 1 {
            w.tick(&world, &input);
            assert_eq!(w.pos, back, "moved during the lockout");
        }
        // Afterwards it walks again (and may well go off the edge again).
        for _ in 0..30 {
            w.tick(&world, &input);
        }
        assert!(w.pos.x > back.x + 0.5, "never walked on after the lockout: {:?}", w.pos);
        // Same for R.
        let mut w = walker_at(Vec3::new(-5.0, 20.0, 0.0));
        run(&mut w, &world, WalkInput::default(), 1.0);
        w.reset();
        let at = w.pos;
        run(&mut w, &world, input, 0.25);
        assert_eq!(w.pos, at);
    }

    #[test]
    fn spots_near_an_edge_or_a_bridge_end_are_not_stable_but_spots_under_a_wall_or_in_the_middle_are() {
        // A strip 3 m wide and 10 m long, with a wall alongside it on one side.
        let strip = aabb([0.0, 19.0, -1.5], [10.0, 20.0, 1.5]);
        let wall = aabb([0.0, 19.0, 1.5], [10.0, 30.0, 3.0]);
        let world = World::new(&[strip, wall]);
        assert!(world.supported_ring(Vec3::new(5.0, 20.0, 0.0), STABLE_RING_RADIUS), "middle of the strip");
        assert!(world.supported_ring(Vec3::new(5.0, 20.0, 1.2), STABLE_RING_RADIUS), "beside the wall: the wall is not an edge");
        assert!(!world.supported_ring(Vec3::new(5.0, 20.0, -1.0), STABLE_RING_RADIUS), "0.5 m from the open side");
        assert!(!world.supported_ring(Vec3::new(0.4, 20.0, 0.0), STABLE_RING_RADIUS), "0.4 m from the end");
        assert!(world.supported_ring(Vec3::new(5.0, 20.0, -0.8), STABLE_RING_RADIUS), "0.7 m from the open side");
    }

    #[test]
    fn jump_is_a_press_not_a_hold_and_a_press_just_before_landing_counts() {
        let world = World::new(&[floor()]);
        let hold = WalkInput { jump: true, ..Default::default() };
        // Holding for three seconds: one jump, however many landings.
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        let mut takeoffs = 0;
        let mut was_grounded = true;
        for _ in 0..180 {
            w.tick(&world, &hold);
            takeoffs += (was_grounded && !w.grounded) as u32;
            was_grounded = w.grounded;
        }
        assert_eq!(takeoffs, 1, "held Space hopped {takeoffs} times");
        assert!(w.grounded);
        // Release and press again: a second jump.
        w.tick(&world, &WalkInput::default());
        w.tick(&world, &hold);
        assert!(!w.grounded && w.vel.y > 3.0);

        // Land time of a plain jump, in steps after the press.
        let mut probe = walker_at(Vec3::ZERO);
        settle(&mut probe, &world);
        probe.tick(&world, &hold);
        let mut land = 1;
        while !probe.grounded {
            probe.tick(&world, &WalkInput::default());
            land += 1;
        }
        // Press `early` steps before the landing step (and release at once): jumps again if the
        // press is within the 0.1 s buffer (6 steps), lands and stays if it is earlier.
        for (early, jumps) in [(3usize, true), (5, true), (12, false), (30, false)] {
            let mut w = walker_at(Vec3::ZERO);
            settle(&mut w, &world);
            w.tick(&world, &hold);
            for i in 1..land + 3 {
                let press = i + early == land;
                w.tick(&world, &WalkInput { jump: press, ..Default::default() });
            }
            assert_eq!(w.pos.y > 0.05, jumps, "press {early} steps before landing: grounded {} y {}", w.grounded, w.pos.y);
        }
    }
}
