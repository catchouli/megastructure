//! Windowed runner: winit window, surface and egui stats overlay around the shared `App`.

use crate::app::App as Game;
use crate::input::InputState;
use crate::offscreen::Offscreen;
use crate::shotio;
use crate::sim::MoveMode;
use anyhow::{anyhow, Context, Result};
use mega_gen::scenes::Scene;
use mega_render::Renderer;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

/// How long a screenshot confirmation or error stays in the overlay.
const TOAST_SECONDS: f32 = 5.0;

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
}

struct Input {
    keys: HashSet<KeyCode>,
    buttons_held: u32,
    grabbed: bool,
    mouse_delta: (f32, f32),
    /// F and R presses since the last frame (a press shorter than a frame still counts).
    toggle_pending: bool,
    reset_pending: bool,
    /// P or F12 pressed since the last frame.
    screenshot_pending: bool,
}

struct App<'a> {
    exit_after_frames: Option<u64>,
    move_mode: Option<MoveMode>,
    scene: &'a Scene,
    gfx: Option<Gfx>,
    game: Option<Game>,
    input: Input,
    last_frame: Instant,
    frame_ms: f32,
    frames: u64,
    error: Option<anyhow::Error>,
    /// Message shown in the overlay for a few seconds: text, is-error, when.
    toast: Option<(String, bool, Instant)>,
}

fn init_gfx(event_loop: &ActiveEventLoop, scene: &Scene, move_mode: Option<MoveMode>) -> Result<(Gfx, Game)> {
    let window = Arc::new(
        event_loop
            .create_window(Window::default_attributes().with_title("megastructure").with_inner_size(PhysicalSize::new(1280, 720)))
            .context("cannot create window")?,
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let surface = instance.create_surface(window.clone()).context("cannot create surface")?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .context("no suitable GPU adapter")?;
    log::info!("adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("game"),
        required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .context("cannot create device")?;

    let size = window.inner_size();
    let mut config = surface
        .get_default_config(&adapter, size.width.max(1), size.height.max(1))
        .ok_or_else(|| anyhow!("surface is not supported by the adapter"))?;
    // Prefer a gamma-space format: egui and the ink look both want display values.
    let caps = surface.get_capabilities(&adapter);
    if let Some(f) = caps.formats.iter().copied().find(|f| !f.is_srgb()) {
        config.format = f;
    }
    surface.configure(&device, &config);

    let renderer = Renderer::new(device.clone(), queue.clone(), config.format, config.width, config.height);
    let mut game = Game::new(renderer, scene, move_mode);
    // The display may run faster than the 60 Hz simulation: show the walker between steps.
    game.sim.interpolate = true;

    let egui_ctx = egui::Context::default();
    let egui_state = egui_winit::State::new(egui_ctx.clone(), egui::ViewportId::ROOT, &*window, Some(window.scale_factor() as f32), window.theme(), Some(device.limits().max_texture_dimension_2d as usize));
    let egui_renderer = egui_wgpu::Renderer::new(&device, config.format, egui_wgpu::RendererOptions::default());

    Ok((Gfx { window, surface, config, device, queue, egui_ctx, egui_state, egui_renderer }, game))
}

impl<'a> App<'a> {
    fn new(exit_after_frames: Option<u64>, scene: &'a Scene, move_mode: Option<MoveMode>) -> Self {
        Self {
            exit_after_frames,
            move_mode,
            scene,
            gfx: None,
            game: None,
            input: Input { keys: HashSet::new(), buttons_held: 0, grabbed: false, mouse_delta: (0.0, 0.0), toggle_pending: false, reset_pending: false, screenshot_pending: false },
            last_frame: Instant::now(),
            frame_ms: 16.0,
            frames: 0,
            error: None,
            toast: None,
        }
    }

    fn set_grab(&mut self, grab: bool) {
        let Some(g) = &self.gfx else { return };
        let ok = if grab {
            g.window.set_cursor_grab(CursorGrabMode::Locked).or_else(|_| g.window.set_cursor_grab(CursorGrabMode::Confined)).is_ok()
        } else {
            g.window.set_cursor_grab(CursorGrabMode::None).is_ok()
        };
        self.input.grabbed = grab && ok;
        g.window.set_cursor_visible(!self.input.grabbed);
    }

    /// Turn the held keys and accumulated mouse movement into an `InputState`.
    fn input_state(&mut self) -> InputState {
        let inp = &mut self.input;
        let looking = inp.grabbed || inp.buttons_held > 0;
        let mouse_delta = if looking { inp.mouse_delta } else { (0.0, 0.0) };
        inp.mouse_delta = (0.0, 0.0);
        let (mut toggle, mut reset) = (std::mem::take(&mut inp.toggle_pending), std::mem::take(&mut inp.reset_pending));
        let down = |k: KeyCode| inp.keys.contains(&k);
        InputState {
            forward: down(KeyCode::KeyW),
            back: down(KeyCode::KeyS),
            left: down(KeyCode::KeyA),
            right: down(KeyCode::KeyD),
            up: down(KeyCode::KeyE),
            down: down(KeyCode::ControlLeft) || down(KeyCode::ControlRight) || down(KeyCode::KeyQ),
            fast: down(KeyCode::ShiftLeft) || down(KeyCode::ShiftRight),
            jump: down(KeyCode::Space),
            toggle_fly: std::mem::take(&mut toggle),
            reset: std::mem::take(&mut reset),
            screenshot: std::mem::take(&mut inp.screenshot_pending),
            look_left: down(KeyCode::ArrowLeft),
            look_right: down(KeyCode::ArrowRight),
            look_up: down(KeyCode::ArrowUp),
            look_down: down(KeyCode::ArrowDown),
            mouse_delta,
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        self.frame_ms += (dt * 1000.0 - self.frame_ms) * 0.05;
        let input = self.input_state();
        let toast = self.toast.as_ref().filter(|t| t.2.elapsed().as_secs_f32() < TOAST_SECONDS).map(|t| (t.0.clone(), t.1));
        let (Some(g), Some(game)) = (self.gfx.as_mut(), self.game.as_mut()) else { return Ok(()) };
        game.update(dt, &input);
        let frame = match g.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                g.surface.configure(&g.device, &g.config);
                f
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                g.surface.configure(&g.device, &g.config);
                return Ok(());
            }
            other => return Err(anyhow!("cannot acquire surface texture: {other:?}")),
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // UI.
        let raw = g.egui_state.take_egui_input(&g.window);
        let (cam, ms, speed) = (game.camera(), self.frame_ms, game.sim.speed);
        let status = match (game.sim.mode, game.sim.walker()) {
            (MoveMode::Walk, Some(w)) => format!("walking{} deaths {}", if w.grounded { "" } else { ", airborne" }, w.deaths),
            _ => format!("flying, speed {speed:.1} m/s"),
        };
        let can_walk = game.sim.can_walk();
        let mut out = g.egui_ctx.run_ui(raw, |ui| {
            egui::Window::new("Stats").resizable(false).show(ui.ctx(), |ui| {
                ui.label(format!("frame {ms:.2} ms ({:.0} fps)", 1000.0 / ms.max(0.001)));
                ui.label(format!("pos {:.1} {:.1} {:.1}", cam.position.x, cam.position.y, cam.position.z));
                ui.label(format!("yaw {:.0} pitch {:.0}  {status}", cam.yaw.to_degrees(), cam.pitch.to_degrees()));
                ui.label("WASD, Shift: run/fast, Space: jump (fly: up), arrows: turn, Tab: grab, Esc: quit");
                ui.label("P or F12: screenshot (screenshots/)");
                if let Some((text, is_err)) = &toast {
                    let colour = if *is_err { egui::Color32::LIGHT_RED } else { egui::Color32::LIGHT_GREEN };
                    ui.colored_label(colour, text);
                }
                ui.label(if can_walk { "F: walk/fly, R: back to stable ground, E/Q/Ctrl and scroll: fly only" } else { "E/Q or Space/Ctrl: up/down, scroll: speed" });
            });
        });
        g.egui_state.handle_platform_output(&g.window, out.platform_output);
        let ppp = out.pixels_per_point;
        let jobs = g.egui_ctx.tessellate(out.shapes, ppp);
        let mut textures = std::mem::take(&mut out.textures_delta);
        for (id, deltas) in &textures.set {
            for delta in deltas {
                g.egui_renderer.update_texture(&g.device, &g.queue, *id, delta);
            }
        }

        let mut encoder = g.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        game.render(&mut encoder, &view);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [g.config.width, g.config.height], pixels_per_point: ppp };
        let extra = g.egui_renderer.update_buffers(&g.device, &g.queue, &mut encoder, &jobs, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            g.egui_renderer.render(&mut pass, &jobs, &screen);
        }
        g.queue.submit(extra.into_iter().chain([encoder.finish()]));
        g.window.pre_present_notify();
        g.queue.present(frame);
        for id in &textures.free {
            g.egui_renderer.free_texture(id);
        }
        textures.clear();

        // Screenshot: the same camera, preset and sun again, offscreen and without the overlay.
        if input.screenshot {
            let (w, h) = (g.config.width, g.config.height);
            let result = Offscreen::new(&g.device, &g.queue, g.config.format, w, h)
                .render_frame(game)
                .and_then(|pixels| shotio::save_screenshot(game, &pixels, w, h, Path::new(shotio::SCREENSHOT_DIR)));
            self.toast = Some(match result {
                Ok(png) => {
                    eprintln!("screenshot: {}", png.display());
                    (format!("saved {}", png.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())), false, Instant::now())
                }
                Err(e) => {
                    eprintln!("megastructure: screenshot failed: {e:#}");
                    log::error!("screenshot failed: {e:#}");
                    (format!("screenshot failed: {e:#}"), true, Instant::now())
                }
            });
        }

        self.frames += 1;
        if self.exit_after_frames.is_some_and(|n| self.frames >= n) {
            log::info!("exiting after {} frames", self.frames);
            event_loop.exit();
        }
        Ok(())
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_none() {
            match init_gfx(event_loop, self.scene, self.move_mode) {
                Ok((g, game)) => {
                    g.window.request_redraw();
                    self.gfx = Some(g);
                    self.game = Some(game);
                }
                Err(e) => {
                    self.error = Some(e);
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(g) = self.gfx.as_mut() else { return };
        let consumed = g.egui_state.on_window_event(&g.window, &event).consumed;
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    g.config.width = size.width;
                    g.config.height = size.height;
                    g.surface.configure(&g.device, &g.config);
                    if let Some(game) = self.game.as_mut() {
                        game.renderer.resize(size.width, size.height);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.redraw(event_loop) {
                    self.error = Some(e);
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    if pressed && code == KeyCode::Escape {
                        event_loop.exit();
                    } else if pressed && !event.repeat && code == KeyCode::Tab {
                        let grab = !self.input.grabbed;
                        self.set_grab(grab);
                    } else if pressed {
                        if !event.repeat {
                            match code {
                                KeyCode::KeyF => self.input.toggle_pending = true,
                                KeyCode::KeyR => self.input.reset_pending = true,
                                KeyCode::KeyP | KeyCode::F12 => self.input.screenshot_pending = true,
                                _ => {}
                            }
                        }
                        self.input.keys.insert(code);
                    } else {
                        self.input.keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput { state, .. } => {
                if state == ElementState::Pressed {
                    if !consumed {
                        self.input.buttons_held += 1;
                    }
                } else {
                    self.input.buttons_held = self.input.buttons_held.saturating_sub(1);
                }
            }
            WindowEvent::MouseWheel { delta, .. } if !consumed => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if let Some(game) = self.game.as_mut() {
                    game.sim.scale_speed(1.2f32.powf(y));
                }
            }
            WindowEvent::Focused(false) => {
                self.input.keys.clear();
                self.input.buttons_held = 0;
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.input.mouse_delta.0 += delta.0 as f32;
            self.input.mouse_delta.1 += delta.1 as f32;
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}

pub fn run(scene: &Scene, exit_after_frames: Option<u64>, move_mode: Option<MoveMode>) -> Result<()> {
    let event_loop = EventLoop::new().context("cannot create event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(exit_after_frames, scene, move_mode);
    event_loop.run_app(&mut app).context("event loop failed")?;
    match app.error.take() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
