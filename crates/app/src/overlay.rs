//! What the TV window draws over the maps: the labels of a measure, and
//! the ruler that checks the size of the screen.
//!
//! The TV pane runs an `egui` of its own, with no input and no panels. It
//! exists so the players read the numbers the DM reads. A measure that
//! showed its length on one screen only would be a measure for the DM.
//!
//! The ruler of issue #7 is a second thing entirely. It belongs to the DM
//! alone, it is shown while they check the size of the TV, and it says
//! nothing about the scene. See [`check_overlay`].
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

/// How many centimetres are in one inch.
const CM_PER_INCH: f32 = 2.54;

/// How tall the bar of the ruler draws, in inches.
///
/// One inch gives each scale half an inch: room for its marks and for the
/// numbers past them, on a bar a hand can hold a real rule against.
const RULER_HEIGHT_INCHES: f32 = 1.0;

/// The gap between the bar of the ruler and its unit label, in inches.
const RULER_LABEL_GAP: f32 = 0.2;

/// How far each bank of marks reaches, as a share of half the bar.
///
/// Longest first: the whole unit, then the half, then the finest bank.
/// The longest mark and the number under it share one half of the bar, so
/// the first of these plus the height of a number must fit in that half.
/// `a_mark_and_its_number_stay_on_their_own_half` holds them to it.
const TICK_REACH: [f32; 3] = [0.40, 0.26, 0.16];

/// How tall a number on the ruler draws, in inches of the canvas.
///
/// Smaller than a measure label, because a number here shares its half of
/// the bar with the mark it belongs to. A quarter inch of text left the
/// two scales reaching across the middle into each other.
const RULER_TEXT_INCHES: f64 = 0.16;

/// The shortest a tick bank may be spaced before it is dropped, in pixels.
///
/// A millimetre is 1.6 px on a 1080p TV of 48 inches, and a bank that
/// close is a grey smear, not a scale. The bank is left out and the
/// coarser one below it carries the reading. Two pixels is a line and a
/// gap, which is the least that reads as two marks.
const MIN_TICK_GAP: f32 = 2.0;

/// The share of the TV width the ruler may take.
///
/// The ruler grows with the TV box, and a box zoomed right in puts six
/// inches well past both edges of the screen. The span gives up whole
/// inches until it fits, and this leaves a margin either side so the ends
/// of it stay on the glass where a rule can reach them.
const RULER_SHARE: f32 = 0.8;

/// The smallest and largest the numbers on the ruler draw, in points.
///
/// A box zoomed far out would ask for text of two points, and one zoomed
/// far in for text taller than the bar it stands on.
const MIN_RULER_TEXT: f64 = 7.0;
const MAX_RULER_TEXT: f64 = 48.0;

/// The narrowest inch the ruler draws in, in TV pixels.
///
/// This measures the inch of the canvas, the one the camera draws with,
/// so the width of the TV box sets it and not the size of the screen. A
/// box wide enough leaves an inch of a few pixels, which carries no scale
/// at all, and the ruler goes until the DM brings the box back in.
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
                check_overlay(&painter, screen, camera, tokens);
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

/// One scale along an edge of the ruler: its unit and how it is marked.
///
/// `unit` is the length of one whole unit in inches, so an inch scale
/// carries 1.0 and a centimetre scale carries 1/2.54. `banks` holds how
/// many parts a unit is cut into, longest mark first: an inch reads 1,
/// then 2 for the half, then 8 for the eighth.
#[derive(Debug, Clone, Copy)]
struct Scale {
    unit: f32,
    banks: [u32; 3],
    label: &'static str,
}

/// How many whole inches of ruler fit across the TV.
///
/// [`RULER_INCHES`] is what the ruler wants. A TV box zoomed in makes an
/// inch wide enough that six of them run off both edges, so the span
/// gives up whole inches until it fits. Whole ones, because a ruler that
/// ends at four and a third inches invites a reading of its last mark
/// that is not there.
fn span_inches(screen: egui::Rect, inch: f32) -> f32 {
    (screen.width() * RULER_SHARE / inch)
        .floor()
        .min(RULER_INCHES)
}

/// How far apart two marks of `parts` stand, in pixels.
fn bank_gap(scale: Scale, parts: u32, inch: f32) -> f32 {
    scale.unit * inch / parts as f32
}

/// How many banks of `scale` a TV of `inch` pixels to the inch can draw.
///
/// The count slices `banks`, which runs coarse to fine. The gap only
/// shrinks along it, so the banks that fit are the first ones and this
/// stops at the first that does not.
///
/// A bank whose marks stand closer than [`MIN_TICK_GAP`] is dropped. A
/// millimetre is 1.6 px on a 1080p TV of 48 inches: drawn, it is a grey
/// smear that hides the half centimetre under it, so the coarser bank
/// carries the reading instead. The whole unit is always drawn, because a
/// scale without it is not a scale.
fn drawn_ranks(scale: Scale, inch: f32) -> usize {
    1 + scale.banks[1..]
        .iter()
        .take_while(|&&parts| bank_gap(scale, parts, inch) >= MIN_TICK_GAP)
        .count()
}

/// Draws the ruler of issue #7 across the middle of the TV.
///
/// The inch here is the inch of the canvas, the one the camera draws and
/// the one the grid is built on. So the ruler grows and shrinks with the
/// TV box, and it never disagrees with the cells beside it.
///
/// At 100 % zoom that canvas inch is a real inch on the glass, which is
/// what true size means, and the box snaps to it. There the DM holds a
/// real rule against this one and reads the error. The program cannot
/// know the real size of the TV, so what it draws is what it believes an
/// inch to be; issue #6 is where the DM corrects that belief.
///
/// This draws no grid. The map already carries one, and a second grid on
/// top of it read as two grids that disagree.
fn check_overlay(
    painter: &egui::Painter,
    screen: egui::Rect,
    camera: &Camera,
    tokens: theme::Tokens,
) {
    let inch = camera.pixels_per_inch as f32;
    if inch < MIN_CHECK_INCH {
        return;
    }
    let span = span_inches(screen, inch);
    if span < 1.0 {
        return;
    }
    // The numbers follow the ruler, and a whole number of points holds
    // the font atlas to the few sizes a drag of the box can ask for.
    let size = (RULER_TEXT_INCHES * camera.pixels_per_inch)
        .round()
        .clamp(MIN_RULER_TEXT, MAX_RULER_TEXT) as f32;
    let bar = egui::Rect::from_center_size(
        screen.center(),
        egui::vec2(span * inch, RULER_HEIGHT_INCHES * inch),
    );
    painter.rect(
        bar,
        0,
        tokens.surface,
        egui::Stroke::new(1.0, tokens.ink),
        egui::StrokeKind::Inside,
    );
    // Both scales start at the left edge of the bar, as the two scales of
    // a real rule share one zero.
    let inches = Scale {
        unit: 1.0,
        banks: [1, 2, 8],
        label: crate::text::overlay_ruler_inches(),
    };
    let metric = Scale {
        unit: 1.0 / CM_PER_INCH,
        banks: [1, 2, 10],
        label: crate::text::overlay_ruler_cm(),
    };
    edge(painter, bar, inch, span, inches, true, tokens, size);
    edge(painter, bar, inch, span, metric, false, tokens, size);
}

/// Draws one scale along one edge of the bar.
///
/// `top` puts the marks on the upper edge, where they hang down, and its
/// numbers below them. The lower edge is its mirror.
#[expect(
    clippy::too_many_arguments,
    reason = "one edge of the ruler: where it is, how long its unit is, which way it faces and how it draws"
)]
fn edge(
    painter: &egui::Painter,
    bar: egui::Rect,
    inch: f32,
    span: f32,
    scale: Scale,
    top: bool,
    tokens: theme::Tokens,
    size: f32,
) {
    let banks = &scale.banks[..drawn_ranks(scale, inch)];
    let base = if top { bar.top() } else { bar.bottom() };
    let half = bar.height() / 2.0;
    // The longest mark belongs to the whole unit, and each finer bank
    // takes less of the half so the eye reads the ranks apart.
    //
    // A mark and the number under it share one half of the bar. The
    // longest mark therefore stops well short of the middle: it and the
    // text below it have to fit above the midline, or the numbers of one
    // scale stand in the marks of the other.
    let reach = |rank: usize| half * TICK_REACH[rank];
    let units = (span / scale.unit) as u32;
    for (rank, &parts) in banks.iter().enumerate() {
        let marks = units * parts;
        let step = scale.unit * inch / parts as f32;
        let long = reach(rank);
        let color = if rank == 0 { tokens.ink } else { tokens.mute };
        for mark in 0..=marks {
            // A mark this bank shares with a longer one is already drawn.
            if rank > 0
                && banks[..rank]
                    .iter()
                    .any(|finer| mark % (parts / finer) == 0)
            {
                continue;
            }
            let x = (mark as f32).mul_add(step, bar.left());
            if x > bar.right() + 0.5 {
                break;
            }
            let end = if top { base + long } else { base - long };
            painter.vline(
                x,
                base.min(end)..=base.max(end),
                egui::Stroke::new(1.0, color),
            );
        }
    }
    numbers(
        painter,
        bar,
        inch,
        units,
        scale,
        top,
        reach(0),
        tokens,
        size,
    );
}

/// Draws the numbers of one scale, and the name of its unit.
///
/// A number is drawn only where the next one has room to stand clear of
/// it. Where it has not, the scale counts by twos, then by fives, so the
/// numbers thin out instead of running together.
#[expect(
    clippy::too_many_arguments,
    reason = "one rank of numbers: where it goes, what it counts, which way it faces and how it draws"
)]
fn numbers(
    painter: &egui::Painter,
    bar: egui::Rect,
    inch: f32,
    units: u32,
    scale: Scale,
    top: bool,
    reach: f32,
    tokens: theme::Tokens,
    size: f32,
) {
    let font = theme::font(size, false);
    let step = scale.unit * inch;
    // Two digits and a space beside them, which is the widest a number on
    // this ruler grows.
    let room = size * 2.0;
    let every = [1, 2, 5]
        .into_iter()
        .find(|&every| step * every as f32 >= room)
        .unwrap_or(5);
    let base = if top { bar.top() } else { bar.bottom() };
    let y = if top { base + reach } else { base - reach };
    let anchor = if top {
        egui::Align2::CENTER_TOP
    } else {
        egui::Align2::CENTER_BOTTOM
    };
    for mark in (every..=units).step_by(every as usize) {
        let x = (mark as f32).mul_add(step, bar.left());
        if x > bar.right() - step / 2.0 {
            break;
        }
        painter.text(
            egui::pos2(x, y),
            anchor,
            mark.to_string(),
            font.clone(),
            tokens.ink,
        );
    }
    // The unit stands past the end of the bar, where no mark reaches it,
    // and in the middle of its own half. Hung off the marks instead, the
    // two units met each other at the middle of the bar.
    let middle = if top {
        bar.top() + bar.height() / 4.0
    } else {
        bar.bottom() - bar.height() / 4.0
    };
    painter.text(
        egui::pos2(bar.right() + RULER_LABEL_GAP * inch, middle),
        egui::Align2::LEFT_CENTER,
        scale.label,
        font,
        tokens.ink,
    );
}

#[cfg(test)]
mod tests {
    use super::{
        CM_PER_INCH, RULER_HEIGHT_INCHES, RULER_INCHES, RULER_TEXT_INCHES, Scale, TICK_REACH,
        bank_gap, drawn_ranks,
    };

    /// How tall a line of text stands, as a share of the size asked for.
    ///
    /// `egui` puts the ascent and the descent of a row above and below the
    /// size given to it. This is the generous end of that.
    const TEXT_SHARE: f64 = 1.5;

    /// A 1080p TV of 48 inches gives 40 pixels to the inch, and a 4K one
    /// gives 80.
    const HD: f32 = 40.0;
    const UHD: f32 = 80.0;

    fn metric() -> Scale {
        Scale {
            unit: 1.0 / CM_PER_INCH,
            banks: [1, 2, 10],
            label: "cm",
        }
    }

    fn imperial() -> Scale {
        Scale {
            unit: 1.0,
            banks: [1, 2, 8],
            label: "in",
        }
    }

    /// Each scale owns half the bar. A mark and the number under it both
    /// stand in that half, so the two together must not cross the middle:
    /// where they did, the numbers of one scale sat in the marks of the
    /// other, and the two unit names met.
    #[test]
    fn a_mark_and_its_number_stay_on_their_own_half() {
        let half = f64::from(RULER_HEIGHT_INCHES) / 2.0;
        let mark = half * f64::from(TICK_REACH[0]);
        let text = RULER_TEXT_INCHES * TEXT_SHARE;
        assert!(
            mark + text <= half,
            "a mark of {mark} and a number of {text} outgrow the {half} they share"
        );
    }

    #[test]
    fn the_two_scales_measure_the_same_bar() {
        // Six inches is 15.24 cm, so the metric scale runs fifteen whole
        // centimetres and stops a little short of the end.
        let centimetres = RULER_INCHES / metric().unit;
        assert!((centimetres - 15.24).abs() < 1e-3, "{centimetres}");
        assert_eq!((RULER_INCHES / imperial().unit) as u32, 6);
    }

    #[test]
    fn a_millimetre_is_too_fine_for_a_1080p_tv() {
        // 1 mm is 1.6 px there, which reads as a smear and not as a mark,
        // so the half centimetre carries the scale instead.
        assert!(bank_gap(metric(), 10, HD) < 2.0);
        assert_eq!(&metric().banks[..drawn_ranks(metric(), HD)], [1, 2]);
        // 4K has 3.1 px for it, and the bank comes back.
        assert!(bank_gap(metric(), 10, UHD) > 2.0);
        assert_eq!(&metric().banks[..drawn_ranks(metric(), UHD)], [1, 2, 10]);
    }

    /// The ruler grows with the TV box. A box zoomed in makes an inch wide
    /// enough that six of them leave the screen, and the span gives up
    /// whole inches until what is left fits.
    #[test]
    fn the_span_gives_up_inches_until_the_ruler_fits() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0));
        // True size on a 1080p TV of 48 inches: the whole six inches fit.
        assert!((super::span_inches(screen, HD) - RULER_INCHES).abs() < 1e-6);
        // Four times in, an inch is 160 px and only nine would fit. Six
        // still do, so the ruler keeps its whole length.
        assert!((super::span_inches(screen, 160.0) - RULER_INCHES).abs() < 1e-6);
        // Far enough in that six inches run off the glass, and the span
        // comes down to whole inches that stay on it.
        let span = super::span_inches(screen, 400.0);
        assert!((span - 3.0).abs() < 1e-6, "{span}");
        assert!(span * 400.0 <= screen.width());
    }

    #[test]
    fn the_whole_unit_is_drawn_however_narrow_the_tv() {
        // Every finer bank may go, but a scale without its own unit is
        // not a scale.
        assert_eq!(drawn_ranks(metric(), 1.0), 1);
        assert_eq!(drawn_ranks(imperial(), 1.0), 1);
        // An eighth of an inch is 5 px on 1080p and stays.
        assert_eq!(&imperial().banks[..drawn_ranks(imperial(), HD)], [1, 2, 8]);
    }
}
