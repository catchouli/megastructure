//! An offscreen colour target with a readback buffer: render the app into it, get RGBA bytes.
//! Shared by the headless runner, the headless `screenshot` action, replay and the windowed
//! screenshot key.

use crate::app::App;
use anyhow::{anyhow, Context, Result};

pub struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded: u32,
}

impl Offscreen {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let padded = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offscreen readback"),
            size: padded as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self { device: device.clone(), queue: queue.clone(), format, width, height, target, view, readback, padded }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Queue a copy of the target into the readback buffer.
    pub fn encode_copy(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &self.target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.padded), rows_per_image: Some(self.height) },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
    }

    /// Wait for submitted work and return the target as tightly packed RGBA8 (BGRA formats are
    /// swapped), top row first.
    pub fn read_pixels(&self) -> Result<Vec<u8>> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| anyhow!("GPU wait failed: {e}"))?;
        rx.recv().context("readback callback never ran")?.context("failed to map readback buffer")?;
        let unpadded = (self.width * 4) as usize;
        let mut pixels = Vec::with_capacity(unpadded * self.height as usize);
        {
            let data = self.readback.slice(..).get_mapped_range().map_err(|e| anyhow!("cannot read mapped buffer: {e}"))?;
            for row in data.chunks(self.padded as usize).take(self.height as usize) {
                pixels.extend_from_slice(&row[..unpadded]);
            }
        }
        self.readback.unmap();
        if matches!(self.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
            for px in pixels.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
        }
        Ok(pixels)
    }

    /// Render one frame of `app` (no overlay) and read it back.
    pub fn render_frame(&self, app: &mut App) -> Result<Vec<u8>> {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("offscreen frame") });
        app.render(&mut encoder, &self.view);
        self.encode_copy(&mut encoder);
        self.queue.submit([encoder.finish()]);
        self.read_pixels()
    }
}
