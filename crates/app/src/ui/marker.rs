//! The marker that points at the TV box when the box is off the canvas.
//! DESIGN.md 5.5. Issue #44.

// Rust guideline compliant 2026-02-21

use crate::theme::Tokens;

use super::canvas::outline_tv_box;
use super::{Frame, Tool, View};

/// How wide the triangle stands across the line to the box, in points.
const MARKER_WIDTH: f32 = 28.0;

/// How far the triangle reaches along the line to the box, in points.
const MARKER_DEPTH: f32 = 20.0;

/// The room the marker leaves between itself and a panel, in points.
const CHROME_GAP: f32 = 4.0;

/// Where the marker stands, and the way it points.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Marker {
    /// The middle of the triangle.
    at: egui::Pos2,
    /// The way to the box center, one point long.
    toward: egui::Vec2,
}

impl Marker {
    /// The three corners of the triangle, the tip first.
    fn corners(self) -> [egui::Pos2; 3] {
        let tip = self.at + self.toward * (MARKER_DEPTH / 2.0);
        let back = self.at - self.toward * (MARKER_DEPTH / 2.0);
        let across = self.toward.rot90() * (MARKER_WIDTH / 2.0);
        [tip, back + across, back - across]
    }
}

/// How far the triangle reaches from its middle, whichever way it points.
fn reach() -> f32 {
    (MARKER_DEPTH / 2.0).hypot(MARKER_WIDTH / 2.0)
}

/// Where the marker goes for `tv_box` on `canvas`, all in screen points.
///
/// `None` while a part of the box is on the canvas. The marker sits where
/// the line from the canvas center to the box center crosses the canvas
/// edge, pulled in so the whole triangle stays on the canvas.
///
/// `chrome` holds the panels, the toolbar and the History button. A
/// marker under one of them steps back along the same line until it
/// stands clear, so the DM always sees it.
fn place(canvas: egui::Rect, tv_box: egui::Rect, chrome: &[egui::Rect]) -> Option<Marker> {
    if canvas.intersects(tv_box) {
        return None;
    }
    let inner = canvas.shrink(reach());
    if !inner.is_positive() {
        return None;
    }
    let from = canvas.center();
    let line = tv_box.center() - from;
    let room = |half: f32, step: f32| {
        if step == 0.0 {
            f32::INFINITY
        } else {
            half / step.abs()
        }
    };
    let mut scale = room(inner.width() / 2.0, line.x).min(room(inner.height() / 2.0, line.y));
    if !scale.is_finite() {
        return None;
    }
    // Each step leaves one panel behind. A panel over the canvas center
    // leaves the marker nowhere to go, and it stays on the edge.
    for _ in 0..=chrome.len() {
        let at = from + line * scale;
        let Some(keep) = chrome
            .iter()
            .map(|rect| rect.expand(reach() + CHROME_GAP))
            .find(|keep| keep.contains(at))
        else {
            break;
        };
        match entry(from, line, keep) {
            Some(enter) if enter > 0.0 => scale = enter,
            _ => break,
        }
    }
    let at = from + line * scale;
    Some(Marker {
        at,
        toward: (tv_box.center() - at).normalized(),
    })
}

/// How far along `line` from `from` the line enters `rect`, as a share of
/// `line`. `None` when it never does.
fn entry(from: egui::Pos2, line: egui::Vec2, rect: egui::Rect) -> Option<f32> {
    let axis = |start: f32, step: f32, low: f32, high: f32| {
        if step == 0.0 {
            (low..=high)
                .contains(&start)
                .then_some((f32::NEG_INFINITY, f32::INFINITY))
        } else {
            let (a, b) = ((low - start) / step, (high - start) / step);
            Some((a.min(b), a.max(b)))
        }
    };
    let (x_in, x_out) = axis(from.x, line.x, rect.min.x, rect.max.x)?;
    let (y_in, y_out) = axis(from.y, line.y, rect.min.y, rect.max.y)?;
    let (enter, leave) = (x_in.max(y_in), x_out.min(y_out));
    (enter <= leave).then_some(enter)
}

/// Where the panels, the toolbar and the History button stood in the last
/// frame. They all float over the canvas in the middle order.
fn chrome(ctx: &egui::Context) -> Vec<egui::Rect> {
    ctx.memory(|memory| {
        memory
            .areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|layer| layer.order == egui::Order::Middle)
            .filter_map(|layer| memory.area_rect(layer.id))
            .collect()
    })
}

/// Paints the marker when the TV box is off `canvas`.
///
/// The marker is paint only: it takes no click and no drag. The TV draws
/// no egui, so the players never see it.
///
/// `outline` is the view the DM works in, and the slot the outline of the
/// box goes into. The Table view draws its own outline, with the wash and
/// the handles. Issue #43.
pub(super) fn mark_tv_box(
    ui: &egui::Ui,
    frame: &Frame<'_>,
    canvas: egui::Rect,
    viewport: (u32, u32),
    outline: (Tool, egui::layers::ShapeIdx),
    tokens: Tokens,
) {
    if let (Tool::Select | Tool::Draw, slot) = outline {
        outline_tv_box(ui, frame, canvas, viewport, slot, tokens);
    }
    let view = View::new(ui, *frame.camera, viewport);
    let corners = frame.scene.tv_box.corners(frame.tv_viewport);
    let tv_box = egui::Rect::from_two_pos(view.to_screen(corners[0]), view.to_screen(corners[2]));
    if let Some(marker) = place(canvas, tv_box, &chrome(ui.ctx())) {
        ui.painter_at(canvas).add(egui::Shape::convex_polygon(
            marker.corners().to_vec(),
            tokens.accent,
            egui::Stroke::NONE,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANVAS: egui::Rect = egui::Rect {
        min: egui::pos2(0.0, 0.0),
        max: egui::pos2(800.0, 600.0),
    };

    fn box_at(x: f32, y: f32) -> egui::Rect {
        egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(160.0, 90.0))
    }

    fn close(a: egui::Pos2, b: egui::Pos2) -> bool {
        (a - b).length() < 1e-3
    }

    fn whole_inside(marker: Marker, canvas: egui::Rect) -> bool {
        marker
            .corners()
            .iter()
            .all(|corner| canvas.contains(*corner))
    }

    #[test]
    fn a_box_on_the_canvas_takes_no_marker() {
        assert_eq!(place(CANVAS, box_at(400.0, 300.0), &[]), None);
        // One corner on the canvas is enough.
        assert_eq!(place(CANVAS, box_at(870.0, 640.0), &[]), None);
    }

    #[test]
    fn the_marker_stands_on_the_edge_toward_the_box() {
        let r = reach();
        let cases = [
            (
                box_at(3000.0, 300.0),
                egui::pos2(800.0 - r, 300.0),
                egui::vec2(1.0, 0.0),
            ),
            (
                box_at(-3000.0, 300.0),
                egui::pos2(r, 300.0),
                egui::vec2(-1.0, 0.0),
            ),
            (
                box_at(400.0, -3000.0),
                egui::pos2(400.0, r),
                egui::vec2(0.0, -1.0),
            ),
            (
                box_at(400.0, 3000.0),
                egui::pos2(400.0, 600.0 - r),
                egui::vec2(0.0, 1.0),
            ),
        ];
        for (tv_box, at, toward) in cases {
            let marker = place(CANVAS, tv_box, &[]).expect("the box is off the canvas");
            assert!(close(marker.at, at), "{marker:?} for {tv_box:?}");
            assert!((marker.toward - toward).length() < 1e-3);
            assert!(whole_inside(marker, CANVAS));
        }
    }

    #[test]
    fn a_box_off_a_corner_puts_the_marker_whole_in_that_corner() {
        let square = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 600.0));
        let marker = place(square, box_at(3300.0, 3300.0), &[]).expect("the box is off");
        let r = reach();
        assert!(close(marker.at, egui::pos2(600.0 - r, 600.0 - r)));
        let angle = marker.toward.angle();
        assert!((angle - std::f32::consts::FRAC_PI_4).abs() < 1e-3);
        assert!(whole_inside(marker, square));
    }

    #[test]
    fn the_marker_steps_out_from_under_a_side_panel() {
        let panel = egui::Rect::from_min_max(egui::pos2(12.0, 12.0), egui::pos2(340.0, 588.0));
        let marker = place(CANVAS, box_at(-3000.0, 300.0), &[panel]).expect("the box is off");
        let clear = egui::Rect::from_points(&marker.corners());
        assert!(!clear.intersects(panel));
        assert!((marker.at.y - 300.0).abs() < 1e-3, "it stays on the line");
        assert!((marker.toward - egui::vec2(-1.0, 0.0)).length() < 1e-3);
    }

    #[test]
    fn the_marker_climbs_over_the_toolbar() {
        let toolbar = egui::Rect::from_min_max(egui::pos2(250.0, 544.0), egui::pos2(550.0, 588.0));
        let marker = place(CANVAS, box_at(400.0, 3000.0), &[toolbar]).expect("the box is off");
        let clear = egui::Rect::from_points(&marker.corners());
        assert!(!clear.intersects(toolbar));
        assert!((marker.toward - egui::vec2(0.0, 1.0)).length() < 1e-3);
    }
}
