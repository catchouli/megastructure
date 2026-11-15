# Capture tool

## Status

Working, 2026-11-15. Renders the hand-built test scene through the ink renderer (`ink-renderer.md`), with debug views, a reprojection-based temporal stability metric and a frame-time benchmark.

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
| `--t0`, `--t1` | Range of `t` for a sequence: frame `i` uses `t = t0 + (t1 - t0) * i / (N - 1)` | 0, 1 |
| `--view <name>` | Debug view: `final`, `id`, `normals`, `depth`, `bands`, `hatching`, `strokes`, `contours`, `lines` | `final` |
| `--metrics` | Print temporal stability figures for the sequence (see below). Switches paper grain off | |
| `--no-png` | Do not write frames (with `--metrics`); `--out` becomes optional | |
| `--error-png` | With `--metrics`, also write `error_NNNN.png`: blurred reprojection residual times 8, white is none | |
| `--bench <N>` | Render one frame (`--t`) N times without readback and print ms per frame | |
| `--hatch-lod <m>` | Diagnostic: `directional` (default), `isotropic`, `fixed` (E002) | |
| `--stroke-vis <m>` | Diagnostic: `block` (default), `centreline`, `pixel` (E003) | |
| `--depth-tol <x>` | Override the preset's stroke depth tolerance | |
| `--list-paths` | Print path names and exit | |

Files are named `frame_0000.png`, `frame_0001.png` and so on; each written path is printed to stdout. `RUST_LOG=info` prints the adapter.

Example: `mega_capture --path corridor_walk --frames 60 --width 1280 --height 720 --out captures/walk`.

### Temporal metrics

With `--metrics` the tool also reads back the depth buffer. For each pair of consecutive frames it prints, averaged over the sequence:

- consecutive MAD: mean absolute grey difference, at full resolution and after a 4x4 box downsample. Dominated by legitimate motion; kept for reference.
- reprojected MAD: every pixel of the current frame is moved back into the previous frame through its depth and both cameras, pixels that were off screen or hidden (linear depth differs by more than 1%) are skipped, and the previous frame is sampled bilinearly. Raw and after a 5x5 box blur, plus the share of pixels off by more than 64 levels ("pops").
- mean ink (255 minus mean grey) and both MADs divided by it, to compare views with different amounts of ink.

Ink that stays on surfaces leaves only resampling error; swimming, popping or shimmering ink adds to it. Compare a view against `strokes` or `lines` on the same stretch. A slow stretch, for example `--path corridor_walk --frames 40 --t0 0.05 --t1 0.062`, keeps per-frame motion near walking speed at 60 fps. See E001.

Benchmark example: `mega_capture --path still_hall --bench 200 --width 2560 --height 1440`. The figure is wall-clock time over back-to-back submissions, so it includes CPU work and is an upper bound on GPU time.

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

- Whether to add an optional contact sheet or video output for judging temporal stability.

## Related experiments

E001 (the temporal metric), E002, E003 (diagnostic switches).
