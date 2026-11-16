# Kit of parts

## Status

In progress, 2026-11-15. First kit built for M1 and used by the One Shaft (`one-shaft.md`): wall with openings, recess, block, slab, beam, square and round columns, true arch, stair with landings, railing, bridge, gallery stack. Every element emits triangles, ink strokes and colliders together. Not yet: support-graph members (M3), decay states, machinery ports, the `ParamSet` lookup by district function (M2).

## What it is

The L4 element layer of `PLAN.md`: parametric functions from a few numbers to geometry. Each element is drawn the way an inker would draw it: ranked creases and joints rather than every edge, hatch directions that follow the form, ids that let near contours outline panels and far contours outline whole elements, and human-scale dimensions fixed regardless of the element's size.

## Design

- **One call, three outputs.** An element writes faces, strokes and colliders into the same `MeshBuilder`, so they cannot drift apart.
- **Stroke ranks.** 0: massing silhouettes and the largest forms (shaft corners and rim, bridge deck long edges, the arch's intrados and jambs, long beams). 1: structural creases (slab edges, column edges and foot and head rings, stair soffit edge, railing top rails, opening outlines in big walls). 2: panel seams and secondary joints (wall joints, voussoir joints, column drums, stair sawtooth profile, door outlines). 3: fine detail (nosings, railing posts and mid rails, soffit joints of the arch). The renderer fades each rank at its own distance, so the drawing simplifies in this order.
- **Curved parts** (arch, round column) emit strokes only at real creases and at rings and joints; facet edges are left to contours, which see one fine id across all facets of a voussoir or drum.
- **Smooth normals on round parts** (2026-11-16): a round column's shaft and the arch's curved soffit carry per-vertex radial normals (`MeshBuilder::smooth_grid`), so their shading is round and only the silhouette shows the facets. This changes the shaft's hall columns too. Flat faces keep one normal per face.
- **Ids.** `id_coarse` per element (each slab, pier, wall, flight, column); `id_fine` per panel (between wall joints, per voussoir, per drum, per tread and riser). A panel may span many triangles and, for curved panels, several normals.
- **Hatch axes** follow the form: vertical on walls and columns, along the length on beams, slabs and bridges, along the run on treads, along the slope on stair sides and soffits, around the curve on an arch soffit, across on risers. `face_hatch` projects the preferred direction into each face and falls back to vertical.
- **Tessellation for AO.** Faces are cut on a grid of at most `step` metres (1.5 m on shaft walls and galleries, 2 m in the hall). All cells of a face use the same global grid lines plus every seam and opening edge, so neighbouring cells share vertices and there are no T-junctions inside a face. AO is baked per vertex after the whole scene exists (`MeshBuilder::bake_ao`).
- **Human scale is fixed.** Stairs rise 0.15 to 0.18 m and run 0.30 m whatever the height; a long climb gets more risers and landings, never bigger steps. Railings 1.1 m; doors 1.2 by 2.2 m.
- **Railings are ink, not geometry.** A top rail, a mid rail and posts as strokes, plus a thin collider. Sub-pixel bars would alias into noise at distance; strokes fade by rank instead.

## Technical details

### Contracts

| Constant | Value | Contract for |
|---|---|---|
| `STAIR_RISE_MIN`, `STAIR_RISE_MAX` | 0.15 m, 0.18 m | Player step-up (0.3 m) |
| `STAIR_RUN` | 0.30 m | Slope 26.6 to 31 degrees, under the 40 degree limit |
| `RAILING_HEIGHT` | 1.1 m | Eye height 1.65 m sees over it |
| `DOOR_HEIGHT`, `DOOR_WIDTH` | 2.2 m, 1.2 m | Capsule 1.8 m by 0.3 m |

`stair_risers(height)` returns the riser count and exact rise; every height from 0.75 m up has a solution, some lower heights do not (0.2 m cannot be climbed in legal risers) and get the nearest count. A caller that has fitted a stair to a run passes the count in `Stair::risers`; the rise is asserted within the limits.

### Elements (`crates/gen/src/kit.rs`)

| Function | Emits | Colliders |
|---|---|---|
| `wall(&Wall)` | Rectangle with joints (`seams_u`, `seams_v`) and rectangular openings. One fine id per panel; seam strokes interrupted by openings; optional crease outline around openings | Boxes for the solid behind the face, split around openings (`thickness > 0`) |
| `reveal(frame, rect, depth, step, walk, edge_rank)` | The lining of a hole through a wall: jambs, lintel, sill, facing into the hole (doors and windows between two spaces, labyrinth) | Threshold slab when `walk` |
| `recess(frame, rect, depth)` | Back, sides, ceiling and floor of a box behind a wall opening (niches, doorways, portals) | Five thin slabs |
| `block(centre, axes, half, faces, hatch, step, edge_ranks)` / `aa_block` | Oriented box with chosen faces; creases between emitted faces ranked by the axis they run along | AABB if axis-aligned, else OBB |
| `beam(a, c, width, depth, rank)` | Box along any axis, hatched along it | OBB or AABB |
| `column_square`, `column_round` | Shaft with drum joints; round: N facets, rings only, plinth and abacus blocks | Box; round gets two boxes (square and 45 degree) forming an octagon |
| `arch(&Arch)` | True semicircular arch filling a rectangle in a wall: jambs, voussoir ring (one fine id per voussoir), spandrels split by angle with the rectangle's corners inserted, curved soffit through the thickness, back face | Piers and eight horizontal slices of the masonry above the springing |
| `stair(&Stair)` | Flights of treads and risers on a sloping waist slab, open side(s) with the sawtooth profile, soffit; landings between flights; railing on the open side. `enclosed` (walls both sides, as in a passage): treads, risers and landings only | A `Ramp` per flight, a box per landing, railings |
| `railing`, `railing_with_gaps` | Top rail (given rank), mid rail and posts (rank 3) as strokes | Thin oriented box, its height measured vertically even on a slope |
| `bridge(&Bridge)` | Deck (ends buried), optional girder, soffit ribs as strokes, railings | Deck, girder, railings |
| `gallery(&Gallery)` | Stack of levels recessed into a wall: slab per level projecting past the façade, back wall with doors into dark rooms, optional piers (none in railing gaps), railing with gaps where bridges meet, recess sides and ceiling. `Gallery::opening()` is the hole its wall must leave | Slabs, back walls, recess sides and ceiling, piers, door rooms, railings |

`Frame` (origin, `u` along, `v` up, `n` out of the face) places wall-like elements. `Rect2` is a rectangle in a frame's (u, v).

### Colliders (`crates/gen/src/mesh.rs`)

```rust
pub enum Collider {
    Aabb { min, max },
    Obb { centre, axes: [[f32; 3]; 3], half },
    Ramp { start, end, half_width, thickness },
}
```

A `Ramp` is a stair flight's walking surface: a sloped slab whose top runs from the centre of the bottom edge (on the lower floor at the foot of the first riser) to the centre of the top edge (on the upper floor at the top of the last riser). Tread tops lie between the surface and one rise above it, so a capsule riding the ramp sinks at most one rise (0.18 m) into the treads visually and never meets a riser. `to_obb`, `bounds` and `contains` work on all three. Nothing consumes colliders yet; `Scene::colliders` carries them to the walking controller.

### AO (`crates/gen/src/ao.rs`)

`ao = openness(p, n) * (1 - LOCAL_AO * (1 - local(p, n)))`:

- **Openness** is supplied by the scene: an analytic, smooth function of position and normal (see `one-shaft.md`). It stands for multi-bounce sky light at the scale of the whole space.
- **Local** is ray-cast: 32 cosine-distributed directions (a Fibonacci spiral on the disc lifted to the hemisphere, the same for every vertex, rotated into its tangent frame) cast against the collider boxes in a uniform 4 m grid, up to 3 m. A hit at distance `t` occludes by `(1 - t/3)^2`, so the value is continuous as a vertex moves and has no step at the search radius. A ray that starts inside a box ignores that box (round column facets lie inside their own octagonal collider).
- Thin colliders (any half extent under 0.25 m: railings, stair flights and landings) do not occlude. They are narrower than the wall tessellation, and their AO on a wall became a row of triangles with hairy hatch ends (E009).
- The bake runs on all cores and is deterministic: each vertex depends only on its own position and normal.

### Culling groups (`MeshBuilder::set_group`, `MeshBuilder::chunk`)

Elements carry no space of their own; a scene calls `set_group(space id)` before emitting a space's elements, then `chunk(band_split, band)` once at the end, which sorts triangles and strokes into contiguous runs per group (cut by height for tall groups) and fills `MeshData::chunks`. Vertices are not moved, so the invariants below still hold. See `labyrinth.md` ("Culling data").

### Mesh invariants (`MeshData::check_invariants`)

Indices in range; unit normals and hatch axes, hatch in the face; ao in 0..1; triangles wound counter-clockwise about the normal and not degenerate; id 0 unused; each `id_fine` one contiguous run of vertices with one `id_coarse`, and every triangle inside one `id_fine`. Strokes: unit normals, rank 0..3, valid kind, nonzero length. The test scene's older test that every fine id is exactly one 3- or 4-vertex face does not hold for tessellated or curved panels and is not applied to the kit.

`MeshData::content_hash` is an FNV-1a hash of vertices, indices and strokes, used for determinism tests.

### Tests

`kit::tests`: riser limits over heights 0.75 to 148 m; stair treads lie within one rise above their ramp, ramps under 40 degrees; wall triangles avoid openings, wall area adds up, wall colliders leave openings clear; every element passes the invariants and gives valid collider bounds. `ao::tests`: open space is open, corners are darker, local AO continuous along a line towards a corner.

## Open questions

- AO from parts narrower than the wall tessellation (stairs against the blank wall, railings) is switched off rather than solved. Adaptive refinement of wall cells near small occluders would bring it back without T-junction cracks only if the refinement is conforming.
- Riser polygons are not tessellated, so the wall-side corner of each riser is dark and the open side light; climbing the stair, the band edge cuts each riser in half into a black "piano key". Graphic, but heavier than intended.
- Round column collider is an octagon circumscribing the circle; good enough for walking, slightly fat.
- No support-graph members yet; decay (M3) needs them from these same functions.
- `ParamSet` by district function is not wired: dimensions are arguments.

## Related experiments

E009 (AO from the generator), E010 (tone and light for the shaft).
