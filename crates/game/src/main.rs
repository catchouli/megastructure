//! Interactive viewer: fly camera over the test scene with an egui stats overlay.

use anyhow::{anyhow, Context, Result};
use glam::Vec3;
use mega_render::{Camera, Renderer};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

const USAGE: &str = "\
megastructure: fly through the test scene.

USAGE:
    megastructure [--exit-after-frames N]

CONTROLS:
    W A S D        move            Space / Ctrl   up / down
    Shift          fast            Scroll         change speed
    Mouse          look while any button is held, or while the cursor is grabbed (Tab)
    Escape         quit";

struct Args {
    exit_after_frames: Option<u64>,
}

fn parse_args() -> Result<Option<Args>> {
    let mut args = Args { exit_after_frames: None };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--exit-after-frames" => {
                let v = it.next().ok_or_else(|| anyhow!("--exit-after-frames needs a value"))?;
                args.exit_after_frames = Some(v.parse().map_err(|_| anyhow!("--exit-after-frames: '{v}' is not a number"))?);
            }
            other => return Err(anyhow!("unknown argument '{other}'\n\n{USAGE}")),
        }
    }
    Ok(Some(args))
}

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
}

struct Input {
    keys: HashSet<KeyCode>,
    buttons_held: u32,
    grabbed: bool,
    mouse_delta: (f32, f32),
    speed: f32,
}

struct App {
    args: Args,
    gfx: Option<Gfx>,
    camera: Camera,
    input: Input,
    last_frame: Instant,
    frame_ms: f32,
    frames: u64,
    error: Option<anyhow::Error>,
}

fn init_gfx(event_loop: &ActiveEventLoop) -> Result<Gfx> {
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

    let mut renderer = Renderer::new(device.clone(), queue.clone(), config.format, config.width, config.height);
    renderer.upload_scene(&mega_gen::test_scene::build());

    let egui_ctx = egui::Context::default();
    let egui_state = egui_winit::State::new(egui_ctx.clone(), egui::ViewportId::ROOT, &*window, Some(window.scale_factor() as f32), window.theme(), Some(device.limits().max_texture_dimension_2d as usize));
    let egui_renderer = egui_wgpu::Renderer::new(&device, config.format, egui_wgpu::RendererOptions::default());

    Ok(Gfx { window, surface, config, device, queue, renderer, egui_ctx, egui_state, egui_renderer })
}

impl App {
    fn new(args: Args) -> Self {
        Self {
            args,
            gfx: None,
            camera: Camera::new(Vec3::new(0.0, mega_render::paths::EYE_HEIGHT, 2.0), 0.0, 0.0),
            input: Input { keys: HashSet::new(), buttons_held: 0, grabbed: false, mouse_delta: (0.0, 0.0), speed: 8.0 },
            last_frame: Instant::now(),
            frame_ms: 16.0,
            frames: 0,
            error: None,
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

    fn update(&mut self, dt: f32) {
        let inp = &mut self.input;
        let looking = inp.grabbed || inp.buttons_held > 0;
        if looking {
            let sens = 0.0025;
            self.camera.yaw += inp.mouse_delta.0 * sens;
            self.camera.pitch = (self.camera.pitch - inp.mouse_delta.1 * sens).clamp(-1.553, 1.553);
        }
        inp.mouse_delta = (0.0, 0.0);

        let down = |k: KeyCode| inp.keys.contains(&k);
        let fwd = self.camera.forward();
        let right = self.camera.right();
        let mut m = Vec3::ZERO;
        if down(KeyCode::KeyW) {
            m += fwd;
        }
        if down(KeyCode::KeyS) {
            m -= fwd;
        }
        if down(KeyCode::KeyD) {
            m += right;
        }
        if down(KeyCode::KeyA) {
            m -= right;
        }
        if down(KeyCode::Space) {
            m += Vec3::Y;
        }
        if down(KeyCode::ControlLeft) || down(KeyCode::ControlRight) {
            m -= Vec3::Y;
        }
        let fast = if down(KeyCode::ShiftLeft) || down(KeyCode::ShiftRight) { 5.0 } else { 1.0 };
        self.camera.position += m.normalize_or_zero() * inp.speed * fast * dt;
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        self.frame_ms += (dt * 1000.0 - self.frame_ms) * 0.05;
        self.update(dt);

        let Some(g) = self.gfx.as_mut() else { return Ok(()) };
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
        let (cam, ms, speed) = (self.camera, self.frame_ms, self.input.speed);
        let mut out = g.egui_ctx.run_ui(raw, |ui| {
            egui::Window::new("Stats").resizable(false).show(ui.ctx(), |ui| {
                ui.label(format!("frame {ms:.2} ms ({:.0} fps)", 1000.0 / ms.max(0.001)));
                ui.label(format!("pos {:.1} {:.1} {:.1}", cam.position.x, cam.position.y, cam.position.z));
                ui.label(format!("yaw {:.0} pitch {:.0}  speed {speed:.1} m/s", cam.yaw.to_degrees(), cam.pitch.to_degrees()));
                ui.label("WASD Space Ctrl Shift, scroll: speed, Tab: grab, Esc: quit");
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
        g.renderer.render(&mut encoder, &view, &self.camera);
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

        self.frames += 1;
        if self.args.exit_after_frames.is_some_and(|n| self.frames >= n) {
            log::info!("exiting after {} frames", self.frames);
            event_loop.exit();
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_none() {
            match init_gfx(event_loop) {
                Ok(g) => {
                    g.window.request_redraw();
                    self.gfx = Some(g);
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
                    g.renderer.resize(size.width, size.height);
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
                self.input.speed = (self.input.speed * 1.2f32.powf(y)).clamp(0.5, 500.0);
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

fn main() -> Result<()> {
    env_logger::init();
    let Some(args) = parse_args()? else { return Ok(()) };
    let event_loop = EventLoop::new().context("cannot create event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(args);
    event_loop.run_app(&mut app).context("event loop failed")?;
    match app.error.take() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
