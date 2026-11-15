# Player movement

## Status

Working, 2026-11-15. A kinematic capsule controller walks the shaft from the spawn gallery, along bridge B1, up the 126-riser stair, along the ledge and through the arch into the hall, and falls, dies and resets as designed. Verified by unit tests on hand-built colliders and by scripted runs in the real shaft scene, with no window: the headless runner and the same scripts driven through `Sim` in tests. The windowed path compiles and shares all of the simulation code, but it has not been run (nothing opens a window in automated checks), so key handling and the on-screen stats are untested. The contract values (below) are unchanged and can be frozen before the generator core (M2).

## What it is

First-person walking, running and jumping through the structure. Falling is physically plain: gravity, air drag and a terminal velocity. Hitting a surface too fast is fatal. Falling into open void simply continues. The fly camera is kept as a debug mode.

## Design

- **Grounded movement.** Walk and run, with step-up over small ledges and smooth stair climbing.
- **Jumping.** A modest, human jump. No double jump, no climbing, no mantling.
- **Ledges.** Nothing stops the player at an edge. Walking or jumping off means falling.
- **Falling.** Gravity with quadratic drag, so speed approaches a terminal velocity instead of growing without bound. Limited air control.
- **Impact.** On landing, the speed along the surface normal decides the outcome: below a threshold nothing happens, above it the player dies. A middle band (stumble, camera dip) is optional.
- **Void.** A fall with nothing below continues indefinitely. The structure is finite, so a fall can leave it; the player keeps falling through open sky. Nothing resets it.
- **Death and reset.** On death, or on the manual reset key, the player returns to the last place they stood on stable ground for at least half a second.
- **Camera.** The eye is 1.65 m above the feet. There is no head bob. Mouse and arrow-key look are the same in both modes.
- **Modes.** Walking is the default in a scene with colliders; a scene without colliders (the test scene) starts in the fly camera. `F` switches between them at the current position, so the fly camera can be used to look at a place or to get somewhere a walker cannot.

Earlier megastructure games drew complaints for first-person platforming, not for scale, so no required route may depend on a jump. Jumping is there for freedom of movement.

### Controls

Bindings are by physical key position, so they do not depend on the keyboard layout.

| Action | Keys | Mode |
|---|---|---|
| Move | W A S D | both |
| Look | Mouse, or the arrow keys (required: the game must be fully playable from the keyboard) | both |
| Jump | Space (hold to hop on landing) | walking |
| Run | Shift | walking (fly: five times faster) |
| Back to the last stable ground | R | walking |
| Switch between walking and the fly camera | F | both |
| Up / down | E or Space / Q or Ctrl | flying only; in walking E, Q and Ctrl do nothing |
| Fly speed | Scroll | flying |
| Quit | Escape | both |

Arrow-key look turns at a fixed angular rate of 90 degrees per second (`ARROW_TURN_RATE_DEG` in `crates/game/src/sim.rs`), independent of frame rate, with pitch clamped as for the mouse, and works alongside mouse look.

`F` and `R` act on the key press (the rising edge): holding the key does not repeat it. In the windowed game a press shorter than one frame still counts.

## Technical details

### Modules (`crates/game/src/`)

| File | Contents |
|---|---|
| `player.rs` | Every constant of the controller, in one place |
| `world.rs` | The collision world: colliders as oriented boxes behind a parry3d BVH, the exact capsule-box distance, the capsule sweep, depenetration, ground support test |
| `walker.rs` | `Walker`: the controller state and the fixed step `tick` (ground move, step-up, snap to ground, air move, impact, death, reset) |
| `sim.rs` | `Sim`: the fly camera and the walker behind one `update(dt, input)`; mode switch, fixed-step accumulator, edge detection for F and R |
| `input.rs`, `script.rs` | `InputState` with `jump`, `toggle_fly`, `reset`; the script actions |
| `shaft_tests.rs` | Test only: scripted walks in the real shaft |

Dependency: `parry3d` 0.31.1 (workspace dependency `parry3d = "0.31"`), used for the BVH (`partitioning::Bvh`, built once per scene from the collider bounds; the shaft has 1,288 colliders) and as the reference in tests. It works in glam 0.33 types through `glamx`; the game and `mega_gen` use glam 0.34, so values cross the boundary as components (`world.rs`). `nalgebra` is not pulled in beyond what parry brings.

### Collision

The position is the capsule's feet. The capsule is 1.8 m high with radius 0.3 m; its axis segment runs from 0.3 m to 1.5 m above the feet. Every `mega_gen::Collider` is a box (`Collider::to_obb`; a `Ramp` is a slanted slab), so there is one exact query: the distance between the axis segment and an oriented box, found by cutting the segment where it crosses the box's face planes and minimising the squared distance, a quadratic on each piece, then subtracting the radius. It gives the gap, the separating normal and the touched point exactly (also for a corner or an edge), and for a segment that meets the box, the depth and the nearest face.

The sweep is conservative advancement: the distance between two convex shapes translated along a line is a convex function of the distance travelled, so advancing by `gap / closing speed` never overshoots and converges in a few iterations. The capsule is treated as swollen by `SKIN` (4 mm), so it stops with that much gap and never rests touching a surface (touching is where numerical normals fail). A cast ignores surfaces it is touching but moving away from or along. parry's own `cast_shapes` was tried first and was not accurate enough near contact (E101).

`World::cast` returns the nearest hit over the boxes whose bounds meet the swept bounds (BVH query). `World::depenetrate` pushes the capsule out of any overlap (a safety net after a teleport or a mode switch; with the skin, a sweep never causes one).

### Moving: collide and slide

Each fixed step does, in order: depenetrate; turn the keys and facing into a wanted velocity; accelerate; then a ground move or an air move.

- **Wanted velocity.** `WALK_SPEED` or `RUN_SPEED` along the held direction (diagonals are not faster). On the ground the horizontal velocity moves towards it at `GROUND_ACCEL` (30 m/s^2: walk speed in 0.08 s, run speed in 0.2 s) or brakes at `GROUND_DECEL` when no key is held. In the air only `AIR_ACCEL` (6 m/s^2) pushes along the wanted direction, up to the wanted speed, and never slows the player down.
- **Slide.** Up to five iterations of: sweep along the remaining motion, move to the hit, remove the part of the motion going into the surface, and if two surfaces wedge the motion, slide along their crease. On the ground a surface steeper than the maximum slope is treated as a vertical wall (its normal flattened to horizontal), so a steep ramp cannot be slid up. In the air surfaces are as they are.
- **Slopes.** A surface is walkable if its normal's Y is at least `cos(40 degrees)`. Walkable surfaces are climbed by sliding along them (horizontal speed falls by cos^2 of the slope, about 25 percent on a stair flight); steeper ones stop the walker.
- **Stick to the ground.** After every ground move a 0.3 m sweep downwards snaps the feet onto walkable ground below; if there is none the walker is in the air (with zero vertical speed). This is what lets the player follow ramps and stairs down and over the ridge at the top of a flight without skipping off them, and it makes the eye follow the surface exactly.
- **Step up.** If a move was stopped by something too steep to stand on, try a step: raise the capsule by up to 0.3 m (less under a ceiling), move forward, sweep down, and accept the landing if it is walkable, if the touched point is no more than 0.3 m above the starting feet, and if it got further forward than the plain slide. The forward move is the step's motion, or 0.15 m if that fails, so the capsule ends far enough over the edge to stand on it and not on its corner (E102). A 0.25 m ledge is stepped, 0.35 m is not.
- **Eye.** `eye = feet + 1.65 m + eye_offset`. After a step-up or a snap down of more than 0.12 m the offset takes up the jump and decays at 14 per second (exponentially), so a ledge does not pop the picture. Slopes and stairs are never smoothed: the eye follows them exactly (E103).
- **Jump.** On the ground with Space held: vertical speed `JUMP_SPEED` (3.2 m/s, apex about 0.52 m, about 0.65 s in the air). Holding Space hops again on landing.

### Falling

`a = g - k * v * |v|` with `k = g / v_t^2` is applied to the whole velocity vector (so horizontal speed also decays slightly). Integration is fixed at `FIXED_DT = 1/60 s`: a midpoint step for the velocity, and the displacement from the average of the old and new velocity, which gives the exact apex for a jump and a speed within 0.1 m/s of the analytic `v_t tanh(g t / v_t)`: 49.0 m/s (89 percent of terminal) after 8 s, 54.99 m/s after a minute, and the speed is clamped to `TERMINAL_VELOCITY` so it never exceeds it. A 400 m fall takes about 11 s.

`Sim` runs the walker in whole fixed steps from an accumulator (`MAX_TICKS_PER_FRAME` = 8, further time is dropped), so the result depends only on the inputs and the number of steps, not on the frame rate; the same script gives bit-identical results. Looking (mouse and arrow keys) is applied per frame with the real `dt`. The position is not interpolated between fixed steps, so on a display faster than 60 Hz the position advances in 60 Hz steps (open question).

### Impact, death and reset

In the air, each contact is checked: the speed along its normal is `-v . n` for the velocity used for that step. Above `FATAL_IMPACT_SPEED` (14 m/s, a drop of about 10 m without drag) the player dies, on any surface. Otherwise a contact with walkable ground while falling is a landing: the vertical speed is lost and the horizontal speed kept. Measured in tests: a 5 m and an 8 m drop are survived, 15 m and 40 m are fatal.

The stable spot is the feet position of the latest step on which the player had been on the ground, without a break, for at least `STABLE_GROUND_TIME` (0.5 s) and had a surface straight below the middle of the capsule (so a perch on a ledge's corner is not recorded, E104). Death and `R` both return to it at rest, on the ground. Deaths and resets are counted. Before any spot qualifies the spawn is the stable spot. `F` into walking keeps the old spot. Walking off an edge returns the player a few centimetres short of that edge.

### Constants

All in `crates/game/src/player.rs`. The first group is the table of the design and the generator's contracts.

| Constant | Value | Contract for the generator? |
|---|---|---|
| `CAPSULE_HEIGHT` / `CAPSULE_RADIUS` | 1.8 m / 0.3 m | Yes: minimum door and corridor clearance |
| `EYE_HEIGHT` | 1.65 m | Yes: railing and sight-line heights |
| `WALK_SPEED` | 2.5 m/s | No |
| `RUN_SPEED` | 6 m/s | No |
| `STEP_HEIGHT` | 0.3 m | Yes: maximum unstepped level change |
| `MAX_SLOPE_DEG` | 40 degrees | Yes |
| `JUMP_SPEED` | 3.2 m/s (apex about 0.5 m) | Yes: no required route needs it |
| `GRAVITY` | 9.81 m/s^2 | No |
| `TERMINAL_VELOCITY` | 55 m/s | No |
| `FATAL_IMPACT_SPEED` (normal component) | 14 m/s (a drop of about 10 m) | No |
| `STABLE_GROUND_TIME` | 0.5 s | No |

Tuning and numerical constants:

| Constant | Value | Meaning |
|---|---|---|
| `FIXED_DT`, `MAX_TICKS_PER_FRAME` | 1/60 s, 8 | Simulation step; most steps per rendered frame |
| `GROUND_ACCEL`, `GROUND_DECEL`, `AIR_ACCEL` | 30, 45, 6 m/s^2 | Control |
| `SKIN` | 4 mm | Gap kept to surfaces |
| `SNAP_DISTANCE` | 0.3 m | Reach of stick-to-ground (equal to the step height) |
| `STEP_MIN_FORWARD` | 0.15 m | Forward probe of a step-up, when the step's own motion is shorter |
| `MAX_SLIDE_ITERATIONS` | 5 | Per move |
| `EYE_SMOOTH_RATE`, `EYE_SMOOTH_MIN_DROP` | 14 per s, 0.12 m | Eye catch-up after steps and drops |

Walk speed is faster than a real walk (1.4 m/s) because the structure is kilometres across. Stair dimensions (rise 150 to 180 mm, run 280 to 320 mm) are set in the generator and must suit the step-up and slope limits here: a 0.17 m rise on a 0.30 m run is a 29.5 degree slope, climbed as a ramp.

World positions are held as a cell index plus a local `f32` offset (or `f64`), so the controller keeps millimetre precision anywhere in the structure. Not built: the controller uses plain `f32` world coordinates, which resolve 0.5 mm out to 8 km from the origin; the shaft is within 600 m of it.

### Tests

Unit tests with no GPU, in `walker.rs`, `world.rs` and `sim.rs` on small hand-built collider sets: standing on a box for 10 s does not move; a wall stops the walker and is slid along at an angle; a 0.25 m ledge is stepped up and a 0.35 m one is not (also at a run); a 35 degree ramp is climbed and 45 and 41 degree ramps are not (39 is); a stair ramp (rise 0.17, run 0.30) is climbed and descended at walk and run with the eye monotonic, in steps within 25 percent of the slope's worth, and never airborne; the jump apex is 0.47 to 0.56 m; free fall reaches 0.89 of terminal at 8 s, approaches 55 m/s and never exceeds it; drops of 5 and 8 m are survived and 15 and 40 m are fatal; death returns to the last stable ground, as does `R`; less than half a second on the ground does not set a spot; stepping into void keeps falling for 15 s with no reset; a scripted run twice is bit-identical; air control is limited; the exact capsule-box distance matches a brute-force search over 3,000 random poses; modes, `F` and `R` edges, and fixed-step behaviour in `Sim`.

Scenes: `shaft_tests.rs` walks the real shaft with the headless scripts: spawn to the hall at run and at walk speed (grounded on every frame, no death, bridge at y 400, stair top at the hall floor 421.42 m, in the hall), the stair up and down at both speeds, the feet against the generator's `stair_surface`, a step off bridge B1 and a 400 m fall ending in death and a return to the bridge, a fall beside the structure that never ends, and a ray down every quarter metre of B1.

## Open questions

- What the player sees on death: a cut to white, an ink splash, or nothing but the reset.
- Whether falling out of the structure entirely should reset after some time or distance. Currently nothing does: 20 s of falling beside the shaft, from the spawn's height, ends 486 m below the shaft's floor at 54.9 m/s and is still going.
- Whether a long fall needs any presentation (wind streak lines, camera shake) or should stay silent and still.
- Field of view. The research found nothing usable; the default 70 degrees is unchanged. There is no head bob.
- The return point after walking off a ledge is the edge itself, so a player still holding forward goes straight off again. A spot recorded half a second late, or a step back from edges, would be gentler (E104).
- Position is not interpolated between 60 Hz steps; at 120 or 144 Hz the motion will judder slightly. Interpolating makes the headless state lag by a step unless handled; not built.
- A held Space hops on every landing. An edge-triggered jump (needs a press, not a hold) may be better.
- Railings are 6 cm thick boxes 1.1 m high: a 1.8 m capsule cannot jump or climb over them, and every bridge, landing and stair flight in the shaft is railed, so in the shaft the player cannot walk off an edge at all (the scripted falls switch to the fly camera to move over the void). Whether that is wanted, or some edges should be open, is a generator decision.
- A capsule's round foot perches on a ledge's corner when its centre is up to 0.19 m past the edge, with the feet a few centimetres low, and slips off beyond that. Realistic, and visible only at edges.
- The windowed path (key handling for F and R, the stats overlay) is untested.

## Related experiments

E101 (parry shape casts or exact advancement), E102 (step-up with a round foot), E103 (smooth eye on stairs), E104 (where death returns to).
