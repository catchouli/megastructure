//! M2 first slice: "the labyrinth", a region of the megastructure generated from a seed as a
//! space graph first and geometry second. See docs/features/labyrinth.md.
//!
//! - `graph`: typed spaces and connections (layer L2).
//! - `plan`: the director (set pieces), the grammar (core cycle, loops, spurs) and the
//!   embedding (placement on a cell grid, passages routed by A*), giving a [`plan::Plan`].
//! - `geom`: the plan built from the kit of parts (layer L4), with colliders.
//! - `light`: openness, propagated from the sky through the spaces on the cell grid.
//! - `walk`: the reachability check in collider space.
//!
//! The region is `NX * CELL` by `NZ * CELL` metres in plan and `NY * LEVEL` metres tall; `x`,
//! `z` from 0, floor of the lowest storey at `y = 0`, the roof at `NY * LEVEL`.

pub mod geom;
pub mod graph;
pub mod grid;
pub mod light;
pub mod plan;
pub mod walk;

use crate::ao::Occluders;
use crate::builder::MeshBuilder;
use crate::mesh::{Collider, MeshData};
use glam::Vec3;

/// Plan size of a grid cell, metres.
pub const CELL: f32 = 4.0;
/// Height of a storey, metres.
pub const LEVEL: f32 = 6.0;
/// Floor slab thickness between storeys.
pub const SLAB: f32 = 0.6;
/// Region size in cells (x, z) and storeys.
pub const NX: i32 = 72;
pub const NZ: i32 = 64;
pub const NY: i32 = 26;
/// Thickness of the outer skin beyond the region's edge cells.
pub const SKIN: f32 = 2.0;
/// Risers per storey: 36 of 0.1667 m.
pub const RISERS_PER_LEVEL: usize = 36;
/// How strongly local occlusion darkens, as in the shaft.
pub const LOCAL_AO: f32 = 0.7;

/// The fixed default seed used by the scene registry.
pub const DEFAULT_SEED: u64 = 0x1ab_0001;

/// A built labyrinth.
pub struct Labyrinth {
    pub plan: plan::Plan,
    pub mesh: MeshData,
    pub colliders: Vec<Collider>,
    /// Milliseconds spent: planning, geometry, light field, AO bake.
    pub timings: [f64; 4],
}

impl Labyrinth {
    pub fn spawn(&self) -> ([f32; 3], f32) {
        (self.plan.spawn.0.to_array(), self.plan.spawn.1)
    }
}

/// Build a labyrinth with its AO baked.
pub fn build(seed: u64) -> Labyrinth {
    build_with(seed, true)
}

/// Build a labyrinth; without `bake` every vertex keeps ao 1 (for tests of layout and
/// colliders, which do not need the light).
pub fn build_with(seed: u64, bake: bool) -> Labyrinth {
    let t0 = std::time::Instant::now();
    let plan = plan::plan(seed);
    let t1 = std::time::Instant::now();
    let mut b = MeshBuilder::new();
    geom::build(&plan, &mut b);
    let t2 = std::time::Instant::now();
    if !bake {
        let (mesh, colliders) = b.finish_with_colliders();
        let ms = (t2 - t0).as_secs_f64() * 1000.0;
        return Labyrinth { plan, mesh, colliders, timings: [ms, 0.0, 0.0, 0.0] };
    }
    let field = light::LightField::new(&plan);
    let t3 = std::time::Instant::now();
    // Thin parts (railings, stair flights, corbels) do not occlude, as in the shaft.
    let occluders: Vec<Collider> = b.colliders.iter().copied().filter(|c| {
        let (_, _, half) = c.to_obb();
        half.iter().all(|&h| h > 0.25)
    }).collect();
    let occ = Occluders::new(&occluders, 4.0, 32, 3.0);
    b.bake_ao(&|p: Vec3, n: Vec3| field.openness(p, n) * (1.0 - LOCAL_AO * (1.0 - occ.local(p, n))));
    let t4 = std::time::Instant::now();
    let ms = |a: std::time::Instant, c: std::time::Instant| (c - a).as_secs_f64() * 1000.0;
    let (mesh, colliders) = b.finish_with_colliders();
    Labyrinth { plan, mesh, colliders, timings: [ms(t0, t1), ms(t1, t2), ms(t2, t3), ms(t3, t4)] }
}

#[cfg(test)]
mod tests;
