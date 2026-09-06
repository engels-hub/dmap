//! The map layer: images drawn on the canvas.

// Rust guideline compliant 2026-02-21

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui_wgpu::wgpu;

use crate::camera::Camera;
use crate::images::Decoded;
use crate::scene::MapObject;

/// Draws a textured quad with normal alpha blending.
const SHADER: &str = "
@group(0) @binding(0) var map_texture: texture_2d<f32>;
@group(0) @binding(1) var map_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@location(0) clip: vec2<f32>, @location(1) uv: vec2<f32>) -> VsOut {
    var out: VsOut;
    out.position = vec4<f32>(clip, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(map_texture, map_sampler, in.uv);
}
";

/// The GPU side of the maps: one texture per image file and one pipeline.
pub struct MapLayer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: HashMap<PathBuf, MapTexture>,
    vertices: wgpu::Buffer,
    capacity: usize,
}

struct MapTexture {
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
}

impl std::fmt::Debug for MapLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapLayer")
            .field("textures", &self.textures.len())
            .finish_non_exhaustive()
    }
}

/// Bytes of one map's six vertices.
const QUAD_BYTES: u64 = std::mem::size_of::<[MapVertex; 6]>() as u64;

impl MapLayer {
    /// Builds the pipeline for surfaces of `format`.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("maps"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("map texture"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("maps"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("maps"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<MapVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("maps"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let capacity = 16;
        Self {
            pipeline,
            layout,
            sampler,
            textures: HashMap::new(),
            vertices: vertex_buffer(device, capacity),
            capacity,
        }
    }

    /// Puts a decoded image on the GPU under `path`.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: PathBuf,
        decoded: &Decoded,
    ) {
        let (width, height) = decoded.size;
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: path.to_str(),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &decoded.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            extent,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: path.to_str(),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.textures.insert(
            path,
            MapTexture {
                bind_group,
                size: decoded.size,
            },
        );
    }

    /// Draws every map that has a texture, in order, as seen by `camera`.
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        maps: &[MapObject],
        camera: &Camera,
        viewport: (u32, u32),
    ) {
        let drawn: Vec<(&MapTexture, [MapVertex; 6])> = maps
            .iter()
            .filter_map(|map| {
                let texture = self.textures.get(&map.path)?;
                Some((texture, map_quad(map.rect(texture.size), camera, viewport)))
            })
            .collect();
        if drawn.is_empty() {
            return;
        }
        if drawn.len() > self.capacity {
            self.capacity = drawn.len().next_power_of_two();
            self.vertices = vertex_buffer(device, self.capacity);
        }
        let bytes: Vec<u8> = drawn
            .iter()
            .flat_map(|(_, quad)| quad.iter().flatten())
            .flat_map(|value| value.to_ne_bytes())
            .collect();
        queue.write_buffer(&self.vertices, 0, &bytes);
        pass.set_pipeline(&self.pipeline);
        for (i, (texture, _)) in drawn.iter().enumerate() {
            let start = i as u64 * QUAD_BYTES;
            pass.set_bind_group(0, &texture.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(start..start + QUAD_BYTES));
            pass.draw(0..6, 0..1);
        }
    }
}

/// A vertex buffer with room for `maps` quads.
fn vertex_buffer(device: &wgpu::Device, maps: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("map quads"),
        size: maps as u64 * QUAD_BYTES,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// One vertex: clip x, clip y, texture u, texture v.
pub type MapVertex = [f32; 4];

/// The two triangles that show a map's world `rect` through `camera`.
pub fn map_quad(
    rect: ((f64, f64), (f64, f64)),
    camera: &Camera,
    viewport: (u32, u32),
) -> [MapVertex; 6] {
    let (min, max) = rect;
    let corner = |u: f32, v: f32| -> MapVertex {
        let world = (
            if u == 0.0 { min.0 } else { max.0 },
            if v == 0.0 { min.1 } else { max.1 },
        );
        let (sx, sy) = camera.world_to_screen(world, viewport);
        let clip_x = (sx / f64::from(viewport.0)) * 2.0 - 1.0;
        let clip_y = 1.0 - (sy / f64::from(viewport.1)) * 2.0;
        [clip_x as f32, clip_y as f32, u, v]
    };
    [
        corner(0.0, 0.0),
        corner(1.0, 0.0),
        corner(0.0, 1.0),
        corner(0.0, 1.0),
        corner(1.0, 0.0),
        corner(1.0, 1.0),
    ]
}

/// The path to store in the project: relative when the file is inside
/// `project_dir`, otherwise as given.
pub fn relative_path(project_dir: &Path, file: &Path) -> PathBuf {
    file.strip_prefix(project_dir).unwrap_or(file).to_path_buf()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{map_quad, relative_path};
    use crate::camera::Camera;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn a_map_rect_becomes_a_clip_quad_with_texture_coordinates() {
        let camera = Camera {
            center: (0.0, 0.0),
            pixels_per_inch: 100.0,
        };
        // A 2 by 1 inch map centered on the origin in a 400 by 200 view.
        let quad = map_quad(((-1.0, -0.5), (1.0, 0.5)), &camera, (400, 200));
        assert_eq!(quad.len(), 6);
        let top_left = quad
            .iter()
            .find(|v| close(v[2], 0.0) && close(v[3], 0.0))
            .unwrap();
        assert!(close(top_left[0], -0.5) && close(top_left[1], 0.5));
        let bottom_right = quad
            .iter()
            .find(|v| close(v[2], 1.0) && close(v[3], 1.0))
            .unwrap();
        assert!(close(bottom_right[0], 0.5) && close(bottom_right[1], -0.5));
    }

    #[test]
    fn a_map_file_inside_the_project_folder_is_stored_relative() {
        let project = Path::new("/home/dm/campaign");
        assert_eq!(
            relative_path(project, Path::new("/home/dm/campaign/maps/crypt.png")),
            Path::new("maps/crypt.png")
        );
    }

    #[test]
    fn a_map_file_elsewhere_keeps_its_absolute_path() {
        let project = Path::new("/home/dm/campaign");
        assert_eq!(
            relative_path(project, Path::new("/mnt/maps/crypt.png")),
            Path::new("/mnt/maps/crypt.png")
        );
    }
}
