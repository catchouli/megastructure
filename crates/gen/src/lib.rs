//! Mesh generation for the megastructure. No GPU dependencies: everything here is plain data
//! that the renderer uploads as-is.

pub mod ao;
pub mod bounce;
pub mod builder;
pub mod kit;
pub mod labyrinth;
pub mod mesh;
pub mod scenes;
pub mod seed;
pub mod shaft;
pub mod test_scene;

pub use builder::{auto_hatch, box_ao, AoFn, FaceMask, MeshBuilder};
pub use mesh::{Chunk, Collider, MeshData, ScenePortal, SpaceBox, Stroke, StrokeKind, Vertex, OUTSIDE};
