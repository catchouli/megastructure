# Experiment log

What we tried and what happened. See `README.md` for the conventions.

## Entry format

```
## E000: short title
- Date:
- Feature: which feature document this belongs to
- Question: what we wanted to find out
- Setup: what was built or changed, with commit or capture paths
- Result: what happened, with numbers or image paths where possible
- Verdict: kept, dropped, or needs follow-up (and the follow-up entry id)
```

## Entries

Paths beginning `<captures>/` were temporary capture output and are not kept; rerun the commands in each entry's setup to regenerate them.

## E001: An objective temporal stability check
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`, `features/capture-tool.md`
- Question: can a number from the capture tool tell hatching that swims or pops apart from hatching that simply moves with the camera?
- Setup: `mega_capture --metrics` on 40-frame slow stretches of `corridor_walk` (t 0.05..0.062, about 5 cm per frame), `corridor_strafe` (t 0.4..0.6) and `hall_orbit` (t 0..0.012). First metric: mean absolute difference (MAD) between consecutive frames, full resolution and 4x4-downsampled. Second: reproject the previous frame into the current one using the current depth buffer and both cameras, skip pixels that were off screen or hidden before (depth mismatch over 1%), and take the MAD of what remains, raw and after a 5x5 box blur, plus the share of pixels off by more than 64 levels. Divide by mean ink (255 minus mean grey) to compare views with different amounts of ink.
- Result: plain consecutive MAD is dominated by legitimate motion of fine ink (final 11.7 against lines 2.2 on the walk) and cannot separate swim from motion. With reprojection, the first run showed a lines baseline of 2.9, which turned out to be the screen-anchored paper grain (uniform noise of amplitude 0.035 gives a mean absolute difference of about 2.8 levels); `--metrics` now switches grain off, and the baseline dropped to 0.6 to 0.7. The raw reprojected MAD of hatching is still mostly bilinear resampling error of 1-pixel lines and scales with ink amount; the blurred and per-unit-ink figures are the useful ones. `--error-png` writes the blurred residual times 8 and was used to look for octave bands (none found) and line beading (found, E006).
- Verdict: kept. Figures quoted in the other entries are blurred reprojected MAD per unit of ink unless stated.

## E002: Hatch octave selection
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`
- Question: does choosing the octave from `length(vec2(dpdx(u), dpdy(u)))` of the hatch coordinate hold line pitch on the long corridor walls and floor at grazing angles, compared with isotropic `fwidth` of position and with no level of detail?
- Setup: diagnostic switch `--hatch-lod directional|isotropic|fixed` (only the octave choice changes; coverage always uses the true derivative). Nested octaves with a linear fade of odd lines. `corridor_walk` t 0.05..0.062, `hatching` view, 40 frames; still at t 0.05 cropped around the vanishing point (`<captures>/ink/lod/crop_*.png`). Measured before E006 and E007, so the absolute numbers are higher than the current ones.
- Result: directional: mean ink 57.3, blurred residual 1.30 (0.023 per unit ink), pops 0.57%. Isotropic: ink 50.7, 1.31 (0.026), pops 0.55%. Fixed octave 0: ink 76.0, 1.83 (0.024), pops 1.27%. Visually the isotropic choice is two to three octaves too coarse for lines running along the corridor on the grazing wall, because `fwidth(P)` there is dominated by the depth direction; the wall loses tone towards the vanishing point and its ruling spreads to a few far-apart lines. The fixed octave aliases into a grey, flickering grating towards the far end (twice the pops). The directional choice keeps the pitch near the target down to the end of the corridor. No band-shaped residual at octave changes was visible in the error images on any path. The linear crossfade changes mean density by at most 6% over an octave cycle (computed), so it was not compensated.
- Verdict: kept the directional derivative, as the plan specified. Follow-up: judge the crossfade on real 60 fps video; the metrics cannot show whether a slow fade is visible as breathing.

## E003: Stroke visibility by depth tolerance
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`
- Question: which depth sample and what tolerance give strokes that neither drop out on grazing faces nor leak through occluders?
- Setup: three variants behind `--stroke-vis`: `pixel` (scene depth at the fragment's own pixel), `centreline` (at the nearest centreline point), `block` (farthest depth of the 3x3 block around the centreline point). Tolerances 0.002, 0.01, 0.05 relative to depth (`--depth-tol`). `strokes` view, 30 frames on `corridor_walk` t 0.05..0.062 and `hall_orbit` t 0..0.012. Measured with the box-filtered coverage of the time.
- Result: at 0.002 the single-sample variants lose strokes on grazing faces next to creases: on the walk, pops 0.15% (centreline) and 0.21% (pixel) against 0.034% for block, and blurred residual 0.33 and 0.30 against 0.15. Block gave identical figures at 0.002, 0.01 and 0.05 on the walk. At 0.05 the hall gains ink (24.94 against 24.61 mean ink) from strokes showing through columns and the platform. At 0.01 all three variants are equal on this scene. No z-fighting in any variant, since there is no hardware depth test. With block, a hidden stroke can show for one pixel at an occluder's silhouette, under the silhouette contour.
- Verdict: kept block with tolerance 0.005, which is robust down to 0.002 and leaks less than 0.01. Follow-up when generated geometry is dense: thin parts (railings, panels under 0.5% of their distance thick) could still let strokes through.

## E004: Distance behaviour of lines in the test scene
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`
- Question: do contours, strokes and hatching simplify towards the far end of the 110 m corridor and the 150 m hall instead of turning to noise or solid black?
- Setup: `corridor_walk` at t 0 (108 m to the step face, hall beyond to 270 m) in the `final`, `contours`, `strokes` and `hatching` views, cropped around the vanishing point at 3x (`<captures>/ink/far/crop_*.png`). Fine-id contours fade between 25 and 60 m; strokes fade by rank (seams, rank 2, between 35 and 70 m); hatching keeps its pixel pitch; fog from 25 m.
- Result: no noise and no solid black at the far end. Panel-seam contours become grey and vanish before the step face, the remaining coarse contours outline the opening, and the hall beyond reads as a few light verticals. Hatching keeps its pitch into the vanishing point and is faded by the atmosphere. In the moving walk the far end showed no flicker in the error images. The test scene is too short to stress this: the plan's real test is a 600 m shaft (M1).
- Verdict: needs follow-up on the M1 shaft. The queued experiment on distance masking stays open for that.

## E005: Anti-aliasing the contours
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`
- Question: the id and depth contours are binary per pixel and stair-step, and where one lies on an anti-aliased stroke the max-blend puts the jaggies back. Is a targeted filter enough, or is FXAA or SMAA on the composite needed?
- Setup: the contour pass now writes an R8 mask; a resolve pass estimates the line direction from the 3x3 structure tensor of the mask gradient, averages seven bilinear taps along it, blends by coherence, and max-blends into ink R. Compared on a 4x crop of `still_hall` in the `lines` view (`<captures>/ink/merge/crop1.png` before, `crop2.png` after), and with `--metrics` on the `contours` view.
- Result: the stair-steps on near-horizontal silhouettes are gone and contour and stroke merge into one smooth line. Temporal figures improved: raw reprojected MAD and pops went from 0.82 and 0.35% to 0.63 and 0.19% on the walk, 0.71 and 0.28% to 0.58 and 0.12% on the strafe, 2.29 and 1.10% to 1.94 and 0.75% on the orbit. FXAA on the composite was not tried: hatching and strokes are already analytically anti-aliased, and a full-frame filter would soften them.
- Verdict: kept. FXAA or SMAA stays unused unless generated geometry produces aliasing that the per-pass coverage misses.

## E006: Tent versus box coverage for thin lines
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`
- Question: the blurred error images of the walk showed beading along thin diagonal floor lines. Does a smoother coverage filter reduce it?
- Setup: coverage across a line changed from the exact overlap with a one-pixel box (`box_cov`) to the overlap with a tent of radius one pixel (`tent_cov`), first for hatching, then for strokes. Same 40-frame stretches as E001, `hatching` and `strokes` views.
- Result: hatching pops fell from 0.73% to 0.20% (walk), 0.52% to 0.19% (strafe) and 0.41% to 0.21% (orbit); blurred residual per unit ink from 0.023 to 0.018, 0.014 to 0.010 and 0.023 to 0.020. Strokes: walk pops 0.025% to 0, orbit 0.126% to 0.110%. Lines look slightly softer at 1:1 but still read as pen lines.
- Verdict: kept for both.

## E007: Tone and lighting for the test scene
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`
- Question: the first frames read as graph paper (white surfaces, every panel seam outlined twice). What makes the test scene read as an ink page?
- Setup: in turn: fine-id contours at 45% strength instead of full; line weight growing towards the next darker band (`hatch_width_gain` 1.2); corridor walls hatched along the corridor instead of vertically; a sparser, thinner crosshatch layer; tapered line ends at breakup gaps; the default sun lowered from about 53 to 32 degrees elevation; stronger test-scene AO (`box_ao` darkening 0.4 over 2 m instead of 0.3 over 1.5 m, floor 0.25). Captures `<captures>/ink/v1_*` to `v5_*`.
- Result: halving fine-id contours removed the doubled panel grid. Hatching along the corridor gives ruling that converges on the vanishing point, the strongest single change towards the references. The width gain makes AO corners thicken into black pools with dripping edges (`walkseq/frame_0005.png`). The lower sun leaves floors lit only in their middle, so scratchy converging lines rake in from the walls. The mesh-like crosshatch improved with a 1.6x pitch and 0.8x width but still reads more mechanical than the references. The hall's big lit floor still reads as graph paper.
- Later, at 1440p 1:1, two of these turned out wrong. With a gain of 1.2, dark lines reached about 5 px; and the taper towards gaps, applied over a fixed range of noise values, turned short segments (where the noise barely crosses the threshold) into lens-shaped spindles. The gain was reduced to 0.8 and the taper removed; hard breakup ends read better. Metrics did not change noticeably (walk hatching 0.018 to 0.019 per unit ink).
- Verdict: kept all except the gap taper, which was dropped, and the width gain, now 0.8. Follow-up: shadows (sun through openings) and per-element AO are what the hall needs; the renderer alone will not fix it.

## E008: Distance behaviour on the 600 m shaft
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`, `features/one-shaft.md`
- Question: follow-up to E004. Looking 300 to 600 m down or up the One Shaft, do lines simplify or turn to noise or black, and which distances and rank cut-offs hold?
- Setup: `shaft_rim_down` (straight down 600 m) and `shaft_look_up` (from the floor), `--t 0.5`, `final`, `strokes`, `contours` and `hatching` views, with `--preset nihei` against `--preset nihei_vast`; crops of the vanishing region at 2x. For example `mega_capture --path shaft_rim_down --t 0.5 --preset nihei --view contours --out <dir>`. Then `--metrics` over 30 frames of each shaft path.
- Result: with `nihei` (contours gone at 400 m, rank 1 at 220 m, fog to 0.85 by about 400 m, no fog darkening) bridges more than 400 m down lose their outlines and become grey blobs in the fog; looking up, the bridges near the rim are flat grey shapes; the crosshatch on walls seen at grazing angles reads as a grid. With `nihei_vast` (contour fade 220 to 650 m, rank fade ends 900, 260, 80, 32 m, fog density 0.0026 and max 0.8, `fog_ao` 1.5, crosshatch narrowed to light 0.36..0.45) every bridge and beam down to the floor keeps a clean coarse-id outline, panel seams and railings drop out by 80 m, hatching on the walls converges into the vanishing point as rules that thin and fade, and the bottom fades to dark grey (about 150 of 255 at 480 m) instead of paper. No solid black and no noise at 300 to 600 m in either direction. Looking up, the bridges near the rim simplify to black undersides with a few rails against the sky. The nearest walls in the look-up view are the busiest part of the picture, not the far ones. Temporal stability over 30 frames is in the test scene's range (blurred per unit ink 0.010 to 0.024; `one-shaft.md` has the table); the far end of `shaft_rim_down` is the steadiest view measured (0.010).
- Verdict: kept `nihei_vast` for the shaft. Closes E004's follow-up and the queued distance experiment. Follow-up: the near walls in the look-up view; and whether `fog_ao` should be replaced by a proper in-scattering term once there are shadows.

## E009: AO and openness from the generator
- Date: 2026-11-15
- Feature: `features/kit-of-parts.md`, `features/ink-renderer.md`
- Question: tone band edges were polygonal because AO was per vertex on 3 to 4 m panels (ink renderer open question). Can the generator give an AO that is smooth and plausible at the scale of the shaft, with band edges that are not obviously polygonal?
- Setup: analytic openness per scene (depth below the rim, distance into the walls, facing; `one-shaft.md`) times local occlusion ray-cast against the scene's own collider boxes (32 fixed cosine-distributed rays, 3 m, `(1 - t/R)^2` falloff, 4 m grid; `kit-of-parts.md`). Faces tessellated on shared global grid lines at 1.5 m (shaft, galleries) or 2 m (hall). Compared with cells forced to at least 4 m (a temporary override of `grid_lines`, since removed) in the `bands` view of `shaft_hall_enter` and `shaft_gallery_down` at t 0.5.
- Result: at 4 m the band edges step along the column bases and the foot of the hall walls; at 1.5 to 2 m they are straight and smooth. Cost: 164k vertices against 57k, 225k triangles against 55k, build 25 ms against 9 ms; frame time unchanged within noise. Three problems appeared and were fixed: round column facets lie inside the column's own octagonal collider, so every ray started inside a solid and the columns baked black (rays starting inside a box now ignore it); the stair, 1.5 m wide against a wall tessellated at 1.5 m, made a row of AO triangles on the blank wall whose jittered hatch ends looked like hair (thin colliders, under 0.25 m half extent, no longer occlude); full-strength corner AO pooled black at every wall foot (local AO now at 70%). A ray-cast local term alone falls to near zero within a few metres of any floor-wall corner, so the large-scale gradient has to be analytic; a physical sky view factor falls as 1/depth^2 and would be black below 100 m, so openness uses an exponential with a floor, standing for interreflection.
- Verdict: kept. Remaining: parts narrower than the tessellation cannot occlude cleanly, and untessellated risers split into a dark wall half and a light open half when climbing. Adaptive conforming refinement near small occluders is the next thing to try.

## E010: Tone and light for the shaft
- Date: 2026-11-15
- Feature: `features/one-shaft.md`, `features/ink-renderer.md`
- Question: with no shadows, how should light and tone bands be set so that a 600 m shaft reads as deep and has a focal point, instead of an even texture?
- Setup: successive captures of `shaft_spawn`, `shaft_look_up`, `shaft_gallery_down`, `shaft_stair_view` and `shaft_hall_enter`: (1) `nihei` with openness alone; (2) ambient 0.75 and a sun scaled by ao (`sun_occlusion`); (3) crosshatch narrowed (`band_cross` 0.36, width gain 1.0); (4) sun moved from the south-east to the west so the blank east wall is the lit one; (5) openness inside the walls made directional and slower (160 m).
- Result: (1) every wall crosshatched at every depth: graph paper, and the blank wall as busy as the others. (2) walls go white near the rim and ruled lower down. `sun_occlusion` was on from the first shaft capture, so a sunlit floor at 600 m was not rendered; by the light formula, without it every up-facing surface would get the same sun term at any depth (0.4 times N.S, enough to lift the floor from crosshatch to white), which is what the missing shadow pass costs. (3) removed the grid on grazing walls; darkness now comes from rules thickening into black, closer to the references. (4) the decisive change: the stair becomes a thin dark diagonal across a white wall, with one focal point in the spawn, bridge and stair views. (5) the hall went from solid black to a legible columned interior, lit on the arch side.
- Verdict: kept all five. With an east-west asymmetry from the sun, whichever wall faces away is uniformly ruled down its whole height; only shadows (light entering at the rim and cutting a band down the walls, bridges casting bars below them) would vary it. That is the main thing holding the image back now.

## E101: Capsule sweep: parry shape casts or exact advancement
- Date: 2026-11-15
- Feature: `features/player-movement.md`
- Question: can the walker use parry3d's `cast_shapes` (capsule against the scene's cuboids) as the sweep of a collide-and-slide controller, as planned, or rapier3d's character controller?
- Setup: first version used `query::cast_shapes` with `target_distance` 0.004 m (the skin) and `stop_at_penetration: false`. A throwaway test then cast a 1.8 m by 0.3 m capsule down onto a big cuboid from gaps of 3 mm to 0.12 m, with vertical and oblique motions, and compared time of impact and normal with the exact answer. Replaced by an exact capsule-against-box distance (segment against box by splitting at the face planes and minimising each quadratic piece) and conservative advancement along the motion, `crates/game/src/world.rs`. Checked against a brute-force search with parry's exact point projection (`Cuboid::project_point`, 3,000 random boxes and capsule poses) and against parry's GJK `query::distance`.
- Result: with a skin, a capsule resting 0.004 m above the floor got `None` back or a wrong normal such as (-0.9989, 0.047, 0) (the first tick of walking in the open ended with the walker in the air and 0.8 mm inside the floor, and after 24 ticks it stopped dead with nothing in the way). With no skin, from a gap of at least 3 mm, normals were within 0.6 percent but the time of impact was wrong by up to 1.6 cm for oblique motion (a 0.3 by 0.3 m motion from a gap of 9 mm hit at 6.1 percent of its length instead of 3.0), which is penetration, not noise. The GJK distance differs from the exact one by up to 7.7 mm; the exact one agrees with the brute force within 0.2 mm. Conservative advancement converges in a few iterations (the distance along a line is convex, so advancing by distance over closing speed never overshoots) and gives exact normals and the touched point, which the step-up logic also needs. rapier3d's character controller was read, not tried: its `snap_to_ground` and `handle_stairs` call the same parry `cast_shape` with a `target_distance` offset, so it inherits the same inaccuracy, and it adds a physics world to the dependencies.
- Verdict: own exact sweep kept. parry3d 0.31.1 stays for the BVH built once per scene (1,288 colliders) and as the reference the tests check against.

## E102: Stepping up a ledge with a round foot
- Date: 2026-11-15
- Feature: `features/player-movement.md`
- Question: how does a capsule step up 0.25 m but not 0.35 m, with the usual raise, forward, drop sequence?
- Setup: three versions, each tested with `walker::tests::a_quarter_metre_ledge_is_stepped_up_and_a_35_cm_ledge_is_not` (ledges of 0.25, 0.29 at a run, and 0.35 m) on a hand-built floor and ledge. (1) Raise by 0.3 m, slide forward by this step's motion, drop, accept if the landing normal is walkable (40 degrees). (2) As (1), and if that fails retry once with the forward probe lengthened to 0.15 m. (3) As (2), and require the height of the touched point (the ledge's corner) to be at most the step height above the starting feet.
- Result: (1) never worked: the capsule stops 0.3 m from the ledge face, 0.04 m forward leaves the centre 0.26 m from it, and the round foot then rests on the corner at 60 degrees from the vertical, steeper than 40, so the landing was rejected on every step and the walker stood at the ledge. Walkable support needs the centre within 0.3 sin 40 = 0.19 m of the edge, which takes about 0.11 m more forward travel than one step gives. (2) worked for 0.25 m, but also climbed a 0.35 m ledge (feet ended at y 0.354 on top of it): the foot rests on the corner at 36 degrees with the feet at 0.299 m, so it looked like a step of under 0.3 m while the corner it touches is 0.35 m high. (3) 0.25 m and 0.29 m ledges (also at a run) are stepped, 0.35 m is not (feet stay at 0, x at about 1.70). The cost is that a step-up moves the player up to about 0.11 m further than the key press alone in that tick, once per ledge; it is not noticeable at walking speed. A ledge climbed this way lifts the feet by up to 0.3 m in one step, which is hidden by the eye offset (E103).
- Verdict: (3) kept.

## E103: Smooth eye height on stairs
- Date: 2026-11-15
- Feature: `features/player-movement.md`, `features/one-shaft.md`
- Question: does walking up and down the shaft's stair (126 risers in three flights, two landings; each flight a `Ramp` collider) need any smoothing of the eye, or does sliding along the ramp with stick-to-ground give a steady eye?
- Setup: horizontal wanted velocity, collide-and-slide along the ramp (walkable surfaces are climbed, steeper ones act as walls), then after every grounded move a 0.3 m sweep downwards that snaps to ground it finds, so the player follows the slope down and over the ridges at the tops of flights. The eye is the feet plus 1.65 m with no smoothing, except an exponential offset (14 per second) after step-ups and after drops of more than 0.12 m. Measured by driving `Sim` with the headless scripts (`shaft_tests.rs`): per-frame change of eye height over the whole stair.
- Result: walking, 1,215 frames on the stair: the eye rose 17.8 mm per frame on average (standard deviation 1.1 mm; the expected value is 2.5 m/s times cos^2 and tan of the 29.5 degree slope times 1/60 s, 17.7 mm), never fell. Running, 505 frames: 42.4 mm per frame (standard deviation 3.9 mm, expected 42.5), never fell. Going down at both speeds: the capsule was on the ground on every frame, the eye fell monotonically within 1.5 mm. The feet stayed 0.004 to 0.050 m above the generator's `stair_surface(z)` over all three flights and both landings (the rounded foot rides 0.046 m above a 29.5 degree slope), so the ramps meet their landings.
- Verdict: no smoothing on slopes; the eye offset is only for ledges and drops.

## E104: Where death and reset return to
- Date: 2026-11-15
- Feature: `features/player-movement.md`
- Question: the rule is "the last position where the player had stood on ground continuously for at least 0.5 s". Taken literally, is that a good place to return to?
- Setup: the stable spot was updated on every step on which the player had been on the ground for 0.5 s or more. Tests: walk off the end of a platform 20 m up (`walker::tests::death_returns_to_the_last_stable_ground_and_so_does_the_reset_key`).
- Result: a walking player is "standing" the whole way to the edge, so the spot ends up at the lip. Worse, the capsule can be grounded with its centre up to 0.19 m beyond the edge, perched on the corner at up to 40 degrees (feet 5 cm low), and that is where the return went (x 3.13 to 3.18 against an edge at 3.0). With a key still held the player fell again at once and died twice from one fall. Changed so a spot only counts if there is a surface straight below the middle of the capsule, so a corner perch is never recorded; the return is then at the last spot with full support, a few centimetres short of the lip.
- Verdict: kept. The return is still at the edge for someone walking off it. Follow-ups worth trying: record the spot half a second late, or step back from edges, if it feels harsh in play.

## Queued

Experiments the plan already calls for, in rough order:

- Stroke depth tolerance on dense generated geometry (thin parts). E003 covered the test scene.
- Hatch octave crossfade judged on 60 fps video rather than metrics (E002 follow-up).
- Sun shadow as solid black shapes: stability of the thresholded shadow edge as the sun moves.
- Far-field sun shadow from a ray march through the 32 m occupancy grid, in place of shadow maps beyond the near cascades.
