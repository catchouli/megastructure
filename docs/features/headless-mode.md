# Headless mode

## Status

Working, 2026-11-15. Verified by tests and headless runs only; the windowed path shares the same simulation code but has not been run in an automated check. Walking (`player-movement.md`) is the default in scenes with colliders; the fly camera is the other mode.

## What it is

`megastructure --headless` runs the game's update and render loop with no window and no surface, at a fixed timestep, into an offscreen texture. Input comes from a short script instead of a keyboard. It lets an agent or a test drive the real game code, see frames, and read back the camera state.

## Design

- The game is split so that nothing but the windowed runner touches winit: `Sim` (the walking controller and the fly camera), `InputState` (held actions), `App` (sim plus renderer: `update(dt, &InputState)` and `render(encoder, target_view)`). The windowed and headless runners both drive `App`.
- Each frame is one `update(dt, input)` followed by one render. Frame `i` (0-based) uses the script input for step `i`.
- Time is fixed: `dt` is never measured, so a run is reproducible.
- The egui overlay is skipped.
- Any wgpu validation error fails the run with a non-zero exit code.

### CLI

```
megastructure --headless [--frames N] [--dt S] [--width W] [--height H]
              [--input SCRIPT] [--out DIR] [--every K] [--scene NAME] [--mode walk|fly]
```

| Option | Meaning | Default |
|---|---|---|
| `--frames N` | Number of frames | Script length in frames, or 1 without a script |
| `--dt S` | Seconds per frame | 1/60 |
| `--width W`, `--height H` | Offscreen target size | 1280 x 720 |
| `--input SCRIPT` | Scripted input (grammar below) | Idle |
| `--out DIR` | Write `frame_NNNN.png` here (created if missing) | No files |
| `--every K` | With `--out`, write frames whose 0-based index is divisible by K, plus the last | 0: last frame only |
| `--scene NAME` | Scene from `mega_gen::scenes` (also for the windowed game) | `mega_gen::scenes::DEFAULT` |
| `--mode walk\|fly` | Start walking (first person, with collision) or flying (the debug camera); also for the windowed game | `walk` if the scene has colliders (`shaft`), else `fly` (`test`) |

`--mode walk` on a scene without colliders is an error. A walking run starts with the eye at the scene's spawn; `toggle_fly` in the script switches mode at the current position. An unknown scene, a bad script or an invalid option prints one `megastructure: error:` line to stderr (an unknown scene lists the available names) and exits with status 1. Options that only make sense headless (`--frames`, `--input`, and so on) are rejected without `--headless`.

### Script grammar

```
script  = segment { "," segment }
segment = action { "+" action } ":" seconds
action  = forward | back | left | right | up | down | fast | run
        | jump | toggle_fly | reset
        | look_left | look_right | look_up | look_down | idle
```

Segments run in sequence; `+` holds several actions at once (`forward+look_left:1.5`). `idle` holds nothing and cannot be combined. `seconds` is a non-negative number. Whitespace around tokens is ignored. After the script ends the input is idle for any remaining frames.

Durations are converted to frames by rounding the cumulative time over `dt`, so whole-second segments at 1/60 s are exactly 60 frames each. The actions are the keyboard's: `forward`/`back`/`left`/`right` are W S A D, `up` E, `down` Q or Ctrl, `jump` Space, `fast` and `run` (the same action) Shift, `toggle_fly` F, `reset` R, and the four `look_*` the arrow keys. Looks turn at 90 degrees per second. `look_right` decreases yaw, since yaw 0 faces +Z and positive yaw turns towards +X.

- **Walking:** `forward` and friends move along the facing at 2.5 m/s, 6 m/s with `run`; `jump` jumps when on the ground (on the first step it is held; held longer, it does not hop again, and a press up to 0.1 s before landing jumps on landing); `up` and `down` do nothing. `reset` returns to the last stable ground and the movement keys are ignored for the next 0.3 s. Walking runs in fixed steps of 1/60 s whatever `--dt` is (a `--dt` of 1/30 runs two steps per frame).
- **Flying:** moves are in the facing direction at the base speed (8 m/s, five times that with `fast` or `run`); `up` and `jump` go up, `down` goes down.
- **`toggle_fly` and `reset` act on the first frame they are held**, so `toggle_fly:1` toggles once, not sixty times. Give the segment a whole frame at least: at 1/60 s `toggle_fly:0.02` is one frame. Scenes without colliders ignore both.

For example, from the shaft's spawn to the hall (walking, about 54 s of simulated time): `idle:1,look_right:1,forward:3.7,look_left:1,run+forward:11,look_left:1,run+forward:16,look_left:1,run+forward:5.1,look_right:1,run+forward:12`. The turns are the arrow keys' exact 90 degrees per second, so each `look_*:1` is a quarter turn.

### What it prints

On success, one line on stdout:

```
state frames=3228 x=-0.7043 y=423.0740 z=-101.1447 yaw_deg=180.000 pitch_deg=0.000 mode=walk grounded=true vy=0.0000 died=false deaths=0 resets=0
```

Camera (eye) position in metres, yaw and pitch in degrees, and the number of frames run; then `mode` (`walk` or `fly`), `grounded` (the walker is standing on something; always `false` in a run that never walked), `vy` (the walker's vertical speed in m/s, negative falling; 0 while grounded and in a run that never walked), `died` (whether the player died at any time in the run), `deaths` (how many times) and `resets` (how many times `reset` was used). PNG paths are reported on stderr as `wrote <path>`, so stdout holds only the state line.

Commands used to check walking in the shaft (add `--width 160 --height 90` to make the offscreen frames cheap; a scripted `--out` still works):

```
# spawn to the hall at run speed
megastructure --headless --scene shaft --input "idle:1,look_right:1,forward:3.7,look_left:1,run+forward:11,look_left:1,run+forward:16,look_left:1,run+forward:5.1,look_right:1,run+forward:12"
# onto bridge B1, over the rail by flying, back to walking: a 400 m fall, death at the bottom, return to the bridge
megastructure --headless --scene shaft --input "idle:1,look_right:1,forward:3.7,look_left:1,run+forward:1,toggle_fly:0.02,right:0.5,toggle_fly:0.02,idle:16"
# out beside the shaft, back to walking: an endless fall
megastructure --headless --scene shaft --input "toggle_fly:0.02,fast+forward:3,toggle_fly:0.02,idle:20"
```

## Technical details

- Code: `crates/game/src/` with `main.rs` (entry, scene lookup), `cli.rs`, `input.rs`, `script.rs`, `sim.rs`, `app.rs`, `headless.rs`, `windowed.rs`, and for walking `player.rs` (constants), `world.rs` (collision), `walker.rs` (controller); `shaft_tests.rs` runs the same scripts through `Sim` without a GPU.
- Target format `Rgba8Unorm`, the same as `mega_capture`; PNG readback pads rows to `COPY_BYTES_PER_ROW_ALIGNMENT` and strips the padding.
- Errors are caught with `Device::on_uncaptured_error` setting a flag that is checked after every frame and at the end.
- Constants: the fly camera's in `sim.rs` (`BASE_SPEED`, `FAST_MULTIPLIER`, `ARROW_TURN_RATE_DEG`, `PITCH_LIMIT`, `MOUSE_SENSITIVITY`), the walker's in `player.rs`.
- The state line is built by `headless::state_line`, which the shaft tests also call.

### How it differs from `mega_capture`

| | `mega_capture` | `megastructure --headless` |
|---|---|---|
| Camera | A named path sampled at t in 0..1 | The player (walking or the fly camera), driven by scripted input at a fixed dt |
| Scene | Test scene | Any named scene |
| Code path | Renderer only | The game's own `App`: simulation and renderer |
| Extras | Debug views, metrics, benchmark | None; state line on stdout |

Use `mega_capture` for look and performance work on a fixed path; use headless mode to check that the game's controls and simulation do what they should.

## Open questions

- Scripted mouse look and a scripted speed change (scroll) are not supported.
- There is no way to start the walker somewhere else than the spawn; the scripts that need to (a fall) switch to the fly camera, move, and switch back.
- Whether the state line should also be available as JSON.

## Related experiments

None yet.
