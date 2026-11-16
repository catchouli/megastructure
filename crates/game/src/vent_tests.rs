//! The walker crawling the labyrinth's real vents (no GPU, no window): for every vent, stand
//! in front of a mouth, crouch, follow the plan's cells to the other end (or to a lookout's
//! grille and back), and stand up outside. Checks: never standing inside, never inside a
//! collider, never leaving through a wall, progress all the way (no stall), no snag on a
//! straight run, never airborne, a steady eye, no death, standing on exit where there is room.
//! The default seed's first vent always; every vent of four seeds with `--ignored`.

use crate::player::*;
use crate::walker::{WalkInput, Walker};
use crate::world::World;
use glam::Vec3;
use mega_gen::labyrinth::{self, graph::SpaceKind, plan::{Passage, PassStyle, Step}};

const CELL: f32 = labyrinth::CELL;
const LEVEL: f32 = labyrinth::LEVEL;

/// Ground below `p` within `reach` for a capsule `height` tall: the feet there, if any.
fn ground(world: &World, p: Vec3, reach: f32, height: f32) -> Option<Vec3> {
    let h = world.cast_h(p, Vec3::NEG_Y * reach, height)?;
    Some(p - Vec3::Y * reach * h.fraction)
}

/// One vent end: the mouth's face centre and outward direction, and where to stand outside.
struct End {
    cell_centre: Vec3,
    out_dir: Vec3,
    /// Feet of a standing (or failing that crouched) spot outside the mouth, if there is one.
    outside: Option<Vec3>,
    /// The other side is not walkable (a lookout's grille).
    closed: bool,
}

fn vent_floor(world: &World, c: Vec3) -> Vec3 {
    ground(world, c + Vec3::Y * 0.8, 2.0, CROUCH_HEIGHT).expect("no vent floor")
}

fn end_of(world: &World, l: &labyrinth::Labyrinth, e: &mega_gen::labyrinth::plan::PortEnd) -> End {
    let floor = vent_floor(world, e.cell.floor_centre());
    let out_dir = e.dir.vec();
    let face = floor + out_dir * (CELL * 0.5);
    let closed = matches!(l.plan.graph.spaces[e.target].kind, SpaceKind::Void | SpaceKind::LightWell);
    let mut outside = None;
    if !closed {
        for d in [1.0f32, 1.5, 2.0, 1.2] {
            if let Some(g) = ground(world, face + out_dir * d + Vec3::Y * 0.7, 12.0, CROUCH_HEIGHT) {
                if world.fits(g, CAPSULE_HEIGHT) && g.y > floor.y - 0.6 {
                    outside = Some(g);
                    break;
                }
            }
        }
    }
    End { cell_centre: floor, out_dir, outside, closed }
}

/// Cells (x, z, storey) the vent occupies, its ramps on both storeys.
fn vent_cells(p: &Passage) -> Vec<(i32, i32, i32)> {
    let mut v = Vec::new();
    for s in &p.path {
        match *s {
            Step::Flat(c) => v.push((c.x, c.z, c.y)),
            Step::Stair { from, dir, up } => {
                for n in 1..=3 {
                    let c = from.step(dir, n);
                    v.push((c.x, c.z, c.y));
                    v.push((c.x, c.z, c.y + if up { 1 } else { -1 }));
                }
            }
        }
    }
    v
}

fn in_vent(cells: &[(i32, i32, i32)], pos: Vec3) -> bool {
    let (cx, cz) = ((pos.x / CELL).floor() as i32, (pos.z / CELL).floor() as i32);
    cells.iter().any(|&(x, z, y)| x == cx && z == cz && pos.y > y as f32 * LEVEL - 0.5 && pos.y < y as f32 * LEVEL + LEVEL - 0.5)
}

#[derive(Debug, Default)]
struct Crawl {
    secs: f32,
    inside_steps: u32,
    airborne_inside: u32,
    slow_inside: u32,
    min_speed_inside: f32,
    /// Steps on a straight run (no turn for half a second) slower than 3 m/s: a snag.
    straight_slow: u32,
    /// Largest change of the eye height in one step inside, metres.
    eye_step: f32,
    stood_on_exit: Option<bool>,
    failures: Vec<String>,
}

/// Crawl `waypoints` (feet positions; only x and z steer) from a standing start at the first,
/// crouched, and stand at the end. `exit_ledge`: stop on arrival and check the player stays.
fn crawl(world: &World, cells: &[(i32, i32, i32)], waypoints: &[Vec3], name: &str) -> Crawl {
    let mut r = Crawl { min_speed_inside: f32::MAX, ..Default::default() };
    let mut w = Walker::new(waypoints[0] + Vec3::Y * EYE_HEIGHT);
    for _ in 0..30 {
        w.tick(world, &WalkInput::default());
    }
    let (mut next, mut steps, mut best, mut since) = (1usize, 0u32, f32::MAX, 0u32);
    let mut entered = false;
    let mut moving_inside = 0u32;
    let (mut last_yaw, mut same_yaw, mut eye) = (f32::NAN, 0u32, w.eye().y);
    while next < waypoints.len() {
        let to = waypoints[next] - w.pos;
        let dh = Vec3::new(to.x, 0.0, to.z).length();
        if dh < 0.3 {
            next += 1;
            best = f32::MAX;
            since = 0;
            continue;
        }
        if dh < best - 0.05 {
            best = dh;
            since = 0;
        } else {
            since += 1;
        }
        if since > 120 {
            r.failures.push(format!("{name}: stuck at {:.2?} heading for waypoint {next} {:.2?} (crouched {}, grounded {})", w.pos, waypoints[next], w.crouched, w.grounded));
            break;
        }
        let yaw = to.x.atan2(to.z);
        same_yaw = if (yaw - last_yaw).abs() < 0.05 { same_yaw + 1 } else { 0 };
        last_yaw = yaw;
        w.tick(world, &WalkInput { forward: 1.0, yaw, crouch: true, ..Default::default() });
        steps += 1;
        let eye_now = w.eye().y;
        let eye_d = (eye_now - eye).abs();
        eye = eye_now;
        if world.overlaps_h(w.pos, w.height()) {
            r.failures.push(format!("{name}: inside a collider at {:.2?}", w.pos));
            break;
        }
        if w.deaths > 0 {
            r.failures.push(format!("{name}: died at {:.2?}", w.pos));
            break;
        }
        let inside = in_vent(cells, w.pos);
        if inside {
            entered = true;
            r.inside_steps += 1;
            if !w.crouched {
                r.failures.push(format!("{name}: standing in the vent at {:.2?}", w.pos));
                break;
            }
            r.airborne_inside += (!w.grounded) as u32;
            let v = Vec3::new(w.vel.x, 0.0, w.vel.z).length();
            moving_inside += 1;
            if moving_inside > 30 {
                r.min_speed_inside = r.min_speed_inside.min(v);
                r.slow_inside += (v < 2.0) as u32;
                r.eye_step = r.eye_step.max(eye_d);
            }
            if same_yaw > 30 && v < 3.0 {
                r.straight_slow += 1;
                if r.straight_slow == 1 {
                    r.failures.push(format!("{name}: snagged on a straight run at {:.2?}: {v:.2} m/s", w.pos));
                }
            }
        } else if entered && next + 1 < waypoints.len() {
            r.failures.push(format!("{name}: left the vent at {:.2?} before its end (waypoint {next})", w.pos));
            break;
        }
        if steps > 60 * 120 {
            r.failures.push(format!("{name}: out of time at {:.2?}", w.pos));
            break;
        }
    }
    r.secs = steps as f32 * FIXED_DT;
    if r.eye_step > 0.05 {
        r.failures.push(format!("{name}: the eye jumped {:.3} m in one step inside", r.eye_step));
    }
    if r.failures.is_empty() {
        // Stop, let go of the key, and see that the player stands where there is room.
        for _ in 0..60 {
            w.tick(world, &WalkInput::default());
        }
        let room = world.fits(w.pos, CAPSULE_HEIGHT);
        r.stood_on_exit = Some(!w.crouched);
        if room && w.crouched {
            r.failures.push(format!("{name}: did not stand on exit at {:.2?}", w.pos));
        }
        let end = *waypoints.last().unwrap();
        if (w.pos.y - end.y).abs() > 0.1 || Vec3::new(w.pos.x - end.x, 0.0, w.pos.z - end.z).length() > 1.0 || w.deaths > 0 {
            r.failures.push(format!("{name}: after stopping, at {:.2?}, not at the exit {:.2?} (deaths {})", w.pos, end, w.deaths));
        }
    }
    r
}

/// Crawl every vent of `seed` both ways (out and back for a lookout); the failures.
fn crawl_all(seed: u64, only: Option<&str>) -> Vec<String> {
    let l = labyrinth::build_with(seed, false);
    let world = World::new(&l.colliders);
    let mut failures = Vec::new();
    for p in l.plan.passages.iter().filter(|p| p.style == PassStyle::Vent) {
        let sid = l.plan.graph.spaces[p.space].sid.clone();
        if only.is_some_and(|o| o != sid) {
            continue;
        }
        let cells = vent_cells(p);
        let ends = [end_of(&world, &l, &p.ends[0]), end_of(&world, &l, &p.ends[1])];
        let mut mid: Vec<Vec3> = p.flats().map(|c| vent_floor(&world, c.floor_centre())).collect();
        mid.dedup();
        let mut routes: Vec<(String, Vec<Vec3>)> = Vec::new();
        for (a, b, tag) in [(0usize, 1usize, "a-b"), (1, 0, "b-a")] {
            let (ea, eb) = (&ends[a], &ends[b]);
            let Some(start) = ea.outside else {
                if !ea.closed {
                    failures.push(format!("seed {seed:#x} {sid} {tag}: no standing spot outside the mouth at {:.2?}", ea.cell_centre + ea.out_dir * CELL * 0.5));
                }
                continue;
            };
            let mut m = mid.clone();
            if a == 1 {
                m.reverse();
            }
            let mut wp = vec![start, ea.cell_centre];
            wp.extend(m.iter().copied());
            if eb.closed {
                // A lookout: to the grille, turn round, and back out.
                wp.push(eb.cell_centre + eb.out_dir * (CELL * 0.5 - 0.6));
                wp.extend(m.iter().rev().copied());
                wp.push(start);
            } else {
                wp.push(eb.cell_centre);
                wp.push(eb.outside.expect("checked"));
            }
            wp.dedup_by(|x, y| (*x - *y).length() < 0.01);
            routes.push((format!("seed {seed:#x} {sid} {tag}"), wp));
        }
        for (name, wp) in routes {
            let r = crawl(&world, &cells, &wp, &name);
            println!("{name} ({:.0} m, {} ramps): {:.1} s, {} steps inside, {} airborne inside, {} slow (<2 m/s), min speed {:.2}, {} slow on straights, eye step {:.3} m, stood on exit {:?}", p.length(), p.stairs(), r.secs, r.inside_steps, r.airborne_inside, r.slow_inside, r.min_speed_inside, r.straight_slow, r.eye_step, r.stood_on_exit);
            failures.extend(r.failures);
        }
    }
    failures
}

#[test]
fn one_vent_of_the_default_labyrinth_is_crawled_both_ways() {
    let f = crawl_all(labyrinth::DEFAULT_SEED, Some("vent/0"));
    assert!(f.is_empty(), "{}", f.join("\n"));
}

#[test]
#[ignore]
fn every_vent_of_several_labyrinths_is_crawled() {
    let mut f = Vec::new();
    for seed in [labyrinth::DEFAULT_SEED, 1, 2, 3] {
        f.extend(crawl_all(seed, None));
    }
    for x in &f {
        println!("FAIL {x}");
    }
    assert!(f.is_empty(), "{} failures", f.len());
}

#[test]
fn crawling_out_onto_the_unrailed_outlet_ledge_and_letting_go_stays_on_it() {
    let l = labyrinth::build_with(labyrinth::DEFAULT_SEED, false);
    let world = World::new(&l.colliders);
    let p = l.plan.passages.iter().find(|p| l.plan.graph.spaces[p.space].sid == "vent/outlet").expect("an outlet");
    let e = p.ends.iter().find(|e| l.plan.graph.spaces[e.target].kind == SpaceKind::Ledge).expect("onto a ledge");
    let inner = vent_floor(&world, e.cell.floor_centre());
    let dir = e.dir.vec();
    let face = inner + dir * (CELL * 0.5);
    // The ledge's edge: the last point out from the face with ground at the ledge's height.
    let ledge_y = ground(&world, face + dir * 1.0 + Vec3::Y * 0.7, 2.0, CROUCH_HEIGHT).expect("ledge").y;
    let mut edge = 1.0;
    while ground(&world, face + dir * (edge + 0.05) + Vec3::Y * 0.7, 1.0, CROUCH_HEIGHT).is_some_and(|g| (g.y - ledge_y).abs() < 0.01) {
        edge += 0.05;
    }
    // Crawl out from the end cell's centre (2 m in, full crawl speed by the mouth) and let go of
    // everything only once down off the lip onto the ledge, still at full speed.
    let mut w = Walker::new(inner + Vec3::Y * CROUCH_EYE_HEIGHT);
    let yaw = dir.x.atan2(dir.z);
    for _ in 0..30 {
        w.tick(&world, &WalkInput { yaw, crouch: true, ..Default::default() });
    }
    let mut n = 0;
    while w.pos.y > ledge_y + 0.05 {
        w.tick(&world, &WalkInput { forward: 1.0, yaw, crouch: true, ..Default::default() });
        n += 1;
        assert!(n < 300, "never reached the mouth: {:?}", w.pos);
    }
    assert!((Vec3::new(w.vel.x, 0.0, w.vel.z).length() - CROUCH_SPEED).abs() < 0.1);
    for _ in 0..120 {
        w.tick(&world, &WalkInput { yaw, ..Default::default() });
    }
    let out = (w.pos - face).dot(dir);
    println!("outlet ledge: edge {edge:.2} m from the face, stopped {out:.2} m out, crouched {}", w.crouched);
    assert!(w.deaths == 0 && w.grounded && (w.pos.y - ledge_y).abs() < 0.01, "{:?}", w.pos);
    assert!(edge - out > 0.5, "stopped {out} m out with the edge at {edge} m");
    assert!(!w.crouched, "did not stand on the ledge");
}
