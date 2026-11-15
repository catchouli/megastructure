//! Mesh generation for the megastructure. No GPU dependencies: everything here is plain data
//! that the renderer uploads as-is.

pub mod builder;
pub mod mesh;
pub mod scenes;
pub mod test_scene;

pub use builder::{auto_hatch, box_ao, AoFn, FaceMask, MeshBuilder};
pub use mesh::{MeshData, Stroke, StrokeKind, Vertex};
