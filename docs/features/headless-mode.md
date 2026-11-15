# Headless mode

## Status

Working, 2026-11-15. Verified by tests and headless runs only; the windowed path shares the same simulation code but has not been run in an automated check.

## What it is

`megastructure --headless` runs the game's update and render loop with no window and no surface, at a fixed timestep, into an offscreen texture. Input comes from a short script instead of a keyboard. It lets an agent or a test drive the real game code, see frames, and read back the camera state.

## Design

- The game is split so that nothing but the windowed runner touches winit: `Sim` (fly camera), `InputState` (held actions), `App` (sim plus renderer: `update(dt, &InputState)` and `render(encoder, target_view)`). The windowed and headless runners both drive `App`.
- Each frame is one `update(dt, input)` followed by one render. Frame `i` (0-based) uses the script input for step `i`.
- Time is fixed: `dt` is never measured, so a run is reproducible.
- The egui overlay is skipped.
- Any wgpu validation error fails the run with a non-zero exit code.

### CLI

```
megastructure --headless [--frames N] [--dt S] [--width W] [--height H]
              [--input SCRIPT] [--out DIR] [--every K] [--scene NAME]
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

An unknown scene, a bad script or an invalid option prints one `megastructure: error:` line to stderr (an unknown scene lists the available names) and exits with status 1. Options that only make sense headless (`--frames`, `--input`, and so on) are rejected without `--headless`.

### Script grammar

```
script  = segment { "," segment }
segment = action { "+" action } ":" seconds
action  = forward | back | left | right | up | down | fast
        | look_left | look_right | look_up | look_down | idle
```

Segments run in sequence; `+` holds several actions at once (`forward+look_left:1.5`). `idle` holds nothing and cannot be combined. `seconds` is a non-negative number. Whitespace around tokens is ignored. After the script ends the input is idle for any remaining frames.

Durations are converted to frames by rounding the cumulative time over `dt`, so whole-second segments at 1/60 s are exactly 60 frames each. The actions are the keyboard's: `forward`/`back`/`left`/`right` are W S A D, `up`/`down` Space and Ctrl, `fast` Shift, and the four `look_*` the arrow keys. Moves are in the facing direction at the base speed (8 m/s, five times that with `fast`); looks turn at 90 degrees per second. `look_right` decreases yaw, since yaw 0 faces +Z and positive yaw turns towards +X.

### What it prints

On success, one line on stdout:

```
state frames=240 x=0.0000 y=1.6500 z=26.0000 yaw_deg=-45.000 pitch_deg=0.000
```

Camera position in metres, yaw and pitch in degrees, and the number of frames run. PNG paths are reported on stderr as `wrote <path>`, so stdout holds only the state line.

## Technical details

- Code: `crates/game/src/` with `main.rs` (entry, scene lookup), `cli.rs`, `input.rs`, `script.rs`, `sim.rs`, `app.rs`, `headless.rs`, `windowed.rs`.
- Target format `Rgba8Unorm`, the same as `mega_capture`; PNG readback pads rows to `COPY_BYTES_PER_ROW_ALIGNMENT` and strips the padding.
- Errors are caught with `Device::on_uncaptured_error` setting a flag that is checked after every frame and at the end.
- Constants in `sim.rs`: `BASE_SPEED`, `FAST_MULTIPLIER`, `ARROW_TURN_RATE_DEG`, `PITCH_LIMIT`, `MOUSE_SENSITIVITY`.

### How it differs from `mega_capture`

| | `mega_capture` | `megastructure --headless` |
|---|---|---|
| Camera | A named path sampled at t in 0..1 | The fly camera, driven by scripted input at a fixed dt |
| Scene | Test scene | Any named scene |
| Code path | Renderer only | The game's own `App`: simulation and renderer |
| Extras | Debug views, metrics, benchmark | None; state line on stdout |

Use `mega_capture` for look and performance work on a fixed path; use headless mode to check that the game's controls and simulation do what they should.

## Open questions

- Scripted mouse look and a scripted speed change (scroll) are not supported.
- Whether the state line should also be available as JSON.

## Related experiments

None yet.
