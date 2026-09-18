//! The picture of one frame, and the pass that lays it on a window.
//!
//! Every window draws its frame into a texture, and one pass lays that
//! texture on the surface. A TV that stands frozen reads no scene at all:
//! it lays down the picture it held, whatever the DM does behind it. A
//! copy of the tree could not do that, because the pixels of a map live
//! outside the tree and another scene brings its own. Issue #78.

// Rust guideline compliant 2026-02-21

use egui_wgpu::wgpu;

/// Lays a picture over the whole surface.
///
/// One triangle covers the screen. The sampler reads the picture across
/// it, so a picture held at one size still fills a window of another.
const SHADER: &str = "
struct Out {
    @builtin(position) at: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var edge: sampler;

@vertex
fn vs(@builtin(vertex_index) index: u32) -> Out {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let at = corners[index];
    var out: Out;
    out.at = vec4<f32>(at, 0.0, 1.0);
    // A texture reads from the top down, and clip space counts up.
    out.uv = vec2<f32>(at.x * 0.5 + 0.5, 0.5 - at.y * 0.5);
    return out;
}

@fragment
fn fs(out: Out) -> @location(0) vec4<f32> {
    return textureSample(picture, edge, out.uv);
}
";

/// One frame of a window, as a texture a pass can read.
pub struct Picture {
    view: wgpu::TextureView,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture").finish_non_exhaustive()
    }
}

impl Picture {
    /// A picture of `size`, in the format the surface takes.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, size: (u32, u32)) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("picture"),
            size: wgpu::Extent3d {
                width: size.0.max(1),
                height: size.1.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
        }
    }

    /// The view the frame writes and the pass reads.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

/// The pipeline that lays a picture on a surface.
pub struct HoldLayer {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    layout: wgpu::BindGroupLayout,
}

impl std::fmt::Debug for HoldLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HoldLayer").finish_non_exhaustive()
    }
}

impl HoldLayer {
    /// Builds the pipeline for surfaces of `format`.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hold"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hold picture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        // A window that changed size lays the picture down over its new
        // one, and a smooth read keeps the edges of the map from steps.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hold"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hold"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hold"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            sampler,
            layout,
        }
    }

    /// Lays `picture` over the whole of `pass`.
    ///
    /// A picture is a view, and a view belongs to the frame that holds it,
    /// so the bind group of the texture belongs to the draw.
    pub fn draw(&self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'_>, picture: &Picture) {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hold picture"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(picture.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
