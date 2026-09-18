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
use crate::grid::Cells;
use crate::stroke::Stroke;
use crate::theme;
use crate::ui::{Paint, measure_overlay, render_pane};

/// How tall a label draws on the TV, in real inches.
///
/// A quarter of an inch is about 18 points on a 1080p TV of 48 inches,
/// which reads across a table. The size comes off the TV, not off the
/// camera, so the players read the same label whatever the TV box holds
/// and a drag of the box does not resize the text under their eyes.
const LABEL_INCHES: f64 = 0.25;

/// How long the ruler of the calibration overlay is, in inches. Issue #7.
///
/// Six inches is the span of a school rule and of half a foot rule, so
/// most tables have something to hold against it. An error of one part in
/// fifty shows as a whole eighth of an inch over this length.
const RULER_INCHES: f32 = 6.0;

/// How tall the bar of the ruler draws, in inches.
const RULER_HEIGHT_INCHES: f32 = 0.5;

/// The gap between the bar of the ruler and its label, in inches.
const RULER_LABEL_GAP: f32 = 0.25;

/// The narrowest cell the calibration grid draws, in TV pixels.
///
/// A window far smaller than a TV puts these lines a few pixels apart,
/// and a grid that dense is a gray wash that hides the map under it. The
/// TV window falls back to 960 pixels with no display of its own, which
/// gives 20 pixels to the inch and still draws.
const MIN_CHECK_INCH: f32 = 8.0;

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
        check: bool,
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
    /// it to ask for a larger interface. A label is sized in real inches
    /// of the TV instead, so it reads across the table at any zoom.
    fn paint(
        &mut self,
        strokes: &[&Stroke],
        camera: &Camera,
        viewport: (u32, u32),
        cells: Cells,
        mode: theme::Mode,
        check: bool,
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
        // The size follows the TV, not the box on it. A size taken from
        // the camera makes a new `FontId` for every frame of a box drag,
        // and egui rasterizes each one into an atlas that never forgets.
        // A whole number of points holds that down to the few sizes a
        // change of the TV resolution can ask for.
        let size = (LABEL_INCHES * crate::tvbox::pixels_per_inch(viewport)).round() as f32;
        let output = self.ctx.run_ui(input, |ui| {
            let painter = ui.ctx().layer_painter(egui::LayerId::background());
            measure_overlay(
                &painter, strokes, camera, viewport, 1.0, cells, tokens, size,
            );
            if check {
                check_overlay(&painter, screen, viewport, tokens, size);
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

/// Where the bar of the ruler sits on a TV of `inch` pixels to the inch.
///
/// The bar stands in the middle of the TV, clear of the corner the grid is
/// measured from, where a hand can hold a real rule flat against it. Its
/// left edge falls on a line of the grid, so those lines cross it at every
/// inch and the bar needs no marks of its own.
fn ruler_bar(screen: egui::Rect, inch: f32) -> egui::Rect {
    let across = (screen.width() / inch).floor();
    let first = ((across - RULER_INCHES) / 2.0).floor().max(0.0);
    egui::Rect::from_min_size(
        egui::pos2(
            first.mul_add(inch, screen.left()),
            screen.center().y - RULER_HEIGHT_INCHES * inch / 2.0,
        ),
        egui::vec2(RULER_INCHES * inch, RULER_HEIGHT_INCHES * inch),
    )
}

/// Draws the 1 inch grid and the 6 inch ruler of issue #7.
///
/// The DM lays a real ruler on the TV and compares it with this one. So
/// every length here comes from [`crate::tvbox::pixels_per_inch`], which
/// measures the screen itself. The TV box and its zoom take no part: a
/// check that moved with the box would say nothing about the TV.
///
/// The cell of the game grid takes no part either. That cell may be a hex,
/// or an inch and a half wide, and this one is an inch because an inch is
/// what the DM holds against it.
fn check_overlay(
    painter: &egui::Painter,
    screen: egui::Rect,
    viewport: (u32, u32),
    tokens: theme::Tokens,
    size: f32,
) {
    let inch = crate::tvbox::pixels_per_inch(viewport) as f32;
    if inch < MIN_CHECK_INCH {
        return;
    }
    let line = egui::Stroke::new(1.0, tokens.ink);
    // The grid starts at the top left corner of the TV, so the DM can lay
    // a ruler along either edge and read the first inch off the corner.
    //
    // Each line is one multiplication from that corner, and not a step
    // added to the line before it. A ruler is what this draws, so the
    // fiftieth inch must stand where the arithmetic says and not a little
    // past it.
    for step in 0..=(screen.width() / inch) as u32 {
        painter.vline(
            (step as f32).mul_add(inch, screen.left()),
            screen.y_range(),
            line,
        );
    }
    for step in 0..=(screen.height() / inch) as u32 {
        painter.hline(
            screen.x_range(),
            (step as f32).mul_add(inch, screen.top()),
            line,
        );
    }
    let bar = ruler_bar(screen, inch);
    painter.rect(
        bar,
        0,
        tokens.surface,
        egui::Stroke::new(2.0, tokens.accent),
        egui::StrokeKind::Inside,
    );
    // The label stands beside the bar, not in it. Half an inch of bar
    // leaves no room for a quarter inch of text and its space.
    painter.text(
        egui::pos2(bar.right() + RULER_LABEL_GAP * inch, bar.center().y),
        egui::Align2::LEFT_CENTER,
        crate::text::overlay_ruler(),
        theme::font(size, true),
        tokens.accent,
    );
}

#[cfg(test)]
mod tests {
    use super::{RULER_INCHES, ruler_bar};

    /// The ruler carries no tick marks. The lines of the grid are its
    /// ticks, so the bar must start on one of them and end on one.
    #[test]
    fn the_grid_lines_fall_on_the_inches_of_the_ruler() {
        // A 1920 pixel TV of 48 inches gives 40 pixels to the inch.
        let inch = crate::tvbox::pixels_per_inch((1920, 1080)) as f32;
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0));
        let bar = ruler_bar(screen, inch);
        let from_edge = (bar.left() - screen.left()) / inch;
        assert!(
            (from_edge - from_edge.round()).abs() < 1e-3,
            "the bar starts {from_edge} inches from the edge, not a whole number"
        );
        assert!((bar.width() / inch - RULER_INCHES).abs() < 1e-3);
        // A ruler off the screen measures nothing.
        assert!(screen.contains_rect(bar));
    }
}
