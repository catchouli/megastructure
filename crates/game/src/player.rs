//! Walking-controller constants, all in one place. The values marked "contract" in
//! docs/features/player-movement.md are relied on by the generator; the rest are tuning.
//!
//! The movement model follows Quake 2's player movement (friction with a stop-speed floor,
//! acceleration limited by the speed already along the wanted direction, the same rule in the
//! air, an immediate jump, step-and-slide moves). Its constants are converted from Quake 2 units
//! with `QUAKE_UNIT`: the Quake 2 player is 56 units tall and this one 1.8 m, so a unit is
//! 1.8 / 56 = 32.1 mm and the proportions to the body are Quake's. Times are unchanged by the
//! scale: Quake's 0.68 s jump is 0.68 s here.

// --- The body (contracts for the generator) --------------------------------------------------

/// Capsule height, metres (contract: minimum door and corridor clearance).
pub const CAPSULE_HEIGHT: f32 = 1.8;
/// Capsule radius, metres (contract).
pub const CAPSULE_RADIUS: f32 = 0.3;
/// Eye height above the feet, metres (contract: railing and sight-line heights).
pub const EYE_HEIGHT: f32 = 1.65;
/// Largest ledge stepped up without jumping, metres (contract). Quake 2's 18 of 56 units would
/// be 0.58 m; 0.45 m (a quarter of the body) is enough for any stair, kerb or low rubble and
/// keeps a hip-high block a thing to jump onto.
pub const STEP_HEIGHT: f32 = 0.45;
/// Steepest slope that can be walked up or stood on, degrees (contract). Quake 2 uses a normal
/// of 0.7 (45.6 degrees); the generator's contract of 40 is kept.
pub const MAX_SLOPE_DEG: f32 = 40.0;

// --- Crouching (contracts for the generator's vents) -----------------------------------------

/// Capsule height while crouched, metres (contract: a vent's interior is at least 1.0 m high
/// and 1.0 m wide). Quake 2's duck is 32 of 56 units; this is half the body.
pub const CROUCH_HEIGHT: f32 = 0.9;
/// Eye height above the feet while crouched, metres: 0.15 m under the top, as standing.
pub const CROUCH_EYE_HEIGHT: f32 = 0.75;
/// Top speed crouched, m/s (Quake 2 halves the wanted speed: 4.8 m/s; a little slower reads as
/// crawling but still crosses 40 m of vent in 10 s). With Shift it is the walk speed.
pub const CROUCH_SPEED: f32 = 4.0;
/// Rate at which the eye follows a crouch or a stand, 1/s (exponential: 95 percent in 0.12 s).
pub const CROUCH_EYE_RATE: f32 = 25.0;

// --- Quake 2 movement ------------------------------------------------------------------------

/// Metres per Quake 2 unit (player height 56 units scaled to `CAPSULE_HEIGHT`).
pub const QUAKE_UNIT: f32 = CAPSULE_HEIGHT / 56.0;
/// Top ground speed when running, m/s (Quake 2 `pm_maxspeed` 300 units/s: 9.64 m/s).
pub const RUN_SPEED: f32 = 300.0 * QUAKE_UNIT;
/// Top ground speed when walking, m/s (not Quake's: a slow pace for looking around).
pub const WALK_SPEED: f32 = 3.0;
/// Running is the default and Shift walks, as in Quake with always-run on. False swaps them
/// back (walk by default, Shift runs).
pub const ALWAYS_RUN: bool = true;
/// Ground acceleration, per second, times the wanted speed (Quake 2 `pm_accelerate` 10): up to
/// 96 m/s^2 at run speed, less the friction.
pub const GROUND_ACCEL: f32 = 10.0;
/// Air acceleration, per second, times the wanted speed (Quake 2 uses its ground rule with 1 in
/// the air): about 9.6 m/s^2 of steering, enough to turn in flight and to gain speed by
/// strafe-jumping.
pub const AIR_ACCEL: f32 = 1.0;
/// Ground friction, per second (Quake 2 `pm_friction` 6): above `STOP_SPEED` the speed decays
/// exponentially at this rate.
pub const FRICTION: f32 = 6.0;
/// Below this speed friction brakes as if at this speed, so the last bit stops quickly and
/// linearly, m/s (Quake 2 `pm_stopspeed` 100 units/s).
pub const STOP_SPEED: f32 = 100.0 * QUAKE_UNIT;
/// Vertical speed at take-off, m/s (Quake 2 270 units/s: 8.68 m/s, apex about 1.44 m with
/// drag, 0.67 s in the air).
pub const JUMP_SPEED: f32 = 270.0 * QUAKE_UNIT;
/// Holding Space hops again on every landing (true) or a jump needs a fresh press (false, as
/// Quake 2). With `JUMP_BUFFER`, a press just before landing still jumps with no lost speed.
pub const AUTO_HOP: bool = false;
/// Touching walkable ground while rising slower than this is a landing, m/s (Quake 2 counts the
/// player as on the ground below 180 units/s up: 5.79 m/s). Without it a hop up a slope meets the
/// slope still rising and slides up it, airborne, instead of landing.
pub const LAND_MAX_RISE: f32 = 180.0 * QUAKE_UNIT;
/// Gravity, m/s^2 (Quake 2 `sv_gravity` 800 units/s^2: 25.7 m/s^2, about 2.6 g).
pub const GRAVITY: f32 = 800.0 * QUAKE_UNIT;

// --- Falling ---------------------------------------------------------------------------------

/// Terminal velocity of a fall, m/s. Drag is `k * v * |v|` with `k = GRAVITY / TERMINAL_VELOCITY^2`.
pub const TERMINAL_VELOCITY: f32 = 55.0;
/// A fall from rest of this height or more ends in a fatal landing, metres. The impact speed is
/// derived from it (`fatal_impact_speed`), so it stays right whatever the gravity.
pub const FATAL_FALL_HEIGHT: f32 = 10.0;
/// Time on the ground, continuously, before a spot counts as stable ground to return to, seconds.
pub const STABLE_GROUND_TIME: f32 = 0.5;

/// A spot only becomes the stable spot to return to if ground (or a wall) lies under a ring of
/// points this far from the capsule's axis, metres. Wider than the capsule, so the return point is
/// at least 0.6 m from any lip (16 samples leave a worst case of 0.65 cos 11 degrees = 0.64 m).
pub const STABLE_RING_RADIUS: f32 = 0.65;
/// Points on that ring.
pub const STABLE_RING_SAMPLES: usize = 16;
/// How far above and below the feet the ground under a ring point may be, metres.
pub const STABLE_RING_REACH: f32 = 0.5;
/// After a death or a reset, movement keys are ignored for this long, seconds.
pub const RESPAWN_LOCKOUT: f32 = 0.3;
/// A jump pressed this long before touching down still happens on landing, seconds.
pub const JUMP_BUFFER: f32 = 0.1;

/// Fixed simulation step, seconds. The walker is only ever advanced in steps of exactly this.
pub const FIXED_DT: f32 = 1.0 / 60.0;
/// Most fixed steps run for one rendered frame; further time is dropped so a stall cannot spiral.
pub const MAX_TICKS_PER_FRAME: u32 = 8;

// --- Numerical -------------------------------------------------------------------------------

/// Gap kept between the capsule and a surface, metres. Shape casts stop this far short, so
/// the capsule is never exactly touching or overlapping a collider.
pub const SKIN: f32 = 0.004;
/// How far below the feet ground is looked for to stay stuck to it, metres. Equal to the
/// step height, so a step down of up to that does not leave the ground.
pub const SNAP_DISTANCE: f32 = STEP_HEIGHT;
/// When a step-up is the only way forward, the forward probe is at least this long, metres, so
/// the capsule ends up far enough over the edge to be standing on it and not on the corner.
pub const STEP_MIN_FORWARD: f32 = 0.15;
/// When a step-up raised as far as there is room is blocked ahead, it is retried this much lower
/// each time, metres: finer than the 0.1 m of play between a crouched capsule and a vent mouth.
pub const STEP_RETRY: f32 = 0.05;
/// Speeds below this are zero on the ground, m/s (Quake 2: 1 unit/s).
pub const REST_SPEED: f32 = QUAKE_UNIT;
/// Collide-and-slide iterations per move (Quake 2 `MAX_CLIP_PLANES` 5).
pub const MAX_SLIDE_ITERATIONS: usize = 5;
/// Rate at which the eye catches up after a step up or down, 1/s (exponential).
pub const EYE_SMOOTH_RATE: f32 = 14.0;
/// A snap down by more than this beyond what the slope under the feet accounts for, metres, is
/// a step and is smoothed in the eye; slopes and stairs are followed exactly.
pub const EYE_SMOOTH_MIN_DROP: f32 = 0.12;

/// `cos(MAX_SLOPE_DEG)`: a surface is walkable when its normal's Y is at least this.
pub fn min_ground_y() -> f32 {
    MAX_SLOPE_DEG.to_radians().cos()
}

/// Drag coefficient `k` in `a = g - k * v * |v|`.
pub const DRAG_K: f32 = GRAVITY / (TERMINAL_VELOCITY * TERMINAL_VELOCITY);

/// Landing along the surface normal faster than this kills, m/s: the speed after a fall of
/// `FATAL_FALL_HEIGHT` from rest under gravity and drag, `v_t sqrt(1 - exp(-2 g h / v_t^2))`
/// (21.7 m/s with the values above).
pub fn fatal_impact_speed() -> f32 {
    let vt = TERMINAL_VELOCITY;
    vt * (1.0 - (-2.0 * GRAVITY * FATAL_FALL_HEIGHT / (vt * vt)).exp()).sqrt()
}
