//! Draws the Lucide glyphs of DESIGN.md 4.

// Rust guideline compliant 2026-02-21

use egui::{Color32, Painter, Pos2, Shape, Stroke, Vec2};

use crate::icons::Icon;

/// The grid the Lucide set draws on. DESIGN.md 4.
const GRID: f32 = 24.0;

/// The stroke of a glyph on that grid, in grid units. DESIGN.md 4.
const STROKE: f32 = 2.0;

/// Draws `icon` at `size` points, centered on `center`.
pub fn paint(painter: &Painter, icon: Icon, center: Pos2, size: f32, color: Color32) {
    let scale = size / GRID;
    let origin = center - Vec2::splat(size / 2.0);
    let stroke = Stroke::new(STROKE * scale, color);
    for run in icon.strokes() {
        let mut points: Vec<Pos2> = run
            .points
            .iter()
            .map(|&(x, y)| origin + Vec2::new(x * scale, y * scale))
            .collect();
        if run.closed {
            points.push(points[0]);
        }
        painter.add(Shape::line(points, stroke));
    }
}

#[cfg(test)]
mod tests {
    use crate::icons::Icon;

    #[test]
    fn every_glyph_holds_a_run() {
        for icon in [
            Icon::Check,
            Icon::ChevronDown,
            Icon::MousePointer,
            Icon::Monitor,
            Icon::SlidersHorizontal,
        ] {
            let strokes = icon.strokes();
            assert!(!strokes.is_empty(), "{icon:?} draws nothing");
            for run in strokes {
                assert!(run.points.len() > 1, "{icon:?} holds a run of one point");
            }
        }
    }

    #[test]
    fn every_point_sits_in_the_grid() {
        // The generator writes the 24 x 24 grid of the Lucide set. A point
        // outside it would say the parser lost its place.
        for run in Icon::MousePointer.strokes() {
            for &(x, y) in run.points {
                assert!((-1.0..=25.0).contains(&x), "x {x} is off the grid");
                assert!((-1.0..=25.0).contains(&y), "y {y} is off the grid");
            }
        }
    }
}
