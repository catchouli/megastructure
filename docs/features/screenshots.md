# Screenshots and replay

## Status

Working, 2026-11-16. The sidecar, the headless `screenshot` action and the replay are verified by tests and headless runs (the replay of a headless screenshot is byte-identical). The windowed key (P and F12) compiles but has not been run: nobody has pressed it in a window yet.

## What it is

Press a key in the windowed game and it saves the current frame as a PNG plus a JSON sidecar holding everything needed to draw that frame again. Later, `megastructure --headless --shot` re-renders the same view from the sidecar, optionally with preset overrides or a debug view, so a problem seen in play can be looked at, changed and compared before and after from one file.

## Design

- **The key.** P or F12 (physical keys, no repeat). The game renders the current frame once more into an offscreen texture of the window's size with the displayed camera (the interpolated one), preset and sun, without the egui overlay, reads it back and writes the pair. The overlay shows `saved <name>.png` for five seconds and the path goes to stderr. A failure (no permission, disk full) is shown in the overlay in red and logged; the game carries on.
- **Where files go.** `screenshots/<YYYYMMDD-HHMMSS>.png` and `.json` under the directory the game was started from (gitignored), local time. If the name exists the stem gets `-2`, `-3` and so on.
- **The sidecar is complete.** It stores every preset field in effect, not only the preset's name, so a later change of the preset defaults does not alter a replay. Numbers are written as the shortest text that reads back as the same `f32`, so values round-trip exactly.
- **Replay needs no simulation.** A replay builds the scene named in the sidecar, puts the camera, preset, sun, debug view and shadow filter in, and renders one frame. Movement state is stored for the reader's information and is not restored.
- **Matching the original.** With no overrides, the same commit and a clean tree, the replay is expected to match the screenshot exactly. It warns on stderr (and carries on) if the sidecar's commit differs from the current one, if either tree is or was dirty, if the sidecar has no git information, or if the scene seed or the near plane differ.

## Technical details

### Sidecar format (version 1)

Written by `mega_shot::Shot` (`crates/shot`, no GPU or window dependencies). Required fields: `version`, `timestamp`, `scene`, `camera`, `width`, `height`, `preset`, `sun`. Optional, with defaults: `git` (none), `seed` (none), `near` (0.1), `view` (`final`), `shadow_filter` (`smooth`), `movement` (none), `note` (empty). Unknown fields are ignored; a missing required field is an error naming it; a `version` newer than the reader's is refused.

| Field | Meaning |
|---|---|
| `version` | Format version, 1 |
| `timestamp` | Local time, ISO 8601 with offset |
| `git` | `commit` (full hash) and `dirty` (uncommitted changes), from `git rev-parse HEAD` and `git status --porcelain` run in the starting directory, else in the executable's directory; absent if git fails |
| `scene`, `seed` | Scene name; the generator seed if the scene has one (the shaft) |
| `camera` | `position` (m), `yaw` and `pitch` (radians), `fov_y` (radians): the displayed camera |
| `near` | Near plane in metres (fixed in the renderer; recorded for completeness) |
| `width`, `height` | Output size in pixels |
| `preset` | `name` the values started from, and `fields`: every field `StylePreset::set` accepts, as a number, a list of numbers, or `null` (`sky_tone: none`) |
| `sun` | `position` (m), `radius`, `intensity` |
| `view` | Debug view name |
| `shadow_filter` | `smooth`, `bilinear` or `hard` |
| `movement` | `mode` (`walk` or `fly`), `grounded`, `velocity` (m/s) |
| `note` | Free text, left empty by the game |

Abridged example (a real one, from the shaft; `fields` cut to four of its 86 entries):

```json
{
  "version": 1,
  "timestamp": "2026-11-15T23:59:59+00:00",
  "git": { "commit": "b292f4527dd0a9ee7fc0882865c6d321a71f6ef4", "dirty": true },
  "scene": "shaft",
  "seed": 1592590337,
  "camera": { "position": [-29.451206, 401.65, 22.921206], "yaw": 0.78539854, "pitch": 0.47123894, "fov_y": 1.2217306 },
  "near": 0.1,
  "width": 960,
  "height": 540,
  "preset": { "name": "nihei_vast", "fields": { "ambient": 0.85, "fog_colour": [0.86, 0.85, 0.82], "fog_density": 0.0026, "sky_tone": 0.0 } },
  "sun": { "position": [-38852160000.0, 144189240000.0, -14141030000.0], "radius": 700000000.0, "intensity": 1.0 },
  "view": "final",
  "shadow_filter": "smooth",
  "movement": { "mode": "walk", "grounded": true, "velocity": [0.0, 0.0, 0.0] },
  "note": ""
}
```

### Commands

```
# Headless screenshot: the script action `screenshot` acts on the first frame it is held.
# Writes screenshots/<time>.png and .json, or into --out. Separate two with another segment.
megastructure --headless --scene shaft --width 960 --height 540 --out shots \
  --input "idle:1,look_right:1,forward:3.7,look_left:0.5,run+forward:2,look_up:0.3,screenshot:0.1,idle:0.2"

# Replay: writes <name>.replay.png next to the sidecar, or into --out
megastructure --headless --shot screenshots/20261115-235959.json

# Same view with a preset change, to test a fix; --set is repeatable and uses StylePreset::set
megastructure --headless --shot screenshots/20261115-235959.json --out after --set fog_density=0.02 --set sun_strength=1.0

# Same view as a debug layer
megastructure --headless --shot screenshots/20261115-235959.json --out debug --view normals
```

`--shot` takes the scene, resolution and view from the sidecar, so it rejects `--scene`, `--mode`, `--width`, `--height`, `--input`, `--frames`, `--dt` and `--every`. `--set` and `--view` only work with `--shot`. `--view` names: `final id normals depth bands hatching strokes contours lines shadow`. A bad `--set` field or value is an error naming it.

### Code

`crates/shot/src/lib.rs` (`Shot`, JSON, git, timestamps, unique names) and in `crates/game/src/`: `shotio.rs` (capture state, write the pair, apply a sidecar and overrides), `offscreen.rs` (offscreen target and readback, shared with the headless runner), `headless.rs` (`replay`, the `screenshot` action), `windowed.rs` (the key and the overlay message).

### Replaying a frame in another preset

A sidecar stores every preset field, so `--set` would need all of them to switch presets. For comparisons (E030), copy the sidecar, set `preset.name` to the other preset and empty `preset.fields`: every field then takes that preset's current default (the replay warns once per missing field). A screenshot taken with `MEGA_PRESET` set records that preset's name and fields like any other.

### Limits

- `mega_render` has no getter for a preset field by name or for the shadow filter, so the game reads the preset from its `Debug` text (a test checks that every name in `StylePreset::FIELDS` is found and that the values round-trip through `set`) and records the shadow filter as `smooth`, which the game never changes. A `get(name)` on `StylePreset` and a `shadow_filter()` accessor on `Renderer` would remove both workarounds.
- The near plane is a constant in the renderer, so it cannot be restored from the sidecar; a replay warns if it differs.
- A windowed screenshot is read back in the surface format (swapped from BGRA if needed). The game prefers a non-sRGB surface, so the bytes match the headless target; if the surface were sRGB the stored bytes would still be what the window shows, and a replay could differ.
- Sidecar sections the replay ignores: `movement`, `note`, `timestamp`.

## Open questions

- Whether the overlay should offer a way to type the note, or the note is filled in by hand.
- Whether the sidecar should record the window's scale factor or the renderer's other diagnostic settings (hatch LOD, stroke visibility), which the game does not change.
- A replay of a windowed screenshot is untested; the replay of a headless one is identical.

## Related experiments

None.
