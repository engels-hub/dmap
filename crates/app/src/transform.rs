//! Math for the Select tool: hit tests, handles, snapping.

// Rust guideline compliant 2026-02-21

use crate::grid::Cells;

/// Rotation handle snaps to multiples of this angle: 15 degrees.
pub const ROTATION_STEP: f64 = std::f64::consts::PI / 12.0;

/// Whether `point` lies inside the convex quad `corners` (any winding).
pub fn hit_test(point: (f64, f64), corners: &[(f64, f64); 4]) -> bool {
    let mut positive = false;
    let mut negative = false;
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let cross = (b.0 - a.0) * (point.1 - a.1) - (b.1 - a.1) * (point.0 - a.0);
        positive |= cross > 0.0;
        negative |= cross < 0.0;
    }
    !(positive && negative)
}

/// Moves `center` so the map's top-left extent sits on the map's own grid.
///
/// `corners` are the map's world corners, turned as drawn. The map's own
/// grid starts at its corner, so the top-left of its bounding box is what
/// should land on a whole inch. For a quarter turn that is a real corner.
///
/// `offset` says where that grid starts, in inches past the canvas grid.
/// A map the DM has never placed by hand has an offset of zero and lands
/// on whole inches. A map the DM placed by hand keeps the spot it got, and
/// steps by whole inches from there. See `corner_offset`.
pub fn snap_corner(
    cells: Cells,
    center: (f64, f64),
    corners: &[(f64, f64); 4],
    offset: (f64, f64),
) -> (f64, f64) {
    let (min_x, min_y) = top_left(corners);
    // The offset says which lattice this map steps on, so the corner
    // comes back to the canvas grid, snaps there, and goes out again.
    let on = cells.snap((min_x - offset.0, min_y - offset.1));
    (
        center.0 + (on.0 + offset.0 - min_x),
        center.1 + (on.1 + offset.1 - min_y),
    )
}

/// Where the grid of a map that the DM placed by hand starts.
///
/// This is how far the map's top-left extent sits past the nearest point
/// of the canvas grid. `snap_corner` takes it back, so the map steps by
/// whole cells from the spot the DM gave it. Issue #32.
pub fn corner_offset(cells: Cells, corners: &[(f64, f64); 4]) -> (f64, f64) {
    let (min_x, min_y) = top_left(corners);
    let on = cells.snap((min_x, min_y));
    (min_x - on.0, min_y - on.1)
}

/// The top-left extent of the map's bounding box, in world inches.
fn top_left(corners: &[(f64, f64); 4]) -> (f64, f64) {
    (
        corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min),
        corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min),
    )
}

/// A map cannot shrink to nothing, or it could never be grabbed again.
const MIN_SCALE: f64 = 0.01;

/// Size factor for a corner drag: how far the cursor is from the center,
/// relative to where the handle started. Never below `MIN_SCALE`.
pub fn scale_from_drag(center: (f64, f64), start: (f64, f64), cursor: (f64, f64)) -> f64 {
    let start_distance = distance(center, start);
    if start_distance <= 0.0 {
        return 1.0;
    }
    (distance(center, cursor) / start_distance).max(MIN_SCALE)
}

/// A scale change per `+`/`-` key press: 10%.
const SCALE_STEP: f64 = 1.1;

/// One `+`/`-` key step applied to `scale`. Never below `MIN_SCALE`.
pub fn step_scale(scale: f64, grow: bool) -> f64 {
    if grow {
        scale * SCALE_STEP
    } else {
        (scale / SCALE_STEP).max(MIN_SCALE)
    }
}

/// The map's rotation after the cursor swept around `center` since `start`.
///
/// With `snap`, the resulting angle rounds to a multiple of `ROTATION_STEP`,
/// so a map that starts off-step can get back on it.
pub fn rotation_from_drag(
    start_rotation: f64,
    center: (f64, f64),
    start: (f64, f64),
    cursor: (f64, f64),
    snap: bool,
) -> f64 {
    let angle = |p: (f64, f64)| (p.1 - center.1).atan2(p.0 - center.0);
    let mut swept = angle(cursor) - angle(start);
    // Keep the change in (-pi, pi], so a small move never reads as a full turn.
    if swept > std::f64::consts::PI {
        swept -= std::f64::consts::TAU;
    } else if swept <= -std::f64::consts::PI {
        swept += std::f64::consts::TAU;
    }
    let rotation = start_rotation + swept;
    if snap {
        (rotation / ROTATION_STEP).round() * ROTATION_STEP
    } else {
        rotation
    }
}

/// The smallest grid size a map can have: one image pixel in a cell.
///
/// A cell of zero pixels would make the map infinitely wide.
pub const MIN_GRID_PX: f64 = 1.0;

/// The largest grid size a map can have.
///
/// One cell this big already fills the largest image the GPU accepts.
pub const MAX_GRID_PX: f64 = 4096.0;

/// The image pixels in one grid cell, measured across one cell.
///
/// `a` and `b` are the two cell corners the DM clicked, in world inches.
/// `grid_px` and `scale` are the map's values while it was measured, since
/// together they say how many image pixels one inch of canvas holds.
/// Returns `None` when the result is outside `MIN_GRID_PX` to `MAX_GRID_PX`,
/// which two clicks on the same spot always are.
pub fn grid_px_from_measure(a: (f64, f64), b: (f64, f64), grid_px: f64, scale: f64) -> Option<f64> {
    let measured = distance(a, b) * grid_px / scale;
    (MIN_GRID_PX..=MAX_GRID_PX)
        .contains(&measured)
        .then_some(measured)
}

/// The midpoint of a straight edge. Shared by `rotation_handle` and by the
/// code that draws the line to it, so the two always agree on where the
/// line starts.
pub fn edge_midpoint(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (f64::midpoint(a.0, b.0), f64::midpoint(a.1, b.1))
}

/// Where the rotation handle sits: `offset` past the middle of the top
/// edge, on the outside of the map. Screen space, y down.
pub fn rotation_handle(top_left: (f64, f64), top_right: (f64, f64), offset: f64) -> (f64, f64) {
    let mid = edge_midpoint(top_left, top_right);
    let length = distance(top_left, top_right);
    if length <= 0.0 {
        return (mid.0, mid.1 - offset);
    }
    let dir = (
        (top_right.0 - top_left.0) / length,
        (top_right.1 - top_left.1) / length,
    );
    // The map lies on the right-hand side of the edge, so outside is the
    // left-hand normal.
    (mid.0 + dir.1 * offset, mid.1 - dir.0 * offset)
}

/// The index of the handle nearest to `cursor` within `radius`, if any.
pub fn pick_handle(cursor: (f64, f64), handles: &[(f64, f64)], radius: f64) -> Option<usize> {
    handles
        .iter()
        .enumerate()
        .map(|(i, &handle)| (i, distance(cursor, handle)))
        .filter(|&(_, d)| d <= radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    use super::{
        MAX_GRID_PX, ROTATION_STEP, corner_offset, edge_midpoint, grid_px_from_measure, hit_test,
        pick_handle, rotation_from_drag, rotation_handle, scale_from_drag, snap_corner, step_scale,
    };
    use crate::grid::Cells;

    /// The square inch grid the program draws when the DM changes nothing.
    fn inch() -> Cells {
        Cells::default()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    const SQUARE: [(f64, f64); 4] = [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)];

    #[test]
    fn a_point_inside_a_quad_hits_and_outside_misses() {
        assert!(hit_test((1.0, 1.0), &SQUARE));
        assert!(hit_test((0.1, 1.9), &SQUARE));
        assert!(!hit_test((2.1, 1.0), &SQUARE));
        assert!(!hit_test((-0.1, -0.1), &SQUARE));
    }

    #[test]
    fn hit_test_works_for_a_turned_quad() {
        // A diamond: the square turned 45 degrees around (1, 1).
        let d = [(1.0, -0.414), (2.414, 1.0), (1.0, 2.414), (-0.414, 1.0)];
        assert!(hit_test((1.0, 1.0), &d));
        assert!(!hit_test((0.0, 0.0), &d));
    }

    #[test]
    fn snapping_moves_the_top_left_corner_onto_whole_inches() {
        // A 6 by 4 map centered at (5.3, 6.6): its corner (2.3, 4.6) moves to (2, 5).
        let corners = [(2.3, 4.6), (8.3, 4.6), (8.3, 8.6), (2.3, 8.6)];
        assert_eq!(
            snap_corner(inch(), (5.3, 6.6), &corners, (0.0, 0.0)),
            (5.0, 7.0)
        );
        // Already on the grid: no change.
        let corners = [(2.0, 5.0), (8.0, 5.0), (8.0, 9.0), (2.0, 9.0)];
        assert_eq!(
            snap_corner(inch(), (5.0, 7.0), &corners, (0.0, 0.0)),
            (5.0, 7.0)
        );
    }

    #[test]
    fn snapping_a_turned_map_uses_its_bounding_box() {
        // The same 6 by 4 map a quarter turn later: 4 wide, 6 tall.
        // Its top-left extent is at (3.3, 3.6) and moves to (3, 4).
        let corners = [(7.3, 3.6), (7.3, 9.6), (3.3, 9.6), (3.3, 3.6)];
        assert_eq!(
            snap_corner(inch(), (5.3, 6.6), &corners, (0.0, 0.0)),
            (5.0, 7.0)
        );
    }

    #[test]
    fn scale_follows_the_distance_of_the_cursor_from_the_center() {
        let factor = scale_from_drag((0.0, 0.0), (3.0, 4.0), (6.0, 8.0));
        assert!(close(factor, 2.0));
        let factor = scale_from_drag((1.0, 1.0), (3.0, 1.0), (2.0, 1.0));
        assert!(close(factor, 0.5));
    }

    #[test]
    fn scale_never_collapses_to_zero() {
        assert!(scale_from_drag((0.0, 0.0), (1.0, 0.0), (0.0, 0.0)) > 0.0);
    }

    #[test]
    fn a_key_step_grows_or_shrinks_by_ten_percent() {
        assert!(close(step_scale(1.0, true), 1.1));
        assert!(close(step_scale(1.1, false), 1.0));
    }

    #[test]
    fn key_steps_compound() {
        let grown = step_scale(step_scale(1.0, true), true);
        assert!(close(grown, 1.21));
    }

    #[test]
    fn shrinking_never_collapses_to_zero() {
        let mut scale = 1.0;
        for _ in 0..200 {
            scale = step_scale(scale, false);
        }
        assert!(scale > 0.0);
    }

    #[test]
    fn rotation_is_the_start_angle_plus_what_the_cursor_swept() {
        let turned = rotation_from_drag(0.1, (0.0, 0.0), (1.0, 0.0), (0.0, 1.0), false);
        assert!(close(turned, 0.1 + FRAC_PI_2));
        let turned = rotation_from_drag(0.0, (0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), false);
        assert!(close(turned.abs(), PI));
    }

    #[test]
    fn rotation_snaps_the_resulting_angle_not_the_change() {
        // A map at 7 degrees dragged by 40 degrees ends at 45, not at 7 + 45.
        let start = 7f64.to_radians();
        let cursor = (40f64.to_radians().cos(), 40f64.to_radians().sin());
        let turned = rotation_from_drag(start, (0.0, 0.0), (1.0, 0.0), cursor, true);
        assert!(close(turned, FRAC_PI_4));
        // A tiny drag back with snap on brings a map at 7 degrees to 0.
        let cursor = ((-1f64).to_radians().cos(), (-1f64).to_radians().sin());
        let turned = rotation_from_drag(start, (0.0, 0.0), (1.0, 0.0), cursor, true);
        assert!(close(turned, 0.0));
        assert!(close(ROTATION_STEP, 15f64.to_radians()));
    }

    #[test]
    fn the_rotation_handle_sits_above_the_top_edge() {
        // Screen space, y down: "above" means smaller y.
        let handle = rotation_handle((0.0, 10.0), (20.0, 10.0), 24.0);
        assert!(close(handle.0, 10.0) && close(handle.1, -14.0));
        // A turned edge: the handle moves along the edge's normal.
        let handle = rotation_handle((10.0, 20.0), (10.0, 0.0), 24.0);
        assert!(close(handle.0, -14.0) && close(handle.1, 10.0));
    }

    #[test]
    fn the_rotation_handle_starts_from_the_shared_edge_midpoint_function() {
        // Any caller that draws the connecting line must use the exact same
        // midpoint the handle position itself is built from, so the line
        // always starts precisely where the handle's offset is measured from.
        let (top_left, top_right) = ((12.0, 40.0), (212.0, 96.0));
        let mid = edge_midpoint(top_left, top_right);
        assert!(close(mid.0, 112.0) && close(mid.1, 68.0));
        let handle = rotation_handle(top_left, top_right, 24.0);
        // The handle sits exactly `offset` from `mid`, along the perpendicular.
        let handle_distance = (handle.0 - mid.0).hypot(handle.1 - mid.1);
        assert!(close(handle_distance, 24.0));
    }

    #[test]
    fn the_nearest_handle_within_reach_is_picked() {
        let handles = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)];
        assert_eq!(pick_handle((103.0, 2.0), &handles, 8.0), Some(1));
        assert_eq!(pick_handle((50.0, 50.0), &handles, 8.0), None);
    }
    #[test]
    fn a_measure_across_one_cell_gives_the_image_pixels_in_it() {
        // A map at 100 pixels per cell draws one image pixel per 0.01 inch.
        // Two clicks half an inch apart therefore span 50 image pixels.
        let measured = grid_px_from_measure((1.0, 1.0), (1.5, 1.0), 100.0, 1.0).unwrap();
        assert!(close(measured, 50.0));
    }

    #[test]
    fn a_measure_reads_the_image_through_the_size_the_map_has_now() {
        // The same map at double size: one inch of canvas holds half as many
        // image pixels, so the same half inch spans 25 image pixels.
        let measured = grid_px_from_measure((1.0, 1.0), (1.5, 1.0), 100.0, 2.0).unwrap();
        assert!(close(measured, 25.0));
    }

    #[test]
    fn a_measure_along_a_diagonal_uses_the_true_distance() {
        let measured = grid_px_from_measure((0.0, 0.0), (0.3, 0.4), 100.0, 1.0).unwrap();
        assert!(close(measured, 50.0));
    }

    #[test]
    fn two_clicks_on_the_same_point_measure_nothing() {
        assert_eq!(
            grid_px_from_measure((2.0, 3.0), (2.0, 3.0), 100.0, 1.0),
            None
        );
    }

    #[test]
    fn a_measure_out_of_range_is_refused() {
        // A whole 4096 pixel map dragged across as if it were one cell.
        let too_big = MAX_GRID_PX + 1.0;
        assert_eq!(
            grid_px_from_measure((0.0, 0.0), (too_big / 100.0, 0.0), 100.0, 1.0),
            None
        );
    }
    #[test]
    fn a_map_steps_along_the_grid_its_own_offset_makes() {
        // A map placed by hand at a quarter inch past the grid keeps that
        // quarter inch: its steps run through where the DM left it.
        let corners = [(2.3, 4.6), (8.3, 4.6), (8.3, 8.6), (2.3, 8.6)];
        assert_eq!(
            snap_corner(inch(), (5.3, 6.6), &corners, (0.25, 0.5)),
            (5.25, 6.5)
        );
    }

    #[test]
    fn a_map_already_on_its_own_grid_does_not_move() {
        // The corner at (2.25, 4.5) is one whole inch from the offset itself.
        let corners = [(2.25, 4.5), (8.25, 4.5), (8.25, 8.5), (2.25, 8.5)];
        assert_eq!(
            snap_corner(inch(), (5.25, 6.5), &corners, (0.25, 0.5)),
            (5.25, 6.5)
        );
    }

    #[test]
    fn a_free_move_leaves_the_offset_it_ended_on() {
        // Issue #32: the spot a free move gave the map is the spot the
        // next snap holds it to, whatever fraction it landed on.
        let corners = [(2.3, 4.6), (8.3, 4.6), (8.3, 8.6), (2.3, 8.6)];
        let offset = corner_offset(inch(), &corners);
        let center = (5.3, 6.6);
        assert_eq!(snap_corner(inch(), center, &corners, offset), center);
    }

    #[test]
    fn an_offset_never_reaches_half_a_cell() {
        // The offset is what the nearest point of the grid left over, so
        // it stays inside half a cell on every side of the origin.
        for corners in [
            [(2.3, 4.6), (8.3, 4.6), (8.3, 8.6), (2.3, 8.6)],
            [(-6.075, -2.5), (0.0, -2.5), (0.0, 1.0), (-6.075, 1.0)],
        ] {
            let offset = corner_offset(inch(), &corners);
            assert!(offset.0.abs() <= 0.5 && offset.1.abs() <= 0.5, "{offset:?}");
            // The map sits on the grid that offset makes, so it stays.
            assert_eq!(
                snap_corner(inch(), (0.0, 0.0), &corners, offset),
                (0.0, 0.0)
            );
        }
    }

    #[test]
    fn a_map_on_a_hex_grid_steps_from_where_the_dm_left_it() {
        // Issue #15: a hex grid holds the same promise as a square one.
        let cells = Cells {
            kind: crate::grid::Kind::HexPointyTop,
            cell: 1.0,
        };
        let corners = [(2.3, 4.6), (8.3, 4.6), (8.3, 8.6), (2.3, 8.6)];
        let offset = corner_offset(cells, &corners);
        let center = (5.3, 6.6);
        assert_eq!(snap_corner(cells, center, &corners, offset), center);
    }

    #[test]
    fn no_grid_holds_a_map_wherever_the_dm_drops_it() {
        let cells = Cells {
            kind: crate::grid::Kind::None,
            cell: 1.0,
        };
        let corners = [(2.3, 4.6), (8.3, 4.6), (8.3, 8.6), (2.3, 8.6)];
        let center = (5.3, 6.6);
        assert_eq!(snap_corner(cells, center, &corners, (0.0, 0.0)), center);
    }
}
