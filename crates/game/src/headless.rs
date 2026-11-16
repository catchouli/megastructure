//! Headless runner: the same update and render loop as the window, into an offscreen texture.
//! GPU setup and PNG readback follow `mega_capture`.

use crate::app::App;
use crate::cli::{HeadlessArgs, ReplayArgs};
use crate::input::InputState;
use crate::offscreen::Offscreen;
use crate::shotio;
use crate::sim::{MoveMode, Sim};
use anyhow::{anyhow, bail, Context, Result};
use mega_gen::scenes::Scene;
use mega_render::Renderer;
use mega_shot::Shot;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub fn run(args: &HeadlessArgs, scene: &Scene, mode: Option<MoveMode>) -> Result<()> {
    if let Some(out) = &args.out {
        std::fs::create_dir_all(out).with_context(|| format!("cannot create output directory {}", out.display()))?;
    }
    let (w, h) = (args.width, args.height);
    let (device, queue, gpu_failed) = init_gpu(w, h)?;

    let mut app = App::new(Renderer::new(device.clone(), queue.clone(), FORMAT, w, h), scene, mode);

    let off = Offscreen::new(&device, &queue, FORMAT, w, h);
    let mut prev_shot = false;

    for i in 0..args.frames {
        let input = args.script.as_ref().map_or_else(InputState::default, |s| s.input_at(i, args.dt));
        app.update(args.dt, &input);
        // `screenshot` acts on the first frame it is held, like toggle_fly.
        if input.screenshot && !prev_shot {
            let dir = args.out.clone().unwrap_or_else(|| PathBuf::from(shotio::SCREENSHOT_DIR));
            let pixels = off.render_frame(&mut app)?;
            if gpu_failed.load(Ordering::SeqCst) {
                bail!("a GPU validation error occurred while rendering frame {i} (see message above)");
            }
            let png = shotio::save_screenshot(&app, &pixels, w, h, &dir)?;
            eprintln!("wrote {}", png.display());
        }
        prev_shot = input.screenshot;

        let last = i + 1 == args.frames;
        let write = args.out.is_some() && (last || (args.every > 0 && i % args.every == 0));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("headless frame") });
        app.render(&mut encoder, off.view());
        if write {
            off.encode_copy(&mut encoder);
        }
        queue.submit([encoder.finish()]);

        if write {
            let pixels = off.read_pixels()?;
            if gpu_failed.load(Ordering::SeqCst) {
                bail!("a GPU validation error occurred while rendering frame {i} (see message above)");
            }
            let file = args.out.as_ref().expect("checked above").join(format!("frame_{i:04}.png"));
            image::save_buffer(&file, &pixels, w, h, image::ColorType::Rgba8).with_context(|| format!("cannot write {}", file.display()))?;
            eprintln!("wrote {}", file.display());
        } else if i % 64 == 63 {
            // Keep the queue from growing without bound when nothing is read back.
            device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| anyhow!("GPU wait failed: {e}"))?;
        }
        if gpu_failed.load(Ordering::SeqCst) {
            bail!("a GPU validation error occurred while rendering frame {i} (see message above)");
        }
    }
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| anyhow!("GPU wait failed: {e}"))?;
    if gpu_failed.load(Ordering::SeqCst) {
        bail!("a GPU validation error occurred (see message above)");
    }

    println!("{}", state_line(&app.sim, args.frames));
    Ok(())
}

/// Create the device for an offscreen run of `w` x `h`; the flag is set by any wgpu validation error.
fn init_gpu(w: u32, h: u32) -> Result<(wgpu::Device, wgpu::Queue, Arc<AtomicBool>)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .context("no suitable GPU adapter found (is a Metal/Vulkan/DX12 device available?)")?;
    let info = adapter.get_info();
    log::info!("adapter: {} ({:?}, {:?})", info.name, info.backend, info.device_type);

    let max_dim = adapter.limits().max_texture_dimension_2d;
    if w > max_dim || h > max_dim {
        bail!("{w}x{h} exceeds this GPU's maximum texture size of {max_dim}");
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("headless"),
        required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .context("failed to create a GPU device")?;

    // Turn wgpu validation errors into a failed run instead of a silent blank image.
    let gpu_failed = Arc::new(AtomicBool::new(false));
    {
        let flag = gpu_failed.clone();
        device.on_uncaptured_error(Arc::new(move |err: wgpu::Error| {
            eprintln!("megastructure: GPU error: {err}");
            flag.store(true, Ordering::SeqCst);
        }));
    }
    Ok((device, queue, gpu_failed))
}

/// The final state line. The first six fields are the fly camera's; the rest describe the walker
/// (`hspeed` is its horizontal speed, m/s; `crouched` its stance).
pub fn state_line(sim: &Sim, frames: u64) -> String {
    let c = sim.camera;
    let (grounded, vy, hspeed, deaths, resets, crouched) = sim.walker().map_or((false, 0.0, 0.0, 0, 0, false), |w| (w.grounded, w.vel.y, glam::Vec2::new(w.vel.x, w.vel.z).length(), w.deaths, w.resets, w.crouched));
    format!(
        "state frames={} x={:.4} y={:.4} z={:.4} yaw_deg={:.3} pitch_deg={:.3} mode={} grounded={} vy={:.4} died={} deaths={} resets={} hspeed={:.4} crouched={}",
        frames,
        c.position.x,
        c.position.y,
        c.position.z,
        c.yaw.to_degrees(),
        c.pitch.to_degrees(),
        sim.mode.name(),
        grounded,
        vy,
        deaths > 0,
        deaths,
        resets,
        hspeed,
        crouched
    )
}

/// Render one frame from a sidecar's state and write `<name>.replay.png`.
pub fn replay(args: &ReplayArgs) -> Result<()> {
    let shot = Shot::read(&args.shot).map_err(|e| anyhow!(e))?;
    shotio::check_scene(&shot)?;
    shotio::warn_on_version_mismatch(&shot);
    let stem = args.shot.file_stem().map_or_else(|| "shot".to_string(), |s| s.to_string_lossy().into_owned());
    let dir = match &args.out {
        Some(d) => d.clone(),
        None => args.shot.parent().filter(|p| !p.as_os_str().is_empty()).map_or_else(|| PathBuf::from("."), PathBuf::from),
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create output directory {}", dir.display()))?;
    let (w, h) = (shot.width, shot.height);
    if w == 0 || h == 0 {
        bail!("sidecar resolution {w}x{h} is not valid");
    }
    let scene = mega_gen::scenes::by_name(&shot.scene).expect("checked by check_scene");
    let (device, queue, gpu_failed) = init_gpu(w, h)?;
    let mut app = App::new(Renderer::new(device.clone(), queue.clone(), FORMAT, w, h), &scene, None);
    shotio::apply_state(&mut app, &shot, &args.overrides)?;
    let off = Offscreen::new(&device, &queue, FORMAT, w, h);
    let pixels = off.render_frame(&mut app)?;
    if gpu_failed.load(Ordering::SeqCst) {
        bail!("a GPU validation error occurred while rendering (see message above)");
    }
    let file = dir.join(format!("{stem}.replay.png"));
    image::save_buffer(&file, &pixels, w, h, image::ColorType::Rgba8).with_context(|| format!("cannot write {}", file.display()))?;
    eprintln!("wrote {}", file.display());
    Ok(())
}
