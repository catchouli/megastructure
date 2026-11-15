# Player movement

## Status

Designed, 2026-11-15. Not implemented. All numbers are provisional until tuned in M1, but the contract values must be frozen before the generator core (M2).

## What it is

First-person walking, running and jumping through the structure. Falling is physically plain: gravity, air drag and a terminal velocity. Hitting a surface too fast is fatal. Falling into open void simply continues.

## Design

- **Grounded movement.** Walk and run, with step-up over small ledges and smooth stair climbing.
- **Jumping.** A modest, human jump. No double jump, no climbing, no mantling.
- **Ledges.** Nothing stops the player at an edge. Walking or jumping off means falling.
- **Falling.** Gravity with quadratic drag, so speed approaches a terminal velocity instead of growing without bound. Limited air control.
- **Impact.** On landing, the speed along the surface normal decides the outcome: below a threshold nothing happens, above it the player dies. A middle band (stumble, camera dip) is optional.
- **Void.** A fall with nothing below continues indefinitely. The structure is finite, so a fall can leave it; the player keeps falling through open sky.
- **Death and reset.** On death, or on a manual reset key, the player returns to the last place they stood on stable ground for more than a moment.

Earlier megastructure games drew complaints for first-person platforming, not for scale, so no required route may depend on a jump. Jumping is there for freedom of movement.

## Technical details

Kinematic capsule controller on parry3d shape casts (or rapier's character controller if it proves adequate). Per-element colliders; a ramp collider for every stair flight.

Falling uses `a = g - k * v * |v|` with `k = g / v_t^2`, integrated at a fixed timestep. With `v_t` = 55 m/s the player reaches about 90% of terminal velocity after roughly 8 s and 270 m. A 16 km fall takes about five minutes.

| Constant | Value | Contract for the generator? |
|---|---|---|
| Capsule height / radius | 1.8 m / 0.3 m | Yes: minimum door and corridor clearance |
| Eye height | 1.65 m | Yes: railing and sight-line heights |
| Walk speed | 2.5 m/s | No |
| Run speed | 6 m/s | No |
| Step-up height | 0.3 m | Yes: maximum unstepped level change |
| Maximum walkable slope | 40 degrees | Yes |
| Jump take-off speed | 3.2 m/s (apex about 0.5 m) | Yes: no required route needs it |
| Gravity | 9.81 m/s^2 | No |
| Terminal velocity | 55 m/s | No |
| Fatal impact speed (normal component) | 14 m/s (a drop of about 10 m) | No |

Walk speed is faster than a real walk (1.4 m/s) because the structure is kilometres across. Stair dimensions (rise 150 to 180 mm, run 280 to 320 mm) are set in the generator and must suit the step-up and slope limits here.

World positions are held as a cell index plus a local `f32` offset (or `f64`), so the controller keeps millimetre precision anywhere in the structure.

## Open questions

- What the player sees on death: a cut to white, an ink splash, or nothing but the reset.
- Whether falling out of the structure entirely should reset after some time or distance.
- Whether a long fall needs any presentation (wind streak lines, camera shake) or should stay silent and still.
- Head bob and field of view. The research found nothing usable; to be tuned by hand.

## Related experiments

None yet.
