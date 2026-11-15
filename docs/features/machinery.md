# Machinery

## Status

Designed in outline, 2026-11-15. Not implemented; scheduled for M4. The four machine types below are the starting vocabulary, chosen because they are easy to generate and instantly recognisable. The set is not limited to them.

## What it is

Wondrous, frightening, decaying machines inside the structure, from small fittings to single machines that fill a hall. Their purpose is implied, never explained.

## Design

Machines are assemblies of parametric parts with typed ports, built by a symmetry-led grammar (radial and bilateral symmetry plus repetition reads as "designed for a purpose"). A district's function picks the vocabulary and scale. Each machine has a state from the power network: powered, failing or dead.

### Starting vocabulary

| Type | Form | Motion when powered | Decay |
|---|---|---|---|
| Giant gears | Toothed discs and rings from metres to hundreds of metres across, in meshed trains, half-buried in walls and floors | Meshed rotation at linked rates; huge gears turn almost imperceptibly | Missing or sheared teeth, seized trains, a gear fallen from its axle |
| Cables | Bundles and single lines strung between anchors, draped over structure, hanging into voids | Slow sway | Snapped cables hanging from one anchor, frayed ends splitting into strands, deepening sag |
| Towers | Pylons, masts, stacks and stacked-disc columns standing in halls and voids or descending from ceilings | Mostly static; small moving parts or lights | Leaning, broken tops, shed cladding, lost rings |
| Turbines | Radial blade arrays in housings, alone or in long repeated rows | Rotation | Missing and bent blades, stalled rotors, housings open to show the rotor |

### Notes per type

- **Gears.** Scale is the point: a tooth taller than the player, with a stair or walkway passing close by. A train that still turns slowly in a dead district is a strong moment.
- **Cables.** The cheapest way to add depth and scale to a void, and the clearest sign of decay. Density should be high where later builders patched things and near power sources.
- **Towers.** They work as landmarks and wayfinding aids across large voids, so the director may place them as set pieces. The reference with the cone descending onto the tiled plain is this type inverted.
- **Turbines.** Suited to arrays: twenty in a row with one stopped or missing.

## Technical details

Every part emits triangles, ink strokes, colliders and support-graph members together, as with architectural elements.

- **Gears.** A tooth profile (trapezoid or approximate involute) arrayed around a pitch circle and extruded. Two gears mesh when their modules match and their centre distance equals the sum of pitch radii; angular rates are in inverse ratio to tooth counts, with a half-tooth phase offset. A train is a tree of such pairs, so one angle drives the whole train. Tooth edges are strokes of low rank so that distant gears reduce to a silhouette and a pitch ring. Animation is a per-instance rotation in the vertex stage; no mesh updates.
- **Cables.** A catenary between two anchors, parameterised by slack; bundles are several catenaries with small offsets. Rendered as swept tubes near the camera and as single strokes farther away. A snapped cable hangs from its remaining anchor and is settled once at generation time (hanging straight down, or draped over what the coarse grid says is below). Frayed ends split the cable into a few short diverging strands, which are strokes only. Sag increases with the decay model's integrity value. Sway is a small vertex-stage offset.
- **Towers.** A vertical grammar: base, repeated shaft segments, rings or platforms, cap. Variants include lattice pylons and stacked discs. Members join the support graph, so micro decay can break a top off and drop it.
- **Turbines.** A hub, `n` blades from a swept profile, and a housing ring or duct. Blades are individual members so decay can remove or bend them. Rotation is a per-instance transform; a failing turbine stutters via hashed noise.

Cables and pipes share the routing network described in `PLAN.md`: ports on machines, trunk lines on walls, and conduit stubs at district boundaries.

The player can ride slow machinery, such as standing on a turning gear (decided 2026-11-15). Colliders for slow, large parts are kinematic and move with the part, and the character controller inherits the velocity of the surface it stands on, including the rotation about the part's axis. Fast parts are not walkable.

Moving parts also affect the ink: Ink strokes on rotating parts are transformed with their instance, so the stroke buffer carries an instance index.

## Open questions

- Whether being caught between meshing teeth is fatal. (Riding machinery is settled: see below.)
- Whether machines make the only sounds in the structure.
- Further types to add later: pistons, tanks, fans and louvres, coils, cranes on rails, conveyors.

## Related experiments

None yet.
