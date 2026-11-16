# Labyrinth

## Status

In progress, 2026-11-16 (round 2). First slice of M2 (generator core): a scene `labyrinth`, generated from a seed as a space graph first and geometry second, built from the kit of parts, walkable, checked for reachability and for open sight lines in collider space, over a 100-seed sweep. Round 2 enlarged the region and the core loop, added three set pieces (cistern, stair hall, cleft), light shafts, lantern crossings, antechambers, eight passage sections, door frames and a detail pass. Verified headless only (tests, the sweep, captures, short headless walks). Nobody has walked it in a window yet. Out of scope: decay and breakage, machinery, streaming, the whole-structure exterior, district functions (`ParamSet`).

## What it is

A block of the megastructure 384 m by 352 m in plan and 156 m tall, mostly solid mass, with about a hundred spaces carved out of it and linked into a labyrinth:

- a deep void open to the sky, crossed by a bridge on the axis of a giant arch into a great hall;
- a dark pillared undercroft lit only by pools under light shafts;
- a cistern: a basin with a forest of columns and a walkway round it;
- a stair hall climbed by one monumental stair;
- a long narrow cleft crossed by two bridges;
- a light well crossed by a bridge without railings;
- stair towers round open wells, crossings under lanterns open to the sky, antechambers before the great doors;
- passages in eight sections (vaulted, colonnaded, slot, low, gallery, plain, duct, service);
- ledges, loggias, a dark chamber.

There are several routes between most places. The core loop climbs and descends more than ten storeys, takes about a quarter of an hour at a walk, and passes through every set piece.

## Design

### Layers

| Layer | Here |
|---|---|
| L0 massing | The director's set-piece boxes in a fixed region; the rest is solid. No hull grammar. |
| L1 districts | None (one function). |
| L2 space graph | `labyrinth/graph.rs`: typed spaces and typed links, built by the director and grammar. |
| L3 layout | `labyrinth/plan.rs`: rooms placed as cell boxes, passages routed by A* on the cell grid. |
| L4 elements | `labyrinth/geom.rs`: the plan built from the kit (`kit-of-parts.md`). |
| L5, L6 | Not built. |

### The director

`Director::new` places the set pieces first, so composition is decided before anything fills around it. One flank, east or west by seed, holds the undercroft and the cistern; the other holds the cleft and the light well:

| Set piece | Size and place |
|---|---|
| Deepest void | 40 to 48 m by 44 to 48 m, the whole height, open to the sky, in the middle |
| Biggest hall | 52 to 60 m wide, 100 to 116 m long, 36 to 42 m tall, floor 7 to 9 storeys up. It runs from the void to the sunlit south face; a one-cell gap holds the arch's thickness |
| Arch axis | Arch, bridge and the far ledge's door on one line near the hall's centre line. The hall's two rows of columns are centred on it, a nave |
| Undercroft | 48 to 60 m by 40 to 52 m, two storeys, at the bottom beside the void; two light shafts 12 m square come down into it from the roof |
| Cistern | 36 to 44 m square, four storeys, beyond the undercroft; a light shaft over its middle |
| Stair hall | 64 to 72 m long and 20 m wide, six storeys tall, against the north face (sky-lit windows); its stair climbs three storeys |
| Cleft | 16 m wide, 64 to 80 m long, from storey 2 to the sky; bridges at two storeys 30 to 40 m apart |
| Light well | 24 to 28 m square, from storey 3 to 5 up to the sky |

The director also picks the storeys of the hall, the spawn ledge, the light well's bridge, the cleft's bridges, the wall stair and the overlook.

### The grammar

The core cycle is seven arcs through every set piece, with fixed links between their parts:

```
spawn ledge -a1-> great hall =arch=> arch ledge =bridge=> far ledge -a2-> undercroft
  -a3-> cistern (walkway) -a4-> stair hall =stair=> landing -a5-> cleft ledge B =bridge=> ledge A
  -a6-> light well ledge B =bridge=> ledge A -a7-> spawn ledge
```

Each arc is rewritten (`Builder::arc`), up to depth 4 on the core and 2 elsewhere, into:

1. a **stair tower** if it changes four or more storeys, kept at least 12 cells from every other tower;
2. else a **junction** if it is long, placed up to 0.45 of the arc's length off the straight line so chains wander and cross. A junction is a plain room, a room with a central pier, or a **crossing** with chamfered corners, which opens into a **lantern** to the sky when the cells above are free;
3. else, if one end is a great space, an **antechamber** before its door, with a short corridor into the great space;
4. else one **passage**, corridor or service passage, with its section chosen per passage.

Then:

- **Loops**, eight independent cycles in all on the default seed: the void's wall stair, the hall's gallery, the cleft's upper bridge, and four chords between arcs on opposite sides of the cycle. Chord ends prefer rooms with few passages, so doors do not run out.
- **Spurs**: the overlook, a dark chamber off the undercroft, two loggias.

The result on the default seed: 108 spaces (102 walkable), cycle rank 7, the core spanning storeys 0 to 13.

### Passage sections

Each passage has one section (`PassStyle`), so a route changes character from passage to passage. A walker can tell which kind of passage they are in.

| Section | Width, height | Treatment |
|---|---|---|
| Plain | 3.2 m, 5.4 m | Pilaster frames and a beam every other cell |
| Vaulted | 3.2 m, 5.2 m crown | Barrel vault (smooth) on straight runs, impost courses, ribs; flat ceiling at turns, a closing face (spandrel) where vault meets a taller space |
| Colonnade | 3.5 m, 5.4 m | Heavy piers with base and capital and a deep beam at every cell |
| Slot | 2.5 m, 5.4 m | Narrow and tall, bare |
| Low | 3.2 m, 3.4 m | Beams close overhead every cell |
| Gallery | 3.2 m, 5.4 m | Openings 3 m wide from a 1 m sill into neighbouring voids, halls and the cistern |
| Duct | 2.0 m, 3.0 m | Three pipes under the ceiling (smooth), junction boxes where they turn, cross beams |
| Service | 2.0 m, 2.8 m | Low and bare |

All passages also have joints and a dado line, closed doors (shallow recesses) along the walls, enclosed stair halls where they change storey, and windows into voids, halls and the outside.

### Rooms and detail

| Space | Built from |
|---|---|
| Void, cleft, light well | Walls with sparse joints and string courses; ledges with corbels and railings; bridges; the wall stair; beams across at free storeys (kept off the sight lines); buttress piers and accreted blocks kept clear of openings, ledges, stairs and corners (E203) |
| Great hall | Joints, a nave of round columns on the arch axis with beams along and across, one column broken to a stump with its drums on the floor, pilasters, plinth, string course and cornice, slot windows in the sunlit end, the gallery and grand stair |
| Undercroft | Square piers every 8 m, beams, plinth, fallen blocks; ceiling holes under the light shafts |
| Cistern | Walkway 4.5 m deep round the walls two storeys above the basin floor, on corbels, railed; a free-standing stair of 72 risers down to the basin; round columns every 6.5 m in the basin; cornice and plinth; ceiling hole under its shaft |
| Stair hall | A stair 8 m wide, 108 risers in four flights, newels at its foot; a landing slab across the far end on square piers, railed; pilasters; plinth and cornice; sky-lit windows in the north wall |
| Stair tower | Flights round an open well, a landing in every corner |
| Junction, antechamber, chamber, loggia | Plinth and cornice; central pier or chamfered crossing with a lantern ring; beams across antechambers; loggia colonnade |
| Doors | Frames by weight of what they lead into: heavy (great spaces and their upper levels: jambs 0.8 m, a 1.1 m lintel, a raised threshold), medium, light (service doors), clamped to fit the passage |
| Floors | Joints every one or two cells and a paved border 1.2 m from the walls |

### Light

Openness (`labyrinth/light.rs`) is propagated on the cell grid from where the sky gets in: the top of every open space (voids, cleft, light well, shafts, lanterns) and the cells behind windows and loggias. It moves cell to cell inside a space, keeping a fraction per kind: 0.985 sideways and 0.962 down in voids, 0.985 down shafts, 0.955 in the hall, 0.95 in the stair hall, 0.9 in corridors, 0.8 in service passages, 0.7 in the undercroft. Through doors and windows it keeps 0.35 to 0.8 by area, down out of a shaft into the room below 0.92, through the arch 0.97. Then each kind has a darkest value:

| Kind | Darkest | Why |
|---|---|---|
| Corridors, junctions, antechambers, landing, loggias | 0.46 | Stay grey, pencil in shade |
| Towers, service passages, hall, stair hall | 0.36 | Darker, still above black |
| Undercroft, chamber, cistern | 0.03 | Fall to black except where light reaches |
| Everything else | 0.2 | |

These are set against the shading's dark end. In `nihei_ink` (E066) a light value below 0.06 is solid black, closing in up to about 0.26. So the deepest places (undercroft, cistern basin, chamber) print black with white rules, the light shafts make grey pools on the undercroft floor, and passages print dark grey with pencil (E209). The hall's far end, the stair hall and the void tops are sunlit or pale. The scene's sun is 62 degrees up at azimuth 135 (`Sun::for_scene("labyrinth")`, E204), so it reaches down into the void.

### Sight lines

The director records views (`Plan::views`) that must stay open:

- from the far ledge's door through the arch to the hall's far end;
- from the spawn ledge's rail down to the arch ledge;
- up the stair hall to its landing;
- across the cleft from the lower bridge up to the underside of the upper one.

Beams in voids are rejected if they cross a view. `World::ray_clear` checks each view in collider space: railings and parts under 10 cm do not count, solids do. The check runs in the tests and in the seed sweep.

### Landmarks

`Plan::landmarks()` gives named cameras (eye, yaw, pitch) at the set pieces and in one passage of each section, computed from the plan. Capture paths built on these follow the generator when it changes, where fixed coordinates go stale (the round-1 `lab_*` paths now point at the wrong places). A test checks every landmark eye is in open space.

## Technical details

### Contracts and constants (`crates/gen/src/labyrinth/mod.rs`)

| Constant | Value | Contract for |
|---|---|---|
| `CELL`, `LEVEL` | 4 m, 6 m | Grid; a passage stair climbs one `LEVEL` in three cells |
| `RISERS_PER_LEVEL` | 36 (rise 0.1667 m) | `kit::STAIR_RISE_MIN..MAX`; player step 0.3 m |
| Narrowest passage | 2.0 m wide, 2.8 m high, doors 1.2 by 2.2 m | Capsule 1.8 m by 0.3 m |
| Lowest overhead in a duct | 2.45 m (pipes) | Capsule |
| Thresholds | 0.12 m | Step 0.3 m |
| `NX`, `NZ`, `NY` | 96, 88 cells, 26 storeys | Region 384 by 352 by 156 m |
| `SLAB`, `SKIN` | 0.6 m, 2 m | |
| `CISTERN_WALK` | 4.5 m | Walkway depth |
| `DEFAULT_SEED` | `0x1ab_0001` | Scene registry |

### Seeding

`crate::seed`: `hash64(parent, stable_id, layer)` and a splitmix `Rng`. Stable ids are paths (`"core/a4/0/0/ante"`, `"cleft/ledge/1b"`); layer tags: director, graph, layout, elements. No hash-map iteration feeds generation. Builds are bit-identical for a seed (tested). A failed embedding retries from `hash64(seed, "attempt/n")`.

### Files

| File | Contents |
|---|---|
| `crates/gen/src/seed.rs` | Hierarchical seeding |
| `labyrinth/mod.rs` | Constants, `build`, `build_with`, timings |
| `graph.rs` | `SpaceKind` (17 kinds), `LinkKind`, `Role`, `SpaceGraph`; cut links and cycle rank |
| `grid.rs` | `Dir`, `Cell`, `CBox`, the occupancy `Grid` |
| `plan.rs` | Director, grammar, placement, routing, `PassStyle`, ledges, uppers (gallery, landing), portals with frame weights, anchors, views, landmarks; `Plan` |
| `geom.rs` | Geometry for every kind of space and section, door frames, wall bands, vaults, pipes |
| `light.rs` | `LightField` |
| `walk.rs` | Reachability flood fill and `ray_clear` |
| `tests.rs` | Tests, report, sweep, diagram |

### Tests (`labyrinth::tests`)

- Same content hash twice for a seed; another seed differs.
- Mesh invariants and finite colliders for five seeds; the baked scene has places darker than 0.15 and lighter than 0.8.
- Graph for six seeds: no core or loop link is a cut link, cycle rank at least 6, spurs exist, at least 30 walkable spaces of at least 14 kinds, all six link kinds, the core spans at least 10 storeys, everything connected to the spawn.
- **Reachability** for three seeds: a flood fill from the spawn's feet over standing places 0.5 m apart, stepping at most 0.3 m, with a capsule 1.8 m by 0.3 m clear of every collider and no drops. It must reach every walkable space's anchors and cross every walkable door (at least 0.9 by 2.0 m). The anchors include the cistern basin, the stair hall's floor and landing, each tower's bottom and top, and just inside every door.
- **Sight lines** for three seeds, as above.
- **Landmarks** in open space for three seeds.
- Stairs: whole treads of 0.30 m, rise 0.15 to 0.18 m, under 40 degrees.
- Seed sweep (ignored): 100 seeds, invariants, reachability and sight lines all pass; mean 1.26 embedding attempts, at most 4.

```
cargo test -p mega_gen labyrinth
cargo test --release -p mega_gen -- --ignored --nocapture labyrinth_report
SWEEP=100 cargo test --release -p mega_gen -- --ignored --nocapture labyrinth_seed_sweep
LABYRINTH_SVG=<file.svg> cargo test --release -p mega_gen -- --ignored labyrinth_diagram
```

### Measured (Apple M3 Max, release, default seed, round 2)

| | Labyrinth | Shaft |
|---|---|---|
| Vertices | 427,240 | 155,106 |
| Triangles | 412,826 (1.8 times the shaft) | 225,670 |
| Strokes | 46,618 (about 3,600 drawn from the spawn) | 6,126 |
| Colliders | 6,095 | 1,288 |
| Build in `mega_capture` | 190 ms (plan 134, geometry 11, light 2, AO 54) | 19 ms |
| Bounce bake | 1.95 s | 0.49 s |
| Frame 2560x1440 at the spawn (`--bench 60`) | 16.1 ms (6.1 ms in round 1; nothing is culled) | |
| Reachability flood fill | 143 ms, 96,554 places | |
| Spaces | 108 (102 walkable), cycle rank 7 | |
| Core loop | about 2.26 km: 15 min walking, 6.3 min running (estimate: passages, tower climbs, a crossing of each room, bridges); loops and spurs add more | |

### Looking at it

The scene has its own preset (`nihei_ink`) and sun, so no flags are needed:

```
cargo run --release --bin mega_capture -- --scene labyrinth --camera 206,79.65,161.8,3.1416,-0.15,1.4 --width 2560 --height 1440 --out <dir>
cargo run --release --bin megastructure -- --headless --scene labyrinth --input "forward:2,look_right:1"
cargo run --release --bin megastructure -- --scene labyrinth        # owner only (windowed)
```

`--camera` takes the field of view in radians. The E205 tour cameras are in `experiments.md`; `Plan::landmarks()` lists the same places from the plan.

## Review notes

- 2026-11-16, owner, first look: "The extra detail really helps actually, the labyrinth is basically a way nicer version of the shaft scene!" The surface articulation (coursing, voussoirs, buttresses and blocks on the void walls, lined reveals, frames) does real work with the wash and pencil look; round 2 gives new passages and set pieces the same level of detail.

## Open questions

- The frame time has grown to 16 ms at 1440p with everything drawn; culling by space (the plan's AABBs are ready for it) will be needed before the region grows again.
- The bounce bake is now about 2 s at load.
- The core-loop time is an estimate from the plan, not a measured walk.
- Gallery passages only open where they run beside a void, the hall or the cistern; elsewhere they look plain.
- Service passages come out as ducts more often than intended.
- Stair towers are spread out now, but junctions still gather where several arcs meet.
- The cleft and light well are less articulated than the deep void (fewer ledges, smaller clutter).
- The exterior is a plain box (M5).
- The diagram does not label the cistern, stair hall and junction kinds.

## Related experiments

E201 (first slice and tour), E202 (embedding failures and door cells), E203 (void wall clutter), E204 (sun elevation), E205 (round 2 tour), E206 (passage sections), E207 (longer, more tangled core), E208 (sight lines), E209 (light in the dark places).
