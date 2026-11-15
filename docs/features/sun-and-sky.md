# Sun and sky

## Status

Designed, 2026-11-15. Not implemented. A fixed sun arrives with M0 or M1; the full sky and the day-night cycle arrive with M5.

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

Shadows:

- **Remote sun:** cascaded orthographic shadow maps around the camera.
- **Near sun:** perspective shadow frusta from the sun's position towards the camera region, with the same cascade splits. A sun inside the structure would need a cube map; not planned unless wanted.
- **Far field:** beyond the last cascade, a coarse ray march through the 32 m occupancy grid towards the sun. Queued as an experiment.
- **Skipped entirely** when intensity is zero, the sun is below the horizon, or no opening to the sky is in range of the camera.

Consequences for other systems:

- Baked per-vertex data must be independent of the sun: ambient occlusion and openness (sky visibility) only. Nothing directional is baked.
- Tone band thresholds depend on the light value, so a moving sun slowly moves band boundaries across surfaces. Whether this reads as shimmer needs testing.
- The sky treatment is selected by sun elevation and intensity, with a blend through dusk.
- The style preset holds the sky's graphic parameters (ruled-line count, disc outline weight, night treatment).

## Open questions

- Day length, and whether time runs by default or is pinned per session.
- Whether a moon or other bodies are wanted at night, or darkness and speckle are enough.
- How local lights (still-powered machinery, lamps) are drawn in ink: white halo shapes, or absence of hatching.
- Whether the sun's orbit is tied to the structure's layout (for example, sky wells aligned to noon).

## Related experiments

None yet. Two are queued in `experiments.md`: stability of the thresholded shadow edge as the sun moves, and the far-field occupancy-grid shadow.
