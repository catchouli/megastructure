# Labyrinth

## Status

In progress, 2026-11-16. First slice of M2 (generator core): a scene `labyrinth`, generated from a seed as a space graph first and geometry second, built from the kit of parts, walkable, and checked for reachability in collider space. Verified headless only (tests, a 60-seed sweep, captures). The walking controller has not been run through the whole scene, and nobody has walked it in a window. Out of scope this round: decay and breakage, machinery, streaming, the whole-structure exterior, district functions (`ParamSet`).

## What it is

A block of the megastructure 288 m by 256 m in plan and 156 m tall, mostly solid mass, with a few dozen spaces carved out of it and linked into a labyrinth: a deep void open to the sky and crossed by a bridge, a great hall with giant columns opening onto the void through a giant arch, a dark pillared undercroft at the bottom, a narrower light well crossed by a bridge without railings, stair towers turning round open wells, tall corridors with stair halls, low service passages, small junction rooms, a dark chamber, loggias open to the outside, and ledges on the void walls. There is more than one route between most places, the core loop climbs and descends more than ten storeys, and tight passages open into the big spaces.

## Design

### Layers

The plan's generation layers (`PLAN.md` section 3), as far as this slice goes:

| Layer | Here |
|---|---|
| L0 massing | The director's set-piece boxes in a fixed region; the rest is solid. No hull grammar. |
| L1 districts | None (one function). |
| L2 space graph | `labyrinth/graph.rs`: typed spaces and typed links, built by the director and grammar. |
| L3 layout | `labyrinth/plan.rs`: rooms placed as cell boxes, passages routed by A* on the cell grid. |
| L4 elements | `labyrinth/geom.rs`: the plan built from the kit (`kit-of-parts.md`). |
| L5, L6 | Not built. |

### The director

`Director::new` places the set pieces before anything else, so composition is decided first and everything else fills around it:

- **The deepest void**: 40 to 48 m by 44 to 48 m in plan, the whole 156 m height, open to the sky.
- **The biggest hall**: 52 to 60 m wide, 84 to 100 m long, 36 to 42 m tall, its floor 7 to 9 storeys up. It runs from the void to the structure's sunlit face (`z = 0`), so its far end has tall slot windows and the sun comes in along its length. A one-cell gap between it and the void is reserved for the thickness of the giant arch.
- **The arch axis**: the arch, the bridge across the void and the door on the far side of the void are on one line, so standing at the far door you look across the bridge, through the arch and down the length of the hall to its windows (tour view 3).
- **The darkest place**: the undercroft, two storeys high at the bottom, on one flank of the void.
- **The light well**: a narrower shaft open to the sky, starting 3 to 5 storeys up, on the other flank.
- **Storeys**: the spawn ledge 4 to 6 storeys above the hall floor on the far wall of the void, so the first view is across and down onto the arch; the light well's bridge storey; the void wall that carries the wall stair; the overlook's storey.

### The grammar

The core cycle is four arcs between set pieces, plus three fixed links:

```
spawn ledge --a1--> great hall ==arch==> arch ledge ==bridge==> far ledge
far ledge --a2--> undercroft --a3--> light well ledge A ==bridge==> ledge B --a4--> spawn ledge
```

Each arc is rewritten (`Builder::arc`) into a chain:

1. If it changes four or more storeys: a **stair tower** placed near the middle of the arc, and the two halves rewritten in turn.
2. Else if it is long (over 16 cells): a **junction** at an intermediate storey, and the two halves rewritten.
3. Else one **passage**: a corridor (60%) or a service passage (40%).

Rewriting stops at depth 2. Then:

- **Loops** (role `Loop`): from a room on the descent (a2 or a3) to a low ledge on a void wall, up the **wall stair** two storeys to a second ledge, and on to a room on the climb (a4 or a1); and from the **hall's gallery** (reached by the grand stair inside the hall) to a room on another arc. Each closes a further cycle with the core.
- **Spurs** (role `Spur`): the **overlook** (a small ledge high on the void, railing broken), a dark **chamber** off the undercroft, and two **loggias** on the outside faces, one on a sunlit face where it fits.

The resulting graph has a 2-edge-connected core (no core or loop link whose loss disconnects it; tested), a cycle rank of at least 3, dead ends hanging off it, 39 or more walkable spaces of up to 12 kinds and all six link kinds.

### Embedding

Cells are 4 m in plan and 6 m a storey (`CELL`, `LEVEL`); the region is 72 by 64 cells and 26 storeys. Every space owns cells; anything not owned is mass. Rooms the grammar creates are placed near their arc's midpoint by an outward search for free cells with a one-cell margin. Then every passage is routed by A* (`plan::route`) from any door cell of one end to any door cell of the other:

- A flat move costs one cell; a **stair move** is three cells of stair over two storeys followed by a flat cell one storey up or down (two flights of 18 risers of 1/6 m with a 1.2 m landing, exactly 12 m), costing five.
- Turns cost extra, more in corridors (which run straight) than in service passages; a seeded noise per cell bends routes.
- Cells that later passages will need as doors cost 15 extra, so earlier routes do not use them up (E202).
- A route that crosses itself is rejected.

Door candidates: any wall cell of a box room at its floor storey (not the corners); on a ledge, cells along its wall (the far ledge is forced onto the arch axis; ledges either side of the wall stair keep their doors on their own side of it); at a stair tower, the two cells beside the corner landing of that storey, or the nearest storey that has one free. If anything fails, the embedding starts again from a derived seed (`hash64(seed, "attempt/n")`); over 100 seeds the mean is 1.42 attempts, at most 4, and every seed passes the reachability check.

Ledge extents are set after routing, to cover their doors, bridge ends, the arch and stair ends, at least 9 m long.

### Building it

Each space draws only its own inward faces, inset from its cells (rooms 0.4 to 1.0 m, passages 0.4 m for corridors and 1.0 m for service passages), so neighbouring spaces have a wall of the two insets between them. Openings (`plan::Portal`) are cut through both faces and lined by a reveal (`kit::reveal`); walkable ones get a threshold collider.

| Space | Built from |
|---|---|
| Void, light well | Four walls with sparse joints and string courses; floor with fallen blocks; ledges (slab, corbels, railings with gaps at bridges); bridges (`kit::bridge`); the wall stair (`kit::stair` against the wall); beams across at free storeys; buttress piers in the free vertical stretches and accreted blocks of very different sizes (some stacked, some with dark slots), kept clear of every opening, ledge, stair and corner (E203) |
| Great hall | Box with joints, two rows of round columns (24 sides, smooth normals) with beams along and across, one column broken to a stump with its drums on the floor, pilasters, slot windows in the exterior end, the gallery (slab, corbels, railing) and the grand stair (108 risers, four flights) |
| Giant arch | `kit::arch` through the gap between hall and void (thickness about 5.6 m), with a floor through it |
| Undercroft | Box, square piers every 8 m, beams, fallen blocks |
| Stair tower | Box 12 m square, flights of 18 risers (3 m) along each wall round an open well 6.6 m across, a landing in every corner, a railing on every well edge; slot windows where it meets the outside |
| Junction, chamber | Box; a central pier in junctions three cells or more across |
| Loggia | Box on the outside face; its outer wall a colonnade of openings onto the outside, railed except one bay |
| Corridor, service passage | Per cell a 3 by 3 sub-grid (centre always open, edge strips open towards the next cell, corners closed) whose open and closed boundaries become wall faces; faces merged along straight runs into long walls with joints and a dado line; headers where the ceiling steps at stair halls; enclosed stairs (`Stair::enclosed`); frames (pilasters and a beam) every other cell in corridors; closed doors (shallow door recesses) along the walls; windows into voids, into the hall and to the outside |
| Outside | Four skin walls 2 m beyond the region and a roof, with holes for windows, loggias and the voids' tops; no colliders |

### Light

Openness (`labyrinth/light.rs`) replaces the shaft's analytic function. It is propagated on the cell grid from where the sky gets in: the top storey of each open void (1.0) and the cells behind windows and loggias (0.35 to 0.95 by opening area). Light moves cell to cell inside a space, keeping a fraction that depends on the kind of space (0.985 horizontally and 0.962 down a storey in voids, 0.955 in the hall, 0.86 in corridors, 0.78 in service passages, 0.84 in the undercroft), and through doors and windows keeping 0.35 to 0.8 by area, through the arch 0.97. It is a max-product propagation (brightest first), smoothed twice within each space, with a floor of 0.03. A vertex samples it 0.45 m off its surface, interpolated only between cells of its own space, times a facing term `0.86 + 0.14 n.y`. The local occlusion term from colliders is as in the shaft (`LOCAL_AO` 0.7). So the void tops and loggias are near white, the hall mid grey, corridors dark grey and the undercroft and service passages near black: genuinely dark deep places for the pencil shading in shade.

The sun is not chosen here: `Sun::for_scene` in the render crate has no entry for `labyrinth`, so it gets the default (32 degrees up, azimuth 135). A higher sun (about 62 degrees) puts sunlight down into the void (E204); captures in E201 use `--sun-elev 62 --sun-azim 135`.

### Walking

Spawn on the spawn ledge, 1.6 m from the void wall in front of its door, looking across the void (yaw pi). Bridges in the void are railed; the light well's bridge has no railings and its ledges none, the overlook's railing has a gap, and one loggia bay is open to the outside: falls are possible there and fatal from most heights.

## Technical details

### Contracts and constants (`crates/gen/src/labyrinth/mod.rs`)

| Constant | Value | Contract for |
|---|---|---|
| `CELL`, `LEVEL` | 4 m, 6 m | Grid; a passage stair climbs one `LEVEL` in three cells |
| `RISERS_PER_LEVEL` | 36 (rise 0.1667 m) | `kit::STAIR_RISE_MIN..MAX`; player step 0.3 m |
| Corridor | 3.2 m wide, 5.4 m high | Capsule 1.8 m by 0.3 m |
| Service passage | 2.0 m wide, 2.8 m high, doors 1.2 by 2.2 m | Capsule |
| Corridor mouths | 3.2 by 4.4 m; doors into the hall 1.8 by 3.2 m, into towers 1.6 by 2.8 m | Capsule |
| `NX`, `NZ`, `NY` | 72, 64 cells, 26 storeys | Region 288 by 256 by 156 m |
| `SLAB` | 0.6 m | Floor between storeys |
| `SKIN` | 2 m | Outer wall beyond the edge cells |
| `DEFAULT_SEED` | `0x1ab_0001` | Scene registry |

### Seeding

`crate::seed`: `hash64(parent, stable_id, layer)` and a splitmix `Rng`. Stable ids are paths (`"core/a1/0/junction"`, `"void/ledge/spawn"`), so the id of a space says how the grammar made it, and adding a spur does not reshuffle the core. Layer tags: director, graph, layout, elements. No hash-map iteration feeds generation; A* breaks ties by insertion order. Builds are bit-identical for a seed (tested).

### Files

| File | Contents |
|---|---|
| `crates/gen/src/seed.rs` | Hierarchical seeding |
| `crates/gen/src/labyrinth/mod.rs` | Constants, `build`, `build_with` (without the AO bake, for tests), timings |
| `graph.rs` | `SpaceKind`, `LinkKind`, `Role`, `SpaceGraph`; cut links (bridges) and cycle rank |
| `grid.rs` | `Dir`, `Cell`, `CBox`, the occupancy `Grid` |
| `plan.rs` | Director, grammar, placement, A* routing, ledges, portals, anchors, spawn; `Plan` |
| `geom.rs` | Geometry for every kind of space |
| `light.rs` | `LightField`: openness |
| `walk.rs` | Reachability flood fill in collider space |
| `tests.rs` | Tests, the report, the seed sweep, the plan and section diagram |

Hooks for later layers: every space and link carries a stable id and a role (`Core`, `Loop`, `Spur`), so macro decay (M6) can cut only links whose loss keeps the core connected (`SpaceGraph::cut_links`), and micro decay (M3) can find each element's space; the light field is where holes letting light in would be added as sources; ledges, bridges and stairs are separate records in the plan for a support graph to use.

### Tests (`labyrinth::tests`)

- Same content hash twice for a seed; another seed differs.
- Mesh invariants and finite colliders for five seeds; the baked scene has places darker than 0.15 and lighter than 0.8.
- Graph for six seeds: no core or loop link is a cut link, cycle rank at least 3, spurs exist, at least 30 walkable spaces, at least 10 kinds, all six link kinds, the core spans at least 10 storeys, everything connected to the spawn.
- **Reachability** for three seeds (`walk.rs`): a flood fill from the spawn's feet over standing places 0.5 m apart, stepping at most 0.3 m (stair ramps give the slope), with a capsule 1.8 m by 0.3 m clear of every collider above the step height and nothing solid at shin height; no drops. Every walkable space's anchors (just inside each of its doors, the middle of each passage, ledge and gallery centres, the bottom and top of each tower, the hall's centre) must be reached, and every walkable door must be at least 0.9 by 2.0 m and crossed. The default seed reaches 55,350 places. `walk::tests` checks the fill itself on a hand-built room: through a door, up a 0.25 m step, not up a 0.5 m block, not through a 1.6 m high door.
- Every stair ramp is whole treads of 0.30 m, rise 0.15 to 0.18 m, under 40 degrees.
- Ignored: `labyrinth_report` (counts, timings, spaces, core loop length, bounce bake), `labyrinth_seed_sweep` (`SWEEP=N`), `labyrinth_diagram` (plan and section as SVG).

```
cargo test -p mega_gen labyrinth
cargo test --release -p mega_gen -- --ignored --nocapture labyrinth_report
SWEEP=200 cargo test --release -p mega_gen -- --ignored --nocapture labyrinth_seed_sweep
LABYRINTH_SVG=<file.svg> cargo test --release -p mega_gen -- --ignored labyrinth_diagram
```

### Measured (Apple M3 Max, release, default seed)

| | Labyrinth | Shaft |
|---|---|---|
| Vertices | 183,162 | 155,106 |
| Triangles | 180,586 | 225,670 |
| Strokes | 16,867 (about 3,400 drawn from the spawn) | 6,126 |
| Colliders | 2,707 | 1,288 |
| Build in `mega_capture` (plan, geometry, light, AO) | 44 ms (plan 22, geometry 5, light 1, AO 19) | 19 ms |
| Bounce bake | 550 ms | 485 ms |
| Frame 2560x1440 at the spawn (`--bench 100`) | 6.1 ms | |
| Reachability flood fill | 72 ms | |
| Spaces | 41 (39 walkable), 41 links, 18 passages | |
| Core loop | about 1.4 km: 9.5 min walking, 4 min running; loops and spurs add more | |

Triangles are below the shaft's and well under the 3 to 4 times budget: the large spaces are big plain faces. There is room for more geometry before culling is needed.

### Looking at it

```
cargo run --release --bin mega_capture -- --scene labyrinth --camera 162,73.65,149.8,3.14159,-0.15,1.4 --preset nihei_ink --sun-elev 62 --sun-azim 135 --width 2560 --height 1440 --out <dir>
cargo run --release --bin megastructure -- --headless --scene labyrinth --input "forward:2,look_right:1"
cargo run --release --bin megastructure -- --scene labyrinth        # owner only (windowed)
```

The `--camera` field of view is in radians. Camera positions of the tour (E201) are in `experiments.md`; named paths for them are still to be added to the render crate.

## Open questions

- The light well's ledges and bridge are small and narrow against its walls; it reads less well than the deep void.
- Corridors are tall plain boxes, dark and alike; the frames and closed doors help a little. Different sections (vaults, a nave with clerestory, galleries along a corridor) would help more.
- Stair towers cluster where several arcs meet near the void; the director could spread arcs out.
- The core loop is about 9.5 minutes at a walk, the low end of the 10 to 20 asked for. Longer arcs (a larger region, or junction chains of depth 3) would lengthen it.
- The exterior is a plain box; nothing outside the region exists yet (M5).
- Whether the sun should be high for this scene (E204) is for the owner and the render crate.

## Related experiments

E201 (the scene and its tour), E202 (embedding failures and door cells), E203 (void wall clutter), E204 (sun elevation).
