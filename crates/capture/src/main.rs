//! Headless renderer: draws a named camera path of a scene to PNG frames.
//!
//! See docs/features/capture-tool.md.

use anyhow::{anyhow, bail, Context, Result};
use mega_render::{paths, DebugView, HatchLod, Renderer, StrokeVisibility, StylePreset, Sun};
use std::time::Instant;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

const USAGE: &str = "\
mega_capture: render a camera path of a scene to PNG frames, no window.

USAGE:
    mega_capture --path <name> --out <dir> [--frames N] [--width W] [--height H] [--t T]
                 [--t0 A --t1 B] [--view V] [--metrics] [--no-png]
    mega_capture --path <name> --bench N [--width W] [--height H] [--t T] [--view V]

OPTIONS:
    --path <name>    camera path (see --list-paths)                    [required]
    --scene <name>   scene to render (see --list-scenes). Default: the scene the path
                     is tuned to, or mega_gen::scenes::DEFAULT for a path without one.
                     The scene also selects its style preset and sun.
    --out <dir>      output directory, created if missing             [required]
    --frames <N>     number of frames, default 1. Frame i uses t = i/(N-1)
    --t <T>          t in 0..1 for a single frame (only with --frames 1), default 0.5
    --t0 <A>         start of the t range for a sequence, default 0
    --t1 <B>         end of the t range for a sequence, default 1
    --view <V>       final, id, normals, depth, bands, hatching, strokes, contours, lines
                     (default final)
    --metrics        print the mean absolute difference between consecutive frames, at
                     full resolution and after a 4x4 box downsample (0..255 grey levels)
                     and after reprojecting the previous frame into the current one by
                     depth. Paper grain is switched off while measuring.
    --no-png         do not write PNG files (with --metrics); --out is then optional
    --error-png      with --metrics, also write the blurred reprojection error of each
                     frame, times 8, as error_NNNN.png (white = no difference)
    --hatch-lod <M>  diagnostic hatch octave selection: directional (default), isotropic,
                     fixed
    --stroke-vis <M> diagnostic stroke visibility test: block (default), centreline, pixel
    --depth-tol <X>  override the preset's stroke depth tolerance
    --bench <N>      render one frame N times without readback and print ms per frame
    --width <W>      image width in pixels, default 1280
    --height <H>     image height in pixels, default 720
    --list-paths     print the available path names (with their scenes) and exit
    --list-scenes    print the available scene names and exit
    --preset <name>  style preset instead of the scene's own: nihei, nihei_vast
    -h, --help       print this help

With --frames N, frame i uses t = t0 + (t1 - t0) * i / (N - 1).
Files are written as frame_0000.png, frame_0001.png, ...
A line 'scene <name>: ...' with vertex, triangle, stroke and collider counts and the scene
build time is printed first.
Set RUST_LOG=info for adapter details.";

struct Args {
    path: String,
    scene: String,
    preset: Option<StylePreset>,
    out: Option<PathBuf>,
    frames: u32,
    t: Option<f32>,
    t0: f32,
    t1: f32,
    width: u32,
    height: u32,
    view: DebugView,
    metrics: bool,
    png: bool,
    bench: Option<u32>,
    error_png: bool,
    hatch_lod: HatchLod,
    stroke_vis: StrokeVisibility,
    depth_tol: Option<f32>,
}

fn parse_args() -> Result<Option<Args>> {
    let mut it = std::env::args().skip(1);
    let (mut path, mut out, mut frames, mut t, mut width, mut height) = (None, None, 1u32, None, 1280u32, 720u32);
    let (mut t0, mut t1, mut view, mut metrics, mut png, mut bench) = (0.0f32, 1.0f32, DebugView::Final, false, true, None);
    let (mut error_png, mut hatch_lod) = (false, HatchLod::Directional);
    let (mut stroke_vis, mut depth_tol) = (StrokeVisibility::CentrelineBlock, None);
    let mut scene: Option<String> = None;
    let mut preset: Option<StylePreset> = None;
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
                    println!("{n} ({})", paths::scene_of(n).unwrap_or(mega_gen::scenes::DEFAULT));
                }
                return Ok(None);
            }
            "--list-scenes" => {
                for n in mega_gen::scenes::NAMES {
                    println!("{n}");
                }
                return Ok(None);
            }
            "--scene" => scene = Some(value(&mut it, &a)?),
            "--preset" => {
                let v = value(&mut it, &a)?;
                preset = Some(StylePreset::by_name(&v).ok_or_else(|| anyhow!("unknown preset '{v}'; available: {}", StylePreset::NAMES.join(", ")))?);
            }
            "--path" => path = Some(value(&mut it, &a)?),
            "--out" => out = Some(PathBuf::from(value(&mut it, &a)?)),
            "--frames" => frames = number(&value(&mut it, &a)?, &a)?,
            "--t" => t = Some(number(&value(&mut it, &a)?, &a)?),
            "--width" => width = number(&value(&mut it, &a)?, &a)?,
            "--height" => height = number(&value(&mut it, &a)?, &a)?,
            "--t0" => t0 = number(&value(&mut it, &a)?, &a)?,
            "--t1" => t1 = number(&value(&mut it, &a)?, &a)?,
            "--view" => {
                let v = value(&mut it, &a)?;
                view = DebugView::from_name(&v).ok_or_else(|| {
                    let names: Vec<&str> = DebugView::ALL.iter().map(|v| v.name()).collect();
                    anyhow!("unknown view '{v}'; available: {}", names.join(", "))
                })?;
            }
            "--metrics" => metrics = true,
            "--no-png" => png = false,
            "--error-png" => error_png = true,
            "--stroke-vis" => {
                let v = value(&mut it, &a)?;
                stroke_vis = StrokeVisibility::from_name(&v).ok_or_else(|| anyhow!("unknown stroke visibility '{v}'; use block, centreline or pixel"))?;
            }
            "--depth-tol" => depth_tol = Some(number(&value(&mut it, &a)?, &a)?),
            "--hatch-lod" => {
                let v = value(&mut it, &a)?;
                hatch_lod = HatchLod::from_name(&v).ok_or_else(|| anyhow!("unknown hatch LOD mode '{v}'; use directional, isotropic or fixed"))?;
            }
            "--bench" => bench = Some(number(&value(&mut it, &a)?, &a)?),
            other => bail!("unknown argument '{other}'\n\n{USAGE}"),
        }
    }
    let path = path.ok_or_else(|| anyhow!("--path is required (one of: {})", paths::NAMES.join(", ")))?;
    if out.is_none() && png && bench.is_none() {
        bail!("--out is required (or use --no-png / --bench)");
    }
    if bench == Some(0) {
        bail!("--bench must be at least 1");
    }
    if !(0.0..=1.0).contains(&t0) || !(0.0..=1.0).contains(&t1) {
        bail!("--t0 and --t1 must be within 0..1");
    }
    if paths::by_name(&path).is_none() {
        bail!("unknown path '{path}'; available: {}", paths::NAMES.join(", "));
    }
    let scene = scene.unwrap_or_else(|| paths::scene_of(&path).unwrap_or(mega_gen::scenes::DEFAULT).to_string());
    if !mega_gen::scenes::NAMES.contains(&scene.as_str()) {
        bail!("unknown scene '{scene}'; available: {}", mega_gen::scenes::NAMES.join(", "));
    }
    if let Some(own) = paths::scene_of(&path) {
        if own != scene {
            eprintln!("mega_capture: warning: path '{path}' is tuned to scene '{own}', rendering it in '{scene}'");
        }
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
    Ok(Some(Args { path, scene, preset, out, frames, t, t0, t1, width, height, view, metrics, png, bench, error_png, hatch_lod, stroke_vis, depth_tol }))
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
    let built = Instant::now();
    let scene = mega_gen::scenes::by_name(&args.scene).expect("validated in parse_args");
    let build_ms = built.elapsed().as_secs_f64() * 1000.0;
    println!(
        "scene {}: {} vertices, {} triangles, {} strokes, {} colliders, built in {build_ms:.0} ms",
        args.scene,
        scene.mesh.vertices.len(),
        scene.mesh.indices.len() / 3,
        scene.mesh.strokes.len(),
        scene.colliders.len()
    );
    renderer.upload_scene(&scene.mesh);
    renderer.set_preset(args.preset.clone().unwrap_or_else(|| StylePreset::for_scene(&args.scene)));
    renderer.set_sun(Sun::for_scene(&args.scene));
    renderer.set_view(args.view);
    renderer.set_hatch_lod(args.hatch_lod);
    renderer.set_stroke_visibility(args.stroke_vis);
    if let Some(tol) = args.depth_tol {
        renderer.set_preset(StylePreset { stroke_depth_tol: tol, ..renderer.preset().clone() });
    }
    if args.metrics {
        // Screen-anchored grain does not move with the scene, so it would read as error.
        renderer.set_preset(StylePreset { paper_grain: 0.0, ..renderer.preset().clone() });
    }

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

    // Depth readback for the reprojection metric (Depth32Float is 4 bytes per texel too).
    let depth_readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture depth readback"),
        size: padded as u64 * h as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let aspect = w as f32 / h as f32;

    if let Some(n) = args.bench {
        let camera = cam_fn(args.t.unwrap_or(0.5));
        let run = |count: u32, renderer: &mut Renderer| -> Result<f64> {
            let start = Instant::now();
            for _ in 0..count {
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("bench") });
                renderer.render(&mut encoder, &target_view, &camera);
                queue.submit([encoder.finish()]);
            }
            device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| anyhow!("GPU wait failed: {e}"))?;
            Ok(start.elapsed().as_secs_f64() * 1000.0 / count as f64)
        };
        run(10.min(n), &mut renderer)?; // warm-up
        let ms = run(n, &mut renderer)?;
        if gpu_failed.load(Ordering::SeqCst) {
            bail!("a GPU validation error occurred while benchmarking (see message above)");
        }
        println!(
            "bench {} {}x{} view {}: {ms:.3} ms/frame over {n} frames ({} of {} strokes drawn)",
            args.path,
            w,
            h,
            args.view.name(),
            renderer.visible_stroke_count(),
            renderer.stroke_count()
        );
        return Ok(());
    }

    let mut prev: Option<Frame> = None;
    let (mut mad_full, mut mad_down, mut rmad, mut rpop) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut rblur, mut ink) = (Vec::new(), Vec::new());
    for i in 0..args.frames {
        let t = if args.frames == 1 {
            args.t.unwrap_or(0.5)
        } else {
            args.t0 + (args.t1 - args.t0) * i as f32 / (args.frames - 1) as f32
        };
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
        if args.metrics {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: renderer.depth_texture(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::DepthOnly,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &depth_readback,
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
        queue.submit([encoder.finish()]);

        let (tx, rx) = std::sync::mpsc::channel();
        let tx2 = tx.clone();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        if args.metrics {
            depth_readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx2.send(r);
            });
        } else {
            drop(tx2);
        }
        device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| anyhow!("GPU wait failed: {e}"))?;
        rx.recv().context("readback callback never ran")?.context("failed to map readback buffer")?;
        if args.metrics {
            rx.recv().context("depth readback callback never ran")?.context("failed to map depth readback buffer")?;
        }
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

        if args.metrics {
            let grey: Vec<f32> = pixels.chunks(4).map(|p| (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0).collect();
            let mut depth = Vec::with_capacity((w * h) as usize);
            {
                let data = depth_readback.slice(..).get_mapped_range().map_err(|e| anyhow!("cannot read mapped buffer: {e}"))?;
                for row in data.chunks(padded as usize).take(h as usize) {
                    depth.extend(row[..unpadded as usize].chunks(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])));
                }
            }
            depth_readback.unmap();
            ink.push(grey.iter().map(|g| 255.0 - g).sum::<f32>() / grey.len() as f32);
            let blurred = box_blur(&grey, w, h, 2);
            let cur = Frame { grey, blurred, depth, view_proj: camera.view_proj(aspect) };
            if let Some(prev) = &prev {
                mad_full.push(mad(&prev.grey, &cur.grey));
                mad_down.push(mad(&downsample4(&prev.grey, w, h), &downsample4(&cur.grey, w, h)));
                let (m, pop, _) = reprojected_mad(prev, &cur, w, h, false);
                let (bm, _, berr) = reprojected_mad(prev, &cur, w, h, true);
                rblur.push(bm);
                if let (true, Some(out)) = (args.error_png, &args.out) {
                    let img: Vec<u8> = berr.iter().map(|e| 255 - ((e * 8.0).min(255.0) as u8)).collect();
                    let file = out.join(format!("error_{i:04}.png"));
                    image::save_buffer(&file, &img, w, h, image::ColorType::L8).with_context(|| format!("cannot write {}", file.display()))?;
                }
                rmad.push(m);
                rpop.push(pop);
            }
            prev = Some(cur);
        }
        if let (true, Some(out)) = (args.png, &args.out) {
            let file = out.join(format!("frame_{i:04}.png"));
            image::save_buffer(&file, &pixels, w, h, image::ColorType::Rgba8).with_context(|| format!("cannot write {}", file.display()))?;
            println!("{}", file.display());
        }
    }
    if args.metrics && !mad_full.is_empty() {
        let stats = |v: &[f32]| {
            let mean = v.iter().sum::<f32>() / v.len() as f32;
            let max = v.iter().cloned().fold(0.0f32, f32::max);
            (mean, max)
        };
        let (fm, fx) = stats(&mad_full);
        let (dm, dx) = stats(&mad_down);
        let (rm, rx_) = stats(&rmad);
        let (pm, px_) = stats(&rpop);
        let (bm, bx) = stats(&rblur);
        let (im, _) = stats(&ink);
        println!(
            "metrics {} view {}: mean ink {im:.2}; reprojected MAD after 5x5 blur mean {bm:.3} max {bx:.3}; per unit ink: raw {:.3}, blurred {:.3}",
            args.path,
            args.view.name(),
            stats(&rmad).0 / im.max(1e-3),
            bm / im.max(1e-3)
        );
        println!(
            "metrics {} view {}: reprojected MAD mean {rm:.3} max {rx_:.3}; pixels off by >64 levels mean {:.3}% max {:.3}%",
            args.path,
            args.view.name(),
            pm * 100.0,
            px_ * 100.0
        );
        println!(
            "metrics {} view {} frames {} t {}..{}: consecutive MAD full mean {fm:.3} max {fx:.3}; 4x4-down mean {dm:.3} max {dx:.3}; ratio {:.2}",
            args.path,
            args.view.name(),
            args.frames,
            args.t0,
            args.t1,
            fm / dm.max(1e-6)
        );
    }
    Ok(())
}

fn mad(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / a.len().max(1) as f32
}

/// 4x4 box downsample of a grey image (edges cropped to a multiple of 4).
fn downsample4(g: &[f32], w: u32, h: u32) -> Vec<f32> {
    let (dw, dh) = ((w / 4) as usize, (h / 4) as usize);
    let mut out = vec![0.0; dw * dh];
    for y in 0..dh * 4 {
        for x in 0..dw * 4 {
            out[(y / 4) * dw + x / 4] += g[y * w as usize + x] / 16.0;
        }
    }
    out
}

struct Frame {
    grey: Vec<f32>,
    blurred: Vec<f32>,
    depth: Vec<f32>,
    view_proj: glam::Mat4,
}

/// Warp the previous frame into the current one using the current depth and both cameras,
/// skip pixels that were hidden or off screen before, and return the mean absolute grey
/// difference and the fraction of compared pixels that differ by more than 64 levels.
/// Ink that stays put on surfaces leaves only resampling error; swimming or popping ink
/// adds to it.
fn reprojected_mad(prev: &Frame, cur: &Frame, w: u32, h: u32, blurred: bool) -> (f32, f32, Vec<f32>) {
    let (pg, cg) = if blurred { (&prev.blurred, &cur.blurred) } else { (&prev.grey, &cur.grey) };
    let mut err = vec![0.0f32; (w * h) as usize];
    let inv = cur.view_proj.inverse();
    let (wf, hf) = (w as f32, h as f32);
    let near = mega_render::camera::NEAR;
    let (mut sum, mut n, mut pops) = (0.0f64, 0u64, 0u64);
    let at = |img: &[f32], x: usize, y: usize| img[y * w as usize + x];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let d = at(&cur.depth, x, y);
            if d <= 0.0 {
                continue;
            }
            let ndc = glam::Vec4::new((x as f32 + 0.5) / wf * 2.0 - 1.0, 1.0 - (y as f32 + 0.5) / hf * 2.0, d, 1.0);
            let world = inv * ndc;
            let world = world / world.w;
            let c = prev.view_proj * world;
            if c.w <= near {
                continue;
            }
            let (qx, qy) = ((c.x / c.w + 1.0) * 0.5 * wf - 0.5, (1.0 - c.y / c.w) * 0.5 * hf - 0.5);
            if qx < 0.0 || qy < 0.0 || qx >= wf - 1.0 || qy >= hf - 1.0 {
                continue;
            }
            // Disocclusion: the previous frame must have seen this same surface point.
            let (nx, ny) = (qx.round() as usize, qy.round() as usize);
            let z_expected = c.w;
            let z_seen = near / at(&prev.depth, nx, ny).max(1e-30);
            if (z_seen - z_expected).abs() > 0.01 * z_expected {
                continue;
            }
            let (x0, y0) = (qx.floor() as usize, qy.floor() as usize);
            let (fx, fy) = (qx - x0 as f32, qy - y0 as f32);
            let g = pg;
            let top = at(g, x0, y0) * (1.0 - fx) + at(g, x0 + 1, y0) * fx;
            let bottom = at(g, x0, y0 + 1) * (1.0 - fx) + at(g, x0 + 1, y0 + 1) * fx;
            let e = (top * (1.0 - fy) + bottom * fy - at(cg, x, y)).abs();
            err[y * w as usize + x] = e;
            sum += e as f64;
            n += 1;
            if e > 64.0 {
                pops += 1;
            }
        }
    }
    if n == 0 {
        return (0.0, 0.0, err);
    }
    ((sum / n as f64) as f32, pops as f32 / n as f32, err)
}

/// Separable box blur of radius r (edges clamped).
fn box_blur(g: &[f32], w: u32, h: u32, r: i32) -> Vec<f32> {
    let (w, h) = (w as i32, h as i32);
    let k = 1.0 / (2 * r + 1) as f32;
    let mut tmp = vec![0.0f32; g.len()];
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for d in -r..=r {
                s += g[(y * w + (x + d).clamp(0, w - 1)) as usize];
            }
            tmp[(y * w + x) as usize] = s * k;
        }
    }
    let mut out = vec![0.0f32; g.len()];
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for d in -r..=r {
                s += tmp[((y + d).clamp(0, h - 1) * w + x) as usize];
            }
            out[(y * w + x) as usize] = s * k;
        }
    }
    out
}
