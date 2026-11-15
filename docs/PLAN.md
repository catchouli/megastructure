# Megastructure: research summary and plan

Status: plan agreed in outline, 2026-11-15. Nothing is implemented yet. Per-feature designs live in `features/`; trials and their results go in `experiments.md`. See `README.md` for the conventions.

A first-person walking game about wandering a vast, decaying, procedurally generated megastructure. Black-and-white ink look in the vein of Tsutomu Nihei's Blame! is the lead style; a Moebius-style flat-colour look is a secondary preset over the same buffers. Rust, wgpu.

## 1. Decisions

| Area | Decision | Close alternative |
|---|---|---|
| Stack | Raw wgpu 30 + winit 0.30, hand-written frame, no engine | Bevy 0.20, only if we later want its ECS and asset tooling |
| Lines | Generator-emitted strokes plus screen-space ID/depth contours, max-blended into one ink target | ID-buffer contours only (cheaper, loses taper and breakup) |
| Tone | Object-space ruled hatching in the forward pass, quantised light bands, baked AO and openness | Tonal art map textures, if procedural strokes look too clean |
| AA | Analytic stroke coverage, SMAA on the composite. No TAA, no upscaling | FXAA |
| World | Finite and huge, one seed, generated top-down; fine layers generated lazily per space | Unbounded generation (loses the whole-structure exterior) |
| Geometry | Parametric polygonal kit of parts; coarse voxel grid for analysis only | SDF plus dual contouring, kept for rubble and eroded faces |
| Connectivity | Cyclic graph grammar, then constraint placement into 3D | WFC, kept as local filler only |
| Composition | A director places set pieces and sight lines first; everything else fills around them | None. This is the main defence against uniform noise |
| Decay | Macro plan on the space graph before layout; micro pass on an element support graph after | Painted-on decay |
| Precision | Camera-relative rendering, reversed-Z with infinite far plane, `Depth32Float` | Origin rebasing |

## 2. Rendering

### Frame

1. **Compute: cull and LOD.** Space/element culling. Stroke buffer to an indirect draw list after frustum, rank and projected-length tests. Silhouette edges for curved meshes (one adjacent face front-facing, one back-facing).
2. **Shadow.** Sun shadow maps, only when the sun is up and an opening to the sky is in range. The sun may be at a finite distance, so the projection is not assumed orthographic.
3. **Forward geometry, MRT.**
   - Depth: `Depth32Float`, reversed-Z.
   - ID: `RG32Uint`. R is the fine id (element, panel). G is the coarse id (district, space). See the note below.
   - Normal: `RG16Float`, octahedral world normal.
   - Fill: `RGBA8`. Palette index, tone band, AO times openness, flags.
   - Ink: `RG8`. R is black ink (hatching, decals, speckle). G is white ink.
4. **Sky.** Gradient or ruled lines, sun disc as an SDF, cloud geometry. Writes a sky id so structure edges against sky get a contour.
5. **Volumes.** Light shafts erase ink into the white channel and add ruled streaks.
6. **Strokes.** Vertex-pulled instanced quads from a storage buffer, depth-tolerance test in the fragment shader, max-blended into ink.
7. **Compute: contours.** Roberts cross on masked ids, depth-relative Sobel on linear depth with an N.V mask, world-keyed wobble. Max-blended into ink.
8. **Composite.** Fill through a palette LUT, atmosphere fade (which also attenuates ink), view-sphere screentone for far tone, white ink, black ink, paper grain.
9. **SMAA, then debug UI.**

### Points that differ from the research report

- **ID layout.** The report proposed four 8-bit fields in one `R32Uint`. 256 panels per part is too few for a large wall, so the plan uses two 32-bit channels. Distance masking compares both channels near, the element bits of R plus G at mid range, and G only when far.
- **Hatch octave selection.** Isotropic `fwidth(u)` will pick the wrong octave at grazing angles, which is most of a long floor. Use the screen-space derivative of `u` alone (`length(vec2(dpdx(u), dpdy(u)))`) and expect to tune the crossfade on video.
- **Impostors.** The rendering report suggested them for far chunks; the generation report advised against. The plan uses real proxy geometry (massing mesh, void-facing proxies), so lines stay consistent.
- **Metal.** No native indirect-count draw. Culling writes indirect arguments from compute; do not rely on `multi_draw_indirect_count`.

### Stroke record

Two endpoints, two adjacent face normals, type (crease, seam, crack, border, white), rank 0 to 3, base weight, seed. Weight is base times depth falloff times taper along the stroke times seeded 1D noise. Minimum width about 1.25 px; below that, fade coverage.

### Style presets

One parameter block covers line weights, taper, overshoot, wobble, id-mask distances, rank cut-offs, tone band thresholds and treatments, hatch spacing and breakup, screentone size, AO weights, palette LUT, ink and paper colours, fog curve, sky treatment and paper grain. Presets: `nihei` (default), `moebius`, and possibly `sketchbook` (overshoot and wash, after the marker-sketch reference).

## 3. Generation

### Layers

| Layer | Data | Method |
|---|---|---|
| L0 Massing | CSG tree of large primitives, 32 m occupancy grid, list of primary voids | Set-piece templates and seeded parameters |
| L1 Districts | Strata split in plan; each district has function, epoch, builder style, `ParamSet` | Rules over L0 |
| L2 Space graph | Typed nodes (hall, shaft, gallery, corridor, stair hall, bridge, machine hall, plain, vista); portal, sightline and support edges | Cyclic graph grammar seeded by the director |
| L3 Layout | Per space: oriented envelope, section, portal rectangles | Greedy constraint placement with backtracking; treemap inside cellular districts |
| L4 Elements | Wall panels, arches, columns, stairs, railings, beams | Split grammars on L3 surfaces |
| L5 Machinery | Assembly graphs with typed ports; routed pipe and cable network | Symmetry-led machine grammar; A* on a surface grid |
| L6 Decay | Per-element integrity and state; fragments and rubble | Fields and support graph |

Every element is a function from parameters to triangles, ink strokes, colliders and support-graph members, emitted together.

### Rules that hold throughout

- **Function drives form through a `ParamSet` lookup.** Habitation, industry, transit, cooling, power and unknown each set module size, spans, rule weights, ornament probability and machinery vocabulary.
- **Seeding.** `seed = hash64(parent_seed, stable_id, layer_tag)`. Stable ids are paths, never counters. No float-dependent branching across threads and no reliance on `HashMap` order.
- **Ownership.** A feature belongs to the coarsest layer whose scope contains it. Streaming is by space and element through an AABB tree, not by grid chunk, so nothing is split at a seam.
- **Walkability.** Required nodes form a 2-edge-connected core, so decay can cut one arc of any cycle. Stairs use human rise and run at any scale. After micro decay, each space is flood-filled between portals and re-seeded on failure.
- **Composition.** Sight lines are hard constraints checked by raycast through the occupancy grid. Size classes alternate (compression, then release). Some surfaces get a detail budget of zero. Human-scale anchors (railings, doors, risers) sit next to giant elements.
- **Exterior and LOD.** The layers are the LODs: L0 massing with a hull grammar far away, void-facing proxies across a shaft, full elements near.

### Decay

Macro, global and coarse: builder epochs and later accretion, exposure to sky, a water drainage walk giving a wetness field, a power graph whose edges fail with age and wetness, and a collapse plan that only removes edges outside the connected core or adds a later-epoch bypass.

Breaks are also how light gets in (requested 2026-11-15): holes in walls, floors and roofs should let sun through as hard-edged shapes, and every break gets a rubble pile beneath it. The shadow pass must therefore resolve openings of a metre or so cleanly.

Micro, per space: integrity per element, support-graph propagation (anything without a path to a root falls), falling and rubble under what fell, Voronoi cracks and missing panels. Machine state (powered, failing, dead) comes from the power graph.

## 4. Workspace layout (proposed)

- `crates/gen`: all generation. Pure data in, pure data out, no GPU. Testable headless.
- `crates/render`: wgpu frame, shaders, presets.
- `crates/game`: window, input, first-person controller, fly and orbit cameras, debug UI.
- `crates/capture`: offscreen render of a seed and camera path to PNG frames. Agents and tests use this to check visual output without a window.

## 5. Milestones

| | Name | Contents | Exit test |
|---|---|---|---|
| M0 | Ink on a test scene | Scaffold, MRT forward pass, contour pass, ruled hatching, composite, capture tool, fly camera | A hand-built corridor and hall hold still under walking and strafing on video |
| M1 | One Shaft | 60 m by 600 m shaft, gallery stacks, 4 to 6 bridges, a stair on a blank wall, one hall with columns and an arch; first kit; stroke pass; walking controller | The scale reads, and distant structure simplifies instead of turning to noise |
| M2 | Generator core | Graph grammar, 3D embedding, reachability validation, seed-sweep harness, two district functions, director with three set pieces | Thousands of seeds pass reachability |
| M3 | Micro decay | Support graph, panel loss, cracks, rubble | |
| M4 | Machinery | Machine grammar, pipe routing, powered and failing animation | |
| M5 | Whole structure | L0 massing, L1 districts, streaming, proxies, exterior view, sky | Orbit the whole structure from outside |
| M6 | Macro decay | Water, power, collapse plan, later-builder accretion | |

M0 and M1 exist to retire the two biggest risks early: lines at distance, and composition reading as noise.

## 6. Delegation

- **Orchestrator (Fable):** interfaces between crates, milestone briefs, review, judging captures against the references.
- **Opus:** renderer core and shaders, graph grammar and embedding, decay model.
- **Sonnet:** kit elements, capture tool, test harness, debug UI, scaffolding.

## 7. Questions settled on 2026-11-15

1. **Size.** Finite, on the order of 8 km by 8 km by 16 km.
2. **Movement.** Walking, running, step-up and jumping. Walking off a ledge means falling, with drag and a terminal velocity; a hard enough impact kills. Falling into open void just continues. See `features/player-movement.md`.
3. **Light.** There is a sun. It can move (day-night cycle) and sit at any distance, including very remote or absent. See `features/sun-and-sky.md`.
4. **Presets.** Nihei only for now. The fill target keeps its palette index so a colour preset stays possible, but no Moebius work is scheduled.
5. **Platforms.** Native only (macOS first, then Windows and Linux). No WebGPU, so compute, storage buffers and indirect draws are all available.

## 8. Gaps in the research

- No developer sources were found for NaissanceE, Babbdi, Kairo, Bernband, INFRA, Echo or Obra Dinn.
- Nothing usable on movement feel: speed, head bob, field of view, stairs, ledges. Expect to tune by hand.
- No published Nihei-style real-time pipeline and no Rust project combining ink lines, hatching and screentone on wgpu.
- Many paper citations in the two design reports came from memory and were not checked.

## 9. Sources

- Mars First Logistics surface-id outlines: https://omar-shehata.medium.com/better-outline-rendering-using-surface-ids-with-webgl-e13cdab1fd94
- Outline technique trade-offs: https://ameye.dev/notes/rendering-outlines/
- Moebius-style post-processing: https://blog.maximeheckel.com/posts/moebius-style-post-processing
- Praun et al., Real-Time Hatching: https://gfx.cs.princeton.edu/proj/hatching/
- Sable art talk: https://gdcvault.com/play/1027721/The-Art-of-Sable-Imperfection
- Infinite WFC: https://marian42.de/article/infinite-wfc/
- Cyclic generation in Unexplored: https://www.boristhebrave.com/2021/04/10/dungeon-generation-in-unexplored/
- Lorn's Lure devlog on its shelved procedural megastructure: https://rubeki.itch.io/lorns-lure/devlog/920655/meganaut
- Blame! and game architecture: https://www.killscreen.com/influence-blame-videogame-architecture-rising/
- Control and brutalism: https://gamedeveloper.com/design/how-remedy-harnessed-the-ethereal-beauty-of-brutalism-to-shape-i-control-i-
- wgpu changelog: https://raw.githubusercontent.com/gfx-rs/wgpu/trunk/CHANGELOG.md
