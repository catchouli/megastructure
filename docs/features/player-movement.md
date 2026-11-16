# Player movement

## Status

Working, 2026-11-16: the movement model was replaced with one modelled on Quake 2's player movement (the owner, 2026-11-16: "the character controller is alright right now, but it feels a tiny bit clumsy. Are you familiar with quake 2? It has a really agile character controller that's a lot of fun to run and jump around with, any chance we can make it work like that?"). The collision primitives, falling, death, the stable spot, the fixed step and interpolation are as before (2026-11-15). Verified by unit tests on hand-built colliders and by scripted runs in the real shaft scene, with no window: the shaft route at run and walk, hopping the bridge and the stair, a strafe-jump bot, falls, and a jump over B1's railing. The feel has not been judged: nothing has been run in a window. The step height contract changed from 0.3 m to 0.45 m.

## What it is

First-person running, walking and jumping through the structure, with Quake's quick starts, crisp stops, big jump, air control and strafe-jumping. Falling is physically plain: gravity, air drag and a terminal velocity. Hitting a surface too fast is fatal. Falling into open void simply continues. The fly camera is kept as a debug mode.

## Design

- **Grounded movement.** Running by default, walking with Shift. Full speed in about 0.15 s, a stop in about 0.35 s. Step-up over ledges up to 0.45 m and smooth stair climbing.
- **Jumping.** Quake's jump in proportion to the body: 1.45 m high, 0.68 s in the air, so ledges up to about 1.9 m can be jumped onto (the step-up works in the air on the way down). No double jump, no climbing, no mantling. The 1.1 m railings can be jumped over.
- **Air control.** Steering in the air, and strafe-jumping (turning while holding a strafe key, hopping on landing) builds speed beyond the run speed, as in Quake.
- **Ledges.** Nothing stops the player at an edge. Walking or jumping off means falling.
- **Falling.** Gravity with quadratic drag, so speed approaches a terminal velocity instead of growing without bound.
- **Impact.** On landing, the speed along the surface normal decides the outcome: below a threshold nothing happens, above it the player dies. The threshold is set as a fall height (10 m). A middle band (stumble, camera dip) is optional.
- **Void.** A fall with nothing below continues indefinitely. The structure is finite, so a fall can leave it; the player keeps falling through open sky. Nothing resets it.
- **Death and reset.** On death, or on the manual reset key, the player returns to the last place they stood on stable ground for at least half a second, with solid ground all round them (not at a lip), and movement keys are ignored for the next 0.3 s.
- **Camera.** The eye is 1.65 m above the feet. There is no head bob. Mouse and arrow-key look are the same in both modes.
- **Modes.** Walking is the default in a scene with colliders; a scene without colliders (the test scene) starts in the fly camera. `F` switches between them at the current position, so the fly camera can be used to look at a place or to get somewhere a walker cannot.

Earlier megastructure games drew complaints for first-person platforming, not for scale, so no required route may depend on a jump. Jumping is there for freedom of movement.

### Controls

Bindings are by physical key position, so they do not depend on the keyboard layout.

| Action | Keys | Mode |
|---|---|---|
| Move (running) | W A S D | both |
| Look | Mouse, or the arrow keys (required: the game must be fully playable from the keyboard) | both |
| Jump | Space (a press; a press up to 0.1 s before landing still jumps) | walking |
| Walk | Shift (running is the default; `ALWAYS_RUN` swaps them) | walking (fly: five times faster) |
| Back to the last stable ground | R | walking |
| Switch between walking and the fly camera | F | both |
| Up / down | E or Space / Q or Ctrl | flying only; in walking E, Q and Ctrl do nothing |
| Fly speed | Scroll | flying |
| Screenshot | P or F12 (`screenshots.md`) | both |
| Quit | Escape | both |

Arrow-key look turns at a fixed angular rate of 90 degrees per second (`ARROW_TURN_RATE_DEG` in `crates/game/src/sim.rs`), independent of frame rate, with pitch clamped as for the mouse, and works alongside mouse look. There is no crouch.

`F` and `R` act on the key press (the rising edge): holding the key does not repeat it. In the windowed game a press shorter than one frame still counts. The stats window shows the horizontal speed while walking.

### For the owner: what to try, and what to change

Run `cargo run --release --bin megastructure -- --scene shaft` and try:

1. Starts and stops on the spawn gallery: tap W, let go. Should feel immediate, with a short slide (1.2 m from a run).
2. Running along B1 and up the stair, then down it at full speed: the eye should neither catch nor bounce.
3. A running jump, and steering in the air with A or D and the mouse.
4. Strafe-jumping on the hall floor: hold W and A, sweep the mouse left steadily (about 30 to 45 degrees per second), and tap Space as you land. The speed in the stats window should climb past 9.6 m/s. With the arrow keys the turn is fixed at 90 degrees per second, which is too fast to gain much.
5. Bunny-hopping along B1 and into a railing: the hop clears the 1.1 m railing and the player falls 400 m and returns to the bridge. Is that wanted?
6. Jump at a block or ledge up to about 1.9 m high: the feet catch its top on the way down and the player lands on it.

Constants to tweak, all in `crates/game/src/player.rs` under "Quake 2 movement": `RUN_SPEED`, `WALK_SPEED`, `ALWAYS_RUN`, `GROUND_ACCEL` (snappier starts), `FRICTION` and `STOP_SPEED` (stops), `AIR_ACCEL` (air control and strafe-jump gain), `JUMP_SPEED` and `GRAVITY` (jump height is `JUMP_SPEED^2 / 2 GRAVITY`, air time `2 JUMP_SPEED / GRAVITY`), `AUTO_HOP` (hold Space to keep hopping), and `STEP_HEIGHT`. `FATAL_FALL_HEIGHT` sets death by height whatever the gravity.

## Technical details

### Modules (`crates/game/src/`)

| File | Contents |
|---|---|
| `player.rs` | Every constant of the controller, in one place |
| `world.rs` | The collision world: colliders as oriented boxes behind a parry3d BVH, the exact capsule-box distance, the capsule sweep, depenetration, ground support test |
| `walker.rs` | `Walker`: the controller state and the fixed step `tick` (Quake-style friction, acceleration and jump, ground move, step-up, snap to ground, air move with air step-up, impact, death, reset) |
| `sim.rs` | `Sim`: the fly camera and the walker behind one `update(dt, input)`; mode switch, fixed-step accumulator, edge detection for F and R |
| `input.rs`, `script.rs` | `InputState` with `jump`, `toggle_fly`, `reset`, and `fast` (Shift: walk, or fly fast); the script actions (`walk` and `fast` are both Shift) |
| `shaft_tests.rs` | Test only: scripted walks in the real shaft |

Dependency: `parry3d` 0.31.1 (workspace dependency `parry3d = "0.31"`), used for the BVH (`partitioning::Bvh`, built once per scene from the collider bounds; the shaft has 1,288 colliders) and as the reference in tests. It works in glam 0.33 types through `glamx`; the game and `mega_gen` use glam 0.34, so values cross the boundary as components (`world.rs`). `nalgebra` is not pulled in beyond what parry brings.

### Collision

The position is the capsule's feet. The capsule is 1.8 m high with radius 0.3 m; its axis segment runs from 0.3 m to 1.5 m above the feet. Every `mega_gen::Collider` is a box (`Collider::to_obb`; a `Ramp` is a slanted slab), so there is one exact query: the distance between the axis segment and an oriented box, found by cutting the segment where it crosses the box's face planes and minimising the squared distance, a quadratic on each piece, then subtracting the radius. It gives the gap, the separating normal and the touched point exactly (also for a corner or an edge), and for a segment that meets the box, the depth and the nearest face.

The sweep is conservative advancement: the distance between two convex shapes translated along a line is a convex function of the distance travelled, so advancing by `gap / closing speed` never overshoots and converges in a few iterations. The capsule is treated as swollen by `SKIN` (4 mm), so it stops with that much gap and never rests touching a surface (touching is where numerical normals fail). A cast ignores surfaces it is touching but moving away from or along. parry's own `cast_shapes` was tried first and was not accurate enough near contact (E101).

`World::cast` returns the nearest hit over the boxes whose bounds meet the swept bounds (BVH query). `World::depenetrate` pushes the capsule out of any overlap (a safety net after a teleport or a mode switch; with the skin, a sweep never causes one).

### Moving: Quake 2's rules in metres

Each fixed step does, in order: depenetrate; a jump if one is due; friction and acceleration on the horizontal velocity; then a ground move or an air move. The rules are those of Quake 2's player movement (`pmove.c` in the GPL source release), re-derived here, not copied: Quake units are converted with `QUAKE_UNIT` = 1.8 / 56 m (the Quake 2 player is 56 units tall), so the proportions to the body are Quake's, and times are unchanged.

- **Wanted direction and speed.** The keys and the facing give a horizontal direction (diagonals are not faster) and the wanted speed: `RUN_SPEED`, or `WALK_SPEED` with Shift (swapped if `ALWAYS_RUN` is false).
- **Friction** (Quake's `PM_Friction`), on the ground only: the speed falls by `FRICTION` (6 per second) times the speed, but never by less than `FRICTION` times `STOP_SPEED` (3.21 m/s), so a run decays exponentially for 0.18 s and then stops linearly in 0.17 s, over 1.2 m in all. Below `REST_SPEED` (1 unit/s) it is zero.
- **Acceleration** (Quake's `PM_Accelerate`): add `accel * wanted speed * dt` along the wanted direction, but only up to the point where the speed along that direction reaches the wanted speed. Speed in other directions is never removed. On the ground `accel` is `GROUND_ACCEL` (10: full run speed in 0.15 s, 90 percent in 0.13 s, against friction). In the air it is `AIR_ACCEL` (1, as Quake 2, which uses the same rule with 1 in the air): about 9.6 m/s^2 of steering. Because only the component along the wanted direction is limited, holding a strafe key while turning so that it stays a little ahead of the velocity adds speed every step: that is strafe-jumping. Plain forward hopping does not gain.
- **Jump.** A press of Space (the rising edge; `AUTO_HOP` makes a held key hop on every landing) sets the vertical speed to `JUMP_SPEED` (8.68 m/s) on top of any upward speed. It is taken before friction, so the take-off step is an air step: a hop pressed on landing, or up to `JUMP_BUFFER` (0.1 s) before it, keeps all its speed. Quake 2 also needs a fresh press for each jump; the buffer is ours.
- **Ground move.** Up to five iterations of: sweep along the remaining motion, move to the hit, remove the part of the motion going into the surface, and if two surfaces wedge the motion, slide along their crease. A surface steeper than the maximum slope is treated as a vertical wall (its normal flattened to horizontal), so a steep ramp cannot be run up. The stored velocity then loses its part into those walls (Quake's velocity clip, with creases and three-plane stops), so speed is not stored against a wall and the speed along a wall is kept. Walkable slopes are climbed by sliding along them (horizontal speed falls by cos^2 of the slope while climbing, about 25 percent on a stair flight).
- **Step up.** If a ground move was stopped by something too steep to stand on, try a step: raise the capsule by up to `STEP_HEIGHT` (0.45 m; less under a ceiling), move forward, sweep down, and accept the landing if it is walkable, if the touched point is no more than the step height above the starting feet, and if it got further forward than the plain slide (Quake's `PM_StepSlideMove` keeps whichever of the two went further). The forward move is the step's motion, or 0.15 m if that is shorter, so the capsule ends far enough over the edge to stand on it and not on its corner (E102).
- **Stick to the ground.** After every ground move a `SNAP_DISTANCE` (0.45 m) sweep downwards snaps the feet onto walkable ground below; if there is none the walker is in the air (with zero vertical speed and its horizontal speed). This lets the player follow ramps and stairs down at full speed and over the ridge at the top of a flight without skipping off them.
- **Air move.** Gravity and drag (below), then the same slide, with surfaces as they are (a steep slope is slid down). Touching walkable ground while rising slower than `LAND_MAX_RISE` (5.79 m/s, Quake's 180 units/s) is a landing: the vertical speed is lost and the horizontal speed kept (E108). Otherwise the velocity is clipped against what was touched. Coming down against something steep, the step-up is tried in the air too, as Quake 2 does: a ledge whose top is within a step of the feet is landed on instead of slid down. That is why a 1.45 m jump reaches ledges up to about 1.9 m.
- **Eye.** `eye = feet + 1.65 m + eye_offset`. After a step-up (on the ground or in the air) or a snap down by more than 0.12 m beyond what the slope under the feet accounts for, the offset takes up the jump and decays at 14 per second (exponentially), so a ledge does not pop the picture. Slopes and stairs are never smoothed, at any speed: the eye follows them exactly (E103). Quake 2 smooths steps linearly over 0.1 s on the client; the exponential was kept.

### Falling

`a = g - k * v * |v|` with `k = g / v_t^2` is applied to the whole velocity vector, in the air only. `GRAVITY` is Quake's 800 units/s^2, 25.7 m/s^2 (2.6 g); `TERMINAL_VELOCITY` stays 55 m/s, so drag is 2.6 times stronger than before and the terminal velocity is reached sooner: 73 percent of it after 2 s, and a 400 m fall takes 8.8 s (11 s before). Drag also takes about 0.5 m/s off a 9.6 m/s run during one jump (Quake has no drag), so plain hopping is slightly slower than running. Integration is fixed at `FIXED_DT = 1/60 s`: a midpoint step for the velocity, and the displacement from the average of the old and new velocity, which gives the exact apex for a jump and a speed within 0.1 m/s of the analytic `v_t tanh(g t / v_t)`; the speed is clamped to `TERMINAL_VELOCITY`.

`Sim` runs the walker in whole fixed steps from an accumulator (`MAX_TICKS_PER_FRAME` = 8, further time is dropped), so the result depends only on the inputs and the number of steps, not on the frame rate; the same script gives bit-identical results. Looking (mouse and arrow keys) is applied per frame with the real `dt`. **Display between steps.** `Sim` keeps the walker's eye before and after the latest fixed step. With `Sim::interpolate` on (set by the windowed runner only) the camera is at `prev + (cur - prev) * alpha` with `alpha = accumulator / FIXED_DT`, so a 120 Hz display sees a smooth glide instead of 60 Hz steps. It lags the simulation by up to one step (17 ms, 16 cm at run speed) and does not slide across a death, a reset or a switch from the fly camera (`Walker::teleports`). Looking is not interpolated; it is immediate. With it off, which is the headless runner and every test, the camera is exactly the eye after the latest step (E106).

### Impact, death and reset

In the air, each contact is checked: the speed along its normal is `-v . n` for the velocity used for that step (and the downward speed for a step-up landing in the air). Above `fatal_impact_speed()` the player dies, on any surface. That speed is derived from `FATAL_FALL_HEIGHT` (10 m), the height kept from the first version: `v_t sqrt(1 - exp(-2 g h / v_t^2))`, 21.7 m/s under Quake gravity (it was 14 m/s under 9.81). Measured in tests: drops of 5, 8 and 9 m are survived and 11, 15 and 40 m are fatal, walking or running off the edge. A jump adds its 1.45 m to the drop below its take-off. Horizontal speed alone (strafe-jumping into a wall at up to 21 m/s) is not fatal.

The stable spot is the feet position of the latest step on which the player had been on the ground, without a break, for at least `STABLE_GROUND_TIME` (0.5 s) and `World::supported_ring` held: 16 points on a ring of `STABLE_RING_RADIUS` (0.65 m) around the capsule's axis each have either solid at their height (a wall or a rail, which is not an edge) or ground within `STABLE_RING_REACH` (0.5 m) above or below the feet. So a spot is never nearer than 0.64 m to a drop, a corner perch or the end of a bridge (E104, E105); on the stair the 1.5 m lane qualifies. Death and `R` both return to it at rest, on the ground, and then `RESPAWN_LOCKOUT` (0.3 s) ignores the movement keys (looking still works), so a held forward key does not carry the player straight back over an edge; afterwards it does. Deaths and resets are counted, and so are jumps (`Walker::jumps`). Before any spot qualifies the spawn is the stable spot. `F` into walking keeps the old spot. Walking off an edge returns the player 0.65 m or more back from that edge; a bunny-hopping player is never on the ground for half a second, so returns to where the hopping began.

### Constants

All in `crates/game/src/player.rs`. The first group is the table of the design and the generator's contracts.

| Constant | Value | Quake 2 origin | Contract for the generator? |
|---|---|---|---|
| `CAPSULE_HEIGHT` / `CAPSULE_RADIUS` | 1.8 m / 0.3 m | 56 / 16 units | Yes: minimum door and corridor clearance |
| `EYE_HEIGHT` | 1.65 m | | Yes: railing and sight-line heights |
| `STEP_HEIGHT` | 0.45 m (was 0.3) | 18 units would be 0.58 m | Yes: maximum unstepped level change |
| `MAX_SLOPE_DEG` | 40 degrees | normal 0.7, 45.6 degrees | Yes |
| `JUMP_SPEED` | 8.68 m/s (apex 1.45 m, 0.68 s) | 270 units/s | Yes: no required route needs it; railings under about 1.4 m can be jumped, ledges up to about 1.9 m reached |
| `RUN_SPEED` | 9.64 m/s | `pm_maxspeed` 300 units/s | No |
| `WALK_SPEED` | 3 m/s | (Quake's walk is 2/3 of the run) | No |
| `GRAVITY` | 25.7 m/s^2 | `sv_gravity` 800 units/s^2 | No |
| `TERMINAL_VELOCITY` | 55 m/s | (none) | No |
| `FATAL_FALL_HEIGHT` | 10 m (impact 21.7 m/s) | (Quake hurts, does not kill, at this height) | No |
| `STABLE_GROUND_TIME` | 0.5 s | | No |

Tuning and numerical constants:

| Constant | Value | Meaning |
|---|---|---|
| `QUAKE_UNIT` | 1.8 / 56 = 32.1 mm | Metres per Quake 2 unit |
| `GROUND_ACCEL`, `AIR_ACCEL` | 10, 1 per s (times the wanted speed) | `pm_accelerate`; Quake 2's air factor |
| `FRICTION`, `STOP_SPEED` | 6 per s, 3.21 m/s | `pm_friction`, `pm_stopspeed` 100 units/s |
| `REST_SPEED` | 0.032 m/s | Below this the ground speed is zero (1 unit/s) |
| `LAND_MAX_RISE` | 5.79 m/s | Rising slower than this onto walkable ground is a landing (180 units/s) |
| `ALWAYS_RUN`, `AUTO_HOP` | true, false | Shift walks; a jump needs a fresh press |
| `FIXED_DT`, `MAX_TICKS_PER_FRAME` | 1/60 s, 8 | Simulation step; most steps per rendered frame |
| `JUMP_BUFFER` | 0.1 s | A jump press this long before landing still jumps |
| `STABLE_RING_RADIUS`, `STABLE_RING_SAMPLES`, `STABLE_RING_REACH` | 0.65 m, 16, 0.5 m | Ground needed around a stable spot |
| `RESPAWN_LOCKOUT` | 0.3 s | Movement keys ignored after a death or reset |
| `SKIN` | 4 mm | Gap kept to surfaces |
| `SNAP_DISTANCE` | 0.45 m | Reach of stick-to-ground (equal to the step height) |
| `STEP_MIN_FORWARD` | 0.15 m | Forward probe of a step-up, when the step's own motion is shorter |
| `MAX_SLIDE_ITERATIONS` | 5 | Per move (`MAX_CLIP_PLANES`) |
| `EYE_SMOOTH_RATE`, `EYE_SMOOTH_MIN_DROP` | 14 per s, 0.12 m | Eye catch-up after steps and drops |

The scale: taking a Quake unit as an inch makes the player 1.42 m tall; scaling by height instead keeps Quake's proportions for a 1.8 m body (run 5.4 body heights per second, jump 0.8 of the body). The structure is kilometres across, so the faster run suits it. Stair dimensions (rise 150 to 180 mm, run 280 to 320 mm) are set in the generator and must suit the step-up and slope limits here: a 0.17 m rise on a 0.30 m run is a 29.5 degree slope, climbed as a ramp.

World positions are held as a cell index plus a local `f32` offset (or `f64`), so the controller keeps millimetre precision anywhere in the structure. Not built: the controller uses plain `f32` world coordinates, which resolve 0.5 mm out to 8 km from the origin; the shaft is within 600 m of it.

### Tests

Unit tests with no GPU, in `walker.rs`, `world.rs` and `sim.rs` on small hand-built collider sets:

- Constants: Quake's values scaled to the body; the fatal speed is 21.7 m/s.
- Standing on a box for 10 s does not move.
- Starts and stops: 90 percent of run speed in 0.13 s, all of it in 0.15 s, never more than `RUN_SPEED`; diagonals not faster; a stop from a run in 0.35 s over 1.2 m; walking tops out at `WALK_SPEED` and stops in under 0.2 s.
- Walls: a wall stops the walker with no speed stored into it; at 45 degrees it slides along; met at 10, 30 and 60 degrees the speed along the wall is never less than the run speed's component along it.
- Steps: 0.25, 0.40 and 0.44 m ledges are stepped and 0.5 m is not, running and walking.
- Slopes: 35 and 39 degree ramps are climbed and 41 and 45 degree ones are not, running and walking; back down 39 degrees the walker stays on the ground and the eye offset stays under 1 mm (E110); dropped onto a 55 degree slope the walker slides down it.
- A stair ramp (rise 0.17, run 0.30) is climbed and descended at walk and run with the eye monotonic, in steps within 25 percent of the slope's worth, and never airborne.
- Jump: apex 1.38 to 1.47 m (1.447 measured), 38 to 41 steps in the air; holding jump in the air adds nothing; the take-off keeps the run speed.
- Free fall: within 0.1 m/s of the analytic speed at 2 s, approaches 55 m/s and never exceeds it.
- Drops of 5, 8 and 9 m are survived and 11, 15 and 40 m are fatal, walking and running off the edge.
- Death returns to the last stable ground, as does `R`; less than half a second on the ground (three hops) does not set a spot; the ring check (a spot near a lip or the end of a strip is not stable, one beside a wall is); after a death with forward held and running the player returns 0.65 m from the edge and stays put for the 0.3 s lockout, as after `R`.
- A held jump jumps once, a second press jumps again, and a press 3 or 5 steps before landing jumps on landing while 12 or 30 steps before does not; a buffered hop keeps its speed (no friction step).
- Air control: a standing jump gains at most `AIR_ACCEL * RUN_SPEED` per second forward; turning with a strafe key in the air turns the heading by about 40 degrees in one running jump with no loss of speed.
- Strafe-jumping: a bot that hops on every landing and keeps the strafe direction at the best angle to the velocity reaches 21 m/s in 5 s; the same bot hopping straight never exceeds the run speed.
- No tunnelling: a 6 cm wall hit horizontally at 10, 20, 35 and 55 m/s at 24 angles up to 80 degrees off its normal, and a 6 cm plate hit from above at the same speeds at 24 tilts: never through, never inside.
- A running jump lands on 1.2, 1.6 and 1.8 m blocks and not on 2.1 m; it clears a 1.1 m railing.
- Stepping into void keeps falling for 15 s with no reset; a scripted run (with hops and strafes) twice is bit-identical; the camera at 240 Hz over the 60 Hz step moves evenly when interpolated and exactly with the eye when not; the exact capsule-box distance matches a brute-force search over 3,000 random poses; modes, `F` and `R` edges, and fixed-step behaviour in `Sim`.

Scenes: `shaft_tests.rs` walks the real shaft with the headless scripts: spawn to the hall at run and at walk speed (grounded on every frame, no death, bridge at y 400, stair top at the hall floor 421.42 m, in the hall, the eye never moving more than the stair slope allows at run speed in one frame); the stair up and down at both speeds, monotonic and never airborne; the feet against the generator's `stair_surface` walking and running; hopping (a press every 0.1 s) along B1, up the stair and down it, 40 hops with no death, no speed gain and never more than the jump's 1.447 m above the last ground; a step off B1 by the fly camera and a 400 m fall in 8.3 to 9.3 s ending in death and a return to the bridge; a running jump over B1's north railing ending the same way; a fall beside the structure that never ends; a ray down every quarter metre of B1; the headless state line of the spawn-to-hall route to the digit; and the stable spot following the player up the stair (R half way up returns within a metre).

## Open questions

- What the player sees on death: a cut to white, an ink splash, or nothing but the reset.
- Whether falling out of the structure entirely should reset after some time or distance. Currently nothing does: 20 s of falling beside the shaft from the spawn's height is still going, at 55 m/s.
- Whether a long fall needs any presentation (wind streak lines, camera shake) or should stay silent and still.
- Field of view. The research found nothing usable; the default 70 degrees is unchanged. There is no head bob.
- Interpolation shows the walker up to one step (17 ms, 16 cm at run speed) behind. Whether that lag is noticeable at 120 Hz is unjudged: nothing was run in a window. Extrapolation would remove it but overshoots at stops and walls; not tried.
- Nowhere narrower than about 1.3 m between drops can hold a stable spot (the ring check), so there a death or `R` goes back to the last wider place. None exists in the shaft; a narrow catwalk in a later scene would need a smaller ring.
- A jump tap shorter than one rendered frame is missed in the windowed game (the key is sampled once per frame); F and R are latched, Space is not. With bunny-hopping this matters more than before.
- **Railings no longer hold the player in** (E109). Railings are 6 cm thick boxes 1.1 m high, lower than the 1.45 m jump, and the air step-up reaches about 1.9 m. A running jump at any railing in the shaft clears it, and so does bunny-hopping into one (hopping down the stair at full speed carries the player across the landing and over its end railing). Death returns the player to the bridge, so it is cheap, but it is a change from "in the shaft the player cannot walk off an edge". Options: keep it (edges are open to a deliberate jump, as the owner wanted walking off an edge to mean falling); taller railings in the generator (about 1.5 m stops a jump, 2 m stops the air step-up as well); a lower `JUMP_SPEED` (an apex under 0.65 m, with the step, keeps 1.1 m railings shut, which is close to the old jump); or no air step-up.
- Pressing into a wall at an angle runs along it faster than the run speed (10.8 m/s at 30 degrees), a consequence of Quake's acceleration rule with the velocity clipped by the wall each step; Quake has the same. Not judged in play.
- Drag costs about 0.5 m/s per jump at run speed, so plain hopping is slightly slower than running; Quake has no drag. Drag on the vertical component only would remove it but let a run off a ledge carry much further across a long fall.
- Walking (Shift) reaches its 3 m/s in about 0.28 s, slower than the run's 0.15 s, because `STOP_SPEED` is above the walk speed; Quake's walk (2/3 of the run) does not have this. Raise `WALK_SPEED` or lower `STOP_SPEED` if walking feels sluggish.
- With the arrow keys (90 degrees per second) strafe-jumping gains little (about 12 m/s at best); it needs a slower, steady turn, which only the mouse gives. A slower arrow-key turn while strafing in the air would make it keyboard-playable; not built.
- No crouch: the capsule would need a second height with a check for room to stand, and Ctrl is down in fly mode.
- Quake 2 stops a jump for a moment after a hard landing (`PMF_TIME_LAND`); not built, so a hop on landing from any height is allowed.
- A capsule's round foot perches on a ledge's corner when its centre is up to 0.19 m past the edge, with the feet a few centimetres low, and slips off beyond that. Realistic, and visible only at edges.
- The windowed path (key handling for F and R, the stats overlay) is untested.

## Related experiments

E101 (parry shape casts or exact advancement), E102 (step-up with a round foot), E103 (smooth eye on stairs), E104 (where death returns to), E105 (ring of ground under the stable spot), E106 (showing the walker between steps), E107 (Quake 2 movement and its scale), E108 (landing while rising on a slope), E109 (railings and the Quake jump), E110 (eye smoothing beyond the slope).
