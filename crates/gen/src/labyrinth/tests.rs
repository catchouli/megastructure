use super::graph::*;
use super::plan::PortalKind;
use super::*;

/// Every walkable space's anchors and every door reached from the spawn in collider space.
fn check_reach(l: &Labyrinth) -> Result<usize, String> {
    let world = walk::World::new(&l.colliders);
    let feet = l.plan.spawn.0 - Vec3::Y * 1.65;
    let reach = walk::flood(&world, feet);
    let mut missing = Vec::new();
    for i in l.plan.graph.walkable() {
        let s = &l.plan.graph.spaces[i];
        if s.anchors.is_empty() {
            missing.push(format!("{} ({}) has no anchors", s.sid, s.kind.name()));
        }
        for a in &s.anchors {
            if !reach.near(*a, 0.75, 0.4) {
                missing.push(format!("{} ({}) at {a:?}", s.sid, s.kind.name()));
            }
        }
    }
    for o in l.plan.portals.iter().filter(|o| o.walk && o.kind == PortalKind::Door) {
        if o.u1 - o.u0 < 2.0 * walk::RADIUS + 0.3 || o.y1 - o.y0 < walk::HEIGHT + 0.2 {
            missing.push(format!("door too small: {o:?}"));
        }
        let um = (o.u0 + o.u1) * 0.5;
        let pm = (o.p0 + o.p1) * 0.5;
        let p = if o.x_plane { Vec3::new(pm, o.y0, um) } else { Vec3::new(um, o.y0, pm) };
        if !reach.near(p, 0.6, 0.4) {
            missing.push(format!("door not crossed at {p:?}"));
        }
    }
    if missing.is_empty() {
        Ok(reach.count)
    } else {
        Err(format!("seed {:#x}: unreachable: {missing:#?}", l.plan.seed))
    }
}

#[test]
fn deterministic_for_a_seed() {
    let a = build_with(DEFAULT_SEED, false).mesh.content_hash();
    let b = build_with(DEFAULT_SEED, false).mesh.content_hash();
    assert_eq!(a, b);
    assert_ne!(a, build_with(7, false).mesh.content_hash());
}

#[test]
fn baked_scene_is_valid() {
    let l = build(DEFAULT_SEED);
    l.mesh.check_invariants().unwrap();
    assert!(l.mesh.vertices.iter().any(|v| v.ao < 0.15), "nowhere dark");
    assert!(l.mesh.vertices.iter().any(|v| v.ao > 0.8), "nowhere open");
}

#[test]
fn invariants_hold_for_several_seeds() {
    for seed in [DEFAULT_SEED, 1, 2, 3, 4] {
        let l = build_with(seed, false);
        l.mesh.check_invariants().unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        for c in &l.colliders {
            let (lo, hi) = c.bounds();
            assert!((0..3).all(|i| lo[i].is_finite() && hi[i].is_finite() && lo[i] < hi[i]), "seed {seed}: {c:?}");
        }
    }
}

/// The graph: a 2-edge-connected core with loops, spurs hanging off it, many kinds of space,
/// real vertical extent.
#[test]
fn graph_has_cycles_spurs_and_height() {
    for seed in [DEFAULT_SEED, 1, 2, 3, 4, 5] {
        let p = plan::plan(seed);
        let g = &p.graph;
        let core = |l: &Link| matches!(l.role, Role::Core | Role::Loop);
        let cuts = g.cut_links(&core);
        assert!(cuts.is_empty(), "seed {seed}: core links that are bridges: {:?}", cuts.iter().map(|&k| (&g.spaces[g.links[k].a].sid, &g.spaces[g.links[k].b].sid)).collect::<Vec<_>>());
        assert!(g.cycle_rank(&core) >= 6, "seed {seed}: cycle rank {}", g.cycle_rank(&core));
        assert!(g.spaces.iter().any(|s| s.role == Role::Spur), "seed {seed}: no spurs");
        let walkable = g.walkable().count();
        assert!(walkable >= 30, "seed {seed}: only {walkable} spaces");
        let mut kinds: Vec<SpaceKind> = g.spaces.iter().map(|s| s.kind).collect();
        kinds.sort();
        kinds.dedup();
        assert!(kinds.len() >= 14, "seed {seed}: {kinds:?}");
        let mut lk: Vec<LinkKind> = g.links.iter().map(|l| l.kind).collect();
        lk.sort();
        lk.dedup();
        assert_eq!(lk.len(), 6, "seed {seed}: {lk:?}");
        let levels: Vec<i32> = g.spaces.iter().filter(|s| s.role == Role::Core).map(|s| s.level).collect();
        let span = levels.iter().max().unwrap() - levels.iter().min().unwrap();
        assert!(span >= 10, "seed {seed}: core spans {span} storeys");
        // Every space connected to the spawn through the graph.
        let seen = g.connected_from(p.spawn_space, &|_| true);
        for i in g.walkable() {
            assert!(seen[i], "seed {seed}: {} not connected", g.spaces[i].sid);
        }
    }
}

#[test]
fn every_space_is_reachable_on_foot() {
    for seed in [DEFAULT_SEED, 1, 2] {
        let l = build_with(seed, false);
        let n = check_reach(&l).unwrap_or_else(|e| panic!("{e}"));
        assert!(n > 10_000, "seed {seed}: only {n} places");
    }
}

/// The director's sight lines are open in collider space.
#[test]
fn sight_lines_are_open() {
    for seed in [DEFAULT_SEED, 1, 2] {
        let l = build_with(seed, false);
        assert!(l.plan.views.len() >= 4, "seed {seed}: {} views", l.plan.views.len());
        let world = walk::World::new(&l.colliders);
        for v in &l.plan.views {
            if let Err(p) = world.ray_clear(v.from, v.to) {
                panic!("seed {seed}: view {} blocked at {p:?} ({:?} to {:?})", v.name, v.from, v.to);
            }
        }
    }
}

/// Every landmark camera stands in open space, for a few seeds.
#[test]
fn landmarks_are_in_open_space() {
    for seed in [DEFAULT_SEED, 1, 2] {
        let l = build_with(seed, false);
        let lm = l.plan.landmarks();
        assert!(lm.len() >= 12, "seed {seed}: {} landmarks", lm.len());
        for m in &lm {
            assert!(!l.colliders.iter().any(|c| c.contains(m.eye.to_array())), "seed {seed}: landmark {} inside a solid at {:?}", m.name, m.eye);
        }
    }
}

#[test]
fn stairs_meet_rise_and_run_limits() {
    let l = build_with(DEFAULT_SEED, false);
    let mut n = 0;
    for c in &l.colliders {
        if let crate::Collider::Ramp { start, end, .. } = c {
            let (a, e) = (Vec3::from(*start), Vec3::from(*end));
            let rise = e.y - a.y;
            let run = (e - a).with_y(0.0).length();
            let steps = (run / crate::kit::STAIR_RUN).round();
            assert!((steps * crate::kit::STAIR_RUN - run).abs() < 1e-2, "run {run} not whole treads");
            let r = rise / steps;
            assert!((crate::kit::STAIR_RISE_MIN - 1e-3..=crate::kit::STAIR_RISE_MAX + 1e-3).contains(&r), "rise {r}");
            assert!((rise / run).atan().to_degrees() < 40.0);
            n += 1;
        }
    }
    assert!(n > 50, "only {n} flights");
}

/// The size of the region and the scene, printed: `cargo test --release -p mega_gen --
/// --ignored --nocapture labyrinth_report`.
#[test]
#[ignore]
fn labyrinth_report() {
    let l = build(DEFAULT_SEED);
    let m = &l.mesh;
    let (lo, hi) = m.bounds().unwrap();
    println!("plan {:.0} ms, geometry {:.0} ms, light {:.0} ms, ao {:.0} ms; attempts {}", l.timings[0], l.timings[1], l.timings[2], l.timings[3], l.plan.attempts);
    println!("vertices {}, triangles {}, strokes {}, colliders {}", m.vertices.len(), m.indices.len() / 3, m.strokes.len(), l.colliders.len());
    println!("bounds {lo:?} .. {hi:?}");
    println!("spawn {:?} yaw {:.2}", l.plan.spawn.0, l.plan.spawn.1);
    for r in &l.plan.rooms {
        println!("  room {:12} {:?} inner {:?}..{:?}", r.kind.name(), r.b, r.inner_min(), r.inner_max());
    }
    for le in &l.plan.ledges {
        println!("  ledge {} side {:?} level {} span {:?}", l.plan.graph.spaces[le.space].sid, le.side, le.level, le.span);
    }
    println!("  arch x {:.1} w {:.1} h {:.1}", l.plan.arch.x, l.plan.arch.w, l.plan.arch.height);
    let g = &l.plan.graph;
    println!("spaces {} (walkable {}), links {}", g.spaces.len(), g.walkable().count(), g.links.len());
    for (i, s) in g.spaces.iter().enumerate() {
        println!("  {i:3} {:16} {:10?} level {:2} {} {:?}", s.kind.name(), s.role, s.level, s.sid, s.anchors.first().map(|a| (a.x.round(), a.y.round(), a.z.round())));
    }
    for k in &l.plan.passages {
        let f: Vec<_> = k.flats().collect();
        let (c, n) = (f[f.len() / 2], f.get(f.len() / 2 + 1).copied().unwrap_or(f[f.len() / 2]));
        println!("  passage {} {} {}: {} cells, {} stairs, {:.0} m, mid {:?} next {:?}", k.space, k.kind.name(), k.style.name(), k.path.len(), k.stairs(), k.length(), (c.x, c.y, c.z), (n.x, n.y, n.z));
    }
    let core = |li: &Link| matches!(li.role, Role::Core | Role::Loop);
    println!("cycle rank {}", g.cycle_rank(&core));
    for m in l.plan.landmarks() {
        println!("  landmark {:20} {:.1},{:.2},{:.1},{:.3},{:.2}", m.name, m.eye.x, m.eye.y, m.eye.z, m.yaw, m.pitch);
    }
    // Rough length of the core loop: core passages, tower climbs (a storey is two sides of
    // about 9.5 m), a crossing of each core room (0.7 of its plan diagonal), the bridges.
    let mut core_m = 0.0;
    for k in l.plan.passages.iter().filter(|k| g.spaces[k.space].role == Role::Core) {
        core_m += k.length();
    }
    for r in l.plan.rooms.iter().filter(|r| g.spaces[r.space].role == Role::Core) {
        if r.kind == SpaceKind::StairTower {
            let levels: Vec<i32> = l.plan.passages.iter().filter(|k| g.spaces[k.space].role == Role::Core).flat_map(|k| k.ends.iter()).filter(|e| e.target == r.space).map(|e| e.cell.y).collect();
            if let (Some(a), Some(b)) = (levels.iter().min(), levels.iter().max()) {
                core_m += (b - a) as f32 * 2.0 * 9.5;
            }
        } else {
            let (lo, hi) = (r.inner_min(), r.inner_max());
            core_m += 0.7 * (hi - lo).with_y(0.0).length();
        }
    }
    for br in &l.plan.bridges {
        if g.spaces[l.plan.ledges[br.a].space].role == Role::Core {
            let ra = &l.plan.rooms[l.plan.ledges[br.a].room];
            core_m += (ra.inner_max() - ra.inner_min()).with_y(0.0).length() * 0.6;
        }
    }
    if let Some(sh) = l.plan.stair_hall {
        core_m += (sh.landing_edge - sh.foot).abs();
    }
    println!("core loop about {core_m:.0} m: {:.1} min walking at 2.5 m/s, {:.1} min running", core_m / 2.5 / 60.0, core_m / 6.0 / 60.0);
    let t = std::time::Instant::now();
    let n = check_reach(&l).unwrap();
    println!("flood: {n} places in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    let t = std::time::Instant::now();
    let baker = crate::bounce::BounceBaker::new(&l.mesh);
    let _ = baker.bake(crate::bounce::BounceSun::Direction(Vec3::new(0.6, 0.53, -0.6).normalize()), crate::bounce::BounceSettings::default());
    println!("bounce bake {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
}

/// Many seeds plan, build and pass the reachability check:
/// `cargo test --release -p mega_gen -- --ignored --nocapture labyrinth_seed_sweep`.
#[test]
#[ignore]
fn labyrinth_seed_sweep() {
    let n: u64 = std::env::var("SWEEP").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
    let mut attempts = Vec::new();
    let mut fails = Vec::new();
    for seed in 0..n {
        let l = build_with(seed, false);
        attempts.push(l.plan.attempts);
        if let Err(e) = l.mesh.check_invariants() {
            fails.push(format!("seed {seed}: {e}"));
        }
        if let Err(e) = check_reach(&l) {
            fails.push(e);
        }
        let world = walk::World::new(&l.colliders);
        for v in &l.plan.views {
            if let Err(p) = world.ray_clear(v.from, v.to) {
                fails.push(format!("seed {seed}: view {} blocked at {p:?}", v.name));
            }
        }
    }
    let mean = attempts.iter().sum::<u32>() as f32 / n as f32;
    println!("{n} seeds: {} failed; embedding attempts mean {mean:.2}, max {}", fails.len(), attempts.iter().max().unwrap());
    assert!(fails.is_empty(), "{fails:#?}");
}

/// A plan and a section of the layout as SVG, for the contact sheet:
/// `LABYRINTH_SVG=<file> cargo test --release -p mega_gen -- --ignored labyrinth_diagram`.
#[test]
#[ignore]
fn labyrinth_diagram() {
    use std::fmt::Write;
    let path = std::env::var("LABYRINTH_SVG").unwrap_or_else(|_| "labyrinth.svg".into());
    let p = plan::plan(DEFAULT_SEED);
    let k = 3.0f32;
    // Outlines as lines: ImageMagick's own SVG renderer drops rect strokes.
    let outline = |s: &mut String, x: f32, y: f32, w: f32, h: f32, sw: f32| {
        for (a, b, c, d) in [(x, y, x + w, y), (x + w, y, x + w, y + h), (x + w, y + h, x, y + h), (x, y + h, x, y)] {
            writeln!(s, r##"<line x1="{a}" y1="{b}" x2="{c}" y2="{d}" stroke="#000" stroke-width="{sw}"/>"##).unwrap();
        }
    };
    let (w, d, h) = (NX as f32 * CELL * k, NZ as f32 * CELL * k, NY as f32 * LEVEL * k);
    let gap = 40.0;
    let mut s = String::new();
    let total_h = d + h + gap * 3.0;
    writeln!(s, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" font-family="Arial" font-size="15">"#, w + 40.0, total_h).unwrap();
    writeln!(s, r##"<rect width="100%" height="100%" fill="#f4f1ea"/>"##).unwrap();
    let grey = |level: i32| {
        let t = level as f32 / NY as f32;
        let v = (220.0 - 190.0 * t) as u8;
        format!("rgb({v},{v},{v})")
    };
    // Plan (x right, z down), origin at (20, gap).
    let (ox, oz) = (20.0, gap);
    writeln!(s, r#"<text x="20" y="25" font-weight="bold">Plan (shade: storey, darker is higher)</text>"#).unwrap();
    outline(&mut s, ox, oz, w, d, 1.0);
    for pass in &p.passages {
        for c in pass.flats() {
            writeln!(s, r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#, ox + c.x as f32 * CELL * k, oz + c.z as f32 * CELL * k, CELL * k, CELL * k, grey(c.y)).unwrap();
        }
        for st in &pass.path {
            if let plan::Step::Stair { from, dir, .. } = *st {
                for i in 1..=3 {
                    let c = from.step(dir, i);
                    writeln!(s, r##"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="#a33" stroke-width="1"/>"##, ox + c.x as f32 * CELL * k, oz + c.z as f32 * CELL * k, CELL * k, CELL * k, grey(from.y)).unwrap();
                }
            }
        }
    }
    for r in &p.rooms {
        let (fill, label) = match r.kind {
            SpaceKind::Void => ("rgba(255,255,255,0.01)", "deep void"),
            SpaceKind::LightWell => ("rgba(255,255,255,0.01)", "light well"),
            SpaceKind::GreatHall => ("rgba(80,80,80,0.25)", "great hall"),
            SpaceKind::Undercroft => ("rgba(0,0,0,0.55)", "undercroft"),
            SpaceKind::StairTower => ("rgba(160,40,40,0.35)", "tower"),
            SpaceKind::Loggia => ("rgba(255,255,255,0.9)", "loggia"),
            _ => ("rgba(60,60,120,0.35)", ""),
        };
        let (x0, z0) = (ox + r.b.x0 as f32 * CELL * k, oz + r.b.z0 as f32 * CELL * k);
        let stroke = if r.open_top { 3.0 } else { 1.0 };
        writeln!(s, r##"<rect x="{x0}" y="{z0}" width="{}" height="{}" fill="{fill}"/>"##, r.b.width() as f32 * CELL * k, r.b.depth() as f32 * CELL * k).unwrap();
        outline(&mut s, x0, z0, r.b.width() as f32 * CELL * k, r.b.depth() as f32 * CELL * k, stroke);
        if !label.is_empty() {
            writeln!(s, r#"<text x="{}" y="{}">{label} {}-{}</text>"#, x0 + 4.0, z0 + 16.0, r.b.y0, r.b.y1 - 1).unwrap();
        }
    }
    for br in &p.bridges {
        let (la, lb) = (&p.ledges[br.a], &p.ledges[br.b]);
        let ra = &p.rooms[la.room];
        let (a, c) = if la.side.is_x() { ((ra.face(la.side), br.u), (ra.face(lb.side), br.u)) } else { ((br.u, ra.face(la.side)), (br.u, ra.face(lb.side))) };
        writeln!(s, r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#000" stroke-width="{}"/>"##, ox + a.0 * k, oz + a.1 * k, ox + c.0 * k, oz + c.1 * k, br.width * k).unwrap();
    }
    let sp = p.spawn.0;
    writeln!(s, r##"<circle cx="{}" cy="{}" r="7" fill="#c00"/><text x="{}" y="{}" fill="#c00">spawn</text>"##, ox + sp.x * k, oz + sp.z * k, ox + sp.x * k + 9.0, oz + sp.z * k + 5.0).unwrap();
    // Section: x right, y up, looking along +z (everything projected).
    let oy = oz + d + gap + h;
    writeln!(s, r#"<text x="20" y="{}" font-weight="bold">Section (all depths projected; storeys 6 m)</text>"#, oz + d + gap - 10.0).unwrap();
    outline(&mut s, ox, oy - h, w, h, 1.0);
    for pass in &p.passages {
        for c in pass.flats() {
            writeln!(s, r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#777"/>"##, ox + c.x as f32 * CELL * k, oy - (c.y + 1) as f32 * LEVEL * k, CELL * k, LEVEL * k * 0.9).unwrap();
        }
    }
    for r in &p.rooms {
        let fill = match r.kind {
            SpaceKind::Void | SpaceKind::LightWell => "rgba(255,255,255,0.01)",
            SpaceKind::Undercroft => "rgba(0,0,0,0.6)",
            SpaceKind::StairTower => "rgba(160,40,40,0.35)",
            _ => "rgba(80,80,80,0.3)",
        };
        let (rx, ry, rw, rh) = (ox + r.b.x0 as f32 * CELL * k, oy - r.b.y1 as f32 * LEVEL * k, r.b.width() as f32 * CELL * k, (r.b.y1 - r.b.y0) as f32 * LEVEL * k);
        writeln!(s, r##"<rect x="{rx}" y="{ry}" width="{rw}" height="{rh}" fill="{fill}"/>"##).unwrap();
        outline(&mut s, rx, ry, rw, rh, if r.open_top { 3.0 } else { 1.0 });
    }
    for l in &p.ledges {
        let r = &p.rooms[l.room];
        let y = oy - l.level as f32 * LEVEL * k;
        let (x0, x1) = if l.side.is_x() { let f = r.face(l.side); (f.min(f - l.side.sign() * l.depth), f.max(f - l.side.sign() * l.depth)) } else { l.span };
        writeln!(s, r##"<line x1="{}" y1="{y}" x2="{}" y2="{y}" stroke="#000" stroke-width="3"/>"##, ox + x0 * k, ox + x1 * k).unwrap();
    }
    writeln!(s, r##"<circle cx="{}" cy="{}" r="7" fill="#c00"/>"##, ox + sp.x * k, oy - (sp.y - 1.65) * k).unwrap();
    writeln!(s, "</svg>").unwrap();
    std::fs::write(&path, s).unwrap();
    println!("wrote {path}");
}


