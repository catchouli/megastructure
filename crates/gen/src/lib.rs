//! Mesh generation for the megastructure. No GPU dependencies: everything here is plain data
//! that the renderer uploads as-is.

pub mod ao;
pub mod builder;
pub mod kit;
pub mod mesh;
pub mod scenes;
pub mod shaft;
pub mod test_scene;

pub use builder::{auto_hatch, box_ao, AoFn, FaceMask, MeshBuilder};
pub use mesh::{Collider, MeshData, Stroke, StrokeKind, Vertex};
