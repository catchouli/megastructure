//! Headless renderer: draws a named camera path of the test scene to PNG frames.
//!
//! See docs/features/capture-tool.md.

use anyhow::{anyhow, bail, Context, Result};
use mega_render::{paths, Renderer};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

const USAGE: &str = "\
mega_capture: render a camera path of the test scene to PNG frames, no window.

USAGE:
    mega_capture --path <name> --out <dir> [--frames N] [--width W] [--height H] [--t T]

OPTIONS:
    --path <name>    camera path (see --list-paths)                    [required]
    --out <dir>      output directory, created if missing             [required]
    --frames <N>     number of frames, default 1. Frame i uses t = i/(N-1)
    --t <T>          t in 0..1 for a single frame (only with --frames 1), default 0.5
    --width <W>      image width in pixels, default 1280
    --height <H>     image height in pixels, default 720
    --list-paths     print the available path names and exit
    -h, --help       print this help

Files are written as frame_0000.png, frame_0001.png, ...
Set RUST_LOG=info for adapter details.";

struct Args {
    path: String,
    out: PathBuf,
    frames: u32,
    t: Option<f32>,
    width: u32,
    height: u32,
}

fn parse_args() -> Result<Option<Args>> {
    let mut it = std::env::args().skip(1);
    let (mut path, mut out, mut frames, mut t, mut width, mut height) = (None, None, 1u32, None, 1280u32, 720u32);
    fn value(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
        it.next().ok_or_else(|| anyhow!("{flag} needs a value"))
    }
    fn number<T: std::str::FromStr>(s: &str, flag: &str) -> Result<T> {
        s.parse().map_err(|_| anyhow!("{flag}: '{s}' is not a valid number"))
    }
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--list-paths" => {
                for n in paths::NAMES {
                    println!("{n}");
                }
                return Ok(None);
            }
            "--path" => path = Some(value(&mut it, &a)?),
            "--out" => out = Some(PathBuf::from(value(&mut it, &a)?)),
            "--frames" => frames = number(&value(&mut it, &a)?, &a)?,
            "--t" => t = Some(number(&value(&mut it, &a)?, &a)?),
            "--width" => width = number(&value(&mut it, &a)?, &a)?,
            "--height" => height = number(&value(&mut it, &a)?, &a)?,
            other => bail!("unknown argument '{other}'\n\n{USAGE}"),
        }
    }
    let path = path.ok_or_else(|| anyhow!("--path is required (one of: {})", paths::NAMES.join(", ")))?;
    let out = out.ok_or_else(|| anyhow!("--out is required"))?;
    if paths::by_name(&path).is_none() {
        bail!("unknown path '{path}'; available: {}", paths::NAMES.join(", "));
    }
    if frames == 0 {
        bail!("--frames must be at least 1");
    }
    if width == 0 || height == 0 {
        bail!("--width and --height must be at least 1");
    }
    if let Some(t) = t {
        if frames != 1 {
            bail!("--t only applies to a single frame (--frames 1); with more frames t runs 0..1 across the sequence");
        }
        if !(0.0..=1.0).contains(&t) {
            bail!("--t must be within 0..1, got {t}");
        }
    }
    Ok(Some(Args { path, out, frames, t, width, height }))
}

fn main() {
    env_logger::init();
    if let Err(e) = run() {
        eprintln!("mega_capture: error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let Some(args) = parse_args()? else { return Ok(()) };
    let cam_fn = paths::by_name(&args.path).expect("validated in parse_args");

    std::fs::create_dir_all(&args.out).with_context(|| format!("cannot create output directory {}", args.out.display()))?;

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
    if args.width > max_dim || args.height > max_dim {
        bail!("{}x{} exceeds this GPU's maximum texture size of {max_dim}", args.width, args.height);
    }

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("capture"),
        required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .context("failed to create a GPU device")?;

    // Turn wgpu validation errors into a failed run instead of a silent blank image.
    let gpu_failed = Arc::new(AtomicBool::new(false));
    {
        let flag = gpu_failed.clone();
        device.on_uncaptured_error(Arc::new(move |err: wgpu::Error| {
            eprintln!("mega_capture: GPU error: {err}");
            flag.store(true, Ordering::SeqCst);
        }));
    }

    let (w, h) = (args.width, args.height);
    let mut renderer = Renderer::new(device.clone(), queue.clone(), FORMAT, w, h);
    let scene = mega_gen::test_scene::build();
    renderer.upload_scene(&scene);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("capture target"),
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
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = unpadded.div_ceil(align) * align;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture readback"),
        size: padded as u64 * h as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    for i in 0..args.frames {
        let t = if args.frames == 1 { args.t.unwrap_or(0.5) } else { i as f32 / (args.frames - 1) as f32 };
        let camera = cam_fn(t);

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("capture") });
        renderer.render(&mut encoder, &target_view, &camera);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        queue.submit([encoder.finish()]);

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

        let file = args.out.join(format!("frame_{i:04}.png"));
        image::save_buffer(&file, &pixels, w, h, image::ColorType::Rgba8).with_context(|| format!("cannot write {}", file.display()))?;
        println!("{}", file.display());
    }
    Ok(())
}
