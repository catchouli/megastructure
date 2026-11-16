//! The walker in the real shaft scene, driven by the same scripts the headless runner takes.
//! No GPU: `Sim` is stepped directly at 1/60 s with `Script::input_at`, which is exactly what
//! `megastructure --headless --input ...` does each frame before it renders.
//!
//! The route (docs/features/one-shaft.md): spawn on the west gallery at y = 400, north along the
//! gallery to bridge B1 (z = 21), east along the bridge to the landing on the blank wall, south up
//! the 126-riser stair to the ledge at the top (y = 421.42), west along the ledge to the arch,
//! south through the arch into the hall. Yaw 0 faces +Z, positive yaw turns towards +X; an
//! arrow-key turn is exactly 90 degrees per second. Running is the default; `walk` is Shift.
//! The player coasts for up to 1.2 m (friction) while a script turns after running.

use crate::headless::state_line;
use crate::player::*;
use crate::script::Script;
use crate::sim::{MoveMode, Sim};
use glam::Vec3;
use mega_gen::scenes;

const DT: f32 = 1.0 / 60.0;

/// Spawn to the hall at run speed, `idle` first so the spawn is seen at rest; the gallery is
/// walked so the turn onto the bridge is in the middle of it. Ends in the hall at about
/// (-1.8, 421.42, -102).
pub const ROUTE_RUN: &str = "idle:1,look_right:1,walk+forward:3.2,look_left:1,forward:6.5,look_left:1,forward:6.5,look_left:1,forward:3.2,look_right:1,forward:7.6";

/// The same route at walking speed (the stair takes about 19 s on its own).
pub const ROUTE_WALK: &str = "idle:1,look_right:1,walk+forward:3.2,look_left:1,walk+forward:21,look_left:1,walk+forward:24,look_left:1,walk+forward:10.2,look_right:1,walk+forward:25";

/// Largest eye movement in one step on the stair (29.5 degrees, climbed at cos^2 of the speed
/// and descended at the full speed), with a quarter for the capsule's round foot.
fn stair_eye_step() -> f32 {
    RUN_SPEED * (0.17 / 0.30) * DT * 1.25 + 0.002
}

fn shaft() -> scenes::Scene {
    scenes::by_name("shaft").expect("shaft scene")
}

/// Run `script` on `sim` at 1/60 s from `first_frame`, calling `watch` after every frame.
fn drive(sim: &mut Sim, script: &str, mut watch: impl FnMut(u64, &Sim)) -> u64 {
    let s = Script::parse(script).unwrap();
    let frames = s.total_frames(DT);
    for i in 0..frames {
        sim.update(DT, &s.input_at(i, DT));
        watch(i, sim);
    }
    frames
}

fn hall_floor() -> f32 {
    mega_gen::shaft::hall_floor()
}

#[test]
fn the_route_from_spawn_to_the_hall_stays_on_the_ground() {
    for (script, name) in [(ROUTE_RUN, "run"), (ROUTE_WALK, "walk")] {
        let scene = shaft();
        let mut sim = Sim::for_scene(&scene, None);
        assert_eq!(sim.mode, MoveMode::Walk);
        // The eye starts at the scene's spawn.
        assert!((sim.walker().unwrap().eye() - Vec3::from(scene.spawn)).length() < 1e-4);
        let mut airborne = 0;
        let mut lowest = f32::MAX;
        let mut at_landing = None;
        let mut stair_top = None;
        let mut prev_eye = sim.camera.position.y;
        let mut eye_jump = 0.0f32;
        let frames = drive(&mut sim, script, |i, s| {
            let w = s.walker().unwrap();
            airborne += (!w.grounded) as u32;
            lowest = lowest.min(w.pos.y);
            assert_eq!(w.deaths, 0, "{name}: died at frame {i}, feet {:?}", w.pos);
            // Eye height: never a jump bigger than the stair slope allows at running speed.
            eye_jump = eye_jump.max((s.camera.position.y - prev_eye).abs());
            prev_eye = s.camera.position.y;
            if at_landing.is_none() && w.pos.x > 29.5 && w.pos.z > 20.0 {
                at_landing = Some(w.pos);
            }
            if stair_top.is_none() && (w.pos.y - hall_floor()).abs() < 0.01 && w.pos.z < -26.4 {
                stair_top = Some((i, w.pos));
            }
        });
        let w = sim.walker().unwrap();
        assert_eq!(airborne, 0, "{name}: left the ground for {airborne} frames");
        assert!(lowest > 399.99, "{name}: sank to {lowest}");
        let landing = at_landing.expect("reached the end of bridge B1");
        assert!((landing.y - 400.0).abs() < 0.02, "{name}: bridge at {landing:?}");
        let (_, top) = stair_top.expect("reached the top of the stair");
        assert!((top.y - hall_floor()).abs() < 0.01 && hall_floor() > 421.4 && hall_floor() < 421.45, "{name}: stair top {top:?}");
        assert!(eye_jump < stair_eye_step(), "{name}: eye moved {eye_jump} in one frame");
        // In the hall, on its floor, well inside it.
        assert!((w.pos.y - hall_floor()).abs() < 0.01 && w.pos.z < -50.0 && w.pos.x.abs() < 5.0, "{name}: ended at {:?}", w.pos);
        assert!(w.grounded && w.deaths == 0);
        let line = state_line(&sim, frames);
        assert!(line.contains("mode=walk grounded=true") && line.contains("died=false deaths=0"), "{line}");
        if name == "run" {
            // The headless output of this script (`megastructure --headless --scene shaft --input
            // ...`), to the digit: any change to the movement shows here.
            assert_eq!(line, "state frames=1980 x=-1.8345 y=423.0740 z=-102.4368 yaw_deg=180.000 pitch_deg=0.000 mode=walk grounded=true vy=0.0000 died=false deaths=0 resets=0 hspeed=9.6429");
        }
    }
}

#[test]
fn the_stair_is_smooth_up_and_down_at_run_and_at_walk() {
    let scene = shaft();
    let climb = "idle:1,look_right:1,walk+forward:3.2,look_left:1,forward:6.5,look_left:1,forward:6.3";
    let walk_climb = "idle:1,look_right:1,walk+forward:3.2,look_left:1,forward:6.5,look_left:1,walk+forward:30";
    let limit = stair_eye_step();
    for (script, down, name) in [(climb, "look_left:2,forward:8", "run"), (walk_climb, "look_left:2,walk+forward:30", "walk")] {
        let mut sim = Sim::for_scene(&scene, None);
        // Up: from the foot of the stair (z = 14) to near the top, the eye only rises, in small steps.
        let mut eyes: Vec<(f32, f32)> = Vec::new();
        drive(&mut sim, script, |_, s| eyes.push((s.camera.position.z, s.camera.position.y)));
        let rising: Vec<f32> = eyes.iter().filter(|(z, _)| *z < 14.0 && *z > -26.0).map(|e| e.1).collect();
        assert!(rising.len() > 100, "{name}: only {} frames on the stair", rising.len());
        for p in rising.windows(2) {
            assert!(p[1] - p[0] > -0.0015, "{name} up: eye dropped {}", p[0] - p[1]);
            assert!(p[1] - p[0] < limit, "{name} up: eye jumped {}", p[1] - p[0]);
        }
        assert!(rising.last().unwrap() - rising.first().unwrap() > 20.0);
        // Down again: the capsule never leaves the ground and the eye only falls.
        let mut airborne = 0;
        let mut eyes: Vec<(f32, f32)> = Vec::new();
        drive(&mut sim, down, |_, s| {
            airborne += (!s.walker().unwrap().grounded) as u32;
            eyes.push((s.camera.position.z, s.camera.position.y));
        });
        let falling: Vec<f32> = eyes.iter().filter(|(z, _)| *z < 14.0 && *z > -26.0).map(|e| e.1).collect();
        assert!(falling.len() > 100, "{name} down: only {} frames on the stair", falling.len());
        for p in falling.windows(2) {
            assert!(p[1] - p[0] < 0.0015, "{name} down: eye rose {}", p[1] - p[0]);
            assert!(p[1] - p[0] > -limit, "{name} down: eye dropped {}", p[0] - p[1]);
        }
        assert_eq!(airborne, 0, "{name}: skipped off the stair {airborne} frames");
        let w = sim.walker().unwrap();
        assert!((w.pos.y - 400.0).abs() < 0.02, "{name}: back at {:?}", w.pos);
    }
}

/// Bridge B1 has railings on both sides, so there is no edge to walk off. The script goes out
/// onto the bridge, switches to the fly camera for a moment to move over the void beside it
/// (stepping over the rail), and switches back to walking: the player drops.
pub const FALL_FROM_BRIDGE: &str = "idle:1,look_right:1,walk+forward:3.2,look_left:1,forward:0.6,toggle_fly:0.02,right:0.5,toggle_fly:0.02,idle:16";

#[test]
fn stepping_off_the_bridge_is_a_long_fall_and_a_fatal_landing_at_the_bottom() {
    let scene = shaft();
    let mut sim = Sim::for_scene(&scene, None);
    let mut on_bridge = None;
    let mut top_speed = 0.0f32;
    let mut death_frame = None;
    let mut lowest = f32::MAX;
    let mut fall_frames = 0;
    let frames = drive(&mut sim, FALL_FROM_BRIDGE, |i, s| {
        let w = s.walker().unwrap();
        if s.mode == MoveMode::Fly && on_bridge.is_none() {
            on_bridge = Some(w.pos);
        }
        top_speed = top_speed.max(-w.vel.y);
        if !w.grounded && s.mode == MoveMode::Walk {
            fall_frames += 1;
            lowest = lowest.min(w.pos.y);
        }
        if w.deaths == 1 && death_frame.is_none() {
            death_frame = Some(i);
        }
    });
    let w = sim.walker().unwrap();
    let on_bridge = on_bridge.unwrap();
    assert!((on_bridge.y - 400.0).abs() < 0.02 && on_bridge.x > -28.0 && on_bridge.x < -22.0 && (on_bridge.z - 21.5).abs() < 0.1, "left the bridge at {on_bridge:?}");
    // 400 m under 25.7 m/s^2 with drag is 8.8 s (`(v_t^2 / g) ln cosh(g t / v_t)`); it died at the bottom.
    assert!(lowest < 5.0, "only fell to {lowest}");
    assert!((8.3..9.3).contains(&(fall_frames as f32 / 60.0)), "fell for {} s", fall_frames as f32 / 60.0);
    assert!(top_speed > 50.0 && top_speed <= TERMINAL_VELOCITY, "top speed {top_speed}");
    assert_eq!((w.deaths, w.resets), (1, 0));
    // Returned to the bridge, where it had stood: the stable spot is where it switched to flying.
    assert!(w.grounded && (w.pos - on_bridge).length() < 0.01, "respawned at {:?}, left from {on_bridge:?}", w.pos);
    let line = state_line(&sim, frames);
    assert!(line.contains("mode=walk grounded=true") && line.contains("died=true deaths=1 resets=0"), "{line}");
}

/// Out over the rim and beyond the shaft's walls nothing lies underneath.
pub const FALL_INTO_THE_VOID: &str = "toggle_fly:0.02,fast+forward:3,toggle_fly:0.02,idle:20";

#[test]
fn falling_beside_the_structure_never_ends_and_nothing_resets_it() {
    let scene = shaft();
    let mut sim = Sim::for_scene(&scene, None);
    let mut last_y = f32::MAX;
    let mut walking_frames = 0;
    let frames = drive(&mut sim, FALL_INTO_THE_VOID, |_, s| {
        if s.mode == MoveMode::Walk {
            walking_frames += 1;
            let w = s.walker().unwrap();
            assert!(w.pos.y < last_y, "position went up while falling");
            assert!(!w.grounded && w.deaths == 0 && w.resets == 0);
            last_y = w.pos.y;
        }
    });
    let w = sim.walker().unwrap();
    assert!(walking_frames > 19 * 60);
    assert!(w.pos.x > 80.0, "fell at x = {}", w.pos.x);
    assert!(w.pos.y < -450.0, "only fell to {}", w.pos.y);
    assert!(w.vel.y < -54.5 && w.vel.y >= -TERMINAL_VELOCITY, "vy {}", w.vel.y);
    let line = state_line(&sim, frames);
    assert!(line.contains("mode=walk grounded=false vy=-5") && line.contains("died=false deaths=0"), "{line}");
}

#[test]
fn bridge_b1_has_ground_under_every_quarter_metre() {
    // A regression guard for the generator: a straight ray down the middle of the bridge, every
    // 0.25 m from the gallery to the landing, finds its deck.
    let scene = shaft();
    let world = crate::world::World::new(&scene.colliders);
    for i in 0..=(58.0 / 0.25) as usize {
        let x = -29.0 + i as f32 * 0.25;
        assert!(world.supported_below(Vec3::new(x, 400.0, 21.0), 0.05), "no ground on bridge B1 at x = {x}");
    }
}

#[test]
fn the_feet_follow_the_generators_stair_surface_over_all_three_flights_and_both_landings() {
    // `stair_surface(z)` is what the camera paths use. A rolling capsule rides up to 0.046 m above
    // a 29.5 degree surface (its rounded foot) plus the skin; a ramp that did not meet its landing
    // would show as a step or a dip.
    let scene = shaft();
    for (stair, frames) in [("walk+forward:30", 500), ("forward:8", 200)] {
        let mut sim = Sim::for_scene(&scene, None);
        let (mut max, mut min, mut n) = (f32::MIN, f32::MAX, 0);
        drive(&mut sim, &format!("idle:1,look_right:1,walk+forward:3.2,look_left:1,forward:6.5,look_left:1,{stair}"), |_, s| {
            let w = s.walker().unwrap();
            if w.pos.z < 14.0 && w.pos.z > -26.4 && w.pos.x > 29.0 {
                let d = w.pos.y - mega_gen::shaft::stair_surface(w.pos.z);
                (max, min, n) = (max.max(d), min.min(d), n + 1);
            }
        });
        assert!(n > frames, "{stair}: only {n} frames on the stair");
        assert!(max < 0.06 && min > -0.005, "{stair}: feet were {min}..{max} above the stair surface");
    }
}

#[test]
fn on_the_stair_the_stable_spot_follows_the_player_and_r_returns_close_by() {
    // The stair lane is 1.5 m wide; the ring check (0.65 m) must still count it as ground. Walk
    // half way up, stand, press R: back within a metre, on the stair, not at the bridge.
    let scene = shaft();
    let mut sim = Sim::for_scene(&scene, None);
    drive(&mut sim, "idle:1,look_right:1,walk+forward:3.2,look_left:1,forward:6.5,look_left:1,forward:2.6,idle:1", |_, _| {});
    let before = sim.walker().unwrap().pos;
    assert!(before.y > 405.0 && before.y < 415.0, "not half way up the stair: {before:?}");
    drive(&mut sim, "reset:0.02,idle:0.5", |_, _| {});
    let after = sim.walker().unwrap();
    assert_eq!(after.resets, 1);
    assert!((after.pos - before).length() < 1.0 && after.grounded, "{:?} vs {before:?}", after.pos);
}


/// `forward` for `seconds` with a jump press every `every` seconds (held for one step), as a
/// script: a player hopping as soon as they land, helped by the jump buffer.
fn hopping(seconds: f32, every: f32) -> String {
    let n = (seconds / every).round() as usize;
    vec![format!("jump+forward:{DT},forward:{}", every - DT); n].join(",")
}

#[test]
fn hopping_along_the_bridge_and_up_and_down_the_stair_is_safe() {
    // Bunny-hop the route from the bridge to the top of the stair and back down, pressing jump
    // every 0.1 s: no death, no catch on the stair, no launch off it, and the player arrives.
    let scene = shaft();
    let mut sim = Sim::for_scene(&scene, None);
    drive(&mut sim, "idle:1,look_right:1,walk+forward:3.2,look_left:1", |_, _| {});
    let mut top_speed = 0.0f32;
    let mut highest_rise = 0.0f32;
    let mut ground_y = sim.walker().unwrap().pos.y;
    let mut watch = |_, s: &Sim| {
        let w = s.walker().unwrap();
        assert_eq!(w.deaths, 0, "died hopping at {:?}", w.pos);
        top_speed = top_speed.max(Vec3::new(w.vel.x, 0.0, w.vel.z).length());
        if w.grounded {
            ground_y = w.pos.y;
        }
        highest_rise = highest_rise.max(w.pos.y - ground_y);
    };
    drive(&mut sim, &hopping(7.0, 0.1), &mut watch);
    let w = sim.walker().unwrap();
    assert!(w.pos.x > 29.0 && (w.pos.y - 400.0).abs() < 1.5, "hopped along the bridge to {:?}", w.pos);
    let bridge_jumps = w.jumps;
    assert!(bridge_jumps >= 8, "only {bridge_jumps} hops on the bridge");
    drive(&mut sim, &format!("look_left:1,{}", hopping(10.0, 0.1)), &mut watch);
    let w = sim.walker().unwrap();
    assert!(w.pos.z < -26.5 && (w.pos.y - hall_floor()).abs() < 1.5, "hopped up the stair to {:?}", w.pos);
    // Down: hop to the foot of the stair and stop hopping on the landing (hopping on into the
    // landing's end railing clears it: see the next test).
    drive(&mut sim, &format!("idle:1,look_left:2,{},idle:1", hopping(4.8, 0.1)), &mut watch);
    let w = sim.walker().unwrap();
    assert!(w.pos.z > 14.0 && (w.pos.y - 400.0).abs() < 0.02 && w.grounded, "hopped down the stair to {:?}", w.pos);
    // Hopping straight on does not gain speed (only strafe-jumping does); the drag of the air
    // takes a little.
    assert!(top_speed <= RUN_SPEED + 1e-3, "top speed {top_speed}");
    // A hop is a jump: up to its apex above the take-off, and no higher (no launch off a ridge).
    assert!(highest_rise < 1.46, "rose {highest_rise} m above the last ground");
    println!("hopping: {} hops in all, highest rise {highest_rise:.3} m", w.jumps);
}

#[test]
fn the_bridge_railing_can_be_jumped_and_that_is_a_fatal_fall_back_to_the_bridge() {
    // The 1.1 m railings are lower than a running jump (apex 1.45 m), so the shaft's edges are
    // open to a deliberate jump: out over B1's north railing, 400 m down, back on the bridge.
    let scene = shaft();
    let mut sim = Sim::for_scene(&scene, None);
    drive(&mut sim, "idle:1,look_right:1,walk+forward:3.2,look_left:1,forward:0.6,idle:0.5,look_right:1,walk+forward:0.5,idle:0.6", |_, _| {});
    let before = sim.walker().unwrap().pos;
    assert!((before.y - 400.0).abs() < 0.02 && before.z > 22.0, "not at the railing: {before:?}");
    // Back off 1.3 m, then a short run-up and a jump at the railing.
    drive(&mut sim, "walk+back:0.5,idle:0.3,forward:0.15,jump+forward:0.05,forward:0.8,idle:11", |_, _| {});
    let w = sim.walker().unwrap();
    assert_eq!(w.deaths, 1, "did not go over: {:?}", w.pos);
    assert!(w.grounded && (w.pos.y - 400.0).abs() < 0.02 && (w.pos - before).length() < 3.0, "back at {:?}", w.pos);
}
