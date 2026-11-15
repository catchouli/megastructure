//! The game proper: simulation plus renderer, driven identically by the windowed and the
//! headless runner. Knows nothing about windows or surfaces.

use crate::input::InputState;
use crate::sim::Sim;
use glam::Vec3;
use mega_gen::scenes::Scene;
use mega_render::{Camera, Renderer};

pub struct App {
    pub sim: Sim,
    pub renderer: Renderer,
}

impl App {
    /// Upload `scene` to `renderer` and place the camera at its spawn point.
    pub fn new(mut renderer: Renderer, scene: &Scene) -> Self {
        renderer.upload_scene(&scene.mesh);
        Self { sim: Sim::new(Vec3::from(scene.spawn), scene.spawn_yaw), renderer }
    }

    pub fn camera(&self) -> Camera {
        self.sim.camera
    }

    pub fn update(&mut self, dt: f32, input: &InputState) {
        self.sim.update(dt, input);
    }

    /// Record the scene into `target_view`. Overlays (egui) are added by the caller.
    pub fn render(&mut self, encoder: &mut wgpu::CommandEncoder, target_view: &wgpu::TextureView) {
        self.renderer.render(encoder, target_view, &self.sim.camera);
    }
}
