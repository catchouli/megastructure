# Capture tool

## Status

Working, 2026-11-15. Renders the hand-built test scene with the placeholder forward pass; it will pick up the ink passes as they land because it only calls `Renderer::render`.

## What it is

`mega_capture` is a headless command-line renderer. It draws a named camera path of the test scene offscreen with wgpu and writes PNG frames. It is how renderer changes are checked without a window, by people, agents and tests.

## Design

- No window and no surface: the target is an `Rgba8Unorm` texture, read back through a row-padded buffer and written with the `image` crate. Shader output is taken as display values.
- Same code path as the game: both call `mega_render::Renderer::render(encoder, target_view, camera)`.
- Output is deterministic for a given path, `t`, size and GPU: cameras are pure functions of `t`.
- Errors are explicit and exit with status 1: unknown path (lists valid names), bad numbers, `--t` with more than one frame, size above the GPU limit, no adapter, unwritable output directory. wgpu validation errors are caught and fail the run rather than producing a blank image.

## Technical details

Run from the workspace root: `cargo run --release -p mega_capture -- <options>`.

| Option | Meaning | Default |
|---|---|---|
| `--path <name>` | Camera path, required | |
| `--out <dir>` | Output directory, created if missing, required | |
| `--frames <N>` | Number of frames. Frame `i` uses `t = i / (N - 1)` | 1 |
| `--t <T>` | `t` in 0..1; only valid with `--frames 1` | 0.5 |
| `--width`, `--height` | Image size in pixels | 1280, 720 |
| `--list-paths` | Print path names and exit | |

Files are named `frame_0000.png`, `frame_0001.png` and so on; each written path is printed to stdout. `RUST_LOG=info` prints the adapter.

Example: `mega_capture --path corridor_walk --frames 60 --width 1280 --height 720 --out captures/walk`.

### Camera paths

Defined in `crates/render/src/paths.rs` as `fn(t: f32) -> Camera`, `t` clamped to 0..1. Coordinates are metres, Y up; the corridor runs along +Z from z=0, the arched doorway is at z=110..120, the hall spans x -30..30, z 120..270. Yaw 0 looks along +Z.

| Name | Motion |
|---|---|
| `corridor_walk` | Eye height 1.65 m, straight along the corridor axis from z=2 to z=160, through the arch and into the hall |
| `corridor_strafe` | Fixed at z=40, x from -1.3 to 1.3, looking down the corridor |
| `hall_orbit` | One lap at radius 22 m and 9 m height around the middle of the hall (z=195), looking at the centre |
| `still_hall` | One fixed view in the hall looking towards the arch, with the stair climbing the left wall; `t` is ignored |

To add a path, write the function, add it to `NAMES` and `by_name`.

### Code locations

- `crates/capture/src/main.rs`: the tool.
- `crates/render/src/paths.rs`: camera paths.
- `crates/gen/src/test_scene.rs`: the scene.

## Open questions

- Colour format of the capture target will need to change once the ink pass writes more than one target; the composite output should stay `Rgba8Unorm`.
- Whether to add an optional contact sheet or video output for judging temporal stability.

## Related experiments

None yet.
