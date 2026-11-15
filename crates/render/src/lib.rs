//! wgpu renderer for the megastructure. Surface-agnostic: it draws into any colour
//! `TextureView` you hand it, so the same code serves the window and the offscreen capture.

pub mod camera;
pub mod paths;
pub mod renderer;
pub mod style;

pub use camera::Camera;
pub use renderer::{DebugView, HatchLod, Renderer, StrokeVisibility, Sun, DEPTH_FORMAT};
pub use style::StylePreset;
