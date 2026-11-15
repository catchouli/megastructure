//! Headless runner: the same update and render loop as the window, into an offscreen texture.
//! GPU setup and PNG readback follow `mega_capture`.

use crate::app::App;
use crate::cli::HeadlessArgs;
use crate::input::InputState;
use crate::sim::{MoveMode, Sim};
use anyhow::{anyhow, bail, Context, Result};
use mega_gen::scenes::Scene;
use mega_render::Renderer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub fn run(args: &HeadlessArgs, scene: &Scene, mode: Option<MoveMode>) -> Result<()> {
    if let Some(out) = &args.out {
        std::fs::create_dir_all(out).with_context(|| format!("cannot create output directory {}", out.display()))?;
    }
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
    let (w, h) = (args.width, args.height);
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

    let mut app = App::new(Renderer::new(device.clone(), queue.clone(), FORMAT, w, h), scene, mode);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("headless target"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let unpadded = w * 4;
    let padded = unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("headless readback"),
        size: padded as u64 * h as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    for i in 0..args.frames {
        let input = args.script.as_ref().map_or_else(InputState::default, |s| s.input_at(i, args.dt));
        app.update(args.dt, &input);

        let last = i + 1 == args.frames;
        let write = args.out.is_some() && (last || (args.every > 0 && i % args.every == 0));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("headless frame") });
        app.render(&mut encoder, &target_view);
        if write {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
        queue.submit([encoder.finish()]);

        if write {
            let (tx, rx) = std::sync::mpsc::channel();
            readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| anyhow!("GPU wait failed: {e}"))?;
            rx.recv().context("readback callback never ran")?.context("failed to map readback buffer")?;
            if gpu_failed.load(Ordering::SeqCst) {
                bail!("a GPU validation error occurred while rendering frame {i} (see message above)");
            }
            let mut pixels = Vec::with_capacity((unpadded * h) as usize);
            {
                let data = readback.slice(..).get_mapped_range().map_err(|e| anyhow!("cannot read mapped buffer: {e}"))?;
                for row in data.chunks(padded as usize).take(h as usize) {
                    pixels.extend_from_slice(&row[..unpadded as usize]);
                }
            }
            readback.unmap();
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

/// The final state line. The first six fields are the fly camera's; the rest describe the walker.
pub fn state_line(sim: &Sim, frames: u64) -> String {
    let c = sim.camera;
    let (grounded, vy, deaths, resets) = sim.walker().map_or((false, 0.0, 0, 0), |w| (w.grounded, w.vel.y, w.deaths, w.resets));
    format!(
        "state frames={} x={:.4} y={:.4} z={:.4} yaw_deg={:.3} pitch_deg={:.3} mode={} grounded={} vy={:.4} died={} deaths={} resets={}",
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
        resets
    )
}
