# Bounced light

## Status

Working, 2026-11-16 (E037). First cheap version: one bounce of sun and sky light, baked per vertex on the CPU, added to the light value in the forward pass. On in the default presets `nihei_ink`, `nihei_ink_wash` and `nihei_ink_small`; off (strength 0, no bake) in every older preset, which draw byte-identically to before. Verified headless only; the windowed game uses the same code path but nobody has looked at it there yet.

## What it is

Until E037 the light value had only direct sun (with cast shadow) and an ambient term from baked openness and local occlusion. Shade next to brightly lit surfaces (under the stair on the blank wall, the hall behind the arch) was therefore as dark as shade with nothing lit around it, which the owner found implausible. This adds an estimate of the light reflected once off the scene's own surfaces: a shadow under the stair lifts towards mid grey because the sunlit wall below it lights it from underneath.

## Design

- **Per vertex, baked on the CPU, in `mega_gen`.** The tone is low-frequency and the kit already tessellates walls to 1.5 to 2 m for the AO bake, so per-vertex irradiance interpolated across triangles is enough. No GPU work beyond one extra vertex attribute and a few multiply-adds per fragment, and no change to the frame layout.
- **Traced against the scene's own triangles**, not the colliders the AO bake uses: the stair, which is excluded from the AO occluders because it is thin, is exactly what casts the shadow under itself and must block and reflect light here. A binned-SAH BVH over the 226k triangles of the shaft is built once per scene.
- **The renderer's own light at each hit.** At a ray's hit on a front face, the light leaving it is the renderer's direct light there, split into its two parts: the sun's `max(N.S, 0)` times its visibility (a shadow ray per vertex of the hit triangle, interpolated), and the sky's baked `ao` (interpolated). The bake stores the two means separately, so the preset's sun and ambient weights, the sun's intensity and the bounce strengths all apply live in the shader. Only a new sun direction needs a new bake. Misses and back faces contribute nothing: direct sky is already in `ao`, and a back face is the inside of a solid.
- **Same ray directions for every vertex** (a Fibonacci spiral in the tangent frame, as in the AO bake), so the result is smooth from vertex to vertex rather than noisy. The cost is a slight bias that is the same everywhere, which no one can see in a tone that is already approximate.
- **A separate vertex stream.** The two values go in a second vertex buffer (slot 1, 8 bytes per vertex) rather than in `Vertex`, which stays 48 bytes. A re-bake rewrites only that buffer.
- **Lazy.** The renderer keeps a copy of the mesh, builds the baker on first use and bakes when a preset with a non-zero bounce strength is drawn and the sun has moved by more than 0.25 degrees since the last bake (a near sun: by more than that angle seen from the centre of the scene). With both strengths 0 nothing is built or baked.

Chosen over the coarse occupancy-grid propagation in PLAN 7a because there is no occupancy grid yet, and a per-vertex bake needs nothing new from the generator. The grid remains the plan for the far field of a kilometre-scale structure (see Open questions).

## Technical details

### The bake (`mega_gen::bounce`)

```
BounceBaker::new(&MeshData)                      distinct (position, normal) pairs, BVH
BounceBaker::bake(BounceSun, BounceSettings) -> Vec<[f32; 2]>   per vertex of the mesh
BounceSun::Direction(towards_sun) | BounceSun::Point(sun_position)
BounceSettings { rays: 64, threads: 0 (all cores) }
```

Per distinct vertex `p` with normal `n`:

1. `direct[v] = max(n.S, 0)` if a shadow ray from `p + 0.03 n` towards the sun hits nothing, else 0.
2. For each of the 64 cosine-distributed directions `d` from `p + 0.03 n`: find the closest hit. On a front face with barycentrics `w`, add `sum w_k direct[k]` to `sun` and `sum w_k ao[k]` to `sky` over the triangle's three vertices.
3. Store `[sun / 64, sky / 64]`.

A cosine-weighted mean is irradiance over pi, so with unit albedo it is the reflected light the receiver gets back. Vertices are deduplicated by exact position and normal bits before baking (164,568 vertices, 140,344 distinct in the shaft) and both steps run on all cores with `std::thread::scope`.

### In the forward pass

```
bounced = bounce * sun_strength * intensity * sun_b + bounce_sky * ambient * sky_b
lit  = (sun term + ambient * ao + noise) * shade + bounced
dark = clamp((ambient * ao * shadow_ambient + noise) * shade, shadow_light_min, shadow_light_max) + bounced
```

It is added after the shadow clamp because it reaches surfaces in cast shadow as much as any, and outside `shade` (`underside_black`, `pool_ao`) because bounced light is what lights undersides. With both strengths 0 the light is exactly what it was (frames byte-identical, checked on `shaft_spawn`, `shaft_stair_below` and `still_hall` for `nihei`, `nihei_vast`, `nihei_wash`, `nihei_hybrid`). Everything downstream (wash, ruling, bands) sees the lifted light, so in `nihei_ink` lifted shade also loses ruling.

### Preset fields

| Field | `nihei_ink` | Meaning |
|---|---|---|
| `bounce` | 0.6 | Strength (albedo) of the sun bounce; 0 off |
| `bounce_sky` | 0.3 | Strength of the sky bounce; 0 off |

Older presets have 0 for both. `mega_capture --set bounce=0 --set bounce_sky=0` reproduces the light without bounce.

### Vertex streams

| Slot | Contents | Stride |
|---|---|---|
| 0 | `mega_gen::Vertex` (position, normal, hatch axis, ids, ao) | 48 bytes |
| 1 | `[sun_b, sky_b]`, `f32` each, shader location 6 | 8 bytes |

Slot 1 is created with the vertex buffer in `Renderer::upload_scene`, zero-filled until baked. The shadow pass uses slot 0 only.

### Cost and the moving sun

Measured on the shaft (Apple M3 Max, 16 cores, release build): BVH build 111 ms, bake 494 to 514 ms (64 rays per vertex), about 0.6 s in all on the first frame; 20 ms for the test scene. `mega_capture` prints `bounce bake N ms`; `cargo test --release -p mega_gen -- --ignored --nocapture shaft_bake_time` times it without a GPU.

How it copes with day and night: intensity changes (dusk, night) need no re-bake, because intensity multiplies the sun part in the shader. A change of sun direction does. The renderer re-bakes when the direction has moved by more than 0.25 degrees, synchronously inside `render`, which is a 0.5 s hitch. `Renderer::update_bounce` can be called right after `set_sun` to take that cost at a moment of the caller's choice. A day-night cycle that moves the sun continuously would need one of: baking on a worker thread and swapping the buffer when done (the bake is plain data and already thread-safe); baking a few key sun directions and interpolating between their buffers; or spreading the vertices of one bake over many frames. None is built yet. The sky part does not depend on the sun and could be baked once and kept; it is recomputed today because it comes almost free with the same rays.

### Files

- `crates/gen/src/bounce.rs`: `BounceBaker`, BVH, bake, tests (including a brute-force check of the BVH and the ignored timing test).
- `crates/render/src/renderer.rs`: the bounce vertex stream, `update_bounce`, `bounce_bake_ms`.
- `crates/render/shaders/forward.wgsl`: the `bounced` term.
- `crates/render/src/style.rs`: `bounce`, `bounce_sky`.

## Open questions

- A continuously moving sun needs the asynchronous or interpolated re-bake described above.
- One bounce only, from a fixed albedo, and the stored light ignores `tone_noise`, `underside_black` and the cast-shadow clamp at the hit, so it slightly overestimates light from shadowed surfaces and underestimates from lit ones near a shadow edge (the visibility at the hit is interpolated from its vertices, about 1.5 m apart).
- Thin parts narrower than the tessellation (railings, stair stringers) are traced, but receivers are sampled only at vertices, so the soft dark band right under the stair is as wide as a wall triangle.
- For a kilometre-scale structure a per-vertex bake of everything will be too slow, and distant geometry beyond the streaming radius will not be in the BVH. The PLAN's coarse occupancy grid (light propagated from lit cells) is the candidate for the far field, with this bake kept for the near field.
- With bounce on, walking pops on `shaft_bridge_walk` rose from 0.046% to 0.103% of compared pixels (E037): likely resampling of contour edges against a lighter grey, not swimming, but not confirmed.

## Related experiments

E037 (bounced light), E038 (the new default), E039 (the owner's frames).
