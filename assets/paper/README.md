# Paper textures

Scanned paper sheets used as the relief of the surface-anchored paper (`paper_kind` in
`StylePreset`; see `docs/features/ink-renderer.md`, "Paper", and experiment E049). Only the
displacement maps are embedded in the binaries (`include_bytes!` in
`crates/render/src/paper.rs`); the colour maps are kept for reference.

All from ambientCG (https://ambientcg.com), licensed under Creative Commons CC0 1.0
Universal (https://docs.ambientcg.com/license/). No attribution is required; it is given here
for reference.

| `paper_kind` | Folder | Source | Size | Notes |
|---|---|---|---|---|
| 1 | `Paper001/` | https://ambientcg.com/view?id=Paper001 | 1024 x 600 | White paper with a cold-pressed, watercolour-like tooth |
| 2 | `Paper002/` | https://ambientcg.com/view?id=Paper002 | 794 x 1024 | Cartonage; fine, even grain (the default) |
| 3 | `Paper004/` | https://ambientcg.com/view?id=Paper004 | 1024 x 1024 | Brown packing paper with laid lines |
| 4 | `Paper006/` | https://ambientcg.com/view?id=Paper006 | 1024 x 1024 | Beige paper, fibrous grain |

Kind 0 is the generated fibre tile (no file). Files: the `_1K-JPG_Color.jpg` and
`_1K-JPG_Displacement.jpg` maps from each 1K JPG download; the other files of the download
were not kept. Paper005 (crumpled) and Paper003 (creased) were looked at and not used.

The maps already tile: measured on the displacement maps (E049), the difference across the
wrap-around edge is within 0 to 23% of the difference between neighbouring rows or columns,
far below the difference between unrelated columns. No seam processing is applied. At startup
each displacement map is resampled whole to 512 x 512 (the shader restores its aspect),
high-passed (minus an 8-texel Gaussian) so no sheet-wide gradient, crease or cloud remains,
and normalised to zero mean and unit deviation, with a mip chain.
