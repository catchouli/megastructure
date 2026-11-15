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

None yet.

## Queued

Experiments the plan already calls for, in rough order:

- Hatch octave selection from the directional derivative of the hatch coordinate, judged on video of a long corridor floor at grazing angles.
- Distance masking of ids and stroke rank cut-offs on a view down a 600 m shaft: does far structure simplify or turn to noise?
- Depth-tolerance test for stroke visibility on dense geometry.
- Sun shadow as solid black shapes: stability of the thresholded shadow edge as the sun moves.
- Far-field sun shadow from a ray march through the 32 m occupancy grid, in place of shadow maps beyond the near cascades.
