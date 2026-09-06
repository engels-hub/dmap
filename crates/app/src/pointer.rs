//! The pointer disc drawn on the TV window instead of the system pointer.

// Rust guideline compliant 2026-02-21

use egui_wgpu::wgpu;

/// Radius of the pointer disc in TV pixels.
///
/// About a quarter inch on a 4K 55 inch TV. Becomes a setting in inches once
/// the TV calibration exists.
pub const RADIUS: f32 = 24.0;

/// Draws a disc that inverts the colors under it.
///
/// A blend of `src * (1 - dst) + dst * (1 - src)` with a white disc gives
/// `1 - dst` inside, `dst` outside, and a mix on the anti-aliased edge.
const SHADER: &str = r#"
struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
};

@vertex
fn vs(@location(0) clip: vec2<f32>, @location(1) local: vec2<f32>) -> VsOut {
    var out: VsOut;
    out.position = vec4<f32>(clip, 0.0, 1.0);
    out.local = local;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let d = length(in.local);
    let edge = fwidth(d);
    let coverage = 1.0 - smoothstep(1.0 - edge, 1.0, d);
    return vec4<f32>(coverage, coverage, coverage, 1.0);
}
"#;

/// The GPU pipeline that draws the pointer disc.
pub struct PointerDisc {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
}

impl std::fmt::Debug for PointerDisc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PointerDisc").finish_non_exhaustive()
    }
}

impl PointerDisc {
    /// Builds the pipeline for surfaces of `format`.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pointer disc"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let invert = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::OneMinusDst,
                dst_factor: wgpu::BlendFactor::OneMinusSrc,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pointer disc"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<DiscVertex>() as u64,
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
                    blend: Some(invert),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pointer disc"),
            size: std::mem::size_of::<[DiscVertex; 6]>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, vertices }
    }

    /// Draws the disc around `center` (window pixels) into the open pass.
    pub fn draw(
        &self,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        center: (f32, f32),
        viewport: (u32, u32),
    ) {
        let quad = disc_quad(center, RADIUS, viewport);
        let bytes: Vec<u8> = quad
            .iter()
            .flatten()
            .flat_map(|value| value.to_ne_bytes())
            .collect();
        queue.write_buffer(&self.vertices, 0, &bytes);
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..6, 0..1);
    }
}

/// One vertex: clip x, clip y, then local x and y in `-1..=1` across the disc.
pub type DiscVertex = [f32; 4];

/// The two triangles that cover a disc of `radius` pixels around `center`.
///
/// `center` is in window pixels with the origin at the top left. The clip
/// coordinates put the origin in the middle with y up, as the GPU expects.
pub fn disc_quad(center: (f32, f32), radius: f32, viewport: (u32, u32)) -> [DiscVertex; 6] {
    let (width, height) = (viewport.0 as f32, viewport.1 as f32);
    let corner = |lx: f32, ly: f32| -> DiscVertex {
        let px = center.0 + lx * radius;
        let py = center.1 + ly * radius;
        [px / width * 2.0 - 1.0, 1.0 - py / height * 2.0, lx, ly]
    };
    [
        corner(-1.0, -1.0),
        corner(1.0, -1.0),
        corner(-1.0, 1.0),
        corner(-1.0, 1.0),
        corner(1.0, -1.0),
        corner(1.0, 1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::disc_quad;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn maps_the_top_left_pixel_to_the_top_left_clip_corner() {
        let quad = disc_quad((0.0, 0.0), 10.0, (100, 50));
        // The quad's top-left vertex is one radius up and left of the center.
        let top_left = quad.iter().find(|v| v[2] < 0.0 && v[3] < 0.0).unwrap();
        assert!(close(top_left[0], -1.0 - 0.2), "x {}", top_left[0]);
        assert!(close(top_left[1], 1.0 + 0.4), "y {}", top_left[1]);
    }

    #[test]
    fn centers_the_quad_on_the_pixel() {
        let quad = disc_quad((50.0, 25.0), 5.0, (100, 50));
        let (mut sx, mut sy) = (0.0, 0.0);
        for v in &quad {
            sx += v[0];
            sy += v[1];
        }
        assert!(close(sx / 6.0, 0.0));
        assert!(close(sy / 6.0, 0.0));
    }

    #[test]
    fn spans_two_triangles_with_unit_local_coordinates() {
        let quad = disc_quad((50.0, 25.0), 5.0, (100, 50));
        assert_eq!(quad.len(), 6);
        assert!(
            quad.iter()
                .all(|v| close(v[2].abs(), 1.0) && close(v[3].abs(), 1.0))
        );
        // All four corners are present.
        for corner in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            assert!(
                quad.iter()
                    .any(|v| close(v[2], corner.0) && close(v[3], corner.1))
            );
        }
    }
}
