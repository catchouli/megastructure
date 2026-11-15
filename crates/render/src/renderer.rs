use crate::Camera;
use bytemuck::{Pod, Zeroable};
use mega_gen::{MeshData, Stroke, Vertex};
use wgpu::util::DeviceExt;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    light_dir: [f32; 4],
    params: [f32; 4],
}

const VERTEX_ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x3, // position
    1 => Float32x3, // normal
    2 => Float32x3, // hatch_axis
    3 => Uint32,    // id_fine
    4 => Uint32,    // id_coarse
    5 => Float32,   // ao
];

/// Frame renderer. Owns the depth buffer and the uploaded scene.
///
/// Seams for later passes: `depth_view()` and `stroke_buffer()` are public, and `render()`
/// is the single entry point that records all passes into the caller's encoder.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    color_format: wgpu::TextureFormat,
    size: (u32, u32),
    depth_view: wgpu::TextureView,
    globals_buf: wgpu::Buffer,
    bind_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    vertex_buf: Option<wgpu::Buffer>,
    index_buf: Option<wgpu::Buffer>,
    index_count: u32,
    stroke_buf: wgpu::Buffer,
    stroke_count: u32,
}

fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn make_stroke_buffer(device: &wgpu::Device, strokes: &[Stroke]) -> wgpu::Buffer {
    // Storage bindings may not be empty, so keep one zeroed stroke when there are none.
    let placeholder = [Stroke::default()];
    let data: &[Stroke] = if strokes.is_empty() { &placeholder } else { strokes };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("strokes"),
        contents: bytemuck::cast_slice(data),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    })
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, color_format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("forward"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/forward.wgsl").into()),
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("forward"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("forward"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &VERTEX_ATTRS,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Greater),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let stroke_buf = make_stroke_buffer(&device, &[]);
        let bind_group = Self::make_bind_group(&device, &bind_layout, &globals_buf, &stroke_buf);
        let depth_view = make_depth(&device, width, height);
        Self {
            device,
            queue,
            color_format,
            size: (width.max(1), height.max(1)),
            depth_view,
            globals_buf,
            bind_layout,
            bind_group,
            pipeline,
            vertex_buf: None,
            index_buf: None,
            index_count: 0,
            stroke_buf,
            stroke_count: 0,
        }
    }

    fn make_bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, globals: &wgpu::Buffer, strokes: &wgpu::Buffer) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: strokes.as_entire_binding() },
            ],
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let size = (width.max(1), height.max(1));
        if size != self.size {
            self.size = size;
            self.depth_view = make_depth(&self.device, size.0, size.1);
        }
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Depth buffer (reversed-Z, `Depth32Float`), for later passes that read it.
    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth_view
    }

    /// Storage buffer holding `mega_gen::Stroke` records (always at least one element).
    pub fn stroke_buffer(&self) -> &wgpu::Buffer {
        &self.stroke_buf
    }

    pub fn stroke_count(&self) -> u32 {
        self.stroke_count
    }

    /// Replace the scene. Strokes go to a storage buffer but are not drawn yet.
    pub fn upload_scene(&mut self, mesh: &MeshData) {
        let usage_v = wgpu::BufferUsages::VERTEX;
        let usage_i = wgpu::BufferUsages::INDEX;
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            self.vertex_buf = None;
            self.index_buf = None;
            self.index_count = 0;
        } else {
            self.vertex_buf = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertices"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: usage_v,
            }));
            self.index_buf = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: usage_i,
            }));
            self.index_count = mesh.indices.len() as u32;
        }
        self.stroke_buf = make_stroke_buffer(&self.device, &mesh.strokes);
        self.stroke_count = mesh.strokes.len() as u32;
        self.bind_group = Self::make_bind_group(&self.device, &self.bind_layout, &self.globals_buf, &self.stroke_buf);
    }

    /// Record the frame into `encoder`, drawing into `target_view` (which must match the
    /// colour format and size given to `new`/`resize`).
    pub fn render(&mut self, encoder: &mut wgpu::CommandEncoder, target_view: &wgpu::TextureView, camera: &Camera) {
        let aspect = self.size.0 as f32 / self.size.1 as f32;
        let light = glam::Vec3::new(0.45, 0.75, -0.35).normalize();
        let globals = Globals {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            camera_pos: camera.position.extend(1.0).to_array(),
            light_dir: light.extend(0.0).to_array(),
            params: [if self.color_format.is_srgb() { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        };
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("forward"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let (Some(vb), Some(ib)) = (&self.vertex_buf, &self.index_buf) {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
        }
    }
}
