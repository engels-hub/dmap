//! What the DM draws on the canvas: the pen, the shapes and the ruler.
//!
//! A stroke holds its points in inches, so it stays where the DM put it
//! when a map moves under it. Every kind of stroke reads as one line
//! through a list of points, and [`Stroke::polyline`] gives that line: a
//! shape is flattened here, and the renderer and the eraser both work on
//! the flat form.

// Rust guideline compliant 2026-02-21

use serde::{Deserialize, Serialize};

use crate::scene::{NodeId, Shown};

/// How many points an ellipse is drawn with.
///
/// A circle of this many sides shows no corner at the zoom a table ever
/// reaches. The TV box holds 48 inches, and a 1 inch circle there is 40
/// pixels across, which is under a pixel to a side.
const ROUND_STEPS: usize = 64;

/// What the DM drew a stroke with. It says how the points read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ink {
    /// A free line through every point.
    Pen,
    /// A straight line from the first point to the last.
    Line,
    /// A box between two corners.
    Rect,
    /// The ellipse that fills the box between two corners.
    Ellipse,
    /// A measure the DM kept, which reads as a bent line with its length.
    Measure,
    /// A round area of effect, from its middle to its edge.
    Burst,
    /// A cone of effect, from its point out to where it ends.
    ///
    /// A cone the DM did not widen ends as wide as it is long, which is
    /// the cone of D&D 5e.
    Cone,
    /// A straight run of effect, one cell wide unless the DM widened it.
    Beam,
}

/// How wide a beam of effect draws, in cells. One cell is five feet.
const BEAM_CELLS: f64 = 1.0;

/// How many points the ring of a burst is drawn with.
const ARC_STEPS: usize = 48;

impl Ink {
    /// Whether the DM sets the width of this ink with a second drag.
    ///
    /// A burst has one number, its radius, and one drag gives it. A cone
    /// and a beam have a length and a width, so the gesture is a drag for
    /// the one and a drag for the other. Issue #12.
    pub fn spans(self) -> bool {
        matches!(self, Self::Cone | Self::Beam)
    }

    /// Whether this ink fills the shape it draws, as an effect does.
    pub fn filled(self) -> bool {
        matches!(self, Self::Burst | Self::Cone | Self::Beam)
    }

    /// How far an effect reaches, in cells, or `None` for other ink.
    ///
    /// A burst gives its radius, and a cone and a beam their length. The
    /// label beside the shape says it in cells and in feet.
    pub fn reach(self, points: &[(f64, f64)]) -> Option<f64> {
        if !self.filled() {
            return None;
        }
        let (from, to) = (points.first()?, points.last()?);
        Some((to.0 - from.0).hypot(to.1 - from.1))
    }

    /// The glyph the objects list and the Draw panel give this kind.
    pub fn glyph(self) -> crate::icons::Icon {
        use crate::icons::Icon;
        match self {
            Self::Pen => Icon::Pencil,
            Self::Line | Self::Beam => Icon::Minus,
            Self::Rect => Icon::Square,
            Self::Ellipse | Self::Burst => Icon::Circle,
            Self::Measure => Icon::Ruler,
            Self::Cone => Icon::Triangle,
        }
    }

    /// What the objects list and the history call this kind of stroke.
    pub fn name(self) -> &'static str {
        match self {
            Self::Pen => crate::text::stroke_pen(),
            Self::Line => crate::text::stroke_line(),
            Self::Rect => crate::text::stroke_rect(),
            Self::Ellipse => crate::text::stroke_ellipse(),
            Self::Measure => crate::text::stroke_measure(),
            Self::Burst => crate::text::stroke_burst(),
            Self::Cone => crate::text::stroke_cone(),
            Self::Beam => crate::text::stroke_beam(),
        }
    }
}

/// How a ruler counts the distance between two points.
///
/// A grid cell is one inch and five feet, and a table agrees before the
/// game which of these it plays by. Roll20 offers the same four. Issue
/// #12.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// The straight line between the two points.
    #[default]
    Euclid,
    /// Every step counts, and no step goes corner to corner.
    Manhattan,
    /// A diagonal costs the same as a straight step. D&D 5e.
    Fifth,
    /// Every second diagonal costs two. D&D 3.5 and Pathfinder.
    Pathfinder,
}

impl Rule {
    /// How many cells lie between two points, by this rule.
    pub fn cells(self, from: (f64, f64), to: (f64, f64)) -> f64 {
        let (across, down) = ((to.0 - from.0).abs(), (to.1 - from.1).abs());
        let (long, short) = (across.max(down), across.min(down));
        match self {
            Self::Euclid => across.hypot(down),
            Self::Manhattan => across + down,
            Self::Fifth => long,
            // The first diagonal costs one cell and the second two, so a
            // pair of them costs three.
            Self::Pathfinder => long - short + short + (short / 2.0).floor(),
        }
    }

    /// What the Draw panel calls this rule.
    pub fn name(self) -> &'static str {
        match self {
            Self::Euclid => crate::text::ruler_euclid(),
            Self::Manhattan => crate::text::ruler_manhattan(),
            Self::Fifth => crate::text::ruler_fifth(),
            Self::Pathfinder => crate::text::ruler_pathfinder(),
        }
    }

    /// Every rule, in the order the panel lists them.
    pub fn all() -> [Self; 4] {
        [Self::Euclid, Self::Fifth, Self::Pathfinder, Self::Manhattan]
    }
}

/// One thing the DM drew, in inches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// The name of this stroke in the tree.
    pub id: NodeId,
    /// Which screens it draws on.
    #[serde(default)]
    pub shown: Shown,
    /// What it was drawn with.
    pub ink: Ink,
    /// The points, in inches. A pen holds many, a shape holds two.
    pub points: Vec<(f64, f64)>,
    /// The color, as red, green, blue and alpha.
    pub color: [u8; 4],
    /// How thick the line draws, in inches.
    pub width: f64,
    /// How wide a cone ends or a beam runs, in cells.
    ///
    /// Zero leaves the shape its own rule: a cone as wide as it is long,
    /// which is D&D 5e, and a beam of one cell. The DM sets it with the
    /// second drag of the gesture. Issue #12.
    #[serde(default)]
    pub span: f64,
    /// How a measure counts its length. It says nothing for other ink.
    ///
    /// A kept measure holds the rule it was made under, so the number it
    /// shows stays the number the DM read when they made it.
    #[serde(default)]
    pub rule: Rule,
}

impl Stroke {
    /// The line this stroke draws, point by point, in inches.
    ///
    /// A pen gives back what it holds. A shape is built here, so the
    /// renderer, the eraser and the hit test all read one flat line.
    pub fn polyline(&self) -> Vec<(f64, f64)> {
        let (Some(first), Some(last)) = (self.points.first(), self.points.last()) else {
            return Vec::new();
        };
        match self.ink {
            Ink::Pen | Ink::Measure => self.points.clone(),
            Ink::Line => vec![*first, *last],
            Ink::Rect => vec![*first, (last.0, first.1), *last, (first.0, last.1), *first],
            Ink::Ellipse => ellipse(*first, *last),
            Ink::Burst => burst(*first, *last),
            Ink::Cone => cone(*first, *last, self.span),
            Ink::Beam => beam(*first, *last, self.span),
        }
    }

    /// The box this stroke covers, in inches, without its width.
    pub fn bounds(&self) -> Option<((f64, f64), (f64, f64))> {
        let line = self.polyline();
        let first = *line.first()?;
        let mut low = first;
        let mut high = first;
        for (x, y) in line {
            low = (low.0.min(x), low.1.min(y));
            high = (high.0.max(x), high.1.max(y));
        }
        Some((low, high))
    }

    /// Whether a disc of `radius` inches around `at` reaches this stroke.
    pub fn touches(&self, at: (f64, f64), radius: f64) -> bool {
        let reach = radius + self.width / 2.0;
        let line = self.polyline();
        line.windows(2)
            .any(|pair| near_segment(at, pair[0], pair[1]) <= reach)
    }
}

/// The round area of effect around a point, out to where the drag ended.
fn burst(center: (f64, f64), edge: (f64, f64)) -> Vec<(f64, f64)> {
    let radius = (edge.0 - center.0).hypot(edge.1 - center.1);
    (0..=ARC_STEPS)
        .map(|step| {
            let angle = std::f64::consts::TAU * step as f64 / ARC_STEPS as f64;
            (
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            )
        })
        .collect()
}

/// The cone of effect from a point, out to where the drag ended.
///
/// A cone is as wide at its end as it is long, which is the cone of
/// D&D 5e, chapter 10. So it draws as a triangle with a flat end, and
/// the end is one length across.
fn cone(point: (f64, f64), end: (f64, f64), span: f64) -> Vec<(f64, f64)> {
    let (dx, dy) = (end.0 - point.0, end.1 - point.1);
    let length = dx.hypot(dy);
    if length <= f64::EPSILON {
        return vec![point];
    }
    let half = if span > 0.0 { span / 2.0 } else { length / 2.0 };
    let side = (-dy / length * half, dx / length * half);
    vec![
        point,
        (end.0 + side.0, end.1 + side.1),
        (end.0 - side.0, end.1 - side.1),
        point,
    ]
}

/// The straight run of effect between two points, one cell wide.
fn beam(from: (f64, f64), to: (f64, f64), span: f64) -> Vec<(f64, f64)> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy);
    if length <= f64::EPSILON {
        return vec![from];
    }
    let half = if span > 0.0 { span } else { BEAM_CELLS } / 2.0;
    let side = (-dy / length * half, dx / length * half);
    vec![
        (from.0 + side.0, from.1 + side.1),
        (to.0 + side.0, to.1 + side.1),
        (to.0 - side.0, to.1 - side.1),
        (from.0 - side.0, from.1 - side.1),
        (from.0 + side.0, from.1 + side.1),
    ]
}

/// The points of the ellipse that fills the box between two corners.
fn ellipse(first: (f64, f64), last: (f64, f64)) -> Vec<(f64, f64)> {
    let center = (
        f64::midpoint(first.0, last.0),
        f64::midpoint(first.1, last.1),
    );
    let radius = ((last.0 - first.0) / 2.0, (last.1 - first.1) / 2.0);
    (0..=ROUND_STEPS)
        .map(|step| {
            let angle = std::f64::consts::TAU * step as f64 / ROUND_STEPS as f64;
            (
                center.0 + radius.0 * angle.cos(),
                center.1 + radius.1 * angle.sin(),
            )
        })
        .collect()
}

/// How far `point` lies from the segment between `from` and `to`.
fn near_segment(point: (f64, f64), from: (f64, f64), to: (f64, f64)) -> f64 {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx * dx + dy * dy;
    let along = if length <= f64::EPSILON {
        0.0
    } else {
        (((point.0 - from.0) * dx + (point.1 - from.1) * dy) / length).clamp(0.0, 1.0)
    };
    let near = (from.0 + along * dx, from.1 + along * dy);
    ((point.0 - near.0).powi(2) + (point.1 - near.1).powi(2)).sqrt()
}

/// What is left of a line once a disc takes a bite out of it.
///
/// The disc is the eraser: `at` in inches, `radius` in inches. The line
/// comes back in runs, one for each part that survived, in the order it
/// was drawn. A run of one point is dropped: a point draws nothing.
///
/// Returns `None` when the disc touched nothing, so the caller knows to
/// leave the stroke alone.
pub fn cut(line: &[(f64, f64)], at: (f64, f64), radius: f64) -> Option<Vec<Vec<(f64, f64)>>> {
    if line.len() < 2 {
        return None;
    }
    let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut run: Vec<(f64, f64)> = Vec::new();
    let mut bitten = false;
    for pair in line.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let inside = crossings(from, to, at, radius);
        let Some((enter, leave)) = inside else {
            // The whole segment is clear of the disc.
            if run.is_empty() {
                run.push(from);
            }
            run.push(to);
            continue;
        };
        bitten = true;
        // The part before the disc keeps what came before it.
        if enter > 0.0 {
            if run.is_empty() {
                run.push(from);
            }
            run.push(walk(from, to, enter));
        }
        if !run.is_empty() {
            runs.push(std::mem::take(&mut run));
        }
        // The part after the disc starts a run of its own.
        if leave < 1.0 {
            run.push(walk(from, to, leave));
            run.push(to);
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    if !bitten {
        return None;
    }
    runs.retain(|run| run.len() > 1);
    Some(runs)
}

/// The point a fraction of the way from `from` to `to`.
fn walk(from: (f64, f64), to: (f64, f64), part: f64) -> (f64, f64) {
    (
        from.0 + (to.0 - from.0) * part,
        from.1 + (to.1 - from.1) * part,
    )
}

/// Where a segment goes into a disc and where it comes out.
///
/// The two numbers are fractions of the way from `from` to `to`, clamped
/// to the segment, so a segment that starts inside the disc enters at 0.
/// Returns `None` when the segment misses the disc.
fn crossings(from: (f64, f64), to: (f64, f64), at: (f64, f64), radius: f64) -> Option<(f64, f64)> {
    // The segment is `from + t * step`. Where it meets the disc solves
    // `|from + t * step - at| = radius`, a square equation in `t`.
    let step = (to.0 - from.0, to.1 - from.1);
    let start = (from.0 - at.0, from.1 - at.1);
    let a = step.0 * step.0 + step.1 * step.1;
    if a <= f64::EPSILON {
        // A segment of no length is inside or outside, whole.
        let inside = start.0 * start.0 + start.1 * start.1 <= radius * radius;
        return inside.then_some((0.0, 1.0));
    }
    let b = 2.0 * (start.0 * step.0 + start.1 * step.1);
    let c = start.0 * start.0 + start.1 * start.1 - radius * radius;
    let under = b * b - 4.0 * a * c;
    if under < 0.0 {
        return None;
    }
    let root = under.sqrt();
    let enter = (-b - root) / (2.0 * a);
    let leave = (-b + root) / (2.0 * a);
    if leave <= 0.0 || enter >= 1.0 {
        return None;
    }
    Some((enter.max(0.0), leave.min(1.0)))
}

#[cfg(test)]
mod tests {
    use super::{Ink, Rule, Stroke, cut};
    use crate::scene::Shown;

    fn stroke(ink: Ink, points: &[(f64, f64)]) -> Stroke {
        Stroke {
            id: 1,
            shown: Shown::default(),
            ink,
            points: points.to_vec(),
            color: [0, 0, 0, 255],
            width: 0.1,
            span: 0.0,
            rule: Rule::default(),
        }
    }

    #[test]
    fn a_burst_reaches_as_far_as_the_drag_took_it() {
        let mark = stroke(Ink::Burst, &[(2.0, 2.0), (2.0, 5.0)]);
        assert!((mark.ink.reach(&mark.points).unwrap() - 3.0).abs() < 1e-9);
        let line = mark.polyline();
        for point in &line {
            let away = (point.0 - 2.0).hypot(point.1 - 2.0);
            assert!((away - 3.0).abs() < 1e-9, "{point:?} lies {away} out");
        }
    }

    #[test]
    fn a_cone_is_as_wide_as_it_is_long() {
        let mark = stroke(Ink::Cone, &[(0.0, 0.0), (4.0, 0.0)]);
        let line = mark.polyline();
        let across = line
            .iter()
            .map(|p| p.1)
            .fold(0.0_f64, |far, y| far.max(y.abs()));
        // The end is one length across, so it reaches half a length out
        // to each side.
        assert!((across - 2.0).abs() < 1e-9, "the cone reaches {across} out");
        assert_eq!(line.first(), line.last());
    }

    #[test]
    fn a_second_drag_sets_the_width_of_a_cone_and_a_beam() {
        let mut mark = stroke(Ink::Beam, &[(0.0, 0.0), (5.0, 0.0)]);
        mark.span = 3.0;
        let line = mark.polyline();
        let width = (line[0].1 - line[3].1).abs();
        assert!((width - 3.0).abs() < 1e-9, "the beam is {width} wide");
        let mut mark = stroke(Ink::Cone, &[(0.0, 0.0), (4.0, 0.0)]);
        mark.span = 1.0;
        let line = mark.polyline();
        let across = line
            .iter()
            .map(|p| p.1)
            .fold(0.0_f64, |far, y| far.max(y.abs()));
        assert!((across - 0.5).abs() < 1e-9, "the cone ends {across} out");
        assert!(Ink::Cone.spans() && Ink::Beam.spans() && !Ink::Burst.spans());
    }

    #[test]
    fn a_beam_is_one_cell_wide_whichever_way_it_runs() {
        for to in [(5.0, 0.0), (0.0, 5.0), (3.0, 4.0)] {
            let mark = stroke(Ink::Beam, &[(0.0, 0.0), to]);
            let line = mark.polyline();
            let width = (line[0].0 - line[3].0).hypot(line[0].1 - line[3].1);
            assert!(
                (width - 1.0).abs() < 1e-9,
                "a beam to {to:?} is {width} wide"
            );
        }
    }

    #[test]
    fn only_an_effect_fills_and_reaches() {
        assert!(Ink::Burst.filled() && Ink::Cone.filled() && Ink::Beam.filled());
        assert!(!Ink::Pen.filled() && !Ink::Rect.filled() && !Ink::Measure.filled());
        assert!(Ink::Pen.reach(&[(0.0, 0.0), (1.0, 0.0)]).is_none());
    }

    #[test]
    fn each_rule_counts_the_cells_its_own_way() {
        let (from, to) = ((0.0, 0.0), (3.0, 4.0));
        assert!((Rule::Euclid.cells(from, to) - 5.0).abs() < 1e-9);
        assert!((Rule::Manhattan.cells(from, to) - 7.0).abs() < 1e-9);
        assert!((Rule::Fifth.cells(from, to) - 4.0).abs() < 1e-9);
        // Three diagonals and one straight step: 3 + 1 + one second
        // diagonal, which costs one more.
        assert!((Rule::Pathfinder.cells(from, to) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn a_straight_run_reads_the_same_under_every_rule() {
        let (from, to) = ((1.0, 1.0), (1.0, 6.0));
        for rule in Rule::all() {
            assert!((rule.cells(from, to) - 5.0).abs() < 1e-9, "{rule:?}");
        }
    }

    #[test]
    fn a_pair_of_diagonals_costs_three_under_pathfinder() {
        let cells = Rule::Pathfinder.cells((0.0, 0.0), (2.0, 2.0));
        assert!((cells - 3.0).abs() < 1e-9, "{cells}");
    }

    #[test]
    fn a_box_comes_back_to_its_first_corner() {
        let line = stroke(Ink::Rect, &[(0.0, 0.0), (2.0, 1.0)]).polyline();
        assert_eq!(line.len(), 5);
        assert_eq!(line[0], (0.0, 0.0));
        assert_eq!(line[1], (2.0, 0.0));
        assert_eq!(line[2], (2.0, 1.0));
        assert_eq!(line[3], (0.0, 1.0));
        assert_eq!(line[4], line[0]);
    }

    #[test]
    fn an_ellipse_fills_the_box_it_was_drawn_in() {
        let line = stroke(Ink::Ellipse, &[(-1.0, -2.0), (1.0, 2.0)]).polyline();
        let far_x = line.iter().map(|p| p.0.abs()).fold(0.0, f64::max);
        let far_y = line.iter().map(|p| p.1.abs()).fold(0.0, f64::max);
        assert!((far_x - 1.0).abs() < 1e-9, "x reaches {far_x}");
        assert!((far_y - 2.0).abs() < 1e-9, "y reaches {far_y}");
    }

    #[test]
    fn a_line_reads_from_its_first_point_to_its_last() {
        let line = stroke(Ink::Line, &[(0.0, 0.0), (3.0, 4.0)]).polyline();
        assert_eq!(line, vec![(0.0, 0.0), (3.0, 4.0)]);
    }

    #[test]
    fn the_eraser_reaches_a_stroke_by_its_width_as_well() {
        let mark = stroke(Ink::Line, &[(0.0, 0.0), (4.0, 0.0)]);
        // Half a width of 0.1 is 0.05, so a disc of 0.1 reaches 0.15.
        assert!(mark.touches((2.0, 0.14), 0.1));
        assert!(!mark.touches((2.0, 0.2), 0.1));
        // Past the end of the line is past the stroke.
        assert!(!mark.touches((4.5, 0.0), 0.1));
    }

    #[test]
    fn a_bite_out_of_the_middle_leaves_two_runs() {
        let line = vec![(0.0, 0.0), (10.0, 0.0)];
        let runs = cut(&line, (5.0, 0.0), 1.0).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0], vec![(0.0, 0.0), (4.0, 0.0)]);
        assert_eq!(runs[1], vec![(6.0, 0.0), (10.0, 0.0)]);
    }

    #[test]
    fn a_bite_at_the_end_leaves_one_run() {
        let line = vec![(0.0, 0.0), (10.0, 0.0)];
        let runs = cut(&line, (10.0, 0.0), 2.0).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0], vec![(0.0, 0.0), (8.0, 0.0)]);
    }

    #[test]
    fn a_disc_over_the_whole_line_leaves_nothing() {
        let line = vec![(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)];
        let runs = cut(&line, (1.0, 0.0), 5.0).unwrap();
        assert!(runs.is_empty());
    }

    #[test]
    fn a_disc_that_misses_leaves_the_stroke_alone() {
        let line = vec![(0.0, 0.0), (10.0, 0.0)];
        assert!(cut(&line, (5.0, 3.0), 1.0).is_none());
    }

    #[test]
    fn a_bite_out_of_a_bend_keeps_both_arms() {
        let line = vec![(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)];
        let runs = cut(&line, (5.0, 0.0), 1.0).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0], vec![(0.0, 0.0), (4.0, 0.0)]);
        assert_eq!(runs[1], vec![(5.0, 1.0), (5.0, 5.0)]);
    }
}
