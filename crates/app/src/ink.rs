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
//!
//! A stroke that reveals draws no ink on the TV. The light pass cuts its
//! triangles out of the darkness, and the DM screen shows a thin outline
//! of a closed one. Issue #65.

// Rust guideline compliant 2026-02-21

use egui_wgpu::wgpu;

use crate::camera::Camera;
use crate::stroke::{Mix, Stroke};

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

/// How wide the outline of a stroke that reveals draws, in screen pixels.
const OUTLINE_PX: f64 = 1.5;

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
    /// yet, and it draws over the rest. A closed shape that reveals draws
    /// as a thin outline, so the TV leaves those strokes out.
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
        let outline = (OUTLINE_PX / camera.pixels_per_inch / 2.0) as f32;
        let mut mesh: Vec<Vertex> = Vec::new();
        for stroke in strokes {
            match stroke.mix {
                Mix::Blend => add_stroke(&mut mesh, stroke),
                // A closed shape shows its edge. An open stroke shows
                // nothing: the hatch of the fog already rims what it opens.
                Mix::Reveal if stroke.ink.closes() => add_line(
                    &mut mesh,
                    &stroke.polyline(),
                    outline,
                    color_of(stroke.color),
                ),
                Mix::Reveal => {}
            }
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
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_pipeline(&self.pipeline);
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

/// How far apart the eyes of an open reveal stand, as a share of the
/// radius of the brush. The edge of the band then dips by 3 % of the
/// radius between two eyes.
const EYE_STEP: f64 = 0.5;

/// The most eyes one open stroke takes. A longer stroke spaces them out.
const MOST_EYES: usize = 256;

/// One eye of a reveal: the point it sees from, and how far it sees, in
/// inches. The light pass treats it as a lamp the walls block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Eye {
    pub center: (f64, f64),
    pub reach: f64,
}

/// What the strokes that reveal open, before the walls cut it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reveal {
    /// The eyes, each one a point the walls hide parts from.
    pub eyes: Vec<Eye>,
    /// The triangles, three points in a row, in world inches. Each point
    /// names the eye that opens its triangle.
    pub mesh: Vec<([f32; 2], u32)>,
}

/// What the strokes that reveal open. Issue #65.
///
/// An open stroke is a brush: a row of discs along its line, each one as
/// wide as the stroke and each one its own eye, so the walls stop the
/// brush as they stop sight. A shape that closes reveals its inside and
/// sees from one point: the middle of a box or an ellipse, the middle of
/// a burst, and the point a cone or a beam starts from.
pub fn reveal_mesh(strokes: &[&Stroke]) -> Reveal {
    let mut reveal = Reveal::default();
    for stroke in strokes.iter().filter(|stroke| stroke.mix == Mix::Reveal) {
        let line = stroke.polyline();
        let half = (stroke.width / 2.0).max(0.0);
        let mut mesh = Vec::new();
        let mut eye = |center: (f64, f64), reach: f64, mesh: &mut Vec<Vertex>| {
            let index = reveal.eyes.len() as u32;
            reveal.eyes.push(Eye { center, reach });
            reveal
                .mesh
                .extend(mesh.drain(..).map(|v| ([v[0], v[1]], index)));
        };
        if stroke.ink.closes() {
            let (Some(first), Some(last)) = (stroke.points.first(), stroke.points.last()) else {
                continue;
            };
            let center = if stroke.ink.filled() {
                *first
            } else {
                (
                    f64::midpoint(first.0, last.0),
                    f64::midpoint(first.1, last.1),
                )
            };
            let reach = line
                .iter()
                .map(|at| (at.0 - center.0).hypot(at.1 - center.1))
                .fold(0.0, f64::max)
                + half;
            add_fill(&mut mesh, &line, [1.0; 4]);
            add_line(&mut mesh, &line, half as f32, [1.0; 4]);
            eye(center, reach, &mut mesh);
            continue;
        }
        if half <= 0.0 {
            continue;
        }
        for center in along(&line, half * EYE_STEP) {
            add_disc(
                &mut mesh,
                [center.0 as f32, center.1 as f32],
                half as f32,
                [1.0; 4],
            );
            eye(center, half, &mut mesh);
        }
    }
    reveal
}

/// Points along `line` no more than `step` apart, from its first point to
/// its last, and no more than [`MOST_EYES`] of them.
fn along(line: &[(f64, f64)], step: f64) -> Vec<(f64, f64)> {
    let length: f64 = line
        .windows(2)
        .map(|pair| (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1))
        .sum();
    let step = step.max(length / (MOST_EYES - 1) as f64);
    let mut points: Vec<(f64, f64)> = line.first().copied().into_iter().collect();
    // How far along the current length the next point stands.
    let mut next = step;
    for pair in line.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let span = (to.0 - from.0).hypot(to.1 - from.1);
        while next <= span && step > 0.0 {
            let share = next / span;
            points.push((
                from.0 + (to.0 - from.0) * share,
                from.1 + (to.1 - from.1) * share,
            ));
            next += step;
        }
        next -= span;
    }
    if let (Some(last), Some(end)) = (points.last(), line.last())
        && last != end
    {
        points.push(*end);
    }
    points
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
    add_line(mesh, &line, half, color);
}

/// Adds a line `half` wide to each side through `line`, with round ends
/// and round bends.
fn add_line(mesh: &mut Vec<Vertex>, line: &[(f64, f64)], half: f32, color: [f32; 4]) {
    if half <= 0.0 || line.is_empty() {
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
///
/// A wide disc takes more sides than [`ROUND`], so the broad brush of a
/// stroke that reveals stays round: one side for each sixteenth of an inch
/// of radius.
fn add_disc(mesh: &mut Vec<Vertex>, at: [f32; 2], radius: f32, color: [f32; 4]) {
    let sides = ROUND.max((radius * 16.0).ceil() as usize);
    let round = |step: usize| {
        let angle = std::f32::consts::TAU * step as f32 / sides as f32;
        [at[0] + radius * angle.cos(), at[1] + radius * angle.sin()]
    };
    for step in 0..sides {
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
    use super::{ROUND, add_stroke, reveal_mesh};
    use crate::scene::Shown;
    use crate::stroke::Mix;
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
            angle: 0.0,
            rule: crate::stroke::Rule::default(),
            mix: crate::stroke::Mix::default(),
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

    #[test]
    fn an_open_reveal_is_a_row_of_eyes_as_wide_as_the_stroke() {
        let mut line = stroke(&[(0.0, 0.0), (4.0, 0.0)], 2.0);
        line.mix = Mix::Reveal;
        let reveal = reveal_mesh(&[&line]);
        // A radius of 1 puts an eye every half inch: nine along 4 inches.
        assert_eq!(reveal.eyes.len(), 9);
        assert!(reveal.eyes.iter().all(|eye| (eye.reach - 1.0).abs() < 1e-9));
        assert_eq!(reveal.eyes[8].center, (4.0, 0.0));
        // Each eye opens its own disc and no other.
        let eyes = reveal.eyes.len() as u32;
        assert!(reveal.mesh.iter().all(|(_, eye)| *eye < eyes));
        assert!(reveal.mesh.iter().all(|(at, eye)| {
            let center = reveal.eyes[*eye as usize].center;
            (f64::from(at[0]) - center.0).hypot(f64::from(at[1]) - center.1) <= 1.0 + 1e-5
        }));
    }

    #[test]
    fn a_closed_reveal_sees_from_its_middle_and_reaches_its_far_corner() {
        let mut room = stroke(&[(0.0, 0.0), (4.0, 2.0)], 0.2);
        room.mix = Mix::Reveal;
        room.ink = Ink::Rect;
        let reveal = reveal_mesh(&[&room]);
        assert_eq!(reveal.eyes.len(), 1);
        assert_eq!(reveal.eyes[0].center, (2.0, 1.0));
        assert!((reveal.eyes[0].reach - (5.0_f64.sqrt() + 0.1)).abs() < 1e-9);
        // A burst sees from its middle, the first point.
        room.ink = Ink::Burst;
        assert_eq!(reveal_mesh(&[&room]).eyes[0].center, (0.0, 0.0));
    }

    #[test]
    fn a_long_reveal_keeps_to_the_most_eyes_and_a_painted_one_has_none() {
        let mut line = stroke(&[(0.0, 0.0), (1000.0, 0.0)], 0.5);
        line.mix = Mix::Reveal;
        assert!(reveal_mesh(&[&line]).eyes.len() <= super::MOST_EYES + 1);
        line.mix = Mix::Blend;
        assert_eq!(reveal_mesh(&[&line]), super::Reveal::default());
    }
}
