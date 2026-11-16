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

## E105: A ring of ground under the stable spot
- Date: 2026-11-15
- Feature: `features/player-movement.md`
- Question: E104 left the return point a few centimetres short of a lip, so a held forward key walked the player straight off again. Does requiring ground under a ring of points around the capsule give a return point that is clear of edges, without making narrow places (the stair lane, bridges) unable to hold a stable spot?
- Setup: a spot counts only if 16 points on a ring of 0.65 m radius around the capsule's axis each have solid at their height (a wall or rail: not an edge) or ground within 0.5 m above or below the feet (`World::supported_ring`). Sixteen points at 0.65 m leave a worst case of 0.64 m to a lip at any angle; with 8 points it is 0.60 only along the axes. After a death or reset the movement keys are ignored for 0.3 s. Tests: a platform 20 m up, forward held and running the whole time (`walker::tests::respawn_is_well_back_from_the_edge_and_held_keys_are_ignored_for_a_moment`); the shaft route, counting frames on which the stable spot updated.
- Result: the player returned 0.650 m from the edge and stayed still for the 0.3 s of the lockout; with the key still held it then walked on. With the ring added to E104's check (ground straight below the axis within 0.028 m), the spot updated on 2,002 of the shaft route's 3,228 frames but on only 28 of the stair's 526: the axis check failed on every slope, because the rounded foot rides up to 0.046 m above a 29.5 degree surface at the axis. Dropping it (the ring covers it, and a corner perch has air under some ring point) gave 2,500 of 3,228 and 526 of 526 on the stair, where the 1.5 m lane with a wall on one side and a rail on the other is still wide enough for the ring, and reset on the stair returned within a metre.
- Verdict: kept. The stable spot is now always at least 0.64 m from a drop. In places narrower than 1.3 m between drops nothing is stable; none exists in the shaft.

## E106: Showing the walker between 60 Hz steps
- Date: 2026-11-15
- Feature: `features/player-movement.md`
- Question: the owner's display runs at 120 Hz and the walker steps at 60 Hz. Does interpolating the eye between the last two steps remove the judder, and can headless output stay bit-identical?
- Setup: `Sim` keeps the walker's eye before and after the latest fixed step; with `interpolate` on (only the windowed runner) the camera is shown at `alpha = accumulator / step` between them, with no sliding across a death, a reset or a switch from the fly camera. Off (the headless runner and the tests) the camera is the eye after the latest step, as before. Test with 240 Hz frames over a floor (`sim::tests::interpolated_walking_moves_the_eye_smoothly_between_fixed_steps_and_off_it_is_exact`).
- Result: with it on, the per-frame movement is the same to within 10 percent (mean 10.4 mm per frame at walking speed); with it off, more than half of the frames do not move at all and the rest move by a whole step. The cost is a lag: the picture shows the position of up to one step (17 ms, 4 cm at walking speed, 10 cm running) ago, and exact multiples of the step show the position one step back. Extrapolating forward instead would remove the lag but overshoots when the player stops or hits a wall; not tried. Headless output is unchanged: the route script ends at the same state line to the digit, and every frame's camera equals the walker's eye bit for bit.
- Verdict: kept, windowed only. Whether the lag is felt at 120 Hz has not been judged by hand (nothing was run in a window).

## E011: Sun shadows, and what a shadow is drawn as
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`, `features/sun-and-sky.md`, `features/one-shaft.md`
- Question: with a real shadow pass feeding the light before band quantisation, how should a shadow be drawn on the shaft: solid black, single hatching, crosshatching, tone by depth, or a mix?
- Setup: shadow pass as described in `ink-renderer.md` (three camera cascades plus one over the scene bounds, 2048 each, refitted every frame). Shaft sun raised from 54 to 74 degrees (azimuth kept at -110): at 54 degrees the west rim shadowed the east wall below 87 m and the spawn view (200 m down) went black; at 74 degrees the east wall is lit to about 220 m, just below the spawn. Five treatments, all with `nihei_vast` otherwise: black (`shadow_attached=1`, shadow light 0), hatch (shadow light fixed at 0.42), cross (0.33), depth (`shadow_ambient=0.7`, max 0.55, attached 1), mix (cast shadow black, `shadow_attached=0.3`). Sheet `compare/E011-shadow-treatment.png` (rows `shaft_spawn`, `shaft_rim_down`, `shaft_hall_enter`, `shaft_look_up`). Regenerate one cell with, for example, `mega_capture --path shaft_spawn --set shadow_attached=1 --out <dir>`.
- Result: black gives the strongest placed shapes (the lit window on the east wall, bridge bars) but turns every interior and the whole look-up view into a black page. Hatch and cross are as uniform as the frame before shadows: shadows become texture, not shapes. Depth is closest to the references in the open shaft (dark ruling going black with depth) but busy. First tries also showed an inconsistency: cast shadow only on sun-facing faces made the hall floor black while the columns, turned away from the sun, stayed white; attached shadow must be drawn like cast shadow, or both by the same rule. It was first implemented as an ink blend and gave grey mixtures on grazing faces; it is now a per-face light blend with a hard facing test. The mix keeps black shapes where the sun is cut off and leaves faces turned away toned by depth, so interiors stay legible.
- Verdict: mix kept as the `nihei_vast` default (`shadow_light_max 0`, `shadow_attached 0.3`), with E016's white ink inside the black. `nihei` keeps shadows off (`sun_shadow 0`): the test scene is closed, so everything would be one shadow. The owner saw it in the windowed game: liked the shadows, intense but right.

## E012: Shadow edge filtering and a moving sun
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`, `features/sun-and-sky.md`
- Question: closes the queued "sun shadow as solid black shapes: stability of the thresholded shadow edge as the sun moves". Is the thresholded edge clean (no acne, steps or peter-panning) and does it move smoothly?
- Setup: filtered visibility thresholded at 0.5 with a one-pixel gradient-based ramp. Diagnostic `--shadow-filter smooth|bilinear|hard`. Still camera, sun azimuth swept -110 to -109.4 degrees over 60 frames (0.01 degrees per frame) and -110 to -107 (0.05 per frame): `mega_capture --path shaft_spawn --t0 0.5 --t1 0.5 --frames 60 --sun-azim -110 --sun-azim-end -109.4 --shadow-filter smooth --metrics --no-png`, also on `shaft_bridge_walk` (t 0.3) and `shaft_arch_view`. New metric: temporal second difference of the blurred frames, near zero for ink moving at a steady rate. Sheet `compare/E012-shadow-edge.png`: 4x crops of 2560x1440 frames.
- Result: second difference at 0.01 degrees per frame: smooth 0.019 to 0.023, bilinear 0.022 to 0.027, hard 0.037 to 0.041; at 0.05 degrees per frame on the spawn: 0.041, 0.061, 0.090. On the sheet, hard shows texel steps, bilinear scallops, smooth a clean curve. Two problems found at 4x and fixed: on the east wall, grazed by the light at 75 degrees, the wide kernel shadowed the wall itself and the edge row was dotted pixel by pixel; a receiver-plane depth bias (each tap compares against the receiver's depth at that tap) and spreading the taps to the pixel's footprint (up to 2 texels) removed the self-shadowing. A dotted row remained on some edges: it is the rails at the top galleries and on the bridges, thinner than one texel of the scene cascade, rasterised as a dotted line into the map. No acne or visible peter-panning at any view checked. A remote sun below the horizon still lit downward faces with the shadow pass skipped; it now counts as intensity 0.
- Verdict: smooth kept. Open: thin occluders (exclude rank-3 parts from the shadow pass, or rasterise them wider). The sheet shows the state after both fixes. Regenerate a column with `mega_capture --path shaft_spawn --width 2560 --height 1440 --shadow-filter hard --out <dir>`.

## E013: A hand-cut shadow edge
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`
- Question: can the shadow edge be slightly irregular, like a cut paper edge, and stay stable?
- Setup: the shadow lookup offset by a world-anchored 3D value noise across the light (`shadow_jitter` metres, wavelength `shadow_jitter_len`). Sheet `compare/E013-shadow-jitter.png`: 0, 0.1 and 0.25 m on a 1:1 crop of `shaft_bridge_walk` at 2560x1440 (`--set shadow_jitter=0.25`).
- Result: first version offset the point off the surface; at 0.4 m lit faces grew black blobs where the shifted lookup fell behind its own grazed surface. With the offset projected into the receiver's plane the blobs are gone and the effect is subtle: visible on near edges, invisible at 100 m (it is in metres). Stability: with the sun moving, blurred MAD 0.026 at 0 and 0.1 m, 0.028 at 0.25 m; camera walking on the bridge: identical figures with and without.
- Verdict: kept at 0.1 m, 1.2 m in `nihei_vast`. A screen-scaled amplitude would read better at distance but would change shape as the camera moves.

## E014: Uneven hatching and indication
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`
- Question: how to stop a 600 m wall being ruled uniformly: world tone noise, per-panel tone, density growing with darkness, or indication (hatching gathered near shadow edges, blank elsewhere)?
- Setup: `tone_noise` (0.08 to 0.1, 25 to 40 m), `hatch_density_gain` 1, `panel_tone` 0.08 to 0.16, `indication` 1 at 3 to 8 m. Sheet `compare/E014-uneven-hatching.png` (rows `shaft_arch_view`, `shaft_rim_down`).
- Result: tone noise gives faint, unconvincing blotches (walls turned away from the sun sit at one light value, so small offsets barely move the bands). Density gain greys the darker end without adding structure. Per-panel tone is the effective one: aligned with the structure, some panels ruled hard, some light; at 0.16 it reads as a quilt. Indication had a sign error at first (a lit point surrounded by shadow counted as far from it); fixed, it greys lit areas near shadow edges, which weakens the white against black that the references rely on.
- Verdict: `panel_tone 0.08` kept in `nihei_vast`; tone noise, density gain and indication kept as parameters, off.

## E015: Crosshatch angle
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`
- Question: review note 4, "the crosshatch is an exact 90 degree mesh". Does an off-90 second layer, varied per element or per panel, read better?
- Setup: `cross_angle` 90 and 75, `cross_angle_jitter` 30 per element or per panel. Sheet `compare/E015-crosshatch-angle.png`: 1:1 crops of `still_hall` at 2560x1440 (the shaft has little crosshatch with the E011 mix).
- Result: 75 degrees loses the graph-paper mesh; per-element jitter gives each column and wall its own angle, as separate hand passes would; per panel breaks at every panel seam, which looks like a fault.
- Verdict: `nihei_vast` uses 75 with 30 degrees jitter per element. `nihei` left at 90 for the owner to decide (the owner liked the test scene's crosshatch in corners).

## E016: Graphic blacks and white ink
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`
- Question: with large solid blacks, how to keep structure legible inside them: white lines, white rules, specks, and should recesses and undersides pool into black?
- Setup: plain black; `white_lines 1` (major lines only, `white_line_rank 1`; rank-3 seams in white looked like a blueprint grid); plus `white_hatch 0.6` at 6x pitch and `speckle 0.1`; plus `pool_ao 0.55` and `underside_black 1`. Sheet `compare/E016-graphic-blacks.png` (rows `shaft_look_up`, `shaft_hall_enter`, `shaft_stair_view`).
- Result: plain black loses the shaft entirely in the look-up view. White lines restore the massing; white rules converging in the black give the look-up view its direction back, scratchboard-like; specks add life (the owner's "sparkles in the shadows": the specks and the broken white rules). Pool and underside darken the lit top of the look-up view too and add nothing elsewhere, since shadows already supply the blacks. Stability: specks add 0.04 levels of blurred error and 0.01% pops on `shaft_hall_enter` and `shaft_bridge_walk`.
- Verdict: white lines, white rules and specks kept in `nihei_vast`; pool and underside off.

## E017: The abyss and the sky
- Date: 2026-11-15
- Feature: `features/sun-and-sky.md`, `features/ink-renderer.md`
- Question: what should the open top and the bottom of the shaft be?
- Setup: `sky_tone` none (fogged paper), 0 (paper), 1 (ink); `fog_ao` 3. Sheet `compare/E017-abyss-sky.png` (rows `shaft_stair_below`, `shaft_look_up --t 0`, `shaft_rim_down`). Few paths see much sky.
- Result: with shadows the abyss is already black (cast shadow plus `fog_black`), so `fog_ao 3` changes little. Paper sky makes the opening a clean white shape against the black; fogged paper is a dull grey; ink sky merges the opening into the black and loses it.
- Verdict: `sky_tone Some(0)` in `nihei_vast`. Follow-up with the sky pass (M5).

## E018: Before and after the shadow and tone pass
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`, `features/one-shaft.md`
- Question: the whole pass, judged on the main views.
- Setup: scene defaults before this work (renders from the previous build) against now. Sheet `compare/E018-before-after.png` (`shaft_spawn`, `shaft_look_up`, `shaft_rim_down`, `shaft_stair_view`, `shaft_bridge_walk`, `still_hall`). Regenerate the after column with `mega_capture --path <name> --out <dir>`.
- Result: the shaft views have placed blacks: a lit window on the east wall with the stair across it, black bars from bridges, the look-up as a black field with white rules converging on a bright opening. `still_hall` (the test scene, `nihei`, shadows off) changes only through E019's patch hatching. Temporal stability (blurred reprojected MAD in levels, pops; before then after): bridge walk 1.10, 0.20% then 0.53, 0.18%; stair climb 1.44, 0.42% then 1.28, 0.62%; rim down 0.31, 0.09% then 1.23, 0.30%; spawn 1.07, 0.03% then 0.68, 0.16%; hall enter 2.14, 0.75% then 1.08, 0.82%; look up 1.29, 0.04% then 0.64, 0.24%; corridor walk 1.30, 0.18% then 1.41, 0.27%; strafe 0.78, 0.16% then 0.62, 0.09%; orbit 1.23, 0.38% then 1.21, 0.41%. Per unit of ink the shaft figures fall (0.003 to 0.017) because mean ink rose from 31-102 to 72-217. The blurred error falls or holds except on the rim-down view, where the west wall is now ruled where it was blank; pops rise where thin white lines on black resample.
- Verdict: kept. Frame time, Apple M3 Max, `--bench 300`: 0.96 to 1.53 ms at 1280x720 and 2.85 to 4.13 ms at 2560x1440 over six shaft and test paths (before 0.49 to 0.58 and 1.58 to 1.74).

## E019: Hatch stroke ends and placement
- Date: 2026-11-15
- Feature: `features/ink-renderer.md`
- Question: owner, on the crosshatch in the windowed game: "not like how a human would place lines; they start and stop very abruptly". Not aliasing. Can strokes taper, and be laid in coherent patches instead of breaking up independently per line?
- Setup: stroke ends anti-aliased along the line from a distance in pixels to the end (exact for patch runs, first order from the noise derivative otherwise, from the light's change per pixel at band edges), width tapering to nothing over `hatch_taper_px` (a quarter of it at the start). Patch hatching (`hatch_patch`): columns of `hatch_patch_lines` lines cut into staggered runs of `hatch_patch_len` breakup lengths; lines of a run start and stop together with small jitter; band-edge jitter shared per run; crosshatch with a larger gap (`cross_break_gap 0.45`), so it lies as patches over the first layer. Sheet `compare/E019-hatch-strokes.png`: 2x crops of 2560x1440 (`still_hall` column and back wall, `shaft_arch_view`), before, taper only, patches.
- Result: a first patch version used a smooth 2D noise field: gap boundaries were its iso-contours and read as white blobs; crossfading two octave layouts over the whole crossfade left many strokes half drawn (grey). Explicit runs, with the layout switching only in the middle 30% of the crossfade, give groups of strokes that start together, trail off with tapered ends and break in aligned gaps. A 12 px taper made needles; 6 px reads as a pen lifting. Stability (blurred MAD per unit ink, pops; per-line then patches): corridor walk 0.020, 0.18% then 0.025, 0.25%; orbit 0.022, 0.38% then 0.023, 0.41%; strafe 0.012, 0.16% then 0.011, 0.09%; stair climb 0.018, 0.90% then 0.016, 0.64%; hall enter 0.005, 0.83% then 0.005, 0.80%. The corridor walk is a little worse, where the layout switch band crosses the long floor. Per-patch angle variation was not tried.
- Verdict: kept as the default in both presets (`hatch_patch 1`, `hatch_taper_px 6`, `hatch_patch_lines 8`, `hatch_patch_len 4`, `cross_break_gap 0.45`). Follow-up: a dyadic patch layout (the next octave's runs as unions of this octave's) to remove the switch; per-patch angle.

## E020: Hatching swims as the camera turns
- Date: 2026-11-16
- Feature: `features/ink-renderer.md`, `features/capture-tool.md`
- Question: the owner, walking the shaft with E019's patches: "the hatching all swims around as I rotate the camera and move around". Object-space hatching should be rigid under rotation. What is view-dependent, why did the metrics miss it, and how is it fixed?
- Setup: new camera paths that turn in place at 90 degrees per second at 60 fps over `--frames 61` (`hall_yaw`, `hall_pitch` from the `still_hall` position, `shaft_spawn_yaw`, `shaft_spawn_pitch` at the spawn) and walk at 1.4 m/s (`hall_walk` with `--t1 0.2333`); `shaft_bridge_walk` t 0.3..0.3267 over 61 frames is also walking speed. New metric line: reprojected MAD after a 13x13 blur (density at the scale of a stroke group). Hatching view. Builds compared: the commit before the tone work (9f7990f), the committed tone work (5185f1b) and this one, each with the octave choice switched by `--hatch-lod` (`screen` = the old screen-derivative choice, `fixed` = octave 0, rigid by construction). Example: `mega_capture --path shaft_spawn_yaw --frames 61 --view hatching --metrics --no-png --hatch-lod screen`. Error images with `--error-png`.
- Result: the metric did separate the cases; the existing paths turned 0.12 degrees per frame, far too slowly to show it. On `hall_yaw` the committed build gave 2.18 (5x5 blurred, levels) and 0.63% pops, against 1.35 and 0.20% before the tone work and 1.36 with patches off: the patches tripled the pops. Causes, in order of size:
  1. The octave was chosen from `dpdx/dpdy` of the hatch coordinate. A rectilinear image magnifies by up to `1 / cos^2` of the angle off the view axis, 1.35 octaves at the side edge of the 70 degree 16:9 view and 1.6 at the corners, so a point on a wall moved through octave crossfades as the view turned. Before the patches that was a gentle density breathing; with E019's patches, whose layout was cut at the drawn octave and switched in mid-crossfade, whole groups of strokes re-formed. Speck cells and white-rule pitch also hung off it. Fixed with a rotation-invariant footprint, `r * a * |e - N (e.d) / (N.d)|` (eye distance, angular size of a centre pixel, view ray), which equals the true derivative at the image centre and depends only on eye position and surface; coverage keeps the true derivative.
  2. The patch layout itself: rebuilt so it depends only on a line's level (nested runs, each level's runs the union of the next finer level's), so as distance changes the octave, lines fade individually and keep their strokes.
  3. Line width was a constant number of screen pixels while the (now fixed) spacing grows towards the image edge, so tone thinned as a surface crossed the screen. Width and taper lengths are now set in invariant pixels.
  4. The camera shadow cascades were spheres around slices of the view frustum, centred ahead of the camera, so which cascade (and resolution) a point used changed as the view turned. Now centred on the eye; also finer (0.8, 2.9, 10.7 cm texels against 1.1, 4.2, 15 cm). Unit test `cascade_choice_does_not_depend_on_view_direction`.
  5. Checked and not view-dependent: band jitter, shadow jitter, panel tone, the light value (no view term).
- Numbers after the fix (5x5 blurred error, pops; hatching view): `hall_yaw` 2.18, 0.63% committed, now 1.21, 0.24% (with the old screen-derivative octave 1.47, 0.38%); `shaft_spawn_yaw`, against a rigid fixed-octave render of the same content: now 2.45, fixed 2.30, screen octave 3.00; `shaft_spawn_pitch` 2.14, fixed 1.94, screen 2.46; `hall_walk` 0.81, 0.23%; `shaft_bridge_walk` 2.12. The shaft figures are high in absolute terms because E021 replaced the black shadows (no error) with dense heavy hatching; the remaining error is bilinear resampling of 2 to 4 px gratings, moire arcs in the error images, shared by the rigid render. The trusted measure is therefore the ratio to a fixed-octave render of the same content (now within 6 to 10% of it), plus the 13x13 figure for coherent changes.
- Verdict: kept. Commands: `mega_capture --path hall_yaw --frames 61 --view hatching --metrics --no-png` (add `--hatch-lod screen` for the old behaviour).

## E021: Cast shadow as hatching, black only in recesses
- Date: 2026-11-16
- Feature: `features/ink-renderer.md`, `features/one-shaft.md`
- Question: the owner chose "hatch" from E011 and asked for much less solid black ("like in dark corners"). How should a hatched shadow be set so it still reads as a shadow, and how much black is left?
- Setup: shadow light 0.40, then 0.33 (dense, heavy single hatching); attached shadow 0.3, then 0; shadow strokes with their own identities and gap fraction (`shadow_break_gap`); a sweep on `shaft_spawn` and `shaft_arch_view` of the overall gap (0.22, 0.12, 0.05), end jitter, density gain and ambient (0.75, 0.85). Black measured as the share of pixels whose 5x5 neighbourhood is all darker than 60 of 255 (so dense hatching does not count). Sheet `compare/E021-shadow-as-hatch.png`: commit 5185f1b against now on four views, black share in the labels.
- Result: at light 0.40 with attached shadow 0.3 the shadow was the same tone as the walls turned from the sun and did not read; confetti-like gaps broke it up. At 0.33 with nearly continuous strokes (`shadow_break_gap` 0.04), ambient 0.85 (walls turned away stay light), smaller gaps elsewhere (0.05) and no density gain on the shaft, the shadows read as dark hatched shapes against white lit walls and light ruled walls. Black share, before and after: `shaft_look_up` 64% to 1.0%, `shaft_hall_enter` 60% to 3.3%, `shaft_stair_view` 54% to 1.6%, `shaft_bridge_walk` 48% to 2.5%, `shaft_spawn` 45% to 2.4%, `shaft_arch_view` 12% to 0.9%, `shaft_rim_down` 5% to 0.1%. The remaining black is in recesses where ao is low; white specks and rules stay there.
- Verdict: kept as `nihei_vast`. Question for the owner on the sheet: do the shadows still read as shadows, and is the shaft now too grey?

## E022: The hatched shadow edge
- Date: 2026-11-16
- Feature: `features/ink-renderer.md`
- Question: with the shadow drawn as hatching, should its edge be a clean cut-off or feathered stroke ends, and should the shadow strokes run in their own direction?
- Setup: crisp (`shadow_feather` 0: each shadow stroke stops on the anti-aliased shadow edge) and feathered (`shadow_feather` 0.6 m: each stroke tests the shadow at a point moved along it by its own random amount); `shadow_hatch_dir` 0 (surface axis) and 1 (the sun's direction projected onto the surface). Sheet `compare/E022-shadow-edge-hatched.png`, 2x crops of `shaft_spawn` and `shaft_bridge_walk` at 2560x1440.
- Result: crisp reads as a cut shape filled with hatching; feathered as staggered pen ends along the edge, closer to the references' tone edges but less like a shadow. The sun direction separates a shadow from a hatched wall; on the shaft's vertical walls under a high sun it is close to the surface axis anyway. Feathered edges show occasional black specks on grazed faces, from the cheaper shadow test without receiver-plane bias. Stability under rotation: feathered 2.03 (5x5) against crisp 2.41 on `shaft_spawn_yaw` (less ink).
- Verdict: crisp, surface direction as default; both parameters kept for the owner to choose. Follow-up if feathered is chosen: the receiver-plane bias in the moved test.

## E023: Crosshatch options
- Date: 2026-11-16
- Feature: `features/ink-renderer.md`
- Question: the owner: crosshatching "always seems to look messy". Why, and what is cleaner?
- Setup: on `still_hall` (`nihei`): the previous crosshatch at 90 degrees; at 75 with 30 degrees jitter; none (`band_cross` 0 with `hatch_density_gain` 1, darker tone from heavier and denser single-direction lines); a shallow, light second layer only at the dark end (`cross_angle` 20, jitter 10, width 0.6, gap 0.6, `band_cross` 0.38). Sheet `compare/E023-crosshatch-options.png`, 2x crops.
- Result: why it was messy: the two layers had nearly the same weight, so neither led; each broke up on its own independent noise, so the second layer's ends landed anywhere and the crossings made a field of short fragments; near 90 degrees those fragments make a mesh. The shallow layer is so light it disappears at 1:1. With no crosshatch the corners still gather into heavy strokes running into black (what the owner liked) and the rest is calm.
- Verdict: no crosshatch in either preset. Question for the owner: is anything lost without it?

## E024: Feathered hatch boundaries
- Date: 2026-11-16
- Feature: `features/ink-renderer.md`
- Question: the owner: "too many hard edges where the hatching all seems to start at once, following a straight line". Where do straight hatch boundaries come from, and can they be feathered without swimming?
- Setup: sources found: per-panel tone (rectangles of tone with straight panel-border edges); every line of a band stopping on the same light contour; E019 patch groups starting together on a common edge; octave and patch-layout boundaries (removed in E020); a hatched shadow's edge (E022); hatching filling each face to its border. Changes: `hatch_feather` moves each stroke's end at a band edge along the line by its own amount in metres (scaled by the spacing of the line's level, so coarser lines run on further and the edge thins out), via the light gradient along the line; `hatch_end_jitter` staggers starts (skewed) and ends inside patch runs; `panel_tone` replaced by soft world noise (`tone_noise` 0.08 over 15 m) in `nihei_vast`. Sheet `compare/E024-feathered-boundaries.png`: 2x crops of `shaft_arch_view`, `shaft_spawn` and `still_hall`, straight against feathered.
- Result: tone edges on big walls become fringes of strokes of varying length with every second or fourth line running on; the panel rectangles are gone. Rotation probes show no change from feathering (`shaft_spawn_yaw` 2.735 with, 2.727 without). Not done: strokes stopping short of creases and face borders; the forward pass has no distance to a face's border.
- Verdict: kept in both presets (`hatch_feather` 6, `hatch_end_jitter` 0.1). Follow-up: an edge-distance attribute from the generator for stopping short of creases.

## E025: Patch hatching, nested
- Date: 2026-11-16
- Feature: `features/ink-renderer.md`
- Question: E019's patches with the owner's "moving in the right direction", rebuilt to be rigid (E020): how do they look?
- Setup: sheet `compare/E025-patches-nested.png`: the committed patches (5185f1b), per-line breakup (`hatch_patch=0`) and the nested patches, 2x crops of `still_hall` and `shaft_arch_view` at each scene's default preset.
- Result: the nested layout no longer makes pale blobs with zigzag edges (the committed version's camouflage look); groups of strokes have ragged starts and ends. Because runs are nested by line level, lines of a higher level continue through the breaks of their neighbours, so a group's break is thinned rather than a clean gap; and on the test hall's back wall some groups still end along a near-horizontal line. On the shaft, with the small gap fraction, the patch structure is subtle.
- Verdict: kept. Question for the owner: are the groups now reading as hand-laid, or still mechanical?

## E026: The owner's two frames: stair shadow edge, and rectangles on the hall wall
- Date: 2026-11-16
- Feature: `features/ink-renderer.md`
- Question: screenshot 20261116-000558 (the stair from below): "the hard edge that seems to follow the stairs but offset looks very unnatural"; strokes ending in identical arrowhead tips, mid-grey strokes, very thick bars near the wall. Screenshot 20261116-000630 (the hall wall at about 10 m): rectangular blocks of solid black and dense ruling with flat tops, turning back into hatching up close.
- Setup: replays with `megastructure --headless --shot screenshots/20261116-000558.json --out <dir>` (and -000630), with `--set` for the new defaults, because a sidecar carries its own preset values: `--set shadow_feather=0.8 --set shadow_light_min=0.36 --set shadow_light_max=0.48 --set shadow_ambient=0.55 --set hatch_density_gain=1`. New camera path `shaft_hall_wall` approaches the 000630 view from 78 m to 2 m (t 0.85 is the screenshot's distance). Sheet `compare/E026-owner-frames.png`: each frame before and after, at half size and as a native crop.
- Causes: 000558's edge is the stair's cast shadow drawn as hatching with the crisp cut-off (E022 default); its tips were the E019 taper, a fixed 6 px at both ends, symmetric, on bars up to 9 px wide (no width cap, `hatch_width_gain` 1); the grey strokes were the octave crossfade and anti-aliased breaks fading by opacity. 000630's rectangles were the diagnosis offered, with one correction: the band-edge jitter was shared by every line of a patch run (E019/E025), so a whole run, a column of 8 lines by one run length, switched band at once. Where the wall's light sat just above `band_black` that gave flat-topped slabs of solid black, and elsewhere flat-topped blocks of heavy ruling; strokes fusing contributed to the slabs' density. It changed with distance because the run size follows the line level drawn, which follows distance.
- Changes: band jitter per line only; feathered shadow edge by default (`shadow_feather` 0.8 m); every fade narrows the stroke instead of greying it (octave crossfade, band edge, break, shadow gate), so each mark is full ink; a stroke thins keeping one straight edge, the side random per stroke, over a taper of `hatch_taper_width` (5) times its width, at least `hatch_taper_px`, varied +-40%, short where it starts and long where it ends; line width capped at `hatch_width_max_px` (1.6 px at 720 lines), with `hatch_density_gain` 1 so spacing carries tone up close; shadow light 0.36 to 0.48 with `shadow_ambient` 0.55 so the deepest shadows stay dark and the rest are lighter hatching.
- Result: 000558's shadow ends in staggered stroke ends with no ruled edge; no arrowheads or grey strokes. 000630 has no flat-topped blocks; it stays a dark wall of dense ruling (the hall is deep and in shadow), with the same character at 78, 30, 10 and 2 m along `shaft_hall_wall`. Stability improved with narrowing: `shaft_spawn_yaw` pops 1.38% to 0.56% (rigid fixed-octave render 0.58%), `shaft_bridge_walk` walking 1.11% to 0.42%. Black share of shaft views 0 to 2.1%. Close to the hall wall (`shaft_hall_wall` t 0.8..0.82, 2.5 cm per frame) the error is high (4.7 levels 5x5, 2.5% pops): dense heavy ruling resampling at 2 to 3 px pitch.
- Verdict: kept. Follow-up: the hall interior is still too dark and dense up close; its openness and the shadow light deserve their own pass.

## Queued

Experiments the plan already calls for, in rough order:

- Stroke depth tolerance on dense generated geometry (thin parts). E003 covered the test scene.
- Hatch octave crossfade judged on 60 fps video rather than metrics (E002 follow-up).
- Far-field sun shadow from a ray march through the 32 m occupancy grid, in place of shadow maps beyond the near cascades (E011 covers M1 with a scene-fitted cascade instead).
- Thin occluders (railings) in the shadow pass (E012 follow-up).
- Per-patch stroke angle and weight variation (E019, E025 follow-up).
- Strokes stopping short of creases and face borders (E024 follow-up; needs an edge-distance attribute).
- Calming heavy distant hatching that resamples into moire in motion (E020 follow-up).
