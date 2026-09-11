//! The grid of the table: what shape a cell is, and where its center sits.
//!
//! DESIGN.md 5.1 and PLAN.md 4. This module holds the math alone. It runs
//! on no GPU, so a test asks it about a point and reads the answer. The
//! shader in `layer` draws the same grid from the same numbers.
//!
//! A hex grid takes its size flat to flat, because a DM measures a map
//! that way. The circumradius is `size = cell / sqrt(3)`.

// Rust guideline compliant 2026-02-21

mod layer;

pub use layer::{GridLayer, Line};

use serde::{Deserialize, Serialize};

/// The smallest and the largest cell the DM may ask for, in inches.
///
/// A quarter inch is a battle map of small squares. Ten inches is a hex
/// crawl where one cell is a day of travel. PLAN.md 4 permits every value
/// between.
pub const MIN_CELL: f64 = 0.25;
/// See [`MIN_CELL`].
pub const MAX_CELL: f64 = 10.0;

/// How wide a cell is when the DM chose nothing, in inches.
///
/// One inch is one cell at 100 percent zoom, which is the square a battle
/// map is drawn on. DESIGN.md 5.1.
pub const DEFAULT_CELL: f64 = 1.0;

/// What shape the cells of the grid are. DESIGN.md 9.2, PLAN.md 4.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Squares of `cell` inches. The grid every battle map is drawn on.
    #[default]
    Square,
    /// Hexes with a corner at the top, in rows that run across.
    HexPointyTop,
    /// Hexes with a flat edge at the top, in columns that run down.
    HexFlatTop,
    /// No lines at all. The snap goes with them.
    None,
}

impl Kind {
    /// Whether this kind draws hexes.
    pub fn is_hex(self) -> bool {
        matches!(self, Self::HexPointyTop | Self::HexFlatTop)
    }

    /// Whether this kind holds cells the snap can reach.
    pub fn snaps(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// What shape the cells of the grid are, and how wide. PLAN.md 4.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cells {
    /// What shape one cell is.
    pub kind: Kind,
    /// How wide one cell is, in inches. A hex measures flat to flat.
    pub cell: f64,
}

impl Default for Cells {
    fn default() -> Self {
        Self {
            kind: Kind::default(),
            cell: DEFAULT_CELL,
        }
    }
}

impl Cells {
    /// How wide one cell is, in inches, and never a width that breaks the
    /// math.
    ///
    /// A config file is text a DM can edit, so a cell of nothing, a cell
    /// of less than nothing, and a cell of infinity all reach here. Each
    /// one reads as the cell the program draws when the DM chose nothing,
    /// so the lines on the screen and the snap under them agree.
    pub fn width(self) -> f64 {
        if self.cell.is_finite() && self.cell > 0.0 {
            self.cell
        } else {
            DEFAULT_CELL
        }
    }

    /// How many cells a measure of `inches` covers.
    ///
    /// A stroke holds inches, because the canvas is inches. A panel and a
    /// label say cells, because a DM counts cells. Issue #15.
    pub fn in_cells(self, inches: f64) -> f64 {
        inches / self.width()
    }

    /// How many inches a measure of `cells` covers. See [`Cells::in_cells`].
    pub fn in_inches(self, cells: f64) -> f64 {
        cells * self.width()
    }

    /// The point of the grid nearest `at`, in inches.
    ///
    /// A square grid answers with the corner of a cell, because a map and
    /// a stroke both line up on the corners of squares. A hex grid
    /// answers with the middle of a hex, because a hex has no corner that
    /// two cells share the way squares do. Issue #15.
    ///
    /// A grid of no cells gives `at` back. A cell of no width reads as
    /// the cell of [`Cells::width`], so the snap holds to the lines the
    /// grid draws.
    pub fn snap(self, at: (f64, f64)) -> (f64, f64) {
        if !self.kind.snaps() {
            return at;
        }
        let cell = self.width();
        match self.kind {
            Kind::Square => ((at.0 / cell).round() * cell, (at.1 / cell).round() * cell),
            Kind::HexPointyTop | Kind::HexFlatTop => center_of(self.kind, cell, at),
            Kind::None => at,
        }
    }

    /// How many hexes a figure walks from `from` to `to`.
    ///
    /// A hex grid has no diagonal: every neighbor is one step away. So the
    /// count is the cube distance, which is the longest of the three cube
    /// coordinates the two hexes differ by. Issue #15.
    ///
    /// A grid of squares has diagonals and a rule that says what they
    /// cost, so it gives `None` and leaves the count to that rule.
    pub fn hex_steps(self, from: (f64, f64), to: (f64, f64)) -> Option<f64> {
        let cell = self.width();
        let (from_q, from_r) = axial(self.kind, cell, from)?;
        let (to_q, to_r) = axial(self.kind, cell, to)?;
        let (q, r) = (to_q - from_q, to_r - from_r);
        let s = -q - r;
        Some(q.abs().max(r.abs()).max(s.abs()))
    }
}

/// Which hex holds `at`, as whole axial coordinates.
///
/// The point becomes axial coordinates and rounds to the nearest hex. Red
/// Blob Games sets out the same steps. A kind that is not a hex has no
/// hex to name, so it gives `None`.
fn axial(kind: Kind, cell: f64, at: (f64, f64)) -> Option<(f64, f64)> {
    // The distance from the middle of a hex to a corner. A hex measured
    // `cell` flat to flat stands `cell / sqrt(3)` tall to its corner.
    let size = cell / f64::sqrt(3.0);
    let (x, y) = (at.0 / size, at.1 / size);
    let (q, r) = match kind {
        Kind::HexPointyTop => (f64::sqrt(3.0) / 3.0 * x - y / 3.0, 2.0 / 3.0 * y),
        Kind::HexFlatTop => (2.0 / 3.0 * x, -x / 3.0 + f64::sqrt(3.0) / 3.0 * y),
        Kind::Square | Kind::None => return None,
    };
    Some(rounded(q, r))
}

/// The middle of the hex that holds `at`, in inches.
///
/// A kind that is not a hex gives `at` back, because it has no hex to
/// name.
fn center_of(kind: Kind, cell: f64, at: (f64, f64)) -> (f64, f64) {
    let size = cell / f64::sqrt(3.0);
    let Some((q, r)) = axial(kind, cell, at) else {
        return at;
    };
    match kind {
        Kind::HexPointyTop => (size * f64::sqrt(3.0) * (q + r / 2.0), size * 1.5 * r),
        Kind::HexFlatTop => (size * 1.5 * q, size * f64::sqrt(3.0) * (q / 2.0 + r)),
        Kind::Square | Kind::None => at,
    }
}

/// The nearest whole hex to a fractional axial coordinate.
///
/// A hex holds three cube coordinates that add to zero. Each one rounds,
/// and the one that moved furthest takes what the other two left over, so
/// the three still add to zero.
fn rounded(q: f64, r: f64) -> (f64, f64) {
    let s = -q - r;
    let (mut whole_q, mut whole_r, whole_s) = (q.round(), r.round(), s.round());
    let (off_q, off_r, off_s) = (
        (whole_q - q).abs(),
        (whole_r - r).abs(),
        (whole_s - s).abs(),
    );
    if off_q > off_r && off_q > off_s {
        whole_q = -whole_r - whole_s;
    } else if off_r > off_s {
        whole_r = -whole_q - whole_s;
    }
    (whole_q, whole_r)
}

#[cfg(test)]
mod tests {
    use super::{Cells, DEFAULT_CELL, Kind};

    /// A grid of this shape, with cells this wide.
    fn cells(kind: Kind, cell: f64) -> Cells {
        Cells { kind, cell }
    }

    /// How close two points must stand to count as the same point.
    const NEAR: f64 = 1e-9;

    fn same(left: (f64, f64), right: (f64, f64)) -> bool {
        (left.0 - right.0).abs() < NEAR && (left.1 - right.1).abs() < NEAR
    }

    #[test]
    fn a_square_grid_takes_a_point_to_the_nearest_corner() {
        assert!(same(cells(Kind::Square, 1.0).snap((5.3, 6.6)), (5.0, 7.0)));
        assert!(same(
            cells(Kind::Square, 1.0).snap((-5.3, -6.6)),
            (-5.0, -7.0)
        ));
    }

    #[test]
    fn a_square_corner_stays_where_it_stands() {
        let at = (5.0, 7.0);
        assert!(same(cells(Kind::Square, 1.0).snap(at), at));
    }

    #[test]
    fn a_square_grid_of_half_an_inch_holds_the_half_marks() {
        assert!(same(cells(Kind::Square, 0.5).snap((5.3, 6.6)), (5.5, 6.5)));
    }

    #[test]
    fn no_grid_leaves_every_point_where_it_stands() {
        let at = (5.3, 6.6);
        assert!(same(cells(Kind::None, 1.0).snap(at), at));
    }

    #[test]
    fn a_measure_turns_between_inches_and_cells() {
        // Issue #15: a four inch burst is two cells when a cell is two
        // inches, and four cells when a cell is one.
        let two = cells(Kind::Square, 2.0);
        assert!((two.in_cells(4.0) - 2.0).abs() < NEAR);
        assert!((two.in_inches(2.0) - 4.0).abs() < NEAR);
        let one = cells(Kind::Square, DEFAULT_CELL);
        assert!((one.in_cells(4.0) - 4.0).abs() < NEAR);
        // A hex grid measures flat to flat, so it turns the same way.
        let hex = cells(Kind::HexPointyTop, 2.0);
        assert!((hex.in_cells(4.0) - 2.0).abs() < NEAR);
    }

    #[test]
    fn a_measure_comes_back_from_its_own_turn() {
        // The panel reads a field in cells and writes it back in inches,
        // so a value the DM never touched must not drift.
        for cell in [0.25, 1.0, 2.5, 10.0, 0.0, f64::NAN] {
            let grid = cells(Kind::Square, cell);
            for inches in [0.5, 4.0, 37.25] {
                let back = grid.in_inches(grid.in_cells(inches));
                assert!((back - inches).abs() < NEAR, "{cell} {inches} -> {back}");
            }
        }
    }

    #[test]
    fn a_cell_that_breaks_the_math_reads_as_the_default_cell() {
        // A config file is text a DM can edit. The grid draws the default
        // cell under each of these, so the snap holds to the same lines.
        let at = (5.3, 6.6);
        let sound = cells(Kind::Square, DEFAULT_CELL).snap(at);
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                (cells(Kind::Square, bad).width() - DEFAULT_CELL).abs() < NEAR,
                "{bad}"
            );
            assert!(same(cells(Kind::Square, bad).snap(at), sound), "{bad}");
            // A hex grid must not divide by it either.
            let hex = cells(Kind::HexPointyTop, bad).snap(at);
            assert!(hex.0.is_finite() && hex.1.is_finite(), "{bad} gave {hex:?}");
        }
        // A cell the DM really chose comes through untouched.
        assert!((cells(Kind::Square, 2.5).width() - 2.5).abs() < NEAR);
    }

    #[test]
    fn a_hex_grid_holds_a_cell_at_the_origin() {
        for kind in [Kind::HexPointyTop, Kind::HexFlatTop] {
            assert!(same(cells(kind, DEFAULT_CELL).snap((0.0, 0.0)), (0.0, 0.0)));
            // A point just off the middle comes back to the same middle.
            assert!(same(cells(kind, DEFAULT_CELL).snap((0.1, 0.1)), (0.0, 0.0)));
        }
    }

    #[test]
    fn a_pointy_top_row_runs_one_cell_across() {
        // Flat to flat is the width of a pointy top hex, so its neighbor
        // to the right stands one whole cell away.
        assert!(same(
            cells(Kind::HexPointyTop, 1.0).snap((1.0, 0.0)),
            (1.0, 0.0)
        ));
        assert!(same(
            cells(Kind::HexPointyTop, 1.0).snap((0.9, 0.05)),
            (1.0, 0.0)
        ));
    }

    #[test]
    fn a_flat_top_column_runs_one_cell_down() {
        // Flat to flat is the height of a flat top hex.
        assert!(same(
            cells(Kind::HexFlatTop, 1.0).snap((0.0, 1.0)),
            (0.0, 1.0)
        ));
        assert!(same(
            cells(Kind::HexFlatTop, 1.0).snap((0.05, 0.9)),
            (0.0, 1.0)
        ));
    }

    #[test]
    fn a_hex_center_stays_where_it_stands() {
        // Every center the snap gives back is a center already, so a
        // second snap moves nothing. A drag that settles twice holds.
        for kind in [Kind::HexPointyTop, Kind::HexFlatTop] {
            for at in [(3.7, -2.4), (-8.1, 5.9), (0.4, 0.4), (12.0, 12.0)] {
                let once = cells(kind, 1.0).snap(at);
                assert!(same(cells(kind, 1.0).snap(once), once), "{kind:?} {at:?}");
            }
        }
    }

    #[test]
    fn a_hex_snap_never_moves_a_point_further_than_a_cell() {
        // The furthest a point can sit from the middle of its hex is the
        // circumradius, which is `cell / sqrt(3)`.
        let reach = 1.0 / f64::sqrt(3.0) + NEAR;
        for kind in [Kind::HexPointyTop, Kind::HexFlatTop] {
            for step in 0..40 {
                let at = (f64::from(step) * 0.37 - 7.0, f64::from(step) * 0.53 - 5.0);
                let center = cells(kind, 1.0).snap(at);
                let gap = f64::hypot(center.0 - at.0, center.1 - at.1);
                assert!(gap <= reach, "{kind:?} {at:?} moved {gap}");
            }
        }
    }

    #[test]
    fn a_bigger_cell_gives_a_bigger_hex() {
        // Two inches flat to flat puts the neighbor two inches away.
        assert!(same(
            cells(Kind::HexPointyTop, 2.0).snap((2.0, 0.0)),
            (2.0, 0.0)
        ));
        assert!(same(
            cells(Kind::HexFlatTop, 2.0).snap((0.0, 2.0)),
            (0.0, 2.0)
        ));
    }
}
