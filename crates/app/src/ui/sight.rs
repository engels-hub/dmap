//! The marks of the walls, the doors and the lights on the DM screen.
//!
//! An import brings these with its map. The light pass of PLAN.md 6 uses
//! them later. Until then the DM sees where they stand, as a line for
//! each wall and a ring for each light. The TV draws no egui, so the
//! players never see the marks. PLAN.md 5.3, the overlay pass.

// Rust guideline compliant 2026-02-21

use crate::icon;
use crate::icons::Icon;
use crate::scene::Node;
use crate::sight::{Kind, Light, Walls};
use crate::theme::Tokens;

use super::{DASH, Frame, View};

/// How thick a wall draws, in points.
const WALL_WIDTH: f32 = 2.5;

/// How much wider the light rim under a wall is, in points, so a dark
/// wall still shows over a dark map.
const WALL_RIM: f32 = 2.0;

/// How thick an open door draws, in points.
const OPEN_WIDTH: f32 = 1.5;

/// How strong an open door draws against a closed one.
const OPEN_STRENGTH: f32 = 0.5;

/// How large the glyph in the middle of a light draws, in points.
const LIGHT_GLYPH: f32 = 16.0;

/// How thick the ring at the bright reach of a light draws, in points.
const BRIGHT_RING: f32 = 1.5;

/// How thick the ring at the dim reach of a light draws, in points.
const DIM_RING: f32 = 1.0;

/// How strong the dim ring draws against the bright one.
const DIM_STRENGTH: f32 = 0.6;

/// Fills `slot` with the marks of every wall set and light the DM sees.
///
/// The slot sits under the TV box outline and the tools, so a selection
/// handle stays on top of a wall.
pub(super) fn mark_sight(
    ui: &egui::Ui,
    frame: &Frame<'_>,
    canvas: egui::Rect,
    viewport: (u32, u32),
    slot: egui::layers::ShapeIdx,
    tokens: Tokens,
) {
    let view = View::new(ui, *frame.camera, viewport);
    let painter = ui.painter_at(canvas);
    let mut shapes = Vec::new();
    for node in crate::scene::sight_order(frame.scene, crate::scene::Audience::Dm) {
        match node {
            Node::Walls(walls) => wall_shapes(&mut shapes, walls, &view, tokens),
            Node::Light(light) => light_shapes(&mut shapes, light, &view, tokens),
            Node::Darkness(area) => {
                // A dashed outline in `ink` on a rim of `surface`, so it
                // reads over a dark map and a pale one. Issue #20.
                let mut ring: Vec<egui::Pos2> =
                    area.corners.iter().map(|&at| view.to_screen(at)).collect();
                ring.push(ring[0]);
                let rim = egui::Stroke::new(WALL_RIM, tokens.surface);
                let line = egui::Stroke::new(1.0, tokens.ink);
                shapes.push(egui::Shape::line(ring.clone(), rim));
                shapes.extend(egui::Shape::dashed_line(&ring, line, DASH, DASH));
            }
            _ => {}
        }
    }
    painter.set(slot, egui::Shape::Vec(shapes));
    // A glyph paints on its own, over the rings.
    for node in crate::scene::sight_order(frame.scene, crate::scene::Audience::Dm) {
        if let Node::Light(light) = node {
            let at = view.to_screen(light.center);
            if canvas.contains(at) {
                icon::paint(&painter, Icon::Sun, at, LIGHT_GLYPH, color_of(light));
            }
        }
    }
}

/// A wall draws in `ink` on a rim of `surface`. A door draws in `accent`,
/// and a secret door draws dashed, so the DM tells the three apart. An
/// open door draws thin and faint, as a gap the light goes through.
fn wall_shapes(shapes: &mut Vec<egui::Shape>, walls: &Walls, view: &View, tokens: Tokens) {
    let rim = egui::Stroke::new(WALL_WIDTH + WALL_RIM, tokens.surface);
    for wall in &walls.segments {
        let ends = [view.to_screen(wall.a), view.to_screen(wall.b)];
        if wall.is_open() {
            let open = egui::Stroke::new(OPEN_WIDTH, tokens.accent.gamma_multiply(OPEN_STRENGTH));
            shapes.push(egui::Shape::line_segment(ends, open));
            continue;
        }
        let color = if wall.is_door() {
            tokens.accent
        } else {
            tokens.ink
        };
        let line = egui::Stroke::new(WALL_WIDTH, color);
        shapes.push(egui::Shape::line_segment(ends, rim));
        match wall.kind {
            Kind::Secret { .. } => {
                shapes.extend(egui::Shape::dashed_line(&ends, line, DASH, DASH));
            }
            Kind::Wall | Kind::Door { .. } => shapes.push(egui::Shape::line_segment(ends, line)),
        }
    }
}

/// A light draws a ring at its bright reach and a thin ring at its dim
/// reach, in its own color. A cone adds the two edges it opens between.
fn light_shapes(shapes: &mut Vec<egui::Shape>, light: &Light, view: &View, tokens: Tokens) {
    let center = view.to_screen(light.center);
    let color = color_of(light);
    let pixels_per_inch = (view.camera.pixels_per_inch / view.ppp) as f32;
    let bright = egui::Stroke::new(BRIGHT_RING, color);
    let dim = egui::Stroke::new(DIM_RING, color.gamma_multiply(DIM_STRENGTH));
    // A disc of the surface under the glyph, so a pale light reads over a
    // pale map.
    shapes.push(egui::Shape::circle_filled(
        center,
        LIGHT_GLYPH * 0.75,
        tokens.surface,
    ));
    if light.bright > 0.0 {
        shapes.push(egui::Shape::circle_stroke(
            center,
            light.bright as f32 * pixels_per_inch,
            bright,
        ));
    }
    if light.dim > light.bright {
        shapes.push(egui::Shape::circle_stroke(
            center,
            light.dim as f32 * pixels_per_inch,
            dim,
        ));
    }
    if light.angle < std::f64::consts::TAU {
        let reach = light.reach() as f32 * pixels_per_inch;
        for side in [-0.5, 0.5] {
            // Zero points straight down, and the angle runs clockwise on
            // the screen, as Foundry VTT measures it.
            let heading = (light.rotation + side * light.angle) as f32;
            let edge = center + egui::vec2(-heading.sin(), heading.cos()) * reach;
            shapes.push(egui::Shape::line_segment([center, edge], bright));
        }
    }
}

/// The color of a light, solid.
fn color_of(light: &Light) -> egui::Color32 {
    let [r, g, b] = light.color;
    egui::Color32::from_rgb(r, g, b)
}
