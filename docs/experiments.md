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

## Queued

Experiments the plan already calls for, in rough order:

- Distance masking of ids and stroke rank cut-offs on a view down a 600 m shaft: does far structure simplify or turn to noise? (E004 covered the 110 m test corridor only.)
- Stroke depth tolerance on dense generated geometry (thin parts). E003 covered the test scene.
- Hatch octave crossfade judged on 60 fps video rather than metrics (E002 follow-up).
- Sun shadow as solid black shapes: stability of the thresholded shadow edge as the sun moves.
- Far-field sun shadow from a ray march through the 32 m occupancy grid, in place of shadow maps beyond the near cascades.
