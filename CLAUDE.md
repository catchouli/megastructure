# Megastructure: notes for the orchestrating session

This file is for the main Claude session working in this repo. Subagents also see it; the "How work is organised" section tells them what applies to them.

## What this is

A first-person walking game about wandering a vast, decaying, procedurally generated megastructure, drawn as black-and-white ink in the manner of Tsutomu Nihei's Blame!, with Moebius as a secondary influence. Rust, raw wgpu, no engine. The mechanics are deliberately minimal; the goal is a feeling of scale and awe.

Read these before doing anything:

1. `docs/PLAN.md`: decisions, frame layout, generation layers, milestones, parked ideas.
2. `docs/README.md`: docs conventions and the index of feature documents.
3. `docs/experiments.md`: everything tried so far and the verdicts. Do not repeat a settled experiment.
4. `docs/features/ink-renderer.md`: the renderer as built, and the owner's dated review notes.
5. `git log`: what landed and in what order.

## How work is organised

The main session is the orchestrator. It was set up on Claude Fable 5.2 and should stay on the most capable model available. Its job is planning, writing briefs, reviewing what comes back, judging rendered frames, talking to the owner, and committing. It delegates implementation and research to subagents:

- **Opus** for work that needs design judgement: renderer and shaders, generation architecture, composition of scenes, decay.
- **Sonnet** for well-specified work: tooling, tests, the game shell, scaffolding, lookups.

The orchestrator does small edits itself (a key binding, a doc note, a one-line fix) and delegates anything larger.

What has worked when briefing subagents:

- Give each agent sole ownership of named crates or files and say who else is working where. Two agents never edit the same crate at once. Concurrent runs so far: one in `crates/render` and `crates/capture`, one in `crates/game`.
- Tell them what to read first, what is out of scope, and what "done" looks like, including the checks to run.
- Ask for a report that separates what was verified from what was not, lists public API changes, and ranks the remaining problems.
- When the owner gives feedback while an agent is running, pass it on with SendMessage, quoting the owner's words, instead of waiting.
- An agent that has finished keeps its context; send it follow-ups rather than starting a fresh one for the same area.
- Experiment ids: the render side numbers from E001 upward, the game side from E101 upward, so parallel agents do not collide.

## Rules that always apply

- **Never open a window.** The owner works at this machine. All checking is headless: `mega_capture` for rendered frames along named camera paths, and `megastructure --headless` for the game loop with scripted input. Only the owner runs the windowed game. Put this rule in every brief.
- **The owner cannot see images in the terminal.** When something needs their eye, give the exact command (`open <file>`, or the `cargo run` line). For aesthetic choices, make labelled side-by-side sheets in `compare/` (gitignored), same camera and frame for every option, filename starting with the experiment id, and put one specific question to the owner per sheet.
- **Docs are part of the work.** Each feature has a document in `docs/features/` kept current as it changes, and every trial with an uncertain outcome gets an entry in `docs/experiments.md`, including failures. Docs give the command that regenerates a capture, never a temporary path.
- **Commits.** The orchestrator commits and pushes to `main` when a step is verified (builds, tests pass, fresh captures looked at). Subagents never commit. Commit messages describe the change only, with no mention of AI or tooling. When agents are working in parallel, stage only the finished agent's paths.
- **`references/` is local only.** It holds published artwork used as visual reference and is gitignored. Agents may open the images to study them. Nothing from it goes into the repo, and code and docs do not name the files.
- **Verify before reporting.** After an agent reports, rebuild, run the tests, render fresh captures and look at them before committing or telling the owner it works. Say plainly what nobody has checked; the windowed path is usually in that category until the owner tries it.
- No emojis in code, commits or docs.

## The owner

A graphics engineer, also experienced with CAD. Be technical; skip rendering basics. They test the windowed build themselves and give quick, candid visual feedback, which should be recorded in the relevant feature document as dated review notes. They prefer a stable, plainer image to a clever one that swims.

Standing art direction, from their reviews:

- Black-and-white ink first. A colour preset is deferred.
- Cast shadows as dense single-direction hatching. Solid black only rarely, in dark enclosed corners.
- White speckle and white rules inside black are liked.
- Crosshatching tends to look messy; default to little or none unless it is clean.
- Hatching should look hand-placed: tapered stroke ends, strokes laid in groups, and feathered, staggered boundaries instead of hatching that starts along a straight line.
- Hatching must be rigid when the camera rotates.
- Geometry is going well. More breakage is wanted later: holes that let light through, with rubble beneath.

## Layout

- `crates/gen` (`mega_gen`): generation, no GPU. Mesh and stroke types, the kit of parts, AO bake, scenes (`test`, `shaft`), colliders.
- `crates/render` (`mega_render`): the wgpu frame, WGSL shaders, `StylePreset`, shadows, camera paths.
- `crates/capture` (`mega_capture`): headless frames, debug views, `--set` preset overrides, temporal metrics, benchmark.
- `crates/game` (`megastructure`): window, fly camera and walking controller, headless mode.

Useful commands:

```
cargo test --workspace
cargo run --release --bin mega_capture -- --list-paths
cargo run --release --bin mega_capture -- --path shaft_spawn --out <dir>
cargo run --release --bin megastructure -- --headless --scene shaft --input "forward:2,look_right:1"
cargo run --release --bin megastructure -- --scene shaft        # owner only
```

## Where things stand

Kept short on purpose; the docs and `git log` are authoritative. As of 2026-11-15:

- Done: M0 (ink renderer on a test scene) and most of M1 (kit of parts, the One Shaft scene, walking controller, sun shadows).
- In progress: the look. As of 2026-11-16 the default presets are `nihei_ink` (shaft) and `nihei_ink_small` (test): a smooth grey wash on surface-anchored layered paper texture with a faint soft-dot halftone in the darker tones (E040; thresholded grain rejected as TV static), soft cast-shadow edges (E041), ruling only at distance and depth merging into black far away, and one bounce of sun and sky light baked per vertex (`mega_gen::bounce`). The owner chose the wash as the base and finds hatching ugly unless sparse and distant, and hard two-level grain harsh. Since E044-E048: generated fibre paper tile (no clouds), halftone capped at 5 cm pitch, minimum penumbra across cascades, smoothed bounce bake, contour line-fit resolve. Since E049: paper relief comes from scanned CC0 sheets in `assets/paper/` (`paper_kind`, default Paper002; generated fibres read as worms, clouds removed). Since E051-E053: Paper001 (watercolour-like) is the default with constant-contrast layer blending and a near fade; Paper004's laid lines are a subtle patch-wise 'pencil' layer in shade (`pencil`). Since E054-E056 the distance ruling is replaced by wash falling to black with gathering pencil marks (`far_mode` 1; ruling kept as `far_mode` 0 until the owner confirms). Since E059: pencil shading is a generated four-level stroke-cluster texture (`shade_*`), laid in patches wherever it is dark (wash + AO), replacing both the laid-line pencil and procedural far strokes; the fall curve is mostly greys with black only at the bottom. A labyrinth scene (first slice of M2, `crates/gen/src/labyrinth`, experiments E201+) is in progress. Open: shading strength and tile character, residual line crawl, a non-blocking bounce re-bake for a moving sun, then a line-imperfection pass on strokes and contours. `MEGA_PRESET=<name>` and `MEGA_SET="field=value,..."` let the owner try looks in the windowed game. `python3 compare/make_index.py` rebuilds the sheet index after adding sheets.
- Next milestones, in order: M2 generator core (graph grammar, 3D embedding, reachability, director and set pieces), M3 micro decay, M4 machinery (gears, cables, towers, turbines first), M5 whole structure with exterior and sky, M6 macro decay.
- Requested and not yet built is tracked in the feature docs' open questions and in `docs/PLAN.md` section 7a.

When resuming in a new session: read the files listed at the top, run the tests, render two or three shaft captures to see the current look, and check `git status` for uncommitted work left by an interrupted agent before starting anything new.
