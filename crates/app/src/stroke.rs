//! What the DM draws on the canvas: the pen, the shapes and the ruler.
//!
//! A stroke holds its points in inches, so it stays where the DM put it
//! when a map moves under it. Every kind of stroke reads as one line
//! through a list of points, and [`Stroke::polyline`] gives that line: a
//! shape is flattened here, and the renderer and the eraser both work on
//! the flat form.

// Rust guideline compliant 2026-02-21

use crate::grid::Cells;

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
    /// It ends as wide as it is long, on a straight edge. The cone of
    /// D&D 5e, chapter 10.
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
    /// A burst and a cone have one number each, their radius and their
    /// length, and one drag gives it. A beam has a length and a width, so
    /// it takes a drag for the one and a drag for the other. Issue #12.
    pub fn spans(self) -> bool {
        matches!(self, Self::Beam)
    }

    /// Whether the eraser takes this ink whole instead of cutting it.
    ///
    /// An area of effect and a kept measure each stand for one thing: a
    /// spell that covers a patch of the map, a distance the DM read off
    /// it. A piece of either one says nothing, so the eraser takes the
    /// whole of it. A pen or a shape is paint, and paint comes off in
    /// parts. Issue #12.
    pub fn whole(self) -> bool {
        self.filled() || matches!(self, Self::Measure)
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
    ///
    /// The three rules that are not the straight line count steps over a
    /// grid, so the shape of that grid decides what a step is. A hex grid
    /// has no diagonal and the three agree on the count. Issue #15.
    pub fn cells(self, cells: Cells, from: (f64, f64), to: (f64, f64)) -> f64 {
        let cell = if cells.cell.is_finite() && cells.cell > 0.0 {
            cells.cell
        } else {
            crate::grid::DEFAULT_CELL
        };
        let (across, down) = ((to.0 - from.0).abs() / cell, (to.1 - from.1).abs() / cell);
        if let Some(steps) = cells.hex_steps(from, to) {
            // A straight line is a straight line on any grid. The rest
            // walk from hex to hex.
            return if self == Self::Euclid {
                across.hypot(down)
            } else {
                steps
            };
        }
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

    /// What the Draw panel calls this rule on this grid.
    ///
    /// A hex grid has no diagonal, so every rule that walks it gives one
    /// count. The three read as one name there. Issue #15.
    pub fn name_on(self, cells: Cells) -> &'static str {
        if cells.kind.is_hex() && self != Self::Euclid {
            return crate::text::ruler_hexes();
        }
        self.name()
    }

    /// The rules the DM may pick on this grid.
    ///
    /// A hex grid offers the straight line and the walk, because the three
    /// rules that walk it agree. `held` is the rule the DM holds now, so a
    /// grid of squares later finds it where the DM left it. D&D 5e stands
    /// for the walk when the DM holds the straight line, because a table
    /// that counts hexes most often counts them that way.
    pub fn all_on(cells: Cells, held: Self) -> Vec<Self> {
        if !cells.kind.is_hex() {
            return Self::all().to_vec();
        }
        let walks = if held == Self::Euclid {
            Self::Fifth
        } else {
            held
        };
        vec![Self::Euclid, walks]
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
    /// How far a rectangle or an ellipse turns, in radians.
    ///
    /// Those two hold two opposite corners and nothing else, so a turn of
    /// the corners would give another box and not the same box at an
    /// angle. The angle sits beside them instead, and the outline takes
    /// it. Every other ink holds its heading in its own points and leaves
    /// this at nothing. Issue #69.
    #[serde(default)]
    pub angle: f64,
    /// How a measure counts its length. It says nothing for other ink.
    ///
    /// A kept measure holds the rule it was made under, so the number it
    /// shows stays the number the DM read when they made it.
    #[serde(default)]
    pub rule: Rule,
}

/// The smallest a growth may leave a stroke, as a share of its size.
///
/// A stroke of no size has no box and no handles, so a DM who pulled a
/// corner past the middle would have nothing left to pull back.
const MIN_FACTOR: f64 = 0.01;

impl Stroke {
    /// The line this stroke draws, point by point, in inches.
    ///
    /// A pen gives back what it holds. A shape is built here, so the
    /// renderer, the eraser and the hit test all read one flat line.
    pub fn polyline(&self) -> Vec<(f64, f64)> {
        let (Some(first), Some(last)) = (self.points.first(), self.points.last()) else {
            return Vec::new();
        };
        let line = match self.ink {
            Ink::Pen | Ink::Measure => self.points.clone(),
            Ink::Line => vec![*first, *last],
            Ink::Rect => vec![*first, (last.0, first.1), *last, (first.0, last.1), *first],
            Ink::Ellipse => ellipse(*first, *last),
            Ink::Burst => burst(*first, *last),
            Ink::Cone => cone(*first, *last),
            Ink::Beam => beam(*first, *last, self.span),
        };
        if self.angle.abs() < f64::EPSILON {
            return line;
        }
        // A box turns around its own middle, so the turn leaves it the
        // size the DM gave it. Issue #69.
        let middle = (
            f64::midpoint(first.0, last.0),
            f64::midpoint(first.1, last.1),
        );
        let (sin, cos) = self.angle.sin_cos();
        line.into_iter()
            .map(|(x, y)| {
                let (dx, dy) = (x - middle.0, y - middle.1);
                (
                    middle.0 + dx * cos - dy * sin,
                    middle.1 + dx * sin + dy * cos,
                )
            })
            .collect()
    }

    /// The points this stroke takes when its reach becomes `cells`.
    ///
    /// The far point moves along the line the shape already runs on, so
    /// the shape keeps its heading and takes a new size. The panel of a
    /// stroke changes an area of effect this way. Issue #12.
    pub fn reached(&self, cells: f64) -> Vec<(f64, f64)> {
        let (Some(from), Some(to)) = (self.points.first(), self.points.last()) else {
            return self.points.clone();
        };
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = dx.hypot(dy);
        if length <= f64::EPSILON {
            // A shape of no length has no heading to keep, so the new
            // reach runs to the right, where a drag would have started.
            return vec![*from, (from.0 + cells, from.1)];
        }
        vec![
            *from,
            (from.0 + dx / length * cells, from.1 + dy / length * cells),
        ]
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

    /// The same stroke, moved by `step` inches.
    ///
    /// A stroke holds points and no center, so a move rewrites each one.
    /// Every transform starts from the stroke as it stood when the drag
    /// began, so a drag of many frames leaves no drift behind. Issue #69.
    pub fn moved(&self, step: (f64, f64)) -> Self {
        self.mapped(|(x, y)| (x + step.0, y + step.1))
    }

    /// The same stroke, grown around `pivot` by `factor`.
    ///
    /// The width grows with it, and so does the reach of an area of
    /// effect, because the reach comes from the points. A factor of
    /// nothing would leave a stroke with no length and no way back, so the
    /// growth stops at [`MIN_FACTOR`].
    pub fn scaled(&self, pivot: (f64, f64), factor: f64) -> Self {
        let factor = factor.max(MIN_FACTOR);
        let mut grown = self.mapped(|(x, y)| {
            (
                pivot.0 + (x - pivot.0) * factor,
                pivot.1 + (y - pivot.1) * factor,
            )
        });
        grown.width *= factor;
        grown.span *= factor;
        grown
    }

    /// The same stroke, turned around `pivot` by `angle` radians.
    ///
    /// The width and the span are lengths across the line, so a turn
    /// leaves both as they were. A rectangle and an ellipse take the turn
    /// in [`Stroke::angle`] and carry their middle around the pivot, so
    /// the box keeps the size the DM gave it.
    pub fn turned(&self, pivot: (f64, f64), angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        let around = |(x, y): (f64, f64)| {
            let (dx, dy) = (x - pivot.0, y - pivot.1);
            (pivot.0 + dx * cos - dy * sin, pivot.1 + dx * sin + dy * cos)
        };
        if !matches!(self.ink, Ink::Rect | Ink::Ellipse) {
            return self.mapped(around);
        }
        let (Some(first), Some(last)) = (self.points.first(), self.points.last()) else {
            return self.clone();
        };
        let middle = (
            f64::midpoint(first.0, last.0),
            f64::midpoint(first.1, last.1),
        );
        let half = ((last.0 - first.0) / 2.0, (last.1 - first.1) / 2.0);
        let moved = around(middle);
        let mut turned = self.clone();
        turned.points = vec![
            (moved.0 - half.0, moved.1 - half.1),
            (moved.0 + half.0, moved.1 + half.1),
        ];
        turned.angle += angle;
        turned
    }

    /// The same stroke, with every point through `place`.
    fn mapped(&self, place: impl Fn((f64, f64)) -> (f64, f64)) -> Self {
        let mut moved = self.clone();
        for point in &mut moved.points {
            *point = place(*point);
        }
        moved
    }

    /// The middle of the box around this stroke, in inches.
    pub fn middle(&self) -> Option<(f64, f64)> {
        let (low, high) = self.bounds()?;
        Some((f64::midpoint(low.0, high.0), f64::midpoint(low.1, high.1)))
    }

    /// Whether a disc of `radius` inches around `at` reaches this stroke.
    ///
    /// A filled shape counts from anywhere inside it, not from its line
    /// alone: the DM sees an area, and the whole of that area is the
    /// shape. Issue #12.
    pub fn touches(&self, at: (f64, f64), radius: f64) -> bool {
        let reach = radius + self.width / 2.0;
        let line = self.polyline();
        if line
            .windows(2)
            .any(|pair| near_segment(at, pair[0], pair[1]) <= reach)
        {
            return true;
        }
        self.ink.filled() && inside(&line, at)
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
/// A cone is as wide across its end as it is long, and its end is
/// straight. D&D 5e, chapter 10. So it draws as a triangle, and the two
/// far corners stand half a length to each side of where the drag ended.
fn cone(point: (f64, f64), end: (f64, f64)) -> Vec<(f64, f64)> {
    let (dx, dy) = (end.0 - point.0, end.1 - point.1);
    let length = dx.hypot(dy);
    if length <= f64::EPSILON {
        return vec![point];
    }
    let half = length / 2.0;
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

/// Whether a point lies inside a shape.
///
/// A ray runs from the point out to the right, and the shape holds the
/// point when the ray crosses the outline an odd number of times.
fn inside(line: &[(f64, f64)], at: (f64, f64)) -> bool {
    let mut held = false;
    for pair in line.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        // Only a side that stands across the ray can cross it.
        if (from.1 > at.1) == (to.1 > at.1) {
            continue;
        }
        let part = (at.1 - from.1) / (to.1 - from.1);
        if at.0 < from.0 + part * (to.0 - from.0) {
            held = !held;
        }
    }
    held
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
    use crate::grid::{Cells, Kind};
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
            angle: 0.0,
            rule: Rule::default(),
        }
    }

    #[test]
    fn a_move_carries_every_point_of_a_drawing() {
        let mark = stroke(Ink::Pen, &[(1.0, 1.0), (2.0, 3.0)]);
        let moved = mark.moved((0.5, -1.0));
        assert_eq!(moved.points, vec![(1.5, 0.0), (2.5, 2.0)]);
        // Nothing but the points moves.
        assert!((moved.width - mark.width).abs() < f64::EPSILON);
        // And the way back is the way there, the other way around.
        assert_eq!(moved.moved((-0.5, 1.0)).points, mark.points);
    }

    #[test]
    fn a_growth_takes_the_width_and_the_reach_with_it() {
        // A burst of two inches of reach, around the origin. Issue #69.
        let mark = stroke(Ink::Burst, &[(0.0, 0.0), (2.0, 0.0)]);
        let grown = mark.scaled((0.0, 0.0), 2.0);
        assert_eq!(grown.points, vec![(0.0, 0.0), (4.0, 0.0)]);
        assert!((grown.width - mark.width * 2.0).abs() < 1e-9);
        // The reach comes from the points, so it grew with them.
        let reach = grown.ink.reach(&grown.points).unwrap();
        assert!((reach - 4.0).abs() < 1e-9);
    }

    #[test]
    fn a_growth_leaves_a_drawing_something_to_hold() {
        // A DM who pulls a corner past the middle would otherwise be left
        // with a drawing of no size, and so with no handles to pull back.
        let mark = stroke(Ink::Line, &[(1.0, 0.0), (3.0, 0.0)]);
        let flat = mark.scaled((2.0, 0.0), 0.0);
        let (low, high) = flat.bounds().unwrap();
        assert!(high.0 - low.0 > 0.0, "a drawing with no width is lost");
    }

    #[test]
    fn a_turned_box_keeps_the_size_the_dm_gave_it() {
        // A rectangle holds two corners, so the turn goes beside them and
        // the outline takes it. Issue #69.
        let mark = stroke(Ink::Rect, &[(0.0, 0.0), (4.0, 2.0)]);
        let turned = mark.turned((2.0, 1.0), std::f64::consts::FRAC_PI_2);
        // A quarter turn about the middle: the box stands on its end.
        let (low, high) = turned.bounds().unwrap();
        assert!((high.0 - low.0 - 2.0).abs() < 1e-9, "{low:?} {high:?}");
        assert!((high.1 - low.1 - 4.0).abs() < 1e-9, "{low:?} {high:?}");
        // And the turn is in the angle, not in the corners.
        assert!((turned.angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert_eq!(turned.points, mark.points);
    }

    #[test]
    fn a_turned_box_carries_its_middle_around_the_pivot() {
        let mark = stroke(Ink::Ellipse, &[(0.0, 0.0), (2.0, 2.0)]);
        // Half a turn about the origin puts the middle on the other side.
        let turned = mark.turned((0.0, 0.0), std::f64::consts::PI);
        let (x, y) = turned.middle().unwrap();
        assert!((x + 1.0).abs() < 1e-9 && (y + 1.0).abs() < 1e-9, "{x} {y}");
    }

    #[test]
    fn a_turn_keeps_the_width_and_the_span() {
        let mark = stroke(Ink::Beam, &[(0.0, 0.0), (2.0, 0.0)]);
        let turned = mark.turned((0.0, 0.0), std::f64::consts::FRAC_PI_2);
        let (x, y) = turned.points[1];
        assert!(x.abs() < 1e-9 && (y - 2.0).abs() < 1e-9, "{x} {y}");
        assert!((turned.width - mark.width).abs() < f64::EPSILON);
        assert!((turned.span - mark.span).abs() < f64::EPSILON);
    }

    #[test]
    fn the_middle_of_a_drawing_is_the_middle_of_its_box() {
        let mark = stroke(Ink::Rect, &[(1.0, 2.0), (3.0, 6.0)]);
        let (x, y) = mark.middle().unwrap();
        assert!((x - 2.0).abs() < 1e-9 && (y - 4.0).abs() < 1e-9);
    }

    #[test]
    fn a_new_reach_keeps_the_heading_of_the_shape() {
        let mark = stroke(Ink::Cone, &[(1.0, 1.0), (4.0, 5.0)]);
        // The shape runs five cells out, three across and four down.
        let points = mark.reached(10.0);
        assert_eq!(points[0], (1.0, 1.0));
        let (dx, dy) = (points[1].0 - 1.0, points[1].1 - 1.0);
        assert!(
            (dx.hypot(dy) - 10.0).abs() < 1e-9,
            "it reaches {}",
            dx.hypot(dy)
        );
        // Six across and eight down is the same heading, twice as far.
        assert!(
            (dx - 6.0).abs() < 1e-9 && (dy - 8.0).abs() < 1e-9,
            "{points:?}"
        );
    }

    #[test]
    fn a_shape_of_no_length_still_takes_a_reach() {
        let mark = stroke(Ink::Burst, &[(2.0, 2.0), (2.0, 2.0)]);
        let points = mark.reached(3.0);
        assert_eq!(points, vec![(2.0, 2.0), (5.0, 2.0)]);
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
    fn a_cone_ends_as_wide_as_it_is_long() {
        let mark = stroke(Ink::Cone, &[(0.0, 0.0), (4.0, 0.0)]);
        let line = mark.polyline();
        assert_eq!(line.first(), line.last());
        // The end is straight, and one length across.
        let (left, right) = (line[1], line[2]);
        assert!((left.0 - 4.0).abs() < 1e-9 && (right.0 - 4.0).abs() < 1e-9);
        let across = (left.0 - right.0).hypot(left.1 - right.1);
        assert!((across - 4.0).abs() < 1e-9, "the cone ends {across} across");
    }

    #[test]
    fn a_second_drag_sets_the_width_of_a_beam_alone() {
        let mut mark = stroke(Ink::Beam, &[(0.0, 0.0), (5.0, 0.0)]);
        mark.span = 3.0;
        let line = mark.polyline();
        let width = (line[0].1 - line[3].1).abs();
        assert!((width - 3.0).abs() < 1e-9, "the beam is {width} wide");
        assert!(Ink::Beam.spans());
        assert!(!Ink::Cone.spans() && !Ink::Burst.spans());
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
    fn a_filled_shape_counts_from_anywhere_inside_it() {
        let beam = stroke(Ink::Beam, &[(0.0, 0.0), (10.0, 0.0)]);
        // The middle of the beam, half a cell from either long side.
        assert!(beam.touches((5.0, 0.0), 0.01));
        assert!(!beam.touches((5.0, 2.0), 0.01));
        let burst = stroke(Ink::Burst, &[(0.0, 0.0), (4.0, 0.0)]);
        assert!(burst.touches((1.0, 1.0), 0.01));
        assert!(!burst.touches((4.0, 4.0), 0.01));
        // A line that fills nothing keeps to itself.
        let box_mark = stroke(Ink::Rect, &[(0.0, 0.0), (10.0, 10.0)]);
        assert!(!box_mark.touches((5.0, 5.0), 0.01));
        assert!(box_mark.touches((0.0, 5.0), 0.01));
    }

    #[test]
    fn the_eraser_takes_an_effect_and_a_measure_whole() {
        assert!(Ink::Burst.whole() && Ink::Cone.whole() && Ink::Beam.whole());
        assert!(Ink::Measure.whole());
        assert!(!Ink::Pen.whole() && !Ink::Line.whole() && !Ink::Rect.whole());
    }

    #[test]
    fn only_an_effect_fills_and_reaches() {
        assert!(Ink::Burst.filled() && Ink::Cone.filled() && Ink::Beam.filled());
        assert!(!Ink::Pen.filled() && !Ink::Rect.filled() && !Ink::Measure.filled());
        assert!(Ink::Pen.reach(&[(0.0, 0.0), (1.0, 0.0)]).is_none());
    }

    /// The square inch grid the program draws when the DM changes nothing.
    fn inch() -> Cells {
        Cells::default()
    }

    /// A grid of hexes one inch flat to flat, with a corner at the top.
    fn hexes() -> Cells {
        Cells {
            kind: Kind::HexPointyTop,
            cell: 1.0,
        }
    }

    #[test]
    fn a_wider_cell_counts_fewer_cells() {
        // Issue #15: the count is in cells, so a two inch cell halves it.
        let two = Cells {
            kind: Kind::Square,
            cell: 2.0,
        };
        assert!((Rule::Euclid.cells(two, (0.0, 0.0), (6.0, 8.0)) - 5.0).abs() < 1e-9);
        assert!((Rule::Manhattan.cells(two, (0.0, 0.0), (6.0, 8.0)) - 7.0).abs() < 1e-9);
    }

    #[test]
    fn a_hex_grid_offers_the_line_and_the_walk() {
        // The three rules that walk a hex grid agree, so the DM picks
        // between two. Issue #15.
        let offered = Rule::all_on(hexes(), Rule::Pathfinder);
        assert_eq!(offered, vec![Rule::Euclid, Rule::Pathfinder]);
        // The rule the DM holds stands for the walk, so a grid of squares
        // later finds Pathfinder where they left it.
        assert_eq!(
            Rule::Pathfinder.name_on(hexes()),
            Rule::Fifth.name_on(hexes())
        );
        assert_eq!(Rule::Euclid.name_on(hexes()), Rule::Euclid.name());
    }

    #[test]
    fn a_dm_on_the_straight_line_walks_by_the_common_rule() {
        // Nothing remembers a walk, so D&D 5e stands in.
        assert_eq!(
            Rule::all_on(hexes(), Rule::Euclid),
            vec![Rule::Euclid, Rule::Fifth]
        );
    }

    #[test]
    fn a_grid_of_squares_offers_every_rule() {
        assert_eq!(Rule::all_on(inch(), Rule::Euclid), Rule::all().to_vec());
        for rule in Rule::all() {
            assert_eq!(rule.name_on(inch()), rule.name());
        }
    }

    #[test]
    fn a_hex_grid_has_no_diagonal_to_price() {
        // Every neighbor of a hex is one step, so the three rules that
        // walk the grid agree. Issue #15.
        let (from, to) = ((0.0, 0.0), (3.0, 0.0));
        let walked: Vec<f64> = [Rule::Manhattan, Rule::Fifth, Rule::Pathfinder]
            .iter()
            .map(|rule| rule.cells(hexes(), from, to))
            .collect();
        assert!((walked[0] - 3.0).abs() < 1e-9, "{walked:?}");
        assert!(walked.iter().all(|step| (step - walked[0]).abs() < 1e-9));
        // The straight line stays the straight line on any grid.
        assert!((Rule::Euclid.cells(hexes(), from, to) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_hex_step_costs_one_whichever_way_it_goes() {
        // The six neighbors of the hex at the origin all cost one.
        let size = 1.0 / f64::sqrt(3.0);
        for turn in 0..6 {
            let angle = std::f64::consts::PI / 3.0 * f64::from(turn) + std::f64::consts::FRAC_PI_6;
            let to = (
                f64::sqrt(3.0) * size * angle.cos(),
                f64::sqrt(3.0) * size * angle.sin(),
            );
            let steps = Rule::Fifth.cells(hexes(), (0.0, 0.0), to);
            assert!((steps - 1.0).abs() < 1e-9, "turn {turn} cost {steps}");
        }
    }

    #[test]
    fn each_rule_counts_the_cells_its_own_way() {
        let (from, to) = ((0.0, 0.0), (3.0, 4.0));
        assert!((Rule::Euclid.cells(inch(), from, to) - 5.0).abs() < 1e-9);
        assert!((Rule::Manhattan.cells(inch(), from, to) - 7.0).abs() < 1e-9);
        assert!((Rule::Fifth.cells(inch(), from, to) - 4.0).abs() < 1e-9);
        // Three diagonals and one straight step: 3 + 1 + one second
        // diagonal, which costs one more.
        assert!((Rule::Pathfinder.cells(inch(), from, to) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn a_straight_run_reads_the_same_under_every_rule() {
        let (from, to) = ((1.0, 1.0), (1.0, 6.0));
        for rule in Rule::all() {
            assert!(
                (rule.cells(inch(), from, to) - 5.0).abs() < 1e-9,
                "{rule:?}"
            );
        }
    }

    #[test]
    fn a_pair_of_diagonals_costs_three_under_pathfinder() {
        let count = Rule::Pathfinder.cells(inch(), (0.0, 0.0), (2.0, 2.0));
        assert!((count - 3.0).abs() < 1e-9, "{count}");
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
