//! Walking-controller constants, all in one place. The values marked "contract" in
//! docs/features/player-movement.md are relied on by the generator; the rest are tuning.

/// Capsule height, metres (contract: minimum door and corridor clearance).
pub const CAPSULE_HEIGHT: f32 = 1.8;
/// Capsule radius, metres (contract).
pub const CAPSULE_RADIUS: f32 = 0.3;
/// Eye height above the feet, metres (contract: railing and sight-line heights).
pub const EYE_HEIGHT: f32 = 1.65;
/// Walking speed, m/s.
pub const WALK_SPEED: f32 = 2.5;
/// Running speed (Shift), m/s.
pub const RUN_SPEED: f32 = 6.0;
/// Largest ledge stepped up without jumping, metres (contract).
pub const STEP_HEIGHT: f32 = 0.3;
/// Steepest slope that can be walked up or stood on, degrees (contract).
pub const MAX_SLOPE_DEG: f32 = 40.0;
/// Vertical speed at take-off, m/s (apex about 0.52 m).
pub const JUMP_SPEED: f32 = 3.2;
/// Gravity, m/s^2.
pub const GRAVITY: f32 = 9.81;
/// Terminal velocity of a fall, m/s. Drag is `k * v * |v|` with `k = GRAVITY / TERMINAL_VELOCITY^2`.
pub const TERMINAL_VELOCITY: f32 = 55.0;
/// Landing along the surface normal faster than this kills, m/s (a drop of about 10 m).
pub const FATAL_IMPACT_SPEED: f32 = 14.0;
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

/// Ground acceleration towards the wanted velocity, m/s^2 (full walk speed in 0.08 s).
pub const GROUND_ACCEL: f32 = 30.0;
/// Ground braking when no direction is held, m/s^2.
pub const GROUND_DECEL: f32 = 45.0;
/// Air control: acceleration along the wanted direction, up to the wanted speed, m/s^2.
pub const AIR_ACCEL: f32 = 6.0;

/// Gap kept between the capsule and a surface, metres. Shape casts stop this far short, so
/// the capsule is never exactly touching or overlapping a collider.
pub const SKIN: f32 = 0.004;
/// How far below the feet ground is looked for to stay stuck to it, metres. Equal to the
/// step height, so a step down of up to 0.3 m does not leave the ground.
pub const SNAP_DISTANCE: f32 = STEP_HEIGHT;
/// When a step-up is the only way forward, the forward probe is at least this long, metres, so
/// the capsule ends up far enough over the edge to be standing on it and not on the corner.
pub const STEP_MIN_FORWARD: f32 = 0.15;
/// Collide-and-slide iterations per move.
pub const MAX_SLIDE_ITERATIONS: usize = 5;
/// Rate at which the eye catches up after a step up or down, 1/s (exponential).
pub const EYE_SMOOTH_RATE: f32 = 14.0;
/// A snap down by more than this, metres, is a step and is smoothed in the eye; smaller ones
/// are slopes and stairs, which the eye follows exactly.
pub const EYE_SMOOTH_MIN_DROP: f32 = 0.12;

/// `cos(MAX_SLOPE_DEG)`: a surface is walkable when its normal's Y is at least this.
pub fn min_ground_y() -> f32 {
    MAX_SLOPE_DEG.to_radians().cos()
}

/// Drag coefficient `k` in `a = g - k * v * |v|`.
pub const DRAG_K: f32 = GRAVITY / (TERMINAL_VELOCITY * TERMINAL_VELOCITY);
