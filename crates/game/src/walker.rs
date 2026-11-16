//! The first-person walking controller: a kinematic capsule moved by collide-and-slide against
//! the scene's colliders, with step-up, stick-to-ground, jumping, drag-limited falling, fatal
//! impacts and a return to the last stable ground. No window, no GPU; advanced only in fixed
//! steps of `FIXED_DT`, so the same inputs give the same result.
//!
//! The velocity rules are Quake 2's player movement, re-derived in metres: on the ground,
//! friction with a stop-speed floor and then acceleration towards the wanted direction that only
//! adds while the speed along that direction is below the wanted speed; in the air the same rule
//! with a small factor, which is what allows steering in flight and strafe-jumping; a jump on the
//! press, taken before friction so a well-timed hop keeps its speed; step-and-slide moves, in
//! the air too. Constants and their Quake origins are in `player.rs`.
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
    /// Shift held: walk instead of run (or run instead of walk when `ALWAYS_RUN` is false).
    pub shift: bool,
    pub jump: bool,
    /// Crouch held (C or Ctrl).
    pub crouch: bool,
}

impl WalkInput {
    /// The wanted top speed for this input, m/s.
    pub fn wish_speed(&self) -> f32 {
        if self.shift == ALWAYS_RUN {
            WALK_SPEED
        } else {
            RUN_SPEED
        }
    }
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
    /// Jumps taken so far.
    pub jumps: u32,
    /// Crouched: the capsule is `CROUCH_HEIGHT` tall and the eye `CROUCH_EYE_HEIGHT` up.
    pub crouched: bool,
    /// Eye correction for a crouch or a stand in progress, metres, easing to zero.
    pub crouch_eye: f32,
    /// Whether the stable spot was recorded crouched (in a vent); a return there is crouched.
    pub stable_crouched: bool,
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

/// Move a capsule `height` tall from `start` by `motion`, sliding along what it meets.
fn slide(world: &World, start: Vec3, motion: Vec3, mode: Mode, height: f32) -> Slide {
    let min_y = min_ground_y();
    let mut out = Slide { pos: start, normals: Vec::new(), wall: false };
    let mut remaining = motion;
    let mut planes: Vec<Vec3> = Vec::new();
    for _ in 0..MAX_SLIDE_ITERATIONS {
        if remaining.length_squared() < 1e-12 {
            break;
        }
        let Some(hit) = world.cast_h(out.pos, remaining, height) else {
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

/// The velocity after touching the surfaces in `normals`, as Quake 2's slide move does it: the
/// part going into a surface is removed; if that sends it into another surface it follows the
/// crease of the two; wedged by three or more it stops. Motion along a surface keeps its speed.
fn clip_velocity(v: Vec3, normals: &[Vec3]) -> Vec3 {
    let mut planes: Vec<Vec3> = Vec::new();
    for n in normals {
        if !planes.iter().any(|p| p.dot(*n) > 0.999) {
            planes.push(*n);
        }
    }
    if !planes.iter().any(|p| v.dot(*p) < 0.0) {
        return v;
    }
    let clear = |c: Vec3, skip: &[usize]| planes.iter().enumerate().all(|(j, p)| skip.contains(&j) || c.dot(*p) >= -1e-5);
    for (i, p) in planes.iter().enumerate() {
        let d = v.dot(*p);
        let c = if d < 0.0 { v - *p * d } else { v };
        if clear(c, &[i]) {
            return c;
        }
    }
    for i in 0..planes.len() {
        for j in i + 1..planes.len() {
            let Some(dir) = planes[i].cross(planes[j]).try_normalize() else { continue };
            let c = dir * v.dot(dir);
            if clear(c, &[i, j]) {
                return c;
            }
        }
    }
    Vec3::ZERO
}

/// Quake 2's `PM_Friction` on a horizontal velocity: the speed falls by `FRICTION` times itself
/// per second, but never by less than `FRICTION` times `STOP_SPEED`, so a stop is crisp.
fn friction(v: Vec3, dt: f32) -> Vec3 {
    let speed = v.length();
    if speed < REST_SPEED {
        return Vec3::ZERO;
    }
    let drop = speed.max(STOP_SPEED) * FRICTION * dt;
    v * ((speed - drop).max(0.0) / speed)
}

/// Quake 2's `PM_Accelerate`: add up to `accel * wish_speed * dt` along `wish_dir`, but only
/// while the speed along `wish_dir` is below `wish_speed`. Speed in other directions is never
/// taken away, which is what lets a strafing jump turn and gain speed.
fn accelerate(v: Vec3, wish_dir: Vec3, wish_speed: f32, accel: f32, dt: f32) -> Vec3 {
    let add = wish_speed - v.dot(wish_dir);
    if add <= 0.0 {
        return v;
    }
    v + wish_dir * (accel * dt * wish_speed).min(add)
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
            jumps: 0,
            crouched: false,
            crouch_eye: 0.0,
            stable_crouched: false,
            jump_prev: false,
            jump_buffer: 0.0,
        }
    }

    /// Eye position.
    pub fn eye(&self) -> Vec3 {
        self.pos + Vec3::Y * (self.eye_height() + self.eye_offset + self.crouch_eye)
    }

    /// Capsule height in the current stance.
    pub fn height(&self) -> f32 {
        if self.crouched {
            CROUCH_HEIGHT
        } else {
            CAPSULE_HEIGHT
        }
    }

    /// Eye height above the feet in the current stance (without the smoothing offsets).
    pub fn eye_height(&self) -> f32 {
        if self.crouched {
            CROUCH_EYE_HEIGHT
        } else {
            EYE_HEIGHT
        }
    }

    /// Crouch or stand as asked, if there is room. On the ground the feet stay and the head
    /// moves; in the air the head stays and the feet move (pulling the legs up), so crouching
    /// in a jump lifts the feet by 0.9 m, which is how a vent mouth above a jump is reached.
    /// Standing needs room for the whole standing capsule; until there is, the walker stays
    /// crouched. Nothing here moves the capsule into geometry.
    fn update_stance(&mut self, world: &World, want: bool) {
        let shrink = CAPSULE_HEIGHT - CROUCH_HEIGHT;
        let eye_drop = EYE_HEIGHT - CROUCH_EYE_HEIGHT;
        if want && !self.crouched {
            // The crouched capsule lies inside the standing one, so it always fits.
            let lift = if self.grounded { 0.0 } else { shrink };
            self.pos.y += lift;
            self.crouched = true;
            self.crouch_eye += eye_drop - lift;
        } else if !want && self.crouched {
            // In the air the feet come down as far as there is room below; on the ground the head
            // goes up.
            let lower = if self.grounded { 0.0 } else { world.cast_h(self.pos, Vec3::NEG_Y * shrink, CROUCH_HEIGHT).map_or(shrink, |h| shrink * h.fraction) };
            let feet = self.pos - Vec3::Y * lower;
            if world.fits(feet, CAPSULE_HEIGHT) {
                self.pos = feet;
                self.crouched = false;
                self.crouch_eye += lower - eye_drop;
            }
        }
    }

    /// Put the player's eye at `eye` at rest (leaving fly mode). Death and reset counts and
    /// the stable spot are kept.
    pub fn place_eye(&mut self, eye: Vec3) {
        self.pos = eye - Vec3::Y * EYE_HEIGHT;
        self.crouched = false;
        self.crouch_eye = 0.0;
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
        self.crouched = self.stable_crouched;
        self.crouch_eye = 0.0;
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
        // Crouch is a stance, not a movement: it is kept through the lockout.
        let input = &if locked { WalkInput { yaw: input.yaw, crouch: input.crouch, ..Default::default() } } else { *input };
        // A jump is a key press, not a held key (unless `AUTO_HOP`); a press shortly before
        // landing is kept.
        if input.jump && (!self.jump_prev || AUTO_HOP) {
            self.jump_buffer = JUMP_BUFFER;
        }
        self.jump_prev = raw_jump;
        self.update_stance(world, input.crouch);
        if !self.crouched && !world.fits(self.pos, CAPSULE_HEIGHT) && world.fits(self.pos, CROUCH_HEIGHT) {
            // Standing does not fit here but crouching does (arriving from the fly camera inside
            // a vent): crouch rather than be pushed out through a wall.
            self.crouched = true;
            self.crouch_eye += EYE_HEIGHT - CROUCH_EYE_HEIGHT;
        }
        self.pos = world.depenetrate_h(self.pos, self.height());
        self.last_impact = 0.0;

        // Wanted direction and speed from the keys and the facing (diagonals are not faster).
        let (sy, cy) = input.yaw.sin_cos();
        let fwd = Vec3::new(sy, 0.0, cy);
        let right = Vec3::new(-cy, 0.0, sy);
        let dir = fwd * input.forward + right * input.right;
        let wish_dir = dir.normalize_or_zero();
        let top = if self.crouched { CROUCH_SPEED.min(input.wish_speed()) } else { input.wish_speed() };
        let wish_speed = if wish_dir == Vec3::ZERO { 0.0 } else { top * dir.length().min(1.0) };

        // The jump comes first: the take-off step is an air step with no friction, so a hop
        // pressed on (or just before) landing loses no speed.
        if self.grounded && self.jump_buffer > 0.0 {
            self.vel.y = self.vel.y.max(0.0) + JUMP_SPEED;
            self.grounded = false;
            self.jump_buffer = 0.0;
            self.jumps += 1;
        }
        let mut vh = horizontal(self.vel);
        if self.grounded {
            vh = friction(vh, dt);
            vh = accelerate(vh, wish_dir, wish_speed, GROUND_ACCEL, dt);
        } else {
            vh = accelerate(vh, wish_dir, wish_speed, AIR_ACCEL, dt);
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
                self.stable_crouched = self.crouched;
            }
        } else {
            self.ground_time = 0.0;
        }
        self.eye_offset *= (-EYE_SMOOTH_RATE * dt).exp();
        if self.eye_offset.abs() < 1e-4 {
            self.eye_offset = 0.0;
        }
        self.crouch_eye *= (-CROUCH_EYE_RATE * dt).exp();
        if self.crouch_eye.abs() < 1e-4 {
            self.crouch_eye = 0.0;
        }
        self.keep_eye_under_ceiling(world);
    }

    /// While the eye lags above its resting height (after crouching, or a step down), keep it
    /// no higher than the capsule could rise from here, so it never shows through a ceiling:
    /// the eye is then 0.15 m under the top of a capsule that fits.
    fn keep_eye_under_ceiling(&mut self, world: &World) {
        let extra = self.eye_offset + self.crouch_eye;
        if extra <= 0.0 {
            return;
        }
        let room = world.cast_h(self.pos, Vec3::Y * extra, self.height()).map_or(extra, |h| extra * h.fraction);
        let mut cut = extra - room;
        if cut <= 0.0 {
            return;
        }
        let c = cut.min(self.crouch_eye.max(0.0));
        self.crouch_eye -= c;
        cut -= c;
        self.eye_offset -= cut.min(self.eye_offset.max(0.0));
    }

    /// Walking: slide along the ground, step up ledges, stay stuck to the ground below.
    fn ground_move(&mut self, world: &World, dt: f32) {
        let motion = horizontal(self.vel) * dt;
        let start = self.pos;
        let h = self.height();
        let a = slide(world, start, motion, Mode::Ground, h);
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
        if !stepped {
            // Lose the speed that went into walls (as they were met: vertical) and keep the
            // speed along them, so a held key does not store speed against a wall and a glancing
            // touch costs only the part into it.
            let min_y = min_ground_y();
            let walls: Vec<Vec3> = a.normals.iter().filter(|n| n.y < min_y).filter_map(|n| horizontal(*n).try_normalize()).collect();
            let v = clip_velocity(horizontal(self.vel), &walls);
            self.vel = Vec3::new(v.x, 0.0, v.z);
        }
        // Stay on the ground: follow slopes and steps down.
        let down = Vec3::NEG_Y * SNAP_DISTANCE;
        match world.cast_h(best, down, h) {
            Some(h) if h.normal.y >= min_ground_y() => {
                let p = best + down * h.fraction;
                // What the slope under the feet accounts for is followed exactly; anything more
                // is a step down, which the eye eases into.
                let slope = (1.0 - h.normal.y * h.normal.y).max(0.0).sqrt() / h.normal.y;
                let drop = best.y - p.y - horizontal_distance(start, best) * slope;
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
        let height = self.height();
        let rise = world.cast_h(start, up, height).map_or(STEP_HEIGHT, |h| STEP_HEIGHT * h.fraction);
        if rise < 0.02 {
            return None;
        }
        let mut lengths = vec![want];
        if want < STEP_MIN_FORWARD {
            lengths.push(STEP_MIN_FORWARD);
        }
        for len in lengths {
            // Raised as far as there is room first; if something at that height is in the way
            // (the wall above a vent mouth), lower and lower, 5 cm at a time, so a mouth only a
            // little taller than the capsule is still found (E112).
            let mut r = rise;
            while r >= 0.02 {
                let raised = start + Vec3::Y * r;
                let s = slide(world, raised, dir * len, Mode::Wall, height);
                if horizontal_distance(raised, s.pos) < len - 1e-3 {
                    r -= STEP_RETRY;
                    continue;
                }
                let down = Vec3::NEG_Y * (r + SNAP_DISTANCE);
                match world.cast_h(s.pos, down, height) {
                    // The ledge's own height counts, not the capsule's: resting on the corner of
                    // a ledge the sphere sits lower than the top it is touching.
                    Some(h) if h.normal.y >= min_ground_y() && h.point.y - start.y <= STEP_HEIGHT + 0.005 => return Some(s.pos + down * h.fraction),
                    // Through at this height but nothing to stand on: lower would not help.
                    _ => break,
                }
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
        let start = self.pos;
        let s = slide(world, start, v_avg * dt, Mode::Air, self.height());
        self.pos = s.pos;

        let impact = s.normals.iter().map(|n| -v_avg.dot(*n)).fold(0.0f32, f32::max);
        self.last_impact = impact;
        if impact > fatal_impact_speed() {
            return true;
        }

        let floor = s.normals.iter().find(|n| n.y >= min_ground_y());
        if let (Some(n), true) = (floor, v1.y < LAND_MAX_RISE) {
            // Landed: keep the horizontal speed, lose the vertical.
            self.grounded = true;
            self.ground_normal = *n;
            self.vel = Vec3::new(v1.x, 0.0, v1.z);
            return false;
        }
        if s.wall && v1.y <= 0.0 {
            // Coming down against something steep: as in Quake 2 the step-up works in the air
            // too, so a ledge whose top is within a step of the feet is landed on instead of
            // slid down. The landing is an impact like any other.
            let progress = horizontal_distance(start, s.pos);
            if let Some(b) = self.try_step(world, start, horizontal(v_avg) * dt) {
                if horizontal_distance(start, b) > progress + 1e-3 {
                    let down = (-v1.y).max(0.0);
                    self.last_impact = self.last_impact.max(down);
                    if down > fatal_impact_speed() {
                        return true;
                    }
                    self.eye_offset -= (b.y - s.pos.y).max(0.0);
                    self.eye_offset = self.eye_offset.clamp(-STEP_HEIGHT * 1.5, STEP_HEIGHT * 1.5);
                    self.pos = b;
                    self.grounded = true;
                    self.ground_normal = Vec3::Y;
                    self.vel = Vec3::new(v1.x, 0.0, v1.z);
                    return false;
                }
            }
        }
        self.vel = clip_velocity(v1, &s.normals);
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

    /// Forward, running (the default).
    fn forward(yaw: f32) -> WalkInput {
        WalkInput { forward: 1.0, yaw, ..Default::default() }
    }

    /// Forward with Shift: walking.
    fn walking(yaw: f32) -> WalkInput {
        WalkInput { shift: true, ..forward(yaw) }
    }

    fn run(w: &mut Walker, world: &World, input: WalkInput, seconds: f32) {
        for _ in 0..(seconds / FIXED_DT).round() as usize {
            w.tick(world, &input);
        }
    }

    fn settle(w: &mut Walker, world: &World) {
        run(w, world, WalkInput::default(), 1.0);
    }

    fn hspeed(w: &Walker) -> f32 {
        horizontal(w.vel).length()
    }

    #[test]
    fn the_constants_are_quake_2_scaled_to_the_body() {
        assert!((QUAKE_UNIT - 0.032143).abs() < 1e-5);
        assert!((RUN_SPEED - 9.643).abs() < 0.01 && (GRAVITY - 25.71).abs() < 0.01 && (JUMP_SPEED - 8.679).abs() < 0.01);
        // The fatal speed is the speed after the fatal height, with drag: about 21.7 m/s.
        assert!((fatal_impact_speed() - 21.7).abs() < 0.1, "{}", fatal_impact_speed());
        const { assert!(WALK_SPEED < STOP_SPEED && STOP_SPEED < RUN_SPEED) };
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
    fn starts_are_quick_top_speed_is_exact_and_stops_are_crisp() {
        let world = World::new(&[floor()]);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        // From rest: 90 percent of run speed within 0.25 s, all of it within 0.5 s, never more.
        let (mut t90, mut t99, mut top) = (None, None, 0.0f32);
        for i in 1..=120 {
            w.tick(&world, &forward(EAST));
            let v = hspeed(&w);
            top = top.max(v);
            if t90.is_none() && v >= 0.9 * RUN_SPEED {
                t90 = Some(i as f32 * FIXED_DT);
            }
            if t99.is_none() && v >= 0.99 * RUN_SPEED {
                t99 = Some(i as f32 * FIXED_DT);
            }
        }
        let (t90, t99) = (t90.expect("never reached 90%"), t99.expect("never reached 99%"));
        println!("run: 90% at {t90:.3} s, 99% at {t99:.3} s");
        assert!(t90 <= 0.25 && t99 <= 0.5, "90% at {t90}, 99% at {t99}");
        assert!(top <= RUN_SPEED + 1e-4, "top {top}");
        assert!(w.pos.z.abs() < 1e-4 && w.grounded);
        // Diagonal keys are not faster.
        let mut d = walker_at(Vec3::ZERO);
        settle(&mut d, &world);
        run(&mut d, &world, WalkInput { right: 1.0, ..forward(EAST) }, 1.0);
        assert!((hspeed(&d) - RUN_SPEED).abs() < 1e-3, "diagonal {}", hspeed(&d));
        // Let go: friction with the stop-speed floor stops it in about a third of a second, over
        // about 1.3 m.
        let x0 = w.pos.x;
        let mut stop = 0;
        while hspeed(&w) > 0.0 && stop < 120 {
            w.tick(&world, &WalkInput { yaw: EAST, ..Default::default() });
            stop += 1;
        }
        let (t, dist) = (stop as f32 * FIXED_DT, w.pos.x - x0);
        println!("stop from run: {t:.3} s over {dist:.3} m");
        assert!((0.25..0.45).contains(&t) && (1.0..1.6).contains(&dist), "stopped in {t} s over {dist} m");
        // Walking (Shift): top speed is the walk speed, and it stops in under a fifth of a second.
        run(&mut w, &world, walking(EAST), 1.0);
        assert!((hspeed(&w) - WALK_SPEED).abs() < 1e-3, "walk {}", hspeed(&w));
        let mut stop = 0;
        while hspeed(&w) > 0.0 && stop < 60 {
            w.tick(&world, &WalkInput { yaw: EAST, ..Default::default() });
            stop += 1;
        }
        assert!(stop as f32 * FIXED_DT < 0.2, "walk stop {stop} steps");
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
        // Against the wall the stored speed is zero, not building up into it.
        assert!(hspeed(&w) < 1e-3, "speed into the wall {}", hspeed(&w));

        // 45 degrees into the wall: x stays at the wall, z carries on.
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST / 2.0), 3.0);
        assert!(w.pos.x < 3.0 - CAPSULE_RADIUS + 0.01 && w.pos.x > 3.0 - CAPSULE_RADIUS - 0.02, "x {}", w.pos.x);
        assert!(w.pos.z > 8.0, "slid only to z {}", w.pos.z);
        assert!(w.grounded);
    }

    #[test]
    fn a_wall_met_at_a_glancing_angle_keeps_the_speed_along_it() {
        let wall = aabb([3.0, 0.0, -50.0], [4.0, 5.0, 50.0]);
        let world = World::new(&[floor(), wall]);
        for deg in [10.0f32, 30.0, 60.0] {
            // Run along +Z at full speed beside the wall, then turn `deg` towards it.
            let mut w = walker_at(Vec3::new(3.0 - CAPSULE_RADIUS - 0.3, 0.0, -40.0));
            settle(&mut w, &world);
            run(&mut w, &world, forward(0.0), 1.0);
            let yaw = deg.to_radians();
            let along = RUN_SPEED * yaw.cos();
            let mut least = f32::MAX;
            let mut touched = false;
            for _ in 0..60 {
                w.tick(&world, &forward(yaw));
                if w.pos.x > 3.0 - CAPSULE_RADIUS - 0.01 {
                    touched = true;
                    least = least.min(w.vel.z);
                }
            }
            assert!(touched, "{deg}: never reached the wall");
            // The part along the wall is never lost; the key held at an angle keeps pushing along it.
            assert!(least > along - 0.05, "{deg} degrees: speed along the wall fell to {least}, expected at least {along}");
            assert!(w.vel.x.abs() < 1e-3, "{deg}: speed into the wall {}", w.vel.x);
            println!("{deg} degrees into a wall: {:.2} m/s along it", w.vel.z);
        }
    }

    fn ledge_test(height: f32, input: WalkInput) -> Walker {
        let ledge = aabb([2.0, -1.0, -50.0], [50.0, height, 50.0]);
        let world = World::new(&[floor(), ledge]);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, input, 3.0);
        w
    }

    #[test]
    fn a_forty_cm_ledge_is_stepped_up_and_a_half_metre_ledge_is_not() {
        for input in [forward(EAST), walking(EAST)] {
            for h in [0.25f32, 0.4, 0.44] {
                let w = ledge_test(h, input);
                assert!(w.pos.x > 3.5 && (w.pos.y - h).abs() < 0.02 && w.grounded, "{h} m, shift {}: {:?}", input.shift, w.pos);
            }
            let w = ledge_test(0.5, input);
            assert!(w.pos.x < 2.0 - CAPSULE_RADIUS + 0.02 && w.pos.y < 0.02 && w.grounded, "0.5 m, shift {}: {:?}", input.shift, w.pos);
        }
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
        for input in [forward(EAST), walking(EAST)] {
            // Long enough to climb, short enough not to run off the 50 m landing.
            let secs = 25.0 / input.wish_speed();
            let (world, end) = ramp_world(35.0, 6.0);
            let mut w = walker_at(Vec3::ZERO);
            settle(&mut w, &world);
            run(&mut w, &world, input, secs);
            assert!(w.grounded && (w.pos.y - end.y).abs() < 0.05 && w.pos.x > end.x + 1.0, "{:?} vs {end:?}", w.pos);

            let (world, end) = ramp_world(45.0, 6.0);
            let mut w = walker_at(Vec3::ZERO);
            settle(&mut w, &world);
            run(&mut w, &world, input, secs);
            assert!(w.grounded && w.pos.y < 0.05 && w.pos.x < 2.0, "climbed a 45 degree ramp: {:?} (top {end:?})", w.pos);
            // A 39 degree ramp is fine, 41 is not.
            let (world, end) = ramp_world(39.0, 6.0);
            let mut w = walker_at(Vec3::ZERO);
            settle(&mut w, &world);
            run(&mut w, &world, input, secs);
            assert!(w.pos.y > end.y - 0.05, "39 degrees: {:?}", w.pos);
            // Back down the 39 degree ramp: on the ground throughout and the eye follows the
            // slope exactly (no step smoothing; at run speed it drops 0.13 m a step).
            let mut worst = 0.0f32;
            for _ in 0..((secs / FIXED_DT) as usize) {
                w.tick(&world, &WalkInput { yaw: -EAST, ..input });
                if w.pos.x > 2.5 && w.pos.x < end.x - 0.5 {
                    worst = worst.max(w.eye_offset.abs());
                    assert!(w.grounded, "39 degrees down: airborne at {:?}", w.pos);
                }
            }
            assert!(worst < 1e-3 && w.pos.y < 0.05, "39 degrees down: eye offset up to {worst}, ended {:?}", w.pos);
            let (world, _) = ramp_world(41.0, 6.0);
            let mut w = walker_at(Vec3::ZERO);
            settle(&mut w, &world);
            run(&mut w, &world, input, secs);
            assert!(w.pos.y < 0.05, "41 degrees: {:?}", w.pos);
        }
    }

    #[test]
    fn a_steep_slope_landed_on_is_slid_down() {
        // Dropped onto a 55 degree slope: it cannot be stood on, so the walker slides off its foot.
        let (s, c) = 55.0f32.to_radians().sin_cos();
        let start = Vec3::new(2.0, 0.0, 0.0);
        let end = start + Vec3::new(6.0 * c, 6.0 * s, 0.0);
        let ramp = Collider::Ramp { start: start.to_array(), end: end.to_array(), half_width: 2.0, thickness: 0.4 };
        let world = World::new(&[floor(), ramp]);
        let mut w = walker_at(Vec3::new(4.0, 4.5, 0.0));
        let mut on_slope = 0;
        for _ in 0..180 {
            w.tick(&world, &WalkInput::default());
            on_slope += (!w.grounded && w.pos.x < 4.0 && w.pos.y > 0.1) as u32;
        }
        assert!(on_slope > 3, "never slid on the slope");
        assert!(w.grounded && w.pos.y < 0.02 && w.pos.x < 2.0, "ended at {:?}", w.pos);
        assert_eq!(w.deaths, 0);
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
        for (shift, name) in [(true, "walk"), (false, "run")] {
            let speed = if shift { WALK_SPEED } else { RUN_SPEED };
            let mut w = walker_at(Vec3::ZERO);
            settle(&mut w, &world);
            let input = WalkInput { shift, ..forward(EAST) };
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
            check_smooth(&eye, 1, speed, &format!("{name} up"));

            // And back down.
            let back = WalkInput { shift, ..forward(-EAST) };
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
            check_smooth(&eye, -1, speed, &format!("{name} down"));
        }
    }

    /// The eye moves one way only (`dir`), by at most the slope's worth per step at `speed`, and
    /// never by a jump.
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

    /// Jump from rest on a floor: (apex above the start, steps in the air).
    fn jump_profile() -> (f32, u32) {
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
        assert!(w.grounded && w.deaths == 0);
        (apex, air)
    }

    #[test]
    fn a_jump_is_quakes_jump_scaled_to_the_body() {
        // 8.68 m/s against 25.7 m/s^2: 1.47 m and 0.675 s without drag; drag takes a little off.
        let (apex, air) = jump_profile();
        println!("jump apex {apex:.3} m, {air} steps in the air");
        assert!((1.38..1.47).contains(&apex), "apex {apex}");
        assert!((38..42).contains(&air), "{air} steps in the air");
        // Not while airborne: holding jump does not add speed in the air.
        let world = World::new(&[floor()]);
        let mut w = walker_at(Vec3::new(0.0, 5.0, 0.0));
        for _ in 0..10 {
            w.tick(&world, &WalkInput { jump: true, ..Default::default() });
        }
        assert!(w.vel.y < 0.0);
        // The take-off keeps the running speed.
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST), 1.0);
        w.tick(&world, &WalkInput { jump: true, ..forward(EAST) });
        // Less only the air drag of one step (k v^2 dt, 1.3 cm/s).
        assert!(!w.grounded && (hspeed(&w) - RUN_SPEED).abs() < 0.02, "{}", hspeed(&w));
    }

    #[test]
    fn free_fall_approaches_terminal_velocity_and_never_exceeds_it() {
        let world = World::new(&[]);
        let mut w = walker_at(Vec3::new(0.0, 1.0e5, 0.0));
        let mut v2 = 0.0;
        let mut top = 0.0f32;
        for i in 0..(60 * 60) {
            w.tick(&world, &WalkInput::default());
            top = top.max(-w.vel.y);
            if i + 1 == 2 * 60 {
                v2 = -w.vel.y;
            }
        }
        assert!(top <= TERMINAL_VELOCITY, "exceeded terminal: {top}");
        assert!(top > TERMINAL_VELOCITY - 0.01, "never got there: {top}");
        assert!(!w.grounded && w.deaths == 0);
        // Analytic: v = vt tanh(g t / vt), 73 percent of terminal after 2 s.
        let exact = TERMINAL_VELOCITY * (GRAVITY * 2.0 / TERMINAL_VELOCITY).tanh();
        assert!((v2 - exact).abs() < 0.1, "{v2} vs {exact}");
    }

    /// A platform at `top` (x -6..3) over a floor at y = 0.
    fn platform_world(top: f32) -> World {
        World::new(&[floor(), aabb([-6.0, top - 1.0, -4.0], [3.0, top, 4.0])])
    }

    #[test]
    fn drops_up_to_nine_metres_are_survived_and_from_eleven_they_kill() {
        for (top, dies) in [(5.0f32, false), (8.0, false), (9.0, false), (11.0, true), (15.0, true), (40.0, true)] {
            for input in [forward(EAST), walking(EAST)] {
                let world = platform_world(top);
                let mut w = walker_at(Vec3::new(-2.0, top, 0.0));
                run(&mut w, &world, WalkInput::default(), 1.0);
                // To the edge at x = 3 and over it, then let go and wait.
                run(&mut w, &world, input, 5.5 / input.wish_speed() + 0.2);
                run(&mut w, &world, WalkInput::default(), 8.0);
                assert_eq!(w.deaths, if dies { 1 } else { 0 }, "drop {top} (shift {}): deaths {}, pos {:?}", input.shift, w.deaths, w.pos);
                assert!(w.grounded);
                if dies {
                    assert!(w.pos.y > top - 0.01, "drop {top}: respawned at {:?}", w.pos);
                } else {
                    assert!(w.pos.y < 0.01, "drop {top}: ended at {:?}", w.pos);
                }
            }
        }
    }

    #[test]
    fn death_returns_to_the_last_stable_ground_and_so_does_the_reset_key() {
        let world = platform_world(20.0);
        let mut w = walker_at(Vec3::new(-5.0, 20.0, 0.0));
        run(&mut w, &world, WalkInput::default(), 1.0);
        // Walk 2.3 m (0.8 s), stand 1 s: that spot is stable.
        run(&mut w, &world, walking(EAST), 0.8);
        run(&mut w, &world, WalkInput::default(), 1.0);
        let spot = w.pos;
        assert_eq!(w.stable_pos, spot);
        // Walk off the edge at x = 3 (it takes about 2 s) and let go.
        run(&mut w, &world, walking(EAST), 2.8);
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
        run(&mut w, &world, walking(-EAST), 1.0);
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
        for _ in 0..3 {
            w.tick(&world, &WalkInput { jump: true, ..walking(EAST) });
            for _ in 0..45 {
                w.tick(&world, &walking(EAST));
            }
        }
        assert_eq!(w.jumps, 3);
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
        let world = World::new(&[floor(), ledge, aabb([-20.0, 9.0, -50.0], [-10.0, 10.0, 50.0]), aabb([-3.0, 0.0, 5.0], [-2.0, 1.2, 6.0])]);
        let script = |w: &mut Walker| {
            run(w, &world, WalkInput::default(), 0.5);
            run(w, &world, forward(EAST), 1.0);
            w.tick(&world, &WalkInput { jump: true, ..forward(EAST) });
            run(w, &world, forward(EAST / 2.0), 2.0);
            run(w, &world, WalkInput { right: 1.0, yaw: 1.0, ..Default::default() }, 1.0);
            for i in 0..120 {
                w.tick(&world, &WalkInput { jump: i % 3 == 0, right: 1.0, yaw: -EAST + i as f32 * 0.02, ..forward(0.0) });
            }
            run(w, &world, walking(-EAST), 3.0);
        };
        let (mut a, mut b) = (walker_at(Vec3::new(0.0, 0.0, 0.0)), walker_at(Vec3::new(0.0, 0.0, 0.0)));
        script(&mut a);
        script(&mut b);
        assert_eq!(a.pos.to_array().map(f32::to_bits), b.pos.to_array().map(f32::to_bits));
        assert_eq!(a.vel.to_array().map(f32::to_bits), b.vel.to_array().map(f32::to_bits));
        assert!(a.pos.x != 0.0 && a.jumps > 3, "the script did not move");
    }

    #[test]
    fn air_control_steers_a_jump_but_is_limited() {
        let world = World::new(&[floor()]);
        // From a standing jump, half a second of forward in the air gives at most
        // AIR_ACCEL * RUN_SPEED * 0.5 = 4.8 m/s, not the run speed.
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        w.tick(&world, &WalkInput { jump: true, ..Default::default() });
        run(&mut w, &world, forward(EAST), 0.5);
        assert!(w.vel.x > 4.0 && w.vel.x < AIR_ACCEL * RUN_SPEED * 0.5 + 0.1, "air speed {}", w.vel.x);
        // Running east and jumping, then holding left strafe and turning: the heading turns
        // in the air. Without keys it goes straight.
        let heading = |turn: bool| {
            let mut w = walker_at(Vec3::ZERO);
            settle(&mut w, &world);
            run(&mut w, &world, forward(EAST), 1.0);
            w.tick(&world, &WalkInput { jump: true, ..forward(EAST) });
            let mut yaw = EAST;
            while !w.grounded {
                let input = if turn {
                    yaw += 60f32.to_radians() * FIXED_DT;
                    WalkInput { right: -1.0, yaw, ..Default::default() }
                } else {
                    WalkInput { yaw, ..Default::default() }
                };
                w.tick(&world, &input);
            }
            (w.vel.z.atan2(w.vel.x).to_degrees(), hspeed(&w))
        };
        let (straight, s0) = heading(false);
        let (turned, s1) = heading(true);
        println!("air turn: heading {straight:.1} -> {turned:.1} degrees, speed {s0:.2} -> {s1:.2}");
        assert!(straight.abs() < 0.01);
        assert!(turned.abs() > 15.0, "heading only turned {turned} degrees");
        assert!(s1 > s0 - 0.1, "turning in the air lost speed: {s0} -> {s1}");
    }

    /// The strafe-jump bot: hop on every landing, hold one strafe key and turn so that the key's
    /// direction stays at the angle to the velocity that gains the most per step.
    fn strafe_jump(seconds: f32, strafe: bool) -> (Walker, f32) {
        let world = World::new(&[aabb([-1000.0, -2.0, -1000.0], [1000.0, 0.0, 1000.0])]);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST), 1.0);
        let mut best = 0.0f32;
        for i in 0..(seconds / FIXED_DT) as usize {
            let jump = i % 2 == 0;
            let v = horizontal(w.vel);
            let input = if strafe && !w.grounded && v.length() > 1.0 {
                let a = AIR_ACCEL * FIXED_DT * RUN_SPEED;
                let theta = ((RUN_SPEED - a) / v.length()).clamp(-1.0, 1.0).acos();
                let phi = v.z.atan2(v.x) + theta;
                let wish = Vec3::new(phi.cos(), 0.0, phi.sin());
                // `right` is (-cos yaw, 0, sin yaw).
                let yaw = wish.z.atan2(-wish.x);
                WalkInput { right: 1.0, yaw, jump, ..Default::default() }
            } else {
                let yaw = v.x.atan2(v.z);
                WalkInput { jump, ..forward(yaw) }
            };
            w.tick(&world, &input);
            best = best.max(hspeed(&w));
        }
        (w, best)
    }

    #[test]
    fn strafe_jumping_gains_speed_and_plain_hopping_does_not() {
        let (w, best) = strafe_jump(5.0, true);
        println!("strafe-jumping 5 s: {best:.2} m/s ({} hops)", w.jumps);
        assert!(best > 1.5 * RUN_SPEED, "strafe-jumping reached only {best}");
        assert!(w.deaths == 0 && w.jumps > 5);
        let (w, best) = strafe_jump(5.0, false);
        assert!(best <= RUN_SPEED + 1e-3, "plain hopping reached {best}");
        assert!(w.jumps > 5);
    }

    #[test]
    fn nothing_tunnels_through_thin_walls_at_speed() {
        // A railing-like wall 6 cm thick and a 6 cm plate, hit at 10 to 55 m/s from many angles.
        let rail = aabb([5.0, -100.0, -20.0], [5.06, 1.1, 20.0]);
        let plate = aabb([-20.0, -0.06, -20.0], [-6.0, 0.0, 20.0]);
        let world = World::new(&[rail, plate]);
        for speed in [10.0f32, 20.0, 35.0, TERMINAL_VELOCITY] {
            for k in 0..24 {
                // Horizontally into the rail at angles up to 80 degrees off its normal.
                let ang = (-80.0 + k as f32 * 160.0 / 23.0).to_radians();
                let mut w = walker_at(Vec3::new(0.0, -0.5, 0.0));
                w.vel = Vec3::new(ang.cos(), 0.0, ang.sin()) * speed;
                for _ in 0..30 {
                    w.tick(&world, &WalkInput::default());
                    assert!(w.pos.x < 5.0 - CAPSULE_RADIUS + 1e-3, "through the rail at {speed} m/s, {} degrees: {:?}", ang.to_degrees(), w.pos);
                    assert!(!world.overlaps(w.pos), "inside the rail at {speed} m/s: {:?}", w.pos);
                    if w.deaths > 0 {
                        break;
                    }
                }
                // Down onto the plate, steeply or obliquely.
                let tilt = (k as f32 * 3.0).to_radians();
                let mut w = walker_at(Vec3::new(-13.0, 3.0, 0.0));
                w.vel = Vec3::new(tilt.sin(), -tilt.cos(), 0.0) * speed;
                for _ in 0..30 {
                    let before = w.pos;
                    w.tick(&world, &WalkInput::default());
                    if w.deaths > 0 {
                        break;
                    }
                    // Over the plate (not beside its edge) the feet never go below its top.
                    let over = before.x < -6.0 - CAPSULE_RADIUS && w.pos.x < -6.0 - CAPSULE_RADIUS;
                    assert!(!over || w.pos.y > -1e-3, "through the plate at {speed} m/s, tilt {k}: {:?}", w.pos);
                    assert!(!world.overlaps(w.pos), "inside the plate at {speed} m/s, tilt {k}: {:?}", w.pos);
                }
            }
        }
    }

    #[test]
    fn stairs_against_a_railing_do_not_trap_the_walker() {
        // A thin wall (a railing) beside the stair lane: walking along it slides.
        let rail = aabb([-50.0, 0.0, 0.5], [50.0, 1.1, 0.56]);
        let world = World::new(&[floor(), rail]);
        for input in [forward(EAST * 0.8), walking(EAST * 0.8)] {
            let mut w = walker_at(Vec3::ZERO);
            settle(&mut w, &world);
            run(&mut w, &world, input, 4.0);
            assert!(w.pos.z < 0.2 + 1e-3 && w.pos.z > 0.1, "z {}", w.pos.z);
            assert!(w.pos.x > 5.0);
        }
    }

    #[test]
    fn a_running_jump_lands_on_a_block_up_to_about_the_body_height_and_clears_a_railing() {
        // The step-up works in the air on the way down, so the reach is the apex plus a step.
        for (h, onto) in [(1.2f32, true), (1.6, true), (1.8, true), (2.1, false)] {
            let block = aabb([4.0, 0.0, -10.0], [40.0, h, 10.0]);
            let world = World::new(&[floor(), block]);
            let mut w = walker_at(Vec3::ZERO);
            settle(&mut w, &world);
            // Run at it and jump 1.2 m short of the face.
            while w.pos.x < 4.0 - CAPSULE_RADIUS - 1.2 {
                w.tick(&world, &forward(EAST));
            }
            w.tick(&world, &WalkInput { jump: true, ..forward(EAST) });
            run(&mut w, &world, forward(EAST), 1.5);
            assert_eq!((w.pos.y - h).abs() < 0.02, onto, "block {h} m: ended at {:?}", w.pos);
            assert!(w.grounded && w.deaths == 0);
        }
        // A 1.1 m railing, 6 cm thick, is jumped over from a run.
        let rail = aabb([4.0, 0.0, -10.0], [4.06, 1.1, 10.0]);
        let world = World::new(&[floor(), rail]);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        while w.pos.x < 4.0 - CAPSULE_RADIUS - 1.5 {
            w.tick(&world, &forward(EAST));
        }
        w.tick(&world, &WalkInput { jump: true, ..forward(EAST) });
        run(&mut w, &world, forward(EAST), 1.0);
        assert!(w.pos.x > 4.5, "did not clear the railing: {:?}", w.pos);
    }

    #[test]
    fn respawn_is_well_back_from_the_edge_and_held_keys_are_ignored_for_a_moment() {
        let world = platform_world(20.0);
        let mut w = walker_at(Vec3::new(-5.0, 20.0, 0.0));
        run(&mut w, &world, WalkInput::default(), 1.0);
        // Forward stays held the whole time, running: off the edge at x = 3, fall, die.
        let input = forward(EAST);
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

    #[test]
    fn a_buffered_hop_loses_no_speed_to_friction() {
        // Running, hop, and press again just before landing: the second take-off is at full speed.
        let world = World::new(&[floor()]);
        let mut w = walker_at(Vec3::ZERO);
        settle(&mut w, &world);
        run(&mut w, &world, forward(EAST), 1.0);
        w.tick(&world, &WalkInput { jump: true, ..forward(EAST) });
        let mut i = 0;
        while w.jumps < 2 && i < 200 {
            // Press every other step: the press before landing is buffered.
            w.tick(&world, &WalkInput { jump: i % 2 == 0, ..forward(EAST) });
            i += 1;
            if w.jumps == 2 {
                // Less only the air drag of the take-off step; friction would take 0.96 m/s.
                assert!((hspeed(&w) - RUN_SPEED).abs() < 0.02, "second take-off at {}", hspeed(&w));
            }
        }
        assert_eq!(w.jumps, 2);
    }
}

#[cfg(test)]
mod crouch_tests {
    use super::*;
    use mega_gen::Collider;

    fn aabb(min: [f32; 3], max: [f32; 3]) -> Collider {
        Collider::Aabb { min, max }
    }

    fn walker_at(feet: Vec3) -> Walker {
        Walker::new(feet + Vec3::Y * EYE_HEIGHT)
    }

    const EAST: f32 = std::f32::consts::FRAC_PI_2;

    fn run(w: &mut Walker, world: &World, input: WalkInput, seconds: f32) {
        for _ in 0..(seconds / FIXED_DT).round() as usize {
            w.tick(world, &input);
        }
    }

    fn crouch_east() -> WalkInput {
        WalkInput { forward: 1.0, yaw: EAST, crouch: true, ..Default::default() }
    }

    /// A 1 m by 1 m vent made of 1 m cells in a solid block between y = 0 and 1 (with a floor
    /// below and a roof above), `cells` empty, the block spanning x 0..20, z -2..13. Outside it
    /// (x < 0 and x > 20) is an open floor.
    fn vent_world(cells: &[(i32, i32)]) -> World {
        let mut c = vec![aabb([-30.0, -1.0, -30.0], [50.0, 0.0, 30.0]), aabb([0.0, 1.0, -2.0], [20.0, 2.0, 13.0])];
        for i in 0..20 {
            for k in -2..13 {
                if !cells.contains(&(i, k)) {
                    c.push(aabb([i as f32, 0.0, k as f32], [i as f32 + 1.0, 1.0, k as f32 + 1.0]));
                }
            }
        }
        World::new(&c)
    }

    /// East along z = 0 to x = 10, north to z = 10, east to the far side: two right-angle bends.
    fn bent_vent() -> (World, Vec<Vec3>) {
        let mut cells: Vec<(i32, i32)> = (0..=10).map(|i| (i, 0)).collect();
        cells.extend((1..=10).map(|k| (10, k)));
        cells.extend((11..20).map(|i| (i, 10)));
        let path = vec![Vec3::new(10.5, 0.0, 0.5), Vec3::new(10.5, 0.0, 10.5), Vec3::new(23.0, 0.0, 10.5)];
        (vent_world(&cells), path)
    }

    /// Steer along `path` with `crouch` as given; `check` sees every step. Returns the steps used.
    fn follow(w: &mut Walker, world: &World, path: &[Vec3], crouch: impl Fn(usize) -> bool, mut check: impl FnMut(usize, &Walker)) -> usize {
        let mut next = 0;
        let mut i = 0;
        while next < path.len() && i < 60 * 60 {
            let to = path[next] - w.pos;
            if Vec3::new(to.x, 0.0, to.z).length() < 0.25 {
                next += 1;
                continue;
            }
            let yaw = to.x.atan2(to.z);
            w.tick(world, &WalkInput { forward: 1.0, yaw, crouch: crouch(i), ..Default::default() });
            check(i, w);
            i += 1;
        }
        i
    }

    #[test]
    fn crouching_is_quick_on_the_eye_and_crouched_speed_is_lower() {
        let world = World::new(&[aabb([-50.0, -1.0, -50.0], [50.0, 0.0, 50.0])]);
        let mut w = walker_at(Vec3::ZERO);
        run(&mut w, &world, WalkInput::default(), 1.0);
        let (feet, eye0) = (w.pos, w.eye().y);
        let mut eyes = vec![eye0];
        for _ in 0..30 {
            w.tick(&world, &WalkInput { crouch: true, ..Default::default() });
            eyes.push(w.eye().y);
        }
        assert!(w.crouched && w.pos == feet, "the feet moved: {:?}", w.pos);
        // No snap: the first step moves the eye by under half the way; 95 percent by 0.15 s.
        let full = EYE_HEIGHT - CROUCH_EYE_HEIGHT;
        assert!(eye0 - eyes[1] < 0.5 * full, "first step {}", eye0 - eyes[1]);
        assert!(eyes[9] - (feet.y + CROUCH_EYE_HEIGHT) < 0.05 * full, "after 0.15 s the eye is at {}", eyes[9]);
        assert!(eyes.windows(2).all(|p| p[1] <= p[0]));
        // Crouched top speed, and with Shift the walk speed.
        run(&mut w, &world, crouch_east(), 1.0);
        assert!((w.vel.x - CROUCH_SPEED).abs() < 1e-3, "{}", w.vel.x);
        run(&mut w, &world, WalkInput { shift: true, ..crouch_east() }, 1.0);
        assert!((w.vel.x - WALK_SPEED).abs() < 1e-3, "{}", w.vel.x);
        // Standing again in the open: up at once, the eye easing up.
        let before = w.eye().y;
        w.tick(&world, &WalkInput { yaw: EAST, ..Default::default() });
        assert!(!w.crouched && w.eye().y - before < 0.5 * full);
        run(&mut w, &world, WalkInput::default(), 0.3);
        assert!((w.eye().y - (w.pos.y + EYE_HEIGHT)).abs() < 1e-3);
    }

    #[test]
    fn under_a_one_metre_slab_only_a_crouch_gets_through_and_the_eye_stays_under_it() {
        let slab = aabb([3.0, 1.0, -5.0], [10.0, 1.5, 5.0]);
        let world = World::new(&[aabb([-50.0, -1.0, -50.0], [50.0, 0.0, 50.0]), slab]);
        // Standing: stopped at the slab's edge.
        let mut w = walker_at(Vec3::ZERO);
        run(&mut w, &world, WalkInput { forward: 1.0, yaw: EAST, ..Default::default() }, 3.0);
        assert!(w.pos.x < 3.0 - CAPSULE_RADIUS + 0.01 && !w.crouched, "{:?}", w.pos);
        // Crouching on the run, early or at the last moment (the standing capsule's front 5 cm
        // from the slab): through, and the eye never above the slab's
        // underside while under it, even though the crouch was still easing down.
        for (start, crouch_at) in [(0.0f32, 0.0f32), (0.0, 2.3), (0.0, 2.6)] {
            let mut w = walker_at(Vec3::new(start, 0.0, 0.0));
            run(&mut w, &world, WalkInput::default(), 0.5);
            let mut under = 0;
            for _ in 0..240 {
                let crouch = w.pos.x >= crouch_at && w.pos.x < 9.0;
                w.tick(&world, &WalkInput { forward: 1.0, yaw: EAST, crouch, ..Default::default() });
                assert!(!world.overlaps_h(w.pos, w.height()), "inside the slab at {:?}", w.pos);
                if w.pos.x > 3.0 && w.pos.x < 10.0 {
                    under += 1;
                    assert!(w.crouched, "stood under the slab at {:?}", w.pos);
                    assert!(w.eye().y < 1.0 - 0.1, "eye at {} under the slab (crouch at {crouch_at})", w.eye().y);
                }
            }
            assert!(under > 30 && w.pos.x > 12.0, "crouch at {crouch_at}: ended at {:?}", w.pos);
            // The key was released at x = 9, under the slab: it stood as soon as there was room.
            assert!(!w.crouched);
        }
    }

    #[test]
    fn a_bent_vent_is_crawled_through_without_standing_jamming_or_entering_a_wall() {
        let (world, path) = bent_vent();
        // Crouch held all the way, then released half way: stays crouched inside, stands outside.
        for release in [None, Some(300usize)] {
            let mut w = walker_at(Vec3::new(-3.0, 0.0, 0.5));
            run(&mut w, &world, WalkInput::default(), 0.5);
            let mut inside = 0;
            let steps = follow(&mut w, &world, &path, |i| release.is_none_or(|r| i < r), |_, w| {
                assert!(!world.overlaps_h(w.pos, w.height()), "in a wall at {:?}", w.pos);
                assert!(w.pos.y.abs() < 0.01, "left the vent floor: {:?}", w.pos);
                let in_vent = w.pos.x > -CAPSULE_RADIUS && w.pos.x < 20.0 + CAPSULE_RADIUS;
                if in_vent {
                    inside += 1;
                    assert!(w.crouched, "stood in the vent at {:?}", w.pos);
                }
            });
            // 32 m of vent at 4 m/s, with two corners: about 8 s.
            let secs = steps as f32 * FIXED_DT;
            println!("bent vent (release {release:?}): {secs:.2} s, {inside} steps inside");
            assert!(w.pos.x > 22.0 && secs < 11.0, "stuck at {:?} after {secs} s", w.pos);
            run(&mut w, &world, WalkInput { crouch: release.is_none(), ..Default::default() }, 0.2);
            assert_eq!(w.crouched, release.is_none());
        }
    }

    #[test]
    fn crouch_spam_in_a_vent_and_under_a_slab_edge_never_pushes_into_a_wall() {
        let (world, path) = bent_vent();
        let mut w = walker_at(Vec3::new(-0.6, 0.0, 0.5));
        run(&mut w, &world, WalkInput::default(), 0.5);
        // Toggle every step, every other step, every third, with jumps mixed in.
        for period in [1usize, 2, 3] {
            let mut w2 = w;
            follow(&mut w2, &world, &path[..1], |i| (i / period) % 2 == 0, |i, w| {
                assert!(!world.overlaps_h(w.pos, w.height()), "period {period} step {i}: in a wall at {:?}, crouched {}", w.pos, w.crouched);
                assert!(w.pos.y > -0.01 && w.pos.y + w.height() < 1.0 + 0.01 || w.pos.x < -CAPSULE_RADIUS, "period {period}: out of the vent at {:?}", w.pos);
            });
        }
        // Spamming crouch and jump together while standing at the mouth.
        let mut w = walker_at(Vec3::new(-1.0, 0.0, 0.5));
        run(&mut w, &world, WalkInput::default(), 0.5);
        for i in 0..600 {
            w.tick(&world, &WalkInput { forward: 1.0, yaw: EAST, crouch: i % 2 == 0, jump: i % 3 == 0, ..Default::default() });
            assert!(!world.overlaps_h(w.pos, w.height()), "step {i}: in a wall at {:?}", w.pos);
        }
        assert!(w.deaths == 0);
    }

    #[test]
    fn jumping_crouched_in_a_vent_does_not_jam() {
        let (world, path) = bent_vent();
        let mut w = walker_at(Vec3::new(-1.0, 0.0, 0.5));
        run(&mut w, &world, WalkInput::default(), 0.5);
        follow(&mut w, &world, &path[..1], |_| true, |_, _| {});
        let (x0, mut hits) = (w.pos.x, 0);
        // Press jump repeatedly while crawling back west.
        for i in 0..120 {
            w.tick(&world, &WalkInput { forward: 1.0, yaw: -EAST, crouch: true, jump: i % 10 == 0, ..Default::default() });
            assert!(!world.overlaps_h(w.pos, w.height()), "in a wall at {:?}", w.pos);
            hits += (w.last_impact > 0.0) as u32;
        }
        assert!(w.deaths == 0 && w.jumps > 0 && hits > 0);
        assert!(x0 - w.pos.x > 5.0, "made only {} m with jumps", x0 - w.pos.x);
        run(&mut w, &world, WalkInput { crouch: true, ..Default::default() }, 0.5);
        assert!(w.grounded && w.crouched);
    }

    /// A wall at x = 3..20 with a vent mouth whose floor is `lip` above the ground, 1 m by 1 m.
    fn mouth_world(lip: f32) -> World {
        World::new(&[
            aabb([-50.0, -1.0, -50.0], [50.0, 0.0, 50.0]),
            aabb([3.0, 0.0, -5.0], [20.0, lip, 5.0]),
            aabb([3.0, lip + 1.0, -5.0], [20.0, 6.0, 5.0]),
            aabb([3.0, lip, -5.0], [20.0, lip + 1.0, -0.5]),
            aabb([3.0, lip, 0.5], [20.0, lip + 1.0, 5.0]),
        ])
    }

    #[test]
    fn a_vent_mouth_045_up_is_stepped_into_crouched() {
        let world = mouth_world(0.45);
        for shift in [false, true] {
            let mut w = walker_at(Vec3::ZERO);
            run(&mut w, &world, WalkInput::default(), 0.5);
            run(&mut w, &world, WalkInput { shift, ..crouch_east() }, 5.0);
            assert!(w.pos.x > 10.0 && (w.pos.y - 0.45).abs() < 0.02 && w.grounded, "shift {shift}: {:?}", w.pos);
        }
        // Standing it is not entered.
        let mut w = walker_at(Vec3::ZERO);
        run(&mut w, &world, WalkInput { forward: 1.0, yaw: EAST, ..Default::default() }, 3.0);
        assert!(w.pos.x < 3.0, "{:?}", w.pos);
    }

    #[test]
    fn a_vent_mouth_14_up_is_reached_by_a_running_jump_and_a_crouch() {
        let world = mouth_world(1.4);
        // Crouch at take-off, near the apex, or before the run; jump from 0.3 to 1.5 m short.
        let mut ok = 0;
        let mut tried = 0;
        for crouch_after in [0usize, 10, 20] {
            for short in [0.3f32, 0.6, 1.0, 1.5] {
                let mut w = walker_at(Vec3::ZERO);
                run(&mut w, &world, WalkInput::default(), 0.5);
                while w.pos.x < 3.0 - CAPSULE_RADIUS - short {
                    w.tick(&world, &WalkInput { forward: 1.0, yaw: EAST, ..Default::default() });
                }
                w.tick(&world, &WalkInput { forward: 1.0, yaw: EAST, jump: true, ..Default::default() });
                for i in 0..120 {
                    w.tick(&world, &WalkInput { forward: 1.0, yaw: EAST, crouch: i >= crouch_after, ..Default::default() });
                    assert!(!world.overlaps_h(w.pos, w.height()), "in a wall at {:?}", w.pos);
                }
                tried += 1;
                let inside = w.pos.x > 5.0 && (w.pos.y - 1.4).abs() < 0.02 && w.crouched;
                println!("lip 1.4: crouch {crouch_after} steps after take-off, jump {short} m short: {}", if inside { "in" } else { "out" });
                ok += inside as u32;
                assert!(w.deaths == 0);
            }
        }
        // Crouched before the jump (the run is crouched too).
        let mut w = walker_at(Vec3::ZERO);
        run(&mut w, &world, WalkInput::default(), 0.5);
        while w.pos.x < 3.0 - CAPSULE_RADIUS - 0.8 {
            w.tick(&world, &crouch_east());
        }
        w.tick(&world, &WalkInput { jump: true, ..crouch_east() });
        run(&mut w, &world, crouch_east(), 2.0);
        assert!(w.pos.x > 5.0 && (w.pos.y - 1.4).abs() < 0.02, "crouched jump: {:?}", w.pos);
        assert!(ok * 4 >= tried * 3, "only {ok} of {tried} running jumps got in");
    }

    #[test]
    fn standing_up_in_the_air_brings_the_feet_down_and_the_eye_does_not_jump() {
        let world = World::new(&[aabb([-50.0, -1.0, -50.0], [50.0, 0.0, 50.0])]);
        let mut w = walker_at(Vec3::ZERO);
        run(&mut w, &world, WalkInput::default(), 0.5);
        w.tick(&world, &WalkInput { jump: true, ..Default::default() });
        let mut prev = w.eye().y;
        for i in 0..80 {
            let crouch = (5..25).contains(&i);
            let feet = w.pos.y;
            w.tick(&world, &WalkInput { crouch, ..Default::default() });
            // The feet lift by 0.9 m on the crouch and drop back on the release, the eye follows
            // the jump smoothly.
            if i == 5 {
                assert!(w.pos.y - feet > 0.8, "feet did not lift: {} -> {}", feet, w.pos.y);
            }
            assert!((w.eye().y - prev).abs() < 0.2, "eye jumped {} at step {i}", w.eye().y - prev);
            prev = w.eye().y;
        }
        assert!(w.grounded && !w.crouched && w.pos.y.abs() < 0.01);
    }

    #[test]
    fn a_return_to_a_spot_in_a_vent_is_crouched() {
        let (world, path) = bent_vent();
        let mut w = walker_at(Vec3::new(-1.0, 0.0, 0.5));
        run(&mut w, &world, WalkInput::default(), 0.5);
        follow(&mut w, &world, &path[..1], |_| true, |_, _| {});
        run(&mut w, &world, WalkInput { crouch: true, ..Default::default() }, 1.0);
        assert!(w.stable_crouched && w.stable_pos.x > 5.0, "stable spot {:?}", w.stable_pos);
        run(&mut w, &world, WalkInput { forward: 1.0, yaw: -EAST, crouch: true, ..Default::default() }, 0.3);
        // The spot follows the crawling player (always on the ground there).
        let spot = w.stable_pos;
        assert!(w.stable_crouched && spot.x > 5.0);
        w.reset();
        // R with the key up: back in the vent, crouched, and not able to stand there.
        run(&mut w, &world, WalkInput::default(), 0.5);
        assert!(w.crouched && w.pos == spot && !world.overlaps_h(w.pos, w.height()), "crouched {} at {:?}, spot {spot:?}, stable_crouched {}", w.crouched, w.pos, w.stable_crouched);
    }

    #[test]
    fn arriving_in_a_vent_from_the_fly_camera_crouches() {
        let (world, _) = bent_vent();
        let mut w = walker_at(Vec3::new(-3.0, 0.0, 0.5));
        w.place_eye(Vec3::new(5.0, EYE_HEIGHT, 0.5));
        let at = w.pos;
        run(&mut w, &world, WalkInput::default(), 0.5);
        assert!(w.crouched && (w.pos - at).length() < 0.01 && !world.overlaps_h(w.pos, w.height()), "{:?} crouched {}", w.pos, w.crouched);
    }

    #[test]
    fn crawling_is_deterministic() {
        let (world, path) = bent_vent();
        let go = || {
            let mut w = walker_at(Vec3::new(-3.0, 0.0, 0.5));
            run(&mut w, &world, WalkInput::default(), 0.5);
            follow(&mut w, &world, &path, |i| i % 7 != 0, |_, _| {});
            w
        };
        let (a, b) = (go(), go());
        assert_eq!(a.pos.to_array().map(f32::to_bits), b.pos.to_array().map(f32::to_bits));
        assert_eq!(a.vel.to_array().map(f32::to_bits), b.vel.to_array().map(f32::to_bits));
    }
}
