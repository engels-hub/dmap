//! The GPU side of what the DM drew. DESIGN.md 8.3 and PLAN.md 5.3.
//!
//! A stroke is a line through points in inches, and this turns it into
//! triangles: a quad for each length of line, and a disc at each end and
//! each bend, so a corner is round and a short stroke still shows. The
//! vertices carry world inches, and the shader moves them with the
//! camera, so the DM screen and the TV draw one mesh from one place.
//!
//! The mesh is built for each frame it draws. A cache belongs here once a
//! scene holds enough ink to feel it, and PLAN.md 5.3 keeps that note.

// Rust guideline compliant 2026-02-21

use egui_wgpu::wgpu;

use crate::camera::Camera;
use crate::stroke::Stroke;

/// Moves a vertex from world inches into the clip space of the surface.
const SHADER: &str = "
struct View {
    viewport: vec2<f32>,
    center: vec2<f32>,
    pixels_per_inch: f32,
    pad_a: f32,
    pad_b: f32,
    pad_c: f32,
};

@group(0) @binding(0) var<uniform> view: View;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs(@location(0) world: vec2<f32>, @location(1) color: vec4<f32>) -> VsOut {
    let screen = view.viewport * 0.5 + (world - view.center) * view.pixels_per_inch;
    var out: VsOut;
    out.position = vec4<f32>(
        screen.x / view.viewport.x * 2.0 - 1.0,
        1.0 - screen.y / view.viewport.y * 2.0,
        0.0,
        1.0,
    );
    out.color = color;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
";

/// How many floats the shader's `View` block holds.
///
/// Two for the viewport, two for the center, the zoom, then three of
/// padding, which keeps the block on the 16 byte boundary a uniform wants.
const FIELDS: usize = 8;

/// How much of its color a filled shape keeps.
///
/// An area of effect lies over the map and the map has to read through
/// it, so the fill goes to a quarter of the alpha the DM picked and the
/// outline keeps the whole of it.
const FILL_ALPHA: f32 = 0.25;

/// How many sides a round cap or a round bend is drawn with.
///
/// Eight reads as round at the width a pen draws, and costs eight
/// triangles at each end and each bend.
const ROUND: usize = 8;

/// One vertex: the world point in inches, then its color.
type Vertex = [f32; 6];

/// The GPU side of the ink: one pipeline, one uniform, one mesh buffer.
pub struct InkLayer {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    mesh: wgpu::Buffer,
    /// How many vertices the mesh buffer has room for.
    room: usize,
}

impl std::fmt::Debug for InkLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InkLayer").finish_non_exhaustive()
    }
}

impl InkLayer {
    /// Builds the pipeline for surfaces of `format`.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ink"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ink view"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ink view"),
            size: (FIELDS * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ink view"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ink"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ink"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
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
        let mesh = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ink mesh"),
            size: 0,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            uniform,
            bind_group,
            mesh,
            room: 0,
        }
    }

    /// Draws every stroke as `camera` sees it.
    ///
    /// `live` is what the DM is drawing right now, which is in no scene
    /// yet, and it draws over the rest.
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        strokes: &[&Stroke],
        camera: &Camera,
        viewport: (u32, u32),
    ) {
        if camera.pixels_per_inch <= 0.0 || !camera.pixels_per_inch.is_finite() {
            return;
        }
        let mut mesh: Vec<Vertex> = Vec::new();
        for stroke in strokes {
            add_stroke(&mut mesh, stroke);
        }
        if mesh.is_empty() {
            return;
        }
        self.room(device, mesh.len());
        let bytes: Vec<u8> = mesh
            .iter()
            .flatten()
            .flat_map(|value| value.to_ne_bytes())
            .collect();
        queue.write_buffer(&self.mesh, 0, &bytes);
        let view: [f32; FIELDS] = [
            viewport.0 as f32,
            viewport.1 as f32,
            camera.center.0 as f32,
            camera.center.1 as f32,
            camera.pixels_per_inch as f32,
            0.0,
            0.0,
            0.0,
        ];
        let view: Vec<u8> = view.iter().flat_map(|value| value.to_ne_bytes()).collect();
        queue.write_buffer(&self.uniform, 0, &view);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.mesh.slice(..));
        pass.draw(0..mesh.len() as u32, 0..1);
    }

    /// Makes sure the mesh buffer holds this many vertices.
    fn room(&mut self, device: &wgpu::Device, vertices: usize) {
        if vertices <= self.room {
            return;
        }
        // The buffer grows in doublings, so a pen that adds a point at a
        // time does not build a buffer at every step.
        let room = vertices.next_power_of_two();
        self.mesh = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ink mesh"),
            size: (room * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.room = room;
    }
}

/// Adds the triangles of one stroke to the mesh.
fn add_stroke(mesh: &mut Vec<Vertex>, stroke: &Stroke) {
    let line = stroke.polyline();
    let color = color_of(stroke.color);
    if stroke.ink.filled() {
        let mut fill = color;
        fill[3] *= FILL_ALPHA;
        add_fill(mesh, &line, fill);
    }
    let half = (stroke.width / 2.0).max(0.0) as f32;
    if half <= 0.0 {
        return;
    }
    let points: Vec<[f32; 2]> = line.iter().map(|(x, y)| [*x as f32, *y as f32]).collect();
    // A stroke of one point is a dot, which the disc alone draws.
    if points.len() == 1 {
        add_disc(mesh, points[0], half, color);
        return;
    }
    for pair in points.windows(2) {
        add_length(mesh, pair[0], pair[1], half, color);
    }
    // A disc at each point rounds both ends and every bend, so no corner
    // shows a notch and no length pulls away from the next.
    for point in &points {
        add_disc(mesh, *point, half, color);
    }
}

/// Fills the inside of a shape with a fan of triangles.
///
/// The fan runs from the first point, which lies on the shape. A burst,
/// a cone and a beam are all round or straight enough for that: no fan
/// of theirs ever reaches outside the shape it fills.
fn add_fill(mesh: &mut Vec<Vertex>, line: &[(f64, f64)], color: [f32; 4]) {
    if line.len() < 3 {
        return;
    }
    let point = |at: (f64, f64)| [at.0 as f32, at.1 as f32];
    let first = point(line[0]);
    for pair in line[1..].windows(2) {
        mesh.push(vertex(first, color));
        mesh.push(vertex(point(pair[0]), color));
        mesh.push(vertex(point(pair[1]), color));
    }
}

/// Adds the two triangles of one length of line.
fn add_length(mesh: &mut Vec<Vertex>, from: [f32; 2], to: [f32; 2], half: f32, color: [f32; 4]) {
    let step = [to[0] - from[0], to[1] - from[1]];
    let length = step[0].hypot(step[1]);
    if length <= f32::EPSILON {
        return;
    }
    let side = [-step[1] / length * half, step[0] / length * half];
    let corners = [
        [from[0] + side[0], from[1] + side[1]],
        [to[0] + side[0], to[1] + side[1]],
        [to[0] - side[0], to[1] - side[1]],
        [from[0] - side[0], from[1] - side[1]],
    ];
    for place in [0, 1, 2, 0, 2, 3] {
        mesh.push(vertex(corners[place], color));
    }
}

/// Adds the fan of triangles that fills a round end or a round bend.
fn add_disc(mesh: &mut Vec<Vertex>, at: [f32; 2], radius: f32, color: [f32; 4]) {
    let round = |step: usize| {
        let angle = std::f32::consts::TAU * step as f32 / ROUND as f32;
        [at[0] + radius * angle.cos(), at[1] + radius * angle.sin()]
    };
    for step in 0..ROUND {
        mesh.push(vertex(at, color));
        mesh.push(vertex(round(step), color));
        mesh.push(vertex(round(step + 1), color));
    }
}

/// One vertex of the mesh: where it sits, and what color it carries.
fn vertex(at: [f32; 2], color: [f32; 4]) -> Vertex {
    [at[0], at[1], color[0], color[1], color[2], color[3]]
}

/// The color of a stroke in the linear light the surface blends in.
///
/// The surface is sRGB, so a color written as a hex value has to go
/// through the transfer function to come out as that value on the screen.
/// The alpha is a weight and not a light, so it goes through as it is.
fn color_of(color: [u8; 4]) -> [f32; 4] {
    [
        crate::color::linear_from_srgb(color[0]) as f32,
        crate::color::linear_from_srgb(color[1]) as f32,
        crate::color::linear_from_srgb(color[2]) as f32,
        f32::from(color[3]) / 255.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::{ROUND, add_stroke};
    use crate::scene::Shown;
    use crate::stroke::{Ink, Stroke};

    fn stroke(points: &[(f64, f64)], width: f64) -> Stroke {
        Stroke {
            id: 1,
            shown: Shown::default(),
            ink: Ink::Pen,
            points: points.to_vec(),
            color: [255, 0, 0, 255],
            width,
            span: 0.0,
            rule: crate::stroke::Rule::default(),
        }
    }

    #[test]
    fn an_effect_fills_its_shape_and_keeps_its_outline() {
        let mut plain = Vec::new();
        let mut filled = Vec::new();
        let mut mark = stroke(&[(0.0, 0.0), (2.0, 0.0)], 0.1);
        add_stroke(&mut plain, &mark);
        mark.ink = Ink::Burst;
        add_stroke(&mut filled, &mark);
        assert!(
            filled.len() > plain.len(),
            "the fill adds nothing: {} against {}",
            filled.len(),
            plain.len()
        );
        // The fill is fainter than the line that goes around it.
        let faintest = filled.iter().map(|v| v[5]).fold(1.0_f32, f32::min);
        assert!(faintest < 1.0, "nothing in the mesh is faint");
    }

    #[test]
    fn a_length_of_line_takes_two_triangles_and_a_disc_at_each_end() {
        let mut mesh = Vec::new();
        add_stroke(&mut mesh, &stroke(&[(0.0, 0.0), (1.0, 0.0)], 0.2));
        // Six vertices for the length, then a fan at each of the two ends.
        assert_eq!(mesh.len(), 6 + 2 * ROUND * 3);
    }

    #[test]
    fn a_stroke_of_no_width_draws_nothing() {
        let mut mesh = Vec::new();
        add_stroke(&mut mesh, &stroke(&[(0.0, 0.0), (1.0, 0.0)], 0.0));
        assert!(mesh.is_empty());
    }

    #[test]
    fn a_stroke_of_one_point_still_draws_a_dot() {
        let mut mesh = Vec::new();
        add_stroke(&mut mesh, &stroke(&[(2.0, 3.0)], 0.2));
        assert_eq!(mesh.len(), ROUND * 3);
        // The fan stands where the point was.
        assert!(mesh.iter().all(|v| (v[0] - 2.0).abs() <= 0.11));
    }

    #[test]
    fn a_length_of_line_stands_half_a_width_to_each_side() {
        let mut mesh = Vec::new();
        add_stroke(&mut mesh, &stroke(&[(0.0, 0.0), (4.0, 0.0)], 1.0));
        let sides: Vec<f32> = mesh[..6].iter().map(|v| v[1]).collect();
        assert!(sides.iter().any(|y| (*y - 0.5).abs() < 1e-6), "{sides:?}");
        assert!(sides.iter().any(|y| (*y + 0.5).abs() < 1e-6), "{sides:?}");
    }

    #[test]
    fn a_point_that_repeats_adds_no_length() {
        let mut mesh = Vec::new();
        add_stroke(&mut mesh, &stroke(&[(1.0, 1.0), (1.0, 1.0)], 0.2));
        // Two discs and no quad between them.
        assert_eq!(mesh.len(), 2 * ROUND * 3);
    }
}
