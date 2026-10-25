//! What the TV window draws over the maps: the labels of a measure, and
//! the 1 inch grid and the 6 inch ruler that check the size of the TV.
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
use crate::grid::Cells;
use crate::stroke::Stroke;
use crate::theme;
use crate::ui::{Paint, measure_overlay, render_pane};

/// How tall a label draws on the TV, in inches.
///
/// A quarter of an inch is about 18 points at true size, which reads
/// across a table. The label grows with the zoom, as the strokes do, so
/// the players see it the same size whatever the TV box holds.
const LABEL_INCHES: f64 = 0.25;

/// How long the ruler of the size check is, in inches. Issue #7.
const RULER_INCHES: u32 = 6;

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

    /// Draws the TV: the maps, the canvas, then the overlay.
    ///
    /// `check` is the pixels in one inch of the TV when the DM asked for
    /// the size check, and `None` when they did not. Issue #7.
    ///
    /// Returns `false` when the surface gave no frame and nothing was shown.
    ///
    /// # Errors
    ///
    /// Returns an error when the surface fails validation.
    #[expect(
        clippy::too_many_arguments,
        reason = "one frame of the TV: what it draws, where it draws it, and the two passes"
    )]
    pub fn render(
        &mut self,
        gpu: &Gpu,
        pane: &mut Pane,
        strokes: &[&Stroke],
        camera: &Camera,
        cells: Cells,
        mode: theme::Mode,
        canvas: egui::Color32,
        check: Option<f64>,
        draw_maps: impl FnOnce(&mut wgpu::RenderPass<'static>),
        draw_canvas: impl FnOnce(&mut wgpu::RenderPass<'static>),
    ) -> Result<bool> {
        let viewport = (pane.config.width, pane.config.height);
        let paint = self.paint(strokes, camera, viewport, cells, mode, check);
        render_pane(
            gpu,
            pane,
            &mut self.renderer,
            paint,
            canvas,
            draw_maps,
            draw_canvas,
        )
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
        cells: Cells,
        mode: theme::Mode,
        check: Option<f64>,
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
            measure_overlay(
                &painter, strokes, camera, viewport, 1.0, cells, tokens, size,
            );
            if let Some(inch) = check {
                size_check(&painter, screen, inch as f32, tokens);
            }
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

/// The 1 inch grid and the 6 inch ruler of the size check. Issue #7.
///
/// Both stand in the pixels of the TV, `inch` pixels to the inch, and not
/// on the canvas. A real ruler on the glass then reads the same lengths,
/// whatever the TV box shows.
fn size_check(painter: &egui::Painter, screen: egui::Rect, inch: f32, tokens: theme::Tokens) {
    if inch < 2.0 {
        return;
    }
    let dark = egui::Stroke::new(3.0, egui::Color32::from_black_alpha(160));
    // The accent sets these lines apart from the grid of the canvas.
    let light = egui::Stroke::new(1.0, tokens.accent);
    // The lines start at the middle of the screen, so the ruler sits on
    // the grid.
    let middle = screen.center();
    let first = |from: f32, to: f32| from - ((from - to) / inch).floor() * inch;
    for stroke in [dark, light] {
        let mut x = first(middle.x, screen.left());
        while x <= screen.right() {
            painter.vline(x, screen.y_range(), stroke);
            x += inch;
        }
        let mut y = first(middle.y, screen.top());
        while y <= screen.bottom() {
            painter.hline(screen.x_range(), y, stroke);
            y += inch;
        }
    }
    let length = RULER_INCHES as f32 * inch;
    let left = middle.x - length / 2.0;
    let bar = egui::Rect::from_min_max(
        egui::pos2(left - inch * 0.25, middle.y),
        egui::pos2(left + length + inch * 0.25, middle.y + inch),
    );
    painter.rect_filled(bar, 0.0, tokens.raised);
    painter.rect_stroke(
        bar,
        0.0,
        egui::Stroke::new(2.0, tokens.ink),
        egui::StrokeKind::Inside,
    );
    let ticks = egui::Stroke::new(2.0, tokens.ink);
    let font = egui::FontId::proportional(inch * 0.3);
    for half in 0..=RULER_INCHES * 2 {
        let x = left + half as f32 * inch / 2.0;
        let whole = half % 2 == 0;
        let reach = if whole { 0.45 } else { 0.25 };
        painter.vline(x, middle.y..=middle.y + inch * reach, ticks);
        if whole {
            painter.text(
                egui::pos2(x, middle.y + inch * 0.5),
                egui::Align2::CENTER_TOP,
                (half / 2).to_string(),
                font.clone(),
                tokens.ink,
            );
        }
    }
}
