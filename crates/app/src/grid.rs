//! The canvas grid: one cell is one inch, on both screens.
//!
//! DESIGN.md 5.1. The grid draws on the GPU, after the maps and under the
//! chrome, so the DM screen and the TV show the same lines from the same
//! code. An `egui` grid would have reached the DM screen alone, because
//! the TV window runs no `egui`.

// Rust guideline compliant 2026-02-21

use egui_wgpu::wgpu;

use crate::camera::Camera;
use crate::gpu::Gpu;

/// Draws grid lines over the whole surface.
///
/// One triangle covers the screen. The fragment turns its own pixel back
/// into a world point, and the distance from there to the nearest line
/// decides how solid the pixel is.
const SHADER: &str = "
struct Grid {
    viewport: vec2<f32>,
    center: vec2<f32>,
    pixels_per_inch: f32,
    step: f32,
    width: f32,
    // Above zero the line takes its color from the map below it.
    automatic: f32,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> grid: Grid;
@group(1) @binding(0) var beneath: texture_2d<f32>;

// How far from the middle gray an automatic line must stay, in the gamma
// space the eye reads. A line that only turns the map around disappears
// over a middle gray, which is a common color for stone. A quarter is a
// step the eye finds over every map, and it is small enough that a light
// map still gets a dark line.
const PUSH: f32 = 0.25;

// The gamma of the sRGB curve, near enough for a line color.
const GAMMA: f32 = 2.2;

// How much of the light of a color each channel carries. Rec. 709.
const LIGHT: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

// The color a line takes over `below`: the light of the map, turned
// around, and then held away from the middle gray.
fn against(below: vec3<f32>) -> vec3<f32> {
    let light = dot(max(below, vec3<f32>(0.0, 0.0, 0.0)), LIGHT);
    let seen = pow(light, 1.0 / GAMMA);
    let away = (1.0 - seen) - 0.5;
    let side = select(-1.0, 1.0, away >= 0.0);
    let held = clamp(0.5 + side * max(abs(away), PUSH), 0.0, 1.0);
    let value = pow(held, GAMMA);
    return vec3<f32>(value, value, value);
}

@vertex
fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(corners[index], 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    // The pass writes the whole surface, because it carries the maps of
    // the frame from the texture the grid reads. A pixel with no line on
    // it gives back the map, unchanged.
    let below = textureLoad(beneath, vec2<i32>(at.xy), 0).rgb;
    if grid.width <= 0.0 {
        return vec4<f32>(below, 1.0);
    }
    let world = grid.center + (at.xy - grid.viewport * 0.5) / grid.pixels_per_inch;
    let cell = world / grid.step;
    // How far this pixel sits from the nearest line, on each axis.
    let away = abs(fract(cell + vec2<f32>(0.5, 0.5)) - vec2<f32>(0.5, 0.5))
        * grid.step * grid.pixels_per_inch;
    let half = grid.width * 0.5;
    // Half a pixel of feather, or a thin line flickers as the camera moves.
    let solid = 1.0 - smoothstep(half - 0.5, half + 0.5, min(away.x, away.y));
    if solid <= 0.0 {
        return vec4<f32>(below, 1.0);
    }
    var line = grid.color.rgb;
    if grid.automatic > 0.0 {
        line = against(below);
    }
    return vec4<f32>(mix(below, line, grid.color.a * solid), 1.0);
}
";

/// The narrowest a cell may draw, in pixels.
///
/// Under this the grid doubles its step, so a camera that shows the whole
/// canvas gets lines that stay apart instead of a solid wash.
const MIN_CELL: f64 = 24.0;

/// How many floats the shader's `Grid` block holds.
///
/// Two for the viewport, two for the center, then the four scalars, then
/// the four of the color. The last of the four scalars says how the line
/// takes its color, and it holds the color on the 16 byte boundary a
/// uniform block wants.
const FIELDS: usize = 12;

/// How a grid line draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    /// The line color in linear light, with straight alpha.
    ///
    /// An automatic line takes the alpha of this color and no more.
    pub color: [f32; 4],
    /// The thickness of a line in surface pixels.
    pub width: f32,
    /// Whether the line takes its color from the map below it.
    pub automatic: bool,
}

/// The GPU side of the grid: one pipeline and one uniform buffer.
pub struct GridLayer {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The layout of the texture the grid reads.
    ///
    /// One grid draws into two windows, and each window holds a texture of
    /// its own. So the bind group of the texture belongs to the frame, not
    /// to the layer, and each draw builds one.
    beneath_layout: wgpu::BindGroupLayout,
}

impl std::fmt::Debug for GridLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GridLayer").finish_non_exhaustive()
    }
}

impl GridLayer {
    /// Builds the pipeline for surfaces of `format`.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid uniform"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid uniform"),
            size: (FIELDS * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid uniform"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let beneath_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid beneath"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid"),
            bind_group_layouts: &[Some(&layout), Some(&beneath_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
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
                    // The pass carries the maps to the window, so it writes
                    // every pixel and mixes the line itself. A blend here
                    // would mix the map with the clear color instead.
                    blend: None,
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            uniform,
            bind_group,
            beneath_layout,
        }
    }

    /// Draws the maps of the frame, and the grid over them.
    ///
    /// `beneath` is the view of [`crate::gpu::Beneath`], which holds what
    /// the pass draws under the lines. The pass writes the whole surface,
    /// so it must run before the ink and the panels.
    pub fn draw(
        &self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        beneath: &wgpu::TextureView,
        camera: &Camera,
        viewport: (u32, u32),
        line: Line,
    ) {
        // A camera with no size gives a step of nothing and a wash of
        // lines. The maps still have to reach the window, so the pass runs
        // with a width of zero and draws them alone.
        let usable = camera.pixels_per_inch.is_finite() && camera.pixels_per_inch > 0.0;
        let pixels_per_inch = if usable { camera.pixels_per_inch } else { 1.0 };
        let width = if usable { line.width } else { 0.0 };
        let values: [f32; FIELDS] = [
            viewport.0 as f32,
            viewport.1 as f32,
            camera.center.0 as f32,
            camera.center.1 as f32,
            pixels_per_inch as f32,
            step_for(pixels_per_inch) as f32,
            width,
            f32::from(u8::from(line.automatic)),
            line.color[0],
            line.color[1],
            line.color[2],
            line.color[3],
        ];
        let bytes: Vec<u8> = values
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect();
        gpu.queue.write_buffer(&self.uniform, 0, &bytes);
        let maps = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid beneath"),
            layout: &self.beneath_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(beneath),
            }],
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(1, &maps, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// How many inches lie between two lines at this zoom.
///
/// One inch is the cell of DESIGN.md 5.1. A camera far enough out would
/// draw those lines closer than a pixel apart, so the step doubles until
/// a cell is at least `MIN_CELL` wide.
fn step_for(pixels_per_inch: f64) -> f64 {
    let mut step = 1.0;
    while step * pixels_per_inch < MIN_CELL {
        step *= 2.0;
    }
    step
}

#[cfg(test)]
mod tests {
    use super::{MIN_CELL, step_for};

    #[test]
    fn a_close_camera_draws_one_line_for_each_inch() {
        assert!((step_for(48.0) - 1.0).abs() < f64::EPSILON);
        assert!((step_for(MIN_CELL) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_far_camera_doubles_the_step_until_a_cell_reads() {
        // Every step is a whole doubling, so the lines that remain are the
        // lines that were there before.
        for zoom in [12.0, 6.0, 3.0, 0.5, 0.01] {
            let step = step_for(zoom);
            assert!(step * zoom >= MIN_CELL, "a cell at {zoom} is too narrow");
            assert!(step.log2().fract().abs() < 1e-9, "{step} is not a doubling");
        }
    }

    #[test]
    fn the_step_never_grows_when_the_camera_comes_closer() {
        let mut last = f64::INFINITY;
        for zoom in [0.5, 1.0, 4.0, 16.0, 64.0, 256.0] {
            let step = step_for(zoom);
            assert!(step <= last, "the step grew at {zoom}");
            last = step;
        }
    }
}
