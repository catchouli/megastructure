# Sun and sky

## Status

In progress, 2026-11-15. The sun and its shadows exist in the ink renderer (`Sun` in `crates/render/src/renderer.rs`; "Shadow pass" in `ink-renderer.md`): a positioned sun that may change every frame, cascaded shadow maps refitted in every `render` (three camera cascades and one fitted to the scene bounds), orthographic for a remote sun and perspective for a near one, skipped when the sun is off or a remote sun is below the horizon, and thresholded into hard, anti-aliased ink edges. The sky is a flat tone from the style preset (`sky_tone`). Not implemented: the drawn sky, the day-night cycle (orbit, time), light shafts, the far-field ray march; the full sky and the cycle arrive with M5.

## What it is

A single sun that lights the structure through its openings and sits in a large abstract sky. The sun can move over a day-night cycle, and its distance is a parameter: it can be an ordinary remote sun, a nearby body with visibly diverging rays, or absent altogether.

## Design

- **The sun is a positioned body, not a direction.** It has a world position, a radius and an intensity. A remote sun is the limit of large distance; "no sun" is zero intensity.
- **Day-night cycle.** The position follows an orbit set by time of day, with period, inclination and distance as parameters. Time can run, be scaled, or be pinned.
- **Distance changes the drawing.** A remote sun gives parallel shadows and a small disc. A near sun gives a large disc, shadows that fan out from it, and light that reaches different parts of the structure from different directions.
- **Light is graphic.** Sunlight contributes to a light value that is quantised into bands (white, hatch, crosshatch, black), so a sun shadow is a solid black or hatched shape with a hard edge. Light shafts through openings erase ink and add ruled streaks.
- **Night and no sun.** With the sun down or absent, interiors rest on baked ambient occlusion, openness and local lights. The sky switches treatment: black with white ink speckle, or blank tone.
- **Sky.** Drawn, not simulated: white or tone with ruled horizontal lines, the sun as an outlined disc, clouds as outlined geometry. It writes its own id so the structure gets a contour against it.

## Technical details

Sun state, per frame:

| Field | Meaning |
|---|---|
| `position` | World position, in the same cell-plus-offset form as the camera |
| `radius` | Physical radius; angular size is `2 * atan(radius / distance)` |
| `intensity` | 0 disables the sun and its shadow pass |
| `falloff` | None for a remote sun; optional inverse-square for a near one |

Shading uses the per-fragment direction `normalize(sun_position - world_position)`, which reduces to a constant direction as distance grows. Above a distance threshold the renderer switches to a pure directional path to avoid precision loss.

What exists (`Sun`, public fields `position`, `radius`, `intensity`; `Sun::remote(dir)`, `Sun::direction_from_angles(elevation, azimuth)`, `Sun::angles_of(dir)`, `Sun::is_up(from)`, `Sun::for_scene`). Azimuth is measured from +Z towards +X. `falloff` and the cell-plus-offset position are not implemented (positions are plain `Vec3`).

Shadows (built; details in `ink-renderer.md`):

- **Remote sun:** four orthographic cascades: three around the camera (frustum slices to 8, 30 and 110 m, bounding spheres snapped to texels) and one over the scene's bounds, all 2048 square, refitted every frame.
- **Near sun:** the same four as perspective frusta from the sun's position. A sun inside the structure would need a cube map; not planned unless wanted.
- **Far field:** beyond the last camera cascade, a coarse ray march through the 32 m occupancy grid towards the sun. Not built; for M1 the scene-bounds cascade covers it. Queued.
- **Skipped** when intensity is zero, a remote sun is below the horizon (its intensity is then treated as 0 everywhere), or the style preset's `sun_shadow` is 0. Not yet skipped when no opening to the sky is in range.
- **Drawn as ink:** the filtered visibility is thresholded at 0.5 into a one-pixel anti-aliased edge before band quantisation; what a shadow is drawn as (solid black, hatching, tone by depth) is a preset choice (E011). Stable as the sun moves (E012).
- **Small openings:** nothing assumes closed geometry; light passes through holes down to about two texels of the cascade in use (2 cm near the camera, 25 cm at 100 m and beyond).

Consequences for other systems:

- Baked per-vertex data must be independent of the sun: ambient occlusion and openness (sky visibility) only. Nothing directional is baked.
- Tone band thresholds depend on the light value, so a moving sun slowly moves band boundaries across surfaces. With the sun moving 0.01 degrees per frame no shimmer was measured (E012); at day-cycle speeds the motion is far slower.
- The sky treatment is selected by sun elevation and intensity, with a blend through dusk.
- The style preset holds the sky's graphic parameters (ruled-line count, disc outline weight, night treatment).

## Open questions

- Day length, and whether time runs by default or is pinned per session.
- Whether a moon or other bodies are wanted at night, or darkness and speckle are enough.
- How local lights (still-powered machinery, lamps) are drawn in ink: white halo shapes, or absence of hatching.
- Whether the sun's orbit is tied to the structure's layout (for example, sky wells aligned to noon).

## Related experiments

E011 (shadow treatment), E012 (shadow edge and a moving sun), E013 (hand-cut edge), E017 (sky tone). Queued: the far-field occupancy-grid shadow.
