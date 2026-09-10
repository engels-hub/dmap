//! What the TV window draws over the maps: the labels of a measure.
//!
//! The TV pane runs an `egui` of its own, with no input and no panels. It
//! exists so the players read the numbers the DM reads: a ruler that
//! showed its length on one screen only would be a ruler for the DM.
//!
//! The context takes no events. Each TV frame hands it the size of the
//! surface, it paints, and [`crate::ui::render_pane`] draws the result
//! over the canvas, exactly as the DM window draws its panels.

// Rust guideline compliant 2026-02-21

use anyhow::Result;
use egui_wgpu::wgpu;

use crate::camera::Camera;
use crate::gpu::{Gpu, Pane};
use crate::stroke::Stroke;
use crate::theme;
use crate::ui::{Paint, measure_overlay, render_pane};

/// How tall a label draws on the TV, in inches.
///
/// A quarter of an inch is about 18 points at true size, which reads
/// across a table. The label grows with the zoom, as the strokes do, so
/// the players see it the same size whatever the TV box holds.
const LABEL_INCHES: f64 = 0.25;

/// The `egui` of the TV window, for the overlays it shows.
pub struct Overlay {
    ctx: egui::Context,
    renderer: egui_wgpu::Renderer,
    /// The theme the context carries, so a change installs once.
    ///
    /// The fonts come with it. A context that never had them panics the
    /// first time a label asks for the bold family.
    mode: theme::Mode,
}

impl std::fmt::Debug for Overlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Overlay").finish_non_exhaustive()
    }
}

impl Overlay {
    /// Builds the context and the renderer for the TV pane.
    pub fn new(gpu: &Gpu, pane: &Pane) -> Self {
        let ctx = egui::Context::default();
        let mode = theme::Mode::default();
        theme::install(&ctx, mode);
        let renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            pane.config.format,
            egui_wgpu::RendererOptions::default(),
        );
        Self {
            ctx,
            renderer,
            mode,
        }
    }

    /// Draws the TV: the canvas through `draw_canvas`, then the overlay.
    ///
    /// # Errors
    ///
    /// Returns an error when the surface has no frame to draw into.
    pub fn render(
        &mut self,
        gpu: &Gpu,
        pane: &mut Pane,
        strokes: &[&Stroke],
        camera: &Camera,
        mode: theme::Mode,
        draw_canvas: impl FnOnce(&mut wgpu::RenderPass<'static>),
    ) -> Result<()> {
        let viewport = (pane.config.width, pane.config.height);
        let paint = self.paint(strokes, camera, viewport, mode);
        let canvas = mode.tokens().canvas;
        render_pane(gpu, pane, &mut self.renderer, paint, canvas, draw_canvas)
    }

    /// Paints the overlay for one frame.
    ///
    /// The TV takes its own pixels as its points, because no DM sits at
    /// it to ask for a larger interface. A label is sized in inches
    /// instead, so it reads across the table at any zoom.
    fn paint(
        &mut self,
        strokes: &[&Stroke],
        camera: &Camera,
        viewport: (u32, u32),
        mode: theme::Mode,
    ) -> Paint {
        let screen = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(viewport.0 as f32, viewport.1 as f32),
        );
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        if self.mode != mode {
            theme::install(&self.ctx, mode);
            self.mode = mode;
        }
        let tokens = mode.tokens();
        let size = (LABEL_INCHES * camera.pixels_per_inch) as f32;
        let output = self.ctx.run_ui(input, |ui| {
            let painter = ui.ctx().layer_painter(egui::LayerId::background());
            measure_overlay(&painter, strokes, camera, viewport, 1.0, tokens, size);
        });
        let jobs = self.ctx.tessellate(output.shapes, 1.0);
        Paint {
            jobs,
            textures_delta: output.textures_delta,
            pixels_per_point: 1.0,
            repaint: false,
        }
    }
}
