# Capture tool

## Status

Working, 2026-11-15. Renders any registered scene (`test`, `shaft`) through the ink renderer (`ink-renderer.md`), with debug views, a reprojection-based temporal stability metric and a frame-time benchmark.

## What it is

`mega_capture` is a headless command-line renderer. It draws a named camera path of a named scene offscreen with wgpu and writes PNG frames. It is how renderer changes are checked without a window, by people, agents and tests.

## Design

- No window and no surface: the target is an `Rgba8Unorm` texture, read back through a row-padded buffer and written with the `image` crate. Shader output is taken as display values.
- Same code path as the game: both call `mega_render::Renderer::render(encoder, target_view, camera)`.
- Output is deterministic for a given path, `t`, size and GPU: cameras are pure functions of `t`.
- Errors are explicit and exit with status 1: unknown path (lists valid names), bad numbers, `--t` with more than one frame, size above the GPU limit, no adapter, unwritable output directory. wgpu validation errors are caught and fail the run rather than producing a blank image.

## Technical details

Run from the workspace root: `cargo run --release -p mega_capture -- <options>`.

| Option | Meaning | Default |
|---|---|---|
| `--path <name>` | Camera path, required unless `--camera` is given | |
| `--camera <x,y,z,yaw,pitch[,fov_y]>` | A fixed camera instead of a path, in metres and radians exactly as in a screenshot sidecar's `camera` block (yaw 0 looks along +Z, positive towards +X; pitch positive up). Use with `--scene`; the path name printed is `camera` (E039) | 70 degree `fov_y` |
| `--scene <name>` | Scene from `mega_gen::scenes::NAMES`. The scene also selects its style preset (`StylePreset::for_scene`) and sun (`Sun::for_scene`). A path tuned to another scene renders with a warning | the path's own scene (`paths::scene_of`), else `mega_gen::scenes::DEFAULT` |
| `--preset <name>` | Style preset instead of the scene's: `nihei`, `nihei_vast`, `nihei_fall`, `nihei_wash`, `nihei_hybrid`, `nihei_ink`, `nihei_ink_wash`, `nihei_ink_small` (`StylePreset::NAMES`). Without it the scene's preset is used, or the one named by `MEGA_PRESET` if that is set | the scene's |
| `--out <dir>` | Output directory, created if missing, required | |
| `--frames <N>` | Number of frames. Frame `i` uses `t = i / (N - 1)` | 1 |
| `--t <T>` | `t` in 0..1; only valid with `--frames 1` | 0.5 |
| `--width`, `--height` | Image size in pixels | 1280, 720 |
| `--t0`, `--t1` | Range of `t` for a sequence: frame `i` uses `t = t0 + (t1 - t0) * i / (N - 1)` | 0, 1 |
| `--view <name>` | Debug view: `final`, `id`, `normals`, `depth`, `bands`, `hatching`, `strokes`, `contours`, `lines`, `shadow` | `final` |
| `--set <field>=<value>` | Override one style preset field, repeatable (`StylePreset::set`; colours and per-rank fields take comma-separated lists, `sky_tone` also `none`). Checked before any GPU work | |
| `--list-fields` | Print the preset field names and exit | |
| `--sun-elev <deg>`, `--sun-azim <deg>` | Sun elevation and azimuth (from +Z towards +X) | the scene's sun |
| `--sun-elev-end <deg>`, `--sun-azim-end <deg>` | Animate the sun linearly to these over the sequence; with `--t0 T --t1 T` the camera stays still | |
| `--sun-dist <m>` | A near sun this far from the centre of the scene bounds (perspective shadows) | remote |
| `--sun-intensity <x>` | Sun intensity; 0 switches the sun and its shadow pass off | the scene's |
| `--shadow-filter <m>` | Diagnostic: `smooth` (default), `bilinear`, `hard` (E012) | |
| `--metrics` | Print temporal stability figures for the sequence (see below). Switches paper grain off | |
| `--centre-ink` | Print the mean ink (255 minus mean grey) of the central 20% x 20% of each frame: the tone of a surface filling the middle of the view, for tone-against-distance checks (E029) | |
| `--no-png` | Do not write frames (with `--metrics`); `--out` becomes optional | |
| `--error-png` | With `--metrics`, also write `error_NNNN.png`: blurred reprojection residual times 8, white is none | |
| `--bench <N>` | Render one frame (`--t`) N times without readback and print ms per frame | |
| `--hatch-lod <m>` | Diagnostic: `directional` (default, rotation-invariant), `isotropic`, `fixed`, `screen` (the screen-derivative choice before E020) (E002, E020). With `tone_model` 1, `fixed` and `screen` act inside the ruled layer (octave 0, or the screen derivative) and `isotropic` has no effect | |
| `--stroke-vis <m>` | Diagnostic: `block` (default), `centreline`, `pixel` (E003) | |
| `--depth-tol <x>` | Override the preset's stroke depth tolerance | |
| `--list-paths` | Print path names with their scenes and exit | |
| `--list-scenes` | Print scene names and exit | |

Before rendering, one line `scene <name>: V vertices, T triangles, S strokes, C colliders, built in N ms` is printed (the build time covers `scenes::by_name`). When the preset uses bounced light, `bounce bake N ms` is printed whenever it is baked (on the first frame, and again when the sun moves; see `bounced-light.md`); with `--bench` it is printed before timing starts, so the bake is not in the frame time.

Reproducing a screenshot (the owner's frame 20261116-000558, from its sidecar's `camera` block):

```
cargo run --release --bin mega_capture -- --scene shaft --camera 11.894625,398.87717,1.8399285,1.7551144,0.053627737,1.2217306 --width 3024 --height 1832 --out <dir>
```

The preset is the current scene default unless `--preset` and `--set` say otherwise; the game's `--shot` replay (`screenshots.md`) also restores the sidecar's preset fields and sun. Files are named `frame_0000.png`, `frame_0001.png` and so on; each written path is printed to stdout. `RUST_LOG=info` prints the adapter.

Example: `mega_capture --path corridor_walk --frames 60 --width 1280 --height 720 --out captures/walk`.

### Temporal metrics

With `--metrics` the tool also reads back the depth buffer. For each pair of consecutive frames it prints, averaged over the sequence:

- consecutive MAD: mean absolute grey difference, at full resolution and after a 4x4 box downsample. Dominated by legitimate motion; kept for reference.
- reprojected MAD: every pixel of the current frame is moved back into the previous frame through its depth and both cameras, pixels that were off screen or hidden (linear depth differs by more than 1%) are skipped, and the previous frame is sampled bilinearly. Raw and after a 5x5 box blur, plus the share of pixels off by more than 64 levels ("pops").
- mean ink (255 minus mean grey) and both MADs divided by it, to compare views with different amounts of ink. With large solid blacks the mean ink is high and the per-ink figure small; compare the blurred MAD itself between such views.
- reprojected MAD after a 13x13 blur: density at the scale of a group of strokes, so coherent changes (patches re-forming, tone breathing) stand out from the resampling error of thin lines (E020).
- temporal second difference: mean of `|f(t) - 2 f(t-1) + f(t-2)|` over the 5x5-blurred frames. Only meaningful with a still camera: ink that moves or changes at a steady rate (a shadow edge under a slowly moving sun) leaves almost none, ink that jumps (texel steps) leaves a lot (E012).

A further line gives contrast-relative pops (E046): the share of pixels whose error is more than half the local contrast (the range of their 3x3 neighbourhood in the current frame, at least 24 levels). A line edge that moves by less than half a pixel changes its pixels by less than half its contrast, so this counts changes that sub-pixel resampling cannot explain, whatever the line's contrast against its background; the fixed 64-level count overstates dark lines on light tone (E043). Both are printed.

Ink that stays on surfaces leaves only resampling error; swimming, popping or shimmering ink adds to it. Compare a view against `strokes` or `lines` on the same stretch. A slow stretch, for example `--path corridor_walk --frames 40 --t0 0.05 --t1 0.062`, keeps per-frame motion near walking speed at 60 fps. See E001.

Benchmark example: `mega_capture --path still_hall --bench 200 --width 2560 --height 1440`. The figure is wall-clock time over back-to-back submissions, so it includes CPU work and is an upper bound on GPU time.

### Camera paths

Defined in `crates/render/src/paths.rs` as `fn(t: f32) -> Camera`, `t` clamped to 0..1. Each path belongs to one scene (`paths::scene_of`): `shaft_*` paths to `shaft` (listed in `one-shaft.md`), the rest to `test`. For the test scene, coordinates are metres, Y up; the corridor runs along +Z from z=0, the arched doorway is at z=110..120, the hall spans x -30..30, z 120..270. Yaw 0 looks along +Z.

| Name | Motion |
|---|---|
| `corridor_walk` | Eye height 1.65 m, straight along the corridor axis from z=2 to z=160, through the arch and into the hall |
| `corridor_strafe` | Fixed at z=40, x from -1.3 to 1.3, looking down the corridor |
| `hall_orbit` | One lap at radius 22 m and 9 m height around the middle of the hall (z=195), looking at the centre |
| `still_hall` | One fixed view in the hall looking towards the arch, with the stair climbing the left wall; `t` is ignored |
| `hall_yaw`, `hall_pitch` | From the `still_hall` position, turning 90 degrees about the vertical, or tipping from 45 degrees down to 45 up, over t 0..1: with `--frames 61` a turn at 90 degrees per second at 60 fps |
| `hall_walk` | From the `still_hall` position, 6 m along its view direction over t 0..1; `--frames 61 --t1 0.2333` is 1.4 m/s at 60 fps |

`shaft_wall_approach` faces the hall's back wall square on from 160 m to 2 m on a log scale, `d = 2 * 80^(1 - t)` (t 1, 0.7909, 0.6327, 0.3819, 0.1582, 0 for 2, 5, 10, 30, 80, 160 m; at 160 m the camera is in the shaft looking through the arch). With `--centre-ink` it gives tone against distance (E029): `mega_capture --path shaft_wall_approach --frames 21 --view hatching --centre-ink --no-png --preset nihei_fall --set paper_grain=0 --set tone_noise=0 --set dark_dist_len=0`.

`shaft_hall_wall` approaches the owner's screenshot 20261116-000630 from 78 m to 2 m (t 0.85 is the screenshot's distance; E026). The shaft has the same rotation probes at the spawn: `shaft_spawn_yaw` and `shaft_spawn_pitch` (E020). Example: `mega_capture --path shaft_spawn_yaw --frames 61 --view hatching --metrics --no-png`.

`shaft_hall_walk` walks 60 m down the hall at eye height, eyes slightly down, so the floor recedes at grazing angles through many octaves of the surface screentone (E036). `--frames 61 --t1 0.02333` is 1.4 m/s at 60 fps: `mega_capture --path shaft_hall_walk --frames 61 --t0 0 --t1 0.02333 --metrics --no-png`.

`shaft_rim_pitch` stands just below the rim and tips from looking across the shaft to straight down (pitch -0.4 to -1.45 over t, 1 degree per frame at `--frames 61`), so the far walls pass through the frame as they descend into darkness (E054).

`shaft_hall_deep_in`, `shaft_hall_deep_back` and `shaft_hall_deep_side` are the darkest places in the scene, at the back of the giant hall: looking further in at the back wall, looking back towards the arch and the lit shaft, and along a side wall from the back corner; t moves the eye 4 m (E056). With `shaft_gallery_down` and `shaft_rim_down` (the bottom of the shaft) they are the views for judging pencil shading.

Labyrinth paths (`lab_*`, scene `labyrinth`) are built from the generator's `Plan::landmarks()` at the default seed (E069), so they follow the layout: `lab_spawn`, `lab_far_ledge_axis`, `lab_hall_from_arch`, `lab_hall_exterior_end`, `lab_void_bottom_up`, `lab_stair_hall_foot`, `lab_stair_hall_landing`, `lab_cistern_walkway`, `lab_cistern_basin`, `lab_undercroft_shaft`, `lab_shaft_up`, `lab_cleft`, `lab_well_bridge`, `lab_lantern_crossing`, and one per passage section: `lab_corridor_vaulted`, `lab_corridor_colonnade`, `lab_corridor_slot`, `lab_corridor_low`, `lab_corridor_gallery`, `lab_corridor_plain`, `lab_duct`, `lab_service`. A name the plan lacks falls back to the spawn. For motion: `lab_axis_walk` (from `far_ledge_axis` to `hall_from_arch`), `lab_far_ledge_yaw` (a 90 degree turn there) and `lab_corridor_walk` (8 m down `corridor_slot`; `--frames 61 --t1 0.175` is 1.4 m/s).

Performance (E069): `--bench N --timing` prints the median GPU time per pass from timestamp queries (shadow cascades, depth prepass, forward, contours, ink lines; see "Measured" in `ink-renderer.md` for how to read them on a tile-based GPU) and the triangles drawn per pass; `--no-cull` draws everything; `--no-portals` keeps frustum culling but not portal visibility (E072); `--prepass` adds a depth prepass. `--timing` reports the minimum over the frames (`MEGA_TIMING_MEDIAN=1` for the median), since other work on the GPU only ever adds time, and the visible-space count. Captures wait for the bounce bake before the first frame.

To add a path, write the function, add it to `NAMES` and `by_name`, and make sure `scene_of` maps it to its scene.

### Code locations

- `crates/capture/src/main.rs`: the tool.

Example, a still camera under a moving sun: `mega_capture --path shaft_spawn --t0 0.5 --t1 0.5 --frames 60 --sun-azim -110 --sun-azim-end -109.4 --metrics --no-png`.
- `crates/render/src/paths.rs`: camera paths.
- `crates/gen/src/scenes.rs`: the scene registry; `test_scene.rs` and `shaft.rs` the scenes.

## Open questions

- Whether to add an optional contact sheet or video output for judging temporal stability.

## Related experiments

E001 (the temporal metric), E002, E003 (diagnostic switches).
