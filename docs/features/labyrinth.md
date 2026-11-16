# Labyrinth

## Status

In progress, 2026-11-16 (round 3). First slice of M2 (generator core): a scene `labyrinth`, generated from a seed as a space graph first and geometry second, built from the kit of parts, walkable, and checked over a 100-seed sweep. The sweep checks reachability for two walker profiles (standing and crouched), open sight lines, and mesh invariants. Round 3 added:

- culling data: chunks per space, portals, and boxes to find the camera's space;
- real ways off the passages: side rooms, branches that rejoin, doors that read as doors;
- vents: crawlways as shortcuts and secrets;
- a detail pass;
- exposed ducts;
- darker light with deliberate accents;
- region size as a parameter.

Verified headless only (tests, the sweep, captures). Nobody has crawled a vent in a window yet. Out of scope: decay and breakage, machinery, streaming, the whole-structure exterior, district functions.

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

## Review notes

- 2026-11-16, owner, first look: "The extra detail really helps actually, the labyrinth is basically a way nicer version of the shaft scene!"
- 2026-11-16, owner, after walking round 2: "I had a really good explore and had a lot of fun. I noticed the corridors have what look like openings that would go out in different directions but they currently don't go anywhere and are very shallow. And also I had the idea of adding vents you can crawl around, which would be really cool too and could be used to connect different areas. In general I'd love to expand the labyrinth and make it much bigger when we're solved the dynamic range and culling, I know we're running out of frame budget right now, and add more detail to the geometry. It's shaping up really nicely, by the way :)" Also on movement (Quake 2 style, railings no longer contain a jump; kept on purpose: "it's way more fun to run around and jump off stuff now") and on light: "I want the darks to be really dark". Round 3 answers the openings (side rooms, branches, real doors), the vents, and the detail; growth waits for culling.

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

### Ways off the passages (round 3)

The owner found openings along corridors that looked like ways onward and were 30 cm deep. Now:

- **Side rooms** (`SpaceKind::SideRoom`, spurs). Small rooms of 1 by 1 to 2 by 2 cells through a door in a passage's side wall: a cell (bench, raised niche), a store room (shelving, crates), a shrine (stepped plinth, altar, stele, two small columns) or a machine closet (cabinets, pipes floor to ceiling). About a third have the door leaf standing ajar onto the dark room. There are 20 to 30 on the default seed.
- **Branches.** Short passages leaving a passage's side and rejoining it further on, or joining a neighbouring passage, possibly on another storey. Up to six per seed. They are loops when both ends are on cycles, spurs otherwise.
- **What stays shut is plainly shut.** Wall features along passages are now:
  - closed doors (a leaf with rails and studs, a threshold);
  - raised niches with a sill at 0.95 m, plainly niches;
  - missing panels showing the structure behind (ribs, a rail, a broken edge).
- **Windows** into voids and halls remain, and vents have slots onto the big spaces.

### Vents (round 3)

Crawlways (`SpaceKind::Vent`, `PassStyle::Vent`, link kind `VentMouth`), routed by the same A* as passages:

- **Cross-section.** 1.2 m wide and 1.0 m high inside. The floor sits 0.3 m above the storey (a lip stepped into while crouched). The mouths are 1.2 by 1.0 m with a flat top.
- **Course.** Bends are 90 degree corners with at least one cell (4 m) of straight between them. A storey is changed on a 12 m ramp of 26.6 degrees inside a taller slot.
- **Detail.** Flanges at every cell, seams on the slopes.

Up to ten per seed, of four sorts:

| Sort | What it does |
|---|---|
| Passage to passage | Between two passages not linked to each other, close in space but far apart by the route or on different storeys |
| Down into the undercroft | From a passage into the undercroft |
| Cistern to passage | Between the cistern's walkway and a passage |
| Outlet | Onto a small ledge (1.8 m deep, no railing) high on the deep void's wall, with a view and a drop |
| Lookout | A dead end: an intact grille in the deep void's or light well's wall, looked through (with a collider) |

At open mouths the grille lies torn off on the floor or hangs from its top edge. Vents are loop or spur links, never the only way to a space on foot: a test checks that every space not reached only by crawling stays connected to the spawn without vent links.

Contract with the crouched walker (`crates/game`): the capsule is 0.9 m tall with radius 0.3 m, a 0.45 m step, 40 degree slopes; a 1.0 m interior is the least. Heights between 1.2 and 1.7 m are avoided so the player is never left wondering why they cannot stand. Exits open into spaces at least 1.8 m high.

Exposed ducts are seen from outside but are not crawlable: a rectangular duct with flanges hung under the great hall's ceiling, and half of the members spanning the voids are ducts instead of beams. (Crawlable vents run through the mass; making some of them visibly cross big spaces is still to do.)

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
| Floors | Joints every one or two cells and a paved border 1.2 m from the walls; in passages, gratings over shallow dark pits in about one cell in twenty |
| Passage walls | Closed doors, raised niches, missing panels (above); conduits on brackets under the ceiling in four passages in ten (plain, slot, gallery, duct) |

### Light

Openness (`labyrinth/light.rs`) is propagated on the cell grid from where the sky gets in: the top of every open space (voids, cleft, light well, shafts, lanterns) and the cells behind windows and loggias. It moves cell to cell inside a space, keeping a fraction per kind, sideways and down:

| Kind | Sideways | Down |
|---|---|---|
| Voids | 0.985 | 0.962 |
| Shafts | 0.97 | 0.992 |
| Great hall | 0.955 | |
| Stair hall | 0.95 | |
| Corridors | 0.9 | |
| Service passages | 0.8 | |
| Cistern | 0.7 | 0.96 |
| Undercroft | 0.62 | 0.97 |
| Vents, side rooms | 0.7 | |

Light falls straight down a shaft into the dark rooms and stays local, so the shafts make pools. Through doors and windows it keeps 0.35 to 0.8 by area, out of a shaft into the room below 0.95, through the arch 0.97. The floor is 0.03 everywhere.

Round 3 drops the per-kind minimums of E209 ("I want the darks to be really dark"; the renderer has a weak player-carried light). A facing term `0.8 + 0.2 n.y` keeps floors, walls and ceilings a step apart in tone.

The shading now prints openness below about 0.12 fully black, with the black closing in below about 0.45. So enclosed passages, side rooms and vents are black except where the player's light reaches, and light is placed as accents:

- the void tops and the sunlit end of the hall;
- the stair hall's windows;
- three shafts into the undercroft (pools round the piers, and one against the far wall so light falls down it);
- two into the cistern (over the basin, and over a corner of the walkway: a lit landing);
- lantern crossings.

The sun is 62 degrees up at azimuth 135 (`Sun::for_scene`).

### Sight lines

The director records views (`Plan::views`) that must stay open:

- from the far ledge's door through the arch to the hall's far end;
- from the spawn ledge's rail down to the arch ledge;
- up the stair hall to its landing;
- across the cleft from the lower bridge up to the underside of the upper one.

Beams in voids are rejected if they cross a view. `World::ray_clear` checks each view in collider space: railings and parts under 10 cm do not count, solids do. The check runs in the tests and in the seed sweep.

### Culling data (round 3)

For the renderer's culling, additive and generic (other scenes leave them empty):

- **`MeshData::chunks: Vec<Chunk>`** (`crates/gen/src/mesh.rs`). Each chunk has `space: u32`, `min`/`max` (bounds of its vertices and stroke ends), `index_start`/`index_count` (a contiguous range of `indices`, a multiple of 3) and `stroke_start`/`stroke_count` (a contiguous range of `strokes`). Chunks cover both arrays completely and in order.
  - Space ids are the volume ids of the plan (`Plan::volume`): a ledge belongs to its void, the gallery to the hall, the landing to the stair hall. `mesh::OUTSIDE` (`u32::MAX`) is the outer skin.
  - Spaces taller than 60 m are cut into 36 m height bands, several chunks with one id (the deep void, cleft, light well, shafts, lanterns).
  - Built by `MeshBuilder::set_group` while emitting and `MeshBuilder::chunk(60.0, 36.0)` at the end, which reorders triangles and strokes and leaves vertices in place.
- **`Scene::portals: Vec<ScenePortal>`**. Each portal has `a`, `b` (space ids, `OUTSIDE` for the sky or outside) and `quad` (four corners in order).
  - Every door, window and vent mouth, on the plane halfway through its wall.
  - The giant arch.
  - The bottom of each light shaft (the ceiling hole into the room below).
  - The open top of every open space.
  - From `Plan::scene_portals`.
- **`Scene::space_boxes: Vec<SpaceBox>`**, to find the space the camera is in. Each box has `space`, `min`, `max`: each room's inner box, each passage cell (stair halls over both storeys, vents at their height), and each walkable doorway's thickness. From `Plan::space_boxes`.

On the default seed: 188 chunks, 265 portals, 1,194 boxes. Tests check:

- the chunks are contiguous, cover the mesh and bound their vertices;
- the void is cut into at least three bands;
- every portal joins spaces that have chunks;
- every such space has boxes;
- the spawn and every landmark lie in a box.

### Landmarks

`Plan::landmarks()` gives named cameras (eye, yaw, pitch) at the set pieces and in one passage of each section, computed from the plan. Capture paths built on these follow the generator when it changes, where fixed coordinates go stale (the round-1 `lab_*` paths now point at the wrong places). A test checks every landmark eye is in open space.

### Size and growth (round 3)

The region size and storey count are parameters: `labyrinth::nx()`, `nz()`, `ny()`, thread-local, `DEFAULT_DIMS` (96, 88, 26), set for one build by `build_sized(seed, dims, bake)`. The director scales its placements with the plan size; set-piece sizes do not change.

Measured (`labyrinth_scaling`, default seed, release; bounce bake times vary by 1.5 times between runs):

| Region (cells x, z, storeys) | Plan | Total build | Bounce bake | Walkable spaces | Triangles |
|---|---|---|---|---|---|
| 96, 88, 26 (default) | 128 ms | 0.30 s | 1.8 to 2.9 s | 146 | 437,346 |
| 136, 124, 26 (2 times the plan area) | 874 ms | 1.05 s | 3.9 s | 188 | 546,368 |
| 192, 176, 26 (4 times) | 2,270 ms | 2.44 s | 2.3 s | 190 | 608,996 |
| 96, 88, 52 (2 times the storeys) | 133 ms | 0.28 s | 1.8 s | 146 | 542,942 |

What breaks or does not scale:

- The director places the same seven set pieces however big the region is, so a larger region is the same labyrinth spread thinner, with longer passages: plan time grows with route length (A* over more cells), the number of spaces barely grows, and the map gets emptier.
- Doubling the storeys only stretches the voids and shafts taller (more wall triangles, a longer bounce bake) without adding anything to walk.
- A* has a cap of 400,000 expansions per route, which very long arcs could reach.
- The bounce bake grows with triangles and is already 2 to 3 s at load.

Real growth needs the plan's L0 and L1 layers: several districts, each with its own set pieces and director, joined by long connectors, generated lazily (M5).

## Technical details

### Contracts and constants (`crates/gen/src/labyrinth/mod.rs`)

| Constant | Value | Contract for |
|---|---|---|
| `CELL`, `LEVEL` | 4 m, 6 m | Grid; a passage stair climbs one `LEVEL` in three cells |
| `RISERS_PER_LEVEL` | 36 (rise 0.1667 m) | `kit::STAIR_RISE_MIN..MAX`; player step 0.3 m |
| Narrowest passage | 2.0 m wide, 2.8 m high, doors 1.2 by 2.2 m | Capsule 1.8 m by 0.3 m |
| `VENT_W`, `VENT_H`, `VENT_LIP` | 1.2 m, 1.0 m, 0.3 m | Crouched capsule 0.9 m by 0.3 m, step 0.45 m |
| Lowest overhead in a duct | 2.45 m (pipes) | Capsule |
| Thresholds | 0.12 m | Step 0.3 m |
| `DEFAULT_DIMS` | 96, 88 cells, 26 storeys | Region 384 by 352 by 156 m |
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
- **Reachability, two profiles**, for three seeds. Standing (1.8 m, radius 0.3 m, step 0.3 m) must reach every space not reached only by crawling and cross every walkable door. Crouched (0.9 m, radius 0.3 m, step 0.45 m) must reach every vent and outlet and cross every vent mouth (at least 1.0 by 1.0 m). The method, for the standing case:
- a flood fill from the spawn's feet over standing places 0.5 m apart, stepping at most 0.3 m, with a capsule 1.8 m by 0.3 m clear of every collider and no drops. It must reach every walkable space's anchors and cross every walkable door (at least 0.9 by 2.0 m). The anchors include the cistern basin, the stair hall's floor and landing, each tower's bottom and top, and just inside every door.
- **Sight lines** for three seeds, as above.
- **Landmarks** in open space for three seeds.
- Stairs: whole treads of 0.30 m, rise 0.15 to 0.18 m, under 40 degrees.
- Chunks, portals and space boxes (above).
- Graph: at least 3 vents, at least 4 side rooms; no space on foot depends on a vent link.
- Seed sweep (ignored): 100 seeds, invariants, reachability for both profiles and sight lines all pass; mean 1.22 embedding attempts, at most 4.
- Ignored tools: `labyrinth_scaling` (above), `debug_budget` (triangles and strokes per kind of space), `debug_vent`.

```
cargo test -p mega_gen labyrinth
cargo test --release -p mega_gen -- --ignored --nocapture labyrinth_report
SWEEP=100 cargo test --release -p mega_gen -- --ignored --nocapture labyrinth_seed_sweep
LABYRINTH_SVG=<file.svg> cargo test --release -p mega_gen -- --ignored labyrinth_diagram
```

### Measured (Apple M3 Max, release, default seed, round 3)

| | Labyrinth | Shaft |
|---|---|---|
| Vertices | 483,557 | 155,106 |
| Triangles | 437,346 (1.94 times the shaft; the cap this round is 2.5) | 225,670 |
| Strokes | 56,498 (about 4,700 drawn from the spawn) | 6,126 |
| Colliders | 8,012 | 1,288 |
| Build in `mega_capture` | 291 ms (plan 130, geometry 13, light 40, AO 97) | 19 ms |
| Bounce bake | 1.7 to 3.0 s (varies with machine load) | 0.49 s |
| Frame 2560x1440 at the spawn (`--bench 60`, with the render crate's culling) | 10.9 ms | |
| Reachability flood fills (both profiles) | 310 ms | |
| Spaces | 154 (146 walkable), 167 links, cycle rank 16 | |
| Core loop | about 2.2 km: 14.7 min walking (estimate); branches, loops, side rooms and vents add more | |

Triangles by kind (`debug_budget`), largest first:

| Kind | Triangles |
|---|---|
| Corridors | 69,000 |
| Great hall | 49,000 |
| Void | 44,000 |
| Service passages | 42,000 |
| Outside | 40,000 |
| Towers | 40,000 |
| Light wells | 37,000 |
| Junctions | 31,000 |
| Cistern | 24,000 |
| Side rooms | 16,000 |
| Vents | 3,400 |

### Looking at it

The scene has its own preset (`nihei_ink`) and sun, so no flags are needed:

```
cargo run --release --bin mega_capture -- --scene labyrinth --camera 206,79.65,161.8,3.1416,-0.15,1.4 --width 2560 --height 1440 --out <dir>
cargo run --release --bin megastructure -- --headless --scene labyrinth --input "forward:2,look_right:1"
cargo run --release --bin megastructure -- --scene labyrinth        # owner only (windowed)
```

`--camera` takes the field of view in radians. The E205 tour cameras are in `experiments.md`; `Plan::landmarks()` lists the same places from the plan.

## Open questions

- Crawlable vents run through the mass; none yet visibly crosses a big space (the exposed ducts there are solid).
- The cleft and light well got no new ledges or stairs this round.
- Gallery passages are drawn towards big spaces by the router; the effect is modest.
- With the darks really dark, most of the labyrinth is black away from the player's light; the review sheet (E210) uses raised ambient for the interiors.

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

E201 (first slice and tour), E202 (embedding failures and door cells), E203 (void wall clutter), E204 (sun elevation), E205 (round 2 tour), E206 (passage sections), E207 (longer, more tangled core), E208 (sight lines), E209 (light in the dark places), E210 (round 3 tour), E211 (culling data), E212 (side rooms, branches, real doors), E213 (vents and the crouched profile), E214 (light accents with really dark darks), E215 (scaling).
