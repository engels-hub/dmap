//! The canvas grid: one cell is one inch, on both screens.
//!
//! DESIGN.md 5.1. The grid draws on the GPU, after the maps and under the
//! chrome, so the DM screen and the TV show the same lines from the same
//! code. An `egui` grid would have reached the DM screen alone, because
//! the TV window runs no `egui`.

// Rust guideline compliant 2026-02-21

use egui_wgpu::wgpu;

use crate::camera::Camera;

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
    pad: f32,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> grid: Grid;

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
    let world = grid.center + (at.xy - grid.viewport * 0.5) / grid.pixels_per_inch;
    let cell = world / grid.step;
    // How far this pixel sits from the nearest line, on each axis.
    let away = abs(fract(cell + vec2<f32>(0.5, 0.5)) - vec2<f32>(0.5, 0.5))
        * grid.step * grid.pixels_per_inch;
    let half = grid.width * 0.5;
    // Half a pixel of feather, or a thin line flickers as the camera moves.
    let solid = 1.0 - smoothstep(half - 0.5, half + 0.5, min(away.x, away.y));
    if solid <= 0.0 {
        discard;
    }
    return vec4<f32>(grid.color.rgb, grid.color.a * solid);
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
/// the four of the color. The seventh scalar is padding that keeps the
/// color on the 16 byte boundary a uniform block wants.
const FIELDS: usize = 12;

/// The GPU side of the grid: one pipeline and one uniform buffer.
pub struct GridLayer {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid"),
            bind_group_layouts: &[Some(&layout)],
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
        }
    }

    /// Draws the grid as `camera` sees it.
    ///
    /// `color` is the line color in linear light, with straight alpha.
    /// `width` is the thickness of a line in surface pixels.
    pub fn draw(
        &self,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        camera: &Camera,
        viewport: (u32, u32),
        color: [f32; 4],
        width: f32,
    ) {
        if camera.pixels_per_inch <= 0.0 || !camera.pixels_per_inch.is_finite() {
            return;
        }
        let values: [f32; FIELDS] = [
            viewport.0 as f32,
            viewport.1 as f32,
            camera.center.0 as f32,
            camera.center.1 as f32,
            camera.pixels_per_inch as f32,
            step_for(camera.pixels_per_inch) as f32,
            width,
            0.0,
            color[0],
            color[1],
            color[2],
            color[3],
        ];
        let bytes: Vec<u8> = values.iter().flat_map(|value| value.to_ne_bytes()).collect();
        queue.write_buffer(&self.uniform, 0, &bytes);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
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
