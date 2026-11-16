//! The game proper: simulation plus renderer, driven identically by the windowed and the
//! headless runner. Knows nothing about windows or surfaces.

use crate::input::InputState;
use crate::sim::{MoveMode, Sim};
use mega_gen::scenes::Scene;
use mega_render::{Camera, Renderer, StylePreset, Sun};

pub struct App {
    pub sim: Sim,
    pub renderer: Renderer,
    /// Scene name, as given to `mega_gen::scenes::by_name`.
    pub scene_name: &'static str,
    /// Name of the preset the renderer started from (`custom` if none matches).
    pub preset_name: &'static str,
}

impl App {
    /// Upload `scene` to `renderer` and place the player at its spawn point, walking if the
    /// scene has colliders (unless `mode` says otherwise).
    pub fn new(mut renderer: Renderer, scene: &Scene, mode: Option<MoveMode>) -> Self {
        let preset = StylePreset::for_scene(scene.name);
        let preset_name = StylePreset::NAMES.iter().copied().find(|n| StylePreset::by_name(n).as_ref() == Some(&preset)).unwrap_or("custom");
        renderer.set_preset(preset);
        renderer.set_sun(Sun::for_scene(scene.name));
        renderer.upload_scene(&scene.mesh);
        Self { sim: Sim::for_scene(scene, mode), renderer, scene_name: scene.name, preset_name }
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
