//! The ink frame: forward MRT pass, stroke pass, contour pass, composite.
//!
//! See docs/features/ink-renderer.md.

use crate::style::{StyleGpu, StylePreset};
use crate::Camera;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use mega_gen::{MeshData, Stroke, Vertex};
use wgpu::util::DeviceExt;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// R = `id_fine`, G = `id_coarse`.
pub const ID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg32Uint;
/// Octahedral world normal.
pub const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
/// Palette index, tone band, ao, flags.
pub const FILL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// R black ink, G white ink.
pub const INK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg8Unorm;
/// Contour strength before anti-aliasing.
pub const CONTOUR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// Image height the preset's pixel sizes are written for.
pub const REFERENCE_HEIGHT: f32 = 720.0;

/// What the renderer shows. Everything except `Final` is for diagnosis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DebugView {
    #[default]
    Final,
    /// `id_fine` hashed to grey levels.
    Id,
    Normals,
    /// log2 distance, 1 m white to 1 km black.
    Depth,
    /// Tone bands: white, light hatch, crosshatch, black.
    Bands,
    /// Hatching and solid black only, no atmosphere.
    Hatching,
    /// Strokes only, no atmosphere.
    Strokes,
    /// Contours only, no atmosphere.
    Contours,
    /// Final without hatching (strokes and contours with atmosphere).
    Lines,
}

impl DebugView {
    pub const ALL: [DebugView; 9] = [
        DebugView::Final,
        DebugView::Id,
        DebugView::Normals,
        DebugView::Depth,
        DebugView::Bands,
        DebugView::Hatching,
        DebugView::Strokes,
        DebugView::Contours,
        DebugView::Lines,
    ];

    pub fn name(self) -> &'static str {
        match self {
            DebugView::Final => "final",
            DebugView::Id => "id",
            DebugView::Normals => "normals",
            DebugView::Depth => "depth",
            DebugView::Bands => "bands",
            DebugView::Hatching => "hatching",
            DebugView::Strokes => "strokes",
            DebugView::Contours => "contours",
            DebugView::Lines => "lines",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.name() == name)
    }

    fn index(self) -> u32 {
        Self::ALL.iter().position(|v| *v == self).unwrap() as u32
    }

    fn draws_strokes(self) -> bool {
        matches!(self, DebugView::Final | DebugView::Strokes | DebugView::Lines)
    }

    fn draws_contours(self) -> bool {
        matches!(self, DebugView::Final | DebugView::Contours | DebugView::Lines)
    }
}

/// How the hatch octave is chosen. Only `Directional` is meant for use; the others exist so
/// the octave experiment (E001 in docs/experiments.md) can be repeated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HatchLod {
    /// From `length(vec2(dpdx(u), dpdy(u)))` of the hatch coordinate itself.
    #[default]
    Directional,
    /// From the isotropic `length(fwidth(world_position))`.
    Isotropic,
    /// Always octave 0 (no level of detail).
    Fixed,
}

impl HatchLod {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "directional" => Some(Self::Directional),
            "isotropic" => Some(Self::Isotropic),
            "fixed" => Some(Self::Fixed),
            _ => None,
        }
    }
}

/// How a stroke fragment decides it is visible. Only `CentrelineBlock` is meant for use;
/// the others exist so experiment E002 can be repeated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StrokeVisibility {
    /// Farthest scene depth in the 3x3 block around the stroke's centreline pixel.
    #[default]
    CentrelineBlock,
    /// Scene depth at the centreline pixel only.
    Centreline,
    /// Scene depth at the fragment's own pixel.
    OwnPixel,
}

impl StrokeVisibility {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "block" => Some(Self::CentrelineBlock),
            "centreline" => Some(Self::Centreline),
            "pixel" => Some(Self::OwnPixel),
            _ => None,
        }
    }
}

/// The sun as a positioned body (see docs/features/sun-and-sky.md). Only a fixed sun is used
/// for now, and there is no shadow pass yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sun {
    pub position: Vec3,
    pub radius: f32,
    /// 0 disables the sun.
    pub intensity: f32,
}

impl Sun {
    /// Beyond this distance from the camera the sun is treated as a direction.
    pub const DIRECTIONAL_DISTANCE: f32 = 1.0e5;
}

impl Default for Sun {
    fn default() -> Self {
        // A remote sun about 32 degrees up, so floors are not fully lit and walls facing +X
        // and -Z catch light.
        let dir = Vec3::new(0.6, 0.53, -0.6).normalize();
        Self { position: dir * 1.5e11, radius: 7.0e8, intensity: 1.0 }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    camera_fwd: [f32; 4],
    sun: [f32; 4],
    sun_params: [f32; 4],
    screen: [f32; 4],
    misc: [f32; 4],
    flags: [u32; 4],
}

const VERTEX_ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x3, // position
    1 => Float32x3, // normal
    2 => Float32x3, // hatch_axis
    3 => Uint32,    // id_fine
    4 => Uint32,    // id_coarse
    5 => Float32,   // ao
];

struct Targets {
    depth_tex: wgpu::Texture,
    depth: wgpu::TextureView,
    id: wgpu::TextureView,
    normal: wgpu::TextureView,
    fill: wgpu::TextureView,
    ink: wgpu::TextureView,
    contour: wgpu::TextureView,
}

fn make_target(device: &wgpu::Device, w: u32, h: u32, format: wgpu::TextureFormat, label: &str) -> wgpu::TextureView {
    make_texture(device, w, h, format, label).create_view(&wgpu::TextureViewDescriptor::default())
}

fn make_texture(device: &wgpu::Device, w: u32, h: u32, format: wgpu::TextureFormat, label: &str) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
}

impl Targets {
    fn new(device: &wgpu::Device, w: u32, h: u32) -> Self {
        let depth_tex = make_texture(device, w, h, DEPTH_FORMAT, "depth");
        Self {
            depth: depth_tex.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_tex,
            id: make_target(device, w, h, ID_FORMAT, "id"),
            normal: make_target(device, w, h, NORMAL_FORMAT, "normal"),
            fill: make_target(device, w, h, FILL_FORMAT, "fill"),
            ink: make_target(device, w, h, INK_FORMAT, "ink"),
            contour: make_target(device, w, h, CONTOUR_FORMAT, "contour mask"),
        }
    }
}

fn storage_buffer<T: Pod + Default>(device: &wgpu::Device, label: &str, data: &[T]) -> wgpu::Buffer {
    // Storage bindings may not be empty, so keep one zeroed element when there are none.
    let placeholder = [T::default()];
    let data: &[T] = if data.is_empty() { &placeholder } else { data };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(data),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    })
}

fn tex_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
        count: None,
    }
}

fn buffer_entry(binding: u32, ty: wgpu::BufferBindingType, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

const UINT: wgpu::TextureSampleType = wgpu::TextureSampleType::Uint;
const FLOAT: wgpu::TextureSampleType = wgpu::TextureSampleType::Float { filterable: false };
const DEPTH: wgpu::TextureSampleType = wgpu::TextureSampleType::Depth;

fn shader(device: &wgpu::Device, label: &str, src: &str) -> wgpu::ShaderModule {
    let full = format!("{}\n{}", include_str!("../shaders/common.wgsl"), src);
    device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(full.into()) })
}

const MAX_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Max },
    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Max },
};

/// Frame renderer. Owns the frame targets, the uploaded scene and the style preset.
///
/// `render()` is the single entry point that records all passes into the caller's encoder
/// and writes the composite into the caller's colour view.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    color_format: wgpu::TextureFormat,
    size: (u32, u32),
    targets: Targets,
    preset: StylePreset,
    sun: Sun,
    view: DebugView,
    hatch_lod: HatchLod,
    stroke_vis: StrokeVisibility,

    globals_buf: wgpu::Buffer,
    style_buf: wgpu::Buffer,
    frame_group: wgpu::BindGroup,

    forward_pipeline: wgpu::RenderPipeline,
    stroke_pipeline: wgpu::RenderPipeline,
    contour_pipeline: wgpu::RenderPipeline,
    resolve_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    stroke_layout: wgpu::BindGroupLayout,
    contour_layout: wgpu::BindGroupLayout,
    resolve_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    stroke_group: wgpu::BindGroup,
    contour_group: wgpu::BindGroup,
    resolve_group: wgpu::BindGroup,
    composite_group: wgpu::BindGroup,

    vertex_buf: Option<wgpu::Buffer>,
    index_buf: Option<wgpu::Buffer>,
    index_count: u32,
    strokes: Vec<Stroke>,
    stroke_buf: wgpu::Buffer,
    visible_buf: wgpu::Buffer,
    visible: Vec<u32>,
}

impl Renderer {
    /// A renderer with the default (`nihei`) preset.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, color_format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        Self::with_preset(device, queue, color_format, width, height, StylePreset::nihei())
    }

    pub fn with_preset(
        device: wgpu::Device,
        queue: wgpu::Queue,
        color_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        preset: StylePreset,
    ) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        let vf = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[buffer_entry(0, wgpu::BufferBindingType::Uniform, vf), buffer_entry(1, wgpu::BufferBindingType::Uniform, vf)],
        });
        let storage = wgpu::BufferBindingType::Storage { read_only: true };
        let stroke_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("strokes"),
            entries: &[buffer_entry(0, storage, vf), buffer_entry(1, storage, vf), tex_entry(2, DEPTH)],
        });
        let contour_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("contours"),
            entries: &[tex_entry(0, UINT), tex_entry(1, DEPTH), tex_entry(2, FLOAT)],
        });
        let resolve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("contour resolve"),
            entries: &[tex_entry(0, FLOAT)],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite"),
            entries: &[tex_entry(0, FLOAT), tex_entry(1, FLOAT), tex_entry(2, DEPTH), tex_entry(3, UINT), tex_entry(4, FLOAT)],
        });

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let style_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("style"),
            size: std::mem::size_of::<StyleGpu>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &frame_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: style_buf.as_entire_binding() },
            ],
        });

        let layout = |label: &str, second: Option<&wgpu::BindGroupLayout>| {
            let groups: Vec<Option<&wgpu::BindGroupLayout>> = match second {
                Some(s) => vec![Some(&frame_layout), Some(s)],
                None => vec![Some(&frame_layout)],
            };
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &groups, immediate_size: 0 })
        };

        // Forward MRT.
        let forward_src = shader(&device, "forward", include_str!("../shaders/forward.wgsl"));
        let forward_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("forward"),
            layout: Some(&layout("forward", None)),
            vertex: wgpu::VertexState {
                module: &forward_src,
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
                module: &forward_src,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[
                    Some(ID_FORMAT.into()),
                    Some(NORMAL_FORMAT.into()),
                    Some(FILL_FORMAT.into()),
                    Some(INK_FORMAT.into()),
                ],
            }),
            multiview_mask: None,
            cache: None,
        });

        let ink_target = [Some(wgpu::ColorTargetState { format: INK_FORMAT, blend: Some(MAX_BLEND), write_mask: wgpu::ColorWrites::ALL })];
        let screen_pipeline = |label: &str, module: &wgpu::ShaderModule, vs: &str, layout: &wgpu::PipelineLayout, targets: &[Option<wgpu::ColorTargetState>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState { module, entry_point: Some(vs), compilation_options: Default::default(), buffers: &[] },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let stroke_src = shader(&device, "strokes", include_str!("../shaders/strokes.wgsl"));
        let stroke_pipeline = screen_pipeline("strokes", &stroke_src, "vs_main", &layout("strokes", Some(&stroke_layout)), &ink_target);
        let contour_src = shader(&device, "contours", include_str!("../shaders/contours.wgsl"));
        let contour_pipeline = screen_pipeline(
            "contours",
            &contour_src,
            "vs_fullscreen",
            &layout("contours", Some(&contour_layout)),
            &[Some(CONTOUR_FORMAT.into())],
        );
        let resolve_src = shader(&device, "contour resolve", include_str!("../shaders/contour_resolve.wgsl"));
        let resolve_pipeline =
            screen_pipeline("contour resolve", &resolve_src, "vs_fullscreen", &layout("contour resolve", Some(&resolve_layout)), &ink_target);
        let composite_src = shader(&device, "composite", include_str!("../shaders/composite.wgsl"));
        let composite_pipeline = screen_pipeline(
            "composite",
            &composite_src,
            "vs_fullscreen",
            &layout("composite", Some(&composite_layout)),
            &[Some(color_format.into())],
        );

        let targets = Targets::new(&device, width, height);
        let stroke_buf = storage_buffer::<Stroke>(&device, "strokes", &[]);
        let visible_buf = storage_buffer::<u32>(&device, "visible strokes", &[]);
        let stroke_group = Self::make_stroke_group(&device, &stroke_layout, &stroke_buf, &visible_buf, &targets);
        let contour_group = Self::make_contour_group(&device, &contour_layout, &targets);
        let resolve_group = Self::make_resolve_group(&device, &resolve_layout, &targets);
        let composite_group = Self::make_composite_group(&device, &composite_layout, &targets);

        Self {
            device,
            queue,
            color_format,
            size: (width, height),
            targets,
            preset,
            sun: Sun::default(),
            view: DebugView::Final,
            hatch_lod: HatchLod::Directional,
            stroke_vis: StrokeVisibility::CentrelineBlock,
            globals_buf,
            style_buf,
            frame_group,
            forward_pipeline,
            stroke_pipeline,
            contour_pipeline,
            resolve_pipeline,
            composite_pipeline,
            stroke_layout,
            contour_layout,
            resolve_layout,
            composite_layout,
            stroke_group,
            contour_group,
            resolve_group,
            composite_group,
            vertex_buf: None,
            index_buf: None,
            index_count: 0,
            strokes: Vec::new(),
            stroke_buf,
            visible_buf,
            visible: Vec::new(),
        }
    }

    fn make_stroke_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        strokes: &wgpu::Buffer,
        visible: &wgpu::Buffer,
        t: &Targets,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strokes"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: strokes.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: visible.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&t.depth) },
            ],
        })
    }

    fn make_contour_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, t: &Targets) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("contours"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&t.id) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&t.depth) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&t.normal) },
            ],
        })
    }

    fn make_resolve_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, t: &Targets) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("contour resolve"),
            layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&t.contour) }],
        })
    }

    fn make_composite_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, t: &Targets) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&t.fill) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&t.ink) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&t.depth) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&t.id) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&t.normal) },
            ],
        })
    }

    fn rebuild_groups(&mut self) {
        self.stroke_group = Self::make_stroke_group(&self.device, &self.stroke_layout, &self.stroke_buf, &self.visible_buf, &self.targets);
        self.contour_group = Self::make_contour_group(&self.device, &self.contour_layout, &self.targets);
        self.resolve_group = Self::make_resolve_group(&self.device, &self.resolve_layout, &self.targets);
        self.composite_group = Self::make_composite_group(&self.device, &self.composite_layout, &self.targets);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let size = (width.max(1), height.max(1));
        if size != self.size {
            self.size = size;
            self.targets = Targets::new(&self.device, size.0, size.1);
            self.rebuild_groups();
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

    pub fn preset(&self) -> &StylePreset {
        &self.preset
    }

    pub fn set_preset(&mut self, preset: StylePreset) {
        self.preset = preset;
    }

    pub fn sun(&self) -> Sun {
        self.sun
    }

    pub fn set_sun(&mut self, sun: Sun) {
        self.sun = sun;
    }

    pub fn view(&self) -> DebugView {
        self.view
    }

    pub fn set_view(&mut self, view: DebugView) {
        self.view = view;
    }

    /// Diagnostic: change how the hatch octave is chosen.
    pub fn set_hatch_lod(&mut self, lod: HatchLod) {
        self.hatch_lod = lod;
    }

    /// Diagnostic: change the stroke visibility test.
    pub fn set_stroke_visibility(&mut self, vis: StrokeVisibility) {
        self.stroke_vis = vis;
    }

    /// Depth buffer (reversed-Z, `Depth32Float`), for later passes that read it.
    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.targets.depth
    }

    /// The depth texture behind [`Renderer::depth_view`] (copyable, for tools that read it back).
    pub fn depth_texture(&self) -> &wgpu::Texture {
        &self.targets.depth_tex
    }

    /// Storage buffer holding `mega_gen::Stroke` records (always at least one element).
    pub fn stroke_buffer(&self) -> &wgpu::Buffer {
        &self.stroke_buf
    }

    pub fn stroke_count(&self) -> u32 {
        self.strokes.len() as u32
    }

    /// Number of strokes that passed the CPU cull in the last `render`.
    pub fn visible_stroke_count(&self) -> u32 {
        self.visible.len() as u32
    }

    /// Replace the scene.
    pub fn upload_scene(&mut self, mesh: &MeshData) {
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            self.vertex_buf = None;
            self.index_buf = None;
            self.index_count = 0;
        } else {
            self.vertex_buf = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertices"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }));
            self.index_buf = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            }));
            self.index_count = mesh.indices.len() as u32;
        }
        self.strokes = mesh.strokes.clone();
        self.stroke_buf = storage_buffer(&self.device, "strokes", &mesh.strokes);
        let zeros = vec![0u32; mesh.strokes.len()];
        self.visible_buf = storage_buffer(&self.device, "visible strokes", &zeros);
        self.visible.clear();
        self.rebuild_groups();
    }

    /// CPU stroke cull: frustum (both endpoints outside one side plane or behind the near
    /// plane) and rank distance. Writes the index list the stroke pass draws.
    fn cull_strokes(&mut self, view_proj: Mat4, eye: Vec3) {
        self.visible.clear();
        let m = view_proj.transpose();
        // Planes in clip space as dot(row, p) >= 0: w+x, w-x, w+y, w-y, w-z (reversed-Z near).
        // A small margin keeps strokes whose width pokes in from just outside.
        let (r0, r1, r2, r3) = (m.x_axis, m.y_axis, m.z_axis, m.w_axis);
        let margin = 1.02;
        let planes = [r3 * margin + r0, r3 * margin - r0, r3 * margin + r1, r3 * margin - r1, r3 - r2];
        let fade_end = self.preset.rank_fade_end;
        for (i, s) in self.strokes.iter().enumerate() {
            let a = Vec3::from(s.a);
            let b = Vec3::from(s.b);
            let (pa, pb) = (a.extend(1.0), b.extend(1.0));
            if planes.iter().any(|pl: &Vec4| pl.dot(pa) < 0.0 && pl.dot(pb) < 0.0) {
                continue;
            }
            let ab = b - a;
            let t = ((eye - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
            let dist = (a + ab * t - eye).length();
            if dist > fade_end[s.rank.min(3) as usize] {
                continue;
            }
            self.visible.push(i as u32);
        }
        if !self.visible.is_empty() {
            self.queue.write_buffer(&self.visible_buf, 0, bytemuck::cast_slice(&self.visible));
        }
    }

    /// Record the frame into `encoder`, drawing into `target_view` (which must match the
    /// colour format and size given to `new`/`resize`).
    pub fn render(&mut self, encoder: &mut wgpu::CommandEncoder, target_view: &wgpu::TextureView, camera: &Camera) {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let view_proj = camera.view_proj(w / h);
        let to_sun = self.sun.position - camera.position;
        let sun = if to_sun.length() > Sun::DIRECTIONAL_DISTANCE {
            to_sun.normalize().extend(0.0)
        } else {
            self.sun.position.extend(1.0)
        };
        let globals = Globals {
            view_proj: view_proj.to_cols_array_2d(),
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            camera_pos: camera.position.extend(crate::camera::NEAR).to_array(),
            camera_fwd: camera.forward().extend(0.0).to_array(),
            sun: sun.to_array(),
            sun_params: [self.sun.radius, self.sun.intensity, 0.0, 0.0],
            screen: [w, h, 1.0 / w, 1.0 / h],
            misc: [h / REFERENCE_HEIGHT, 2.0 * (camera.fov_y * 0.5).tan() / h, 0.0, 0.0],
            flags: [self.view.index(), self.color_format.is_srgb() as u32, self.hatch_lod as u32, self.stroke_vis as u32],
        };
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        self.queue.write_buffer(&self.style_buf, 0, bytemuck::bytes_of(&self.preset.to_gpu()));
        self.cull_strokes(view_proj, camera.position);

        let clear = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })
        };

        // 1. Forward geometry, MRT.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("forward"),
                color_attachments: &[clear(&self.targets.id), clear(&self.targets.normal), clear(&self.targets.fill), clear(&self.targets.ink)],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let (Some(vb), Some(ib)) = (&self.vertex_buf, &self.index_buf) {
                pass.set_pipeline(&self.forward_pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.index_count, 0, 0..1);
            }
        }

        // 2. Contour detection into the contour mask.
        if self.view.draws_contours() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("contours"),
                color_attachments: &[clear(&self.targets.contour)],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.contour_pipeline);
            pass.set_bind_group(0, &self.frame_group, &[]);
            pass.set_bind_group(1, &self.contour_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // 3. Strokes and resolved contours, max-blended into ink.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ink lines"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.ink,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.frame_group, &[]);
            if self.view.draws_strokes() && !self.visible.is_empty() {
                pass.set_pipeline(&self.stroke_pipeline);
                pass.set_bind_group(1, &self.stroke_group, &[]);
                pass.draw(0..6, 0..self.visible.len() as u32);
            }
            if self.view.draws_contours() {
                pass.set_pipeline(&self.resolve_pipeline);
                pass.set_bind_group(1, &self.resolve_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }

        // 4. Composite into the caller's target.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &self.frame_group, &[]);
            pass.set_bind_group(1, &self.composite_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_view_names_round_trip() {
        for v in DebugView::ALL {
            assert_eq!(DebugView::from_name(v.name()), Some(v));
        }
        assert_eq!(DebugView::from_name("nope"), None);
        // The shader constants VIEW_* in common.wgsl follow this order.
        assert_eq!(DebugView::Final.index(), 0);
        assert_eq!(DebugView::Lines.index(), 8);
    }

    #[test]
    fn uniform_layouts_match_wgsl() {
        // struct Globals: two mat4x4 and seven vec4. struct Style: seventeen vec4.
        assert_eq!(std::mem::size_of::<Globals>(), 2 * 64 + 7 * 16);
        assert_eq!(std::mem::size_of::<StyleGpu>(), 17 * 16);
    }

    #[test]
    fn default_sun_is_directional_and_above_the_horizon() {
        let s = Sun::default();
        assert!(s.position.length() > Sun::DIRECTIONAL_DISTANCE);
        assert!(s.position.y > 0.0 && s.intensity > 0.0);
    }

    #[test]
    fn nihei_bands_are_ordered() {
        let p = StylePreset::nihei();
        assert!(p.band_hatch > p.band_cross && p.band_cross > p.band_black && p.band_black > 0.0);
        assert!(p.contour_fine_near < p.contour_fine_far);
        for r in 0..4 {
            assert!(p.rank_fade_start[r] < p.rank_fade_end[r]);
        }
    }
}
