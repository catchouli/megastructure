# One Shaft

## Status

Working, 2026-11-15. Milestone M1 scene: generated from a seed, registered as `"shaft"`, walkable route checked in collider space, eleven camera paths, sun shadows (E011). The walking controller that will consume the colliders is not built. The sky pass and screentone are out of scope.

## What it is

A square shaft 60 m across and 600 m deep, open to the sky, built from the kit of parts (`kit-of-parts.md`): gallery stacks on three walls, six bridges at different depths in both directions, four beams and struts, one long thin stair climbing an otherwise blank wall, and a giant arch into a columned hall. Its job is to answer two questions: does the scale read, and does distant structure simplify like a drawing instead of turning to noise?

## Design

- **One focal point per view.** The blank east wall is the lit one (the sun is in the west), so from the spawn the stair crosses a white field as a thin dark diagonal, as in the reference page of a single stair on an immense wall. The other walls are ruled and darken with depth.
- **Emptiness is the point.** The east wall has no galleries, one horizontal joint at 240 m, and nothing above 250 m but the stair and bridge B1's landing. Other walls have two vertical and up to seven horizontal joints at irregular positions, not a grid.
- **Uneven detail.** Gallery stacks come in runs with gaps (three on the west, two on the south and north, none on the east), at different heights and lengths; some have piers, some do not.
- **Cramped then vast.** Spawn in a 4.5 m deep gallery, out onto a 3.6 m bridge over a 400 m drop, up a 1.5 m stair on a 600 m wall, along a 3.5 m ledge, through a 31 m arch into a 52 m hall.
- **Human-scale anchors** next to giant parts: risers on the blank wall, railings everywhere, two 2.2 m doors on the ledge beside the 31 m arch, a 2.2 m door at the far end of the hall.
- **Light falls in from the top.** Openness (below) is a smooth function of depth: walls near the rim are white, mid-depth walls ruled, the bottom crosshatched and black in recesses; the fog darkens with it, so the abyss goes dark.

## Technical details

### Layout

Metres, Y up; shaft `x, z` in -30..30, floor `y = 0`, rim `y = 600`, walls 8 m thick. Constants in `crates/gen/src/shaft.rs`.

| Part | Where |
|---|---|
| Spawn | West gallery level `y = 400`, eye at (-31, 401.65, 12), yaw pi/2 (looking east across the void) |
| West galleries | Levels 380..420 (spawn stack, no piers), 520 upward (six to eight levels), 155..190 |
| South galleries | 300..340 (B3 lands at 330), 460..480 (B2 lands at 470) |
| North galleries | 455..480 (B2), 315..345 (B3) |
| B1 | `y = 400`, along x at z 21, west gallery to a landing on the east wall (x 26..30, z 14..26) |
| B2 | `y = 470`, along z at x -12, galleries to galleries |
| B3 | `y = 330`, along z at x 14, heavy girder |
| B4 | `y = 250`, along z at x -4, 11 m wide with a 6 m girder, into portals |
| B5 | `y = 120`, along x at z 0, west portal into the east wall |
| B6 | `y = 545`, along z at x 18, thin, into portals |
| Beams | z-beam at `y = 366`, x-beam at 205, z-beam at 586 (silhouette against the sky), an inclined strut across the south-west corner 268..284 |
| Stair | East wall lane x 28.5..30, from B1's landing (z 14, y 400) south to the ledge edge (z -26.5): 126 risers of 0.17 m in three flights of 42, two landings of 1.35 m, top at y 421.42 |
| Ledge | South wall, x -22..30, z -30..-26.5, top at the stair head; two doors at x 23 and -17.5 |
| Arch | South wall centred on x 0: span 26 m, springing 17 m, ring 2.2 m, 19 voussoirs, 8 m deep, in a 40 by 36 m rectangle |
| Hall | x -30..30, z -138..-38, 52 m high; two rows of six round columns (radius 1.7 m) at x +-12, the fourth of the east row missing (its plinth remains); ceiling beams along each row |

`stair_layout()` fits whole 0.30 m treads plus landings of 1.2 to 2.5 m exactly into the run, so the stair meets the ledge. `stair_surface(z)` gives the walking height along it (used by camera paths).

### Seed

`shaft::build(seed)`; the registry uses `DEFAULT_SEED`. The seed moves gallery run ends, the number of levels in one stack, which stacks have piers, door patterns, wall joints, fallen blocks on the floor and B2's width. The route, bridges, stair, arch and hall are fixed. Deterministic: no hash maps in generation order, AO per vertex independent of threading.

### Openness

`openness(p, n) = sky(depth) * inner(into) * facing`:

- `sky = 0.45 + 0.55 * exp(-depth / 250)`, depth below the rim: 1 at the rim, 0.82 at 100 m, 0.70 at 200 m, 0.56 at 400 m, 0.50 at 600 m.
- `inner = exp(-into / 160)`, `into` the distance outside the shaft's prism (into galleries, portals, the hall).
- `facing`: in the shaft `1 + 0.25 n.y` (soffits darker, floors lighter); more than 6 m into the walls it becomes `0.8 + 0.3 n.(direction to the shaft) + 0.1 n.y`, so hall columns are lit on the side facing the arch.

Times local occlusion from the colliders at 70% strength (`kit-of-parts.md`, E009).

### Look

`StylePreset::nihei_vast` (`ink-renderer.md`) and `Sun::for_scene("shaft")`: a remote sun 74 degrees up at azimuth -110 (west-south-west), with cast shadows. The east wall is lit from the rim down to about 220 m deep, just below the spawn level, so the stair crosses a white field; below that the west rim's shadow cuts across it, and bridges throw black bars on the walls below them. Cast shadow is solid black with white ink inside it, faces turned away from the sun are toned by depth (E011 to E019). Until E011 the sun was 54 degrees up at (-0.55, 0.8, -0.2) and scaled by ao; with real shadows that sun lit only the top 87 m of the east wall and the spawn view went black. `mega_capture` and the game pick preset and sun from the scene name.

### Camera paths

| Name | What |
|---|---|
| `shaft_spawn` | The spawn view; head turns slowly |
| `shaft_gallery_down` | At the spawn gallery's rail, tipping from across the void to straight down |
| `shaft_bridge_walk` | Walking B1 east |
| `shaft_stair_climb` | Climbing the stair, wall on the right |
| `shaft_stair_view` | From the ledge by the stair head, looking down along the blank wall |
| `shaft_stair_below` | From B3 (100 m below) looking up at the stair |
| `shaft_look_up` | From the floor looking up 600 m, turning |
| `shaft_hall_enter` | Looking up at the arch from the ledge, then through it 50 m into the hall |
| `shaft_arch_view` | The arch whole from B1, across the shaft |
| `shaft_rim_down` | From just below the rim, straight down 600 m |
| `shaft_b2_down` | Over B2's railing at 470 m, looking down past the lower bridges |

Reproduce, from the workspace root (each writes `frame_NNNN.png` under `--out`):

```
cargo run --release --bin mega_capture -- --path shaft_spawn --out <dir>
cargo run --release --bin mega_capture -- --path shaft_spawn --width 2560 --height 1440 --out <dir>
cargo run --release --bin mega_capture -- --path shaft_bridge_walk --frames 24 --out <dir>
cargo run --release --bin mega_capture -- --path shaft_rim_down --preset nihei --view contours --out <dir>
cargo run --release --bin mega_capture -- --path shaft_hall_enter --t0 0.5 --t1 0.52 --frames 30 --metrics --no-png
cargo run --release --bin mega_capture -- --path shaft_spawn --bench 200 --width 2560 --height 1440
```

### Measured (Apple M3 Max, Metal, default seed)

| | |
|---|---|
| Vertices | 164,568 (57k with 4 m cells, E009) |
| Triangles | 225,670 |
| Strokes | 6,126; 330 to 1,135 drawn per view |
| Colliders | 1,288 |
| Build (`scenes::by_name("shaft")`, including AO bake) | 23 to 37 ms |
| Frame, 1280x720 (`--bench 300`, five shaft paths, with shadows) | 0.96 to 1.53 ms (0.51 to 0.58 ms before shadows) |
| Frame, 2560x1440 | 2.85 to 4.13 ms (1.60 to 1.73 ms before) |

Temporal stability before shadows, 30 frames, blurred reprojected MAD per unit of ink and pops (method E001); the figures with shadows are in `ink-renderer.md` and E018:

| Path, stretch | final | hatching | strokes | contours |
|---|---|---|---|---|
| `shaft_bridge_walk` t 0.3..0.33 | 0.024, 0.20% | 0.021 | 0.015 | 0.004 |
| `shaft_stair_climb` t 0.4..0.42 | 0.024, 0.42% | 0.022 | 0.016 | 0.015 |
| `shaft_rim_down` t 0..1 | 0.010, 0.09% | 0.007 | 0.003 | 0.013 |
| `shaft_spawn` t 0.4..0.6 | 0.018, 0.03% | 0.020 | 0.005 | 0.006 |
| `shaft_hall_enter` t 0.5..0.52 | 0.021, 0.75% | 0.021 | 0.003 | 0.018 |
| `shaft_look_up` t 0..0.1 | 0.013, 0.04% | 0.015 | 0.001 | 0.004 |

Same range as the test scene (0.012 to 0.022). The hall has the most pops (0.75%), from contours and hatching on the column rows at grazing angles.

### Tests

`shaft::tests`: same hash twice for one seed; different seeds differ; invariants for four seeds; ramps are whole treads with rise 0.15..0.18 and slope under 40 degrees, and every riser in the geometry is 0.15..0.18 tall; colliders finite with orthonormal axes, spawn stands on a floor with headroom; the route (spawn, B1, landing, stair, ledge, arch, hall) walked in 0.2 m steps always has ground within 0.3 m and room for a capsule. `scenes::tests`: every name builds and passes the invariants.

### Files

- `crates/gen/src/shaft.rs`: layout, openness, route, tests.
- `crates/gen/src/scenes.rs`: registry entry.
- `crates/render/src/paths.rs`: `shaft_*` paths.
- `crates/render/src/style.rs`, `renderer.rs`: `nihei_vast`, `Sun::for_scene`.

## Open questions

- Scale reads in the spawn and stair views (stair against an empty lit wall) and when looking straight down; it reads less well from the bridge, where every wall is ruled at the same density and nothing says how far away it is.
- Looking up from the floor the walls are a dense radial burst of rules; strong, but close to noise near the camera. Openness might fall off faster near the bottom so the lowest walls go solid black.
- Shadows are in (E011): the rim cuts a lit band down the east wall and bridges cast bars. Looking up from the floor the whole lower shaft is in shadow, a black field with white rules; whether the sun should be higher, or the camera path should see more sky, is open.
- Debris, cables, pipes and decay are absent; the walls are too clean compared with the references.
- The route is checked against colliders, not by a controller. The walking controller should be run along it when it exists.

## Related experiments

E008 (distance), E009 (AO), E010 (tone and light), E011 to E019 (shadows and tone composition).
