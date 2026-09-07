//! Math for the Select tool: hit tests, handles, snapping.

// Rust guideline compliant 2026-02-21

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

/// Moves `center` so the map's top-left extent sits on whole inches.
///
/// `corners` are the map's world corners, turned as drawn. The map's own
/// grid starts at its corner, so the top-left of its bounding box is what
/// should land on the canvas grid. For a quarter turn that is a real corner.
pub fn snap_corner(center: (f64, f64), corners: &[(f64, f64); 4]) -> (f64, f64) {
    let min_x = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let min_y = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    (
        center.0 + (min_x.round() - min_x),
        center.1 + (min_y.round() - min_y),
    )
}

/// Size factor for a corner drag: how far the cursor is from the center,
/// relative to where the handle started. Never below `MIN_SCALE`.
pub fn scale_from_drag(center: (f64, f64), start: (f64, f64), cursor: (f64, f64)) -> f64 {
    /// A map cannot shrink to nothing, or it could never be grabbed again.
    const MIN_SCALE: f64 = 0.01;
    let start_distance = distance(center, start);
    if start_distance <= 0.0 {
        return 1.0;
    }
    (distance(center, cursor) / start_distance).max(MIN_SCALE)
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

/// The index one step forward (`toward_end`) or backward in a list of
/// `len` items, or `None` when already at that end. Used to reorder maps
/// in their draw order, which is also their stacking order: later draws
/// on top.
pub fn reorder(index: usize, len: usize, toward_end: bool) -> Option<usize> {
    if toward_end {
        (index + 1 < len).then_some(index + 1)
    } else {
        index.checked_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    use super::{
        ROTATION_STEP, edge_midpoint, hit_test, pick_handle, reorder, rotation_from_drag,
        rotation_handle, scale_from_drag, snap_corner,
    };

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
        assert_eq!(snap_corner((5.3, 6.6), &corners), (5.0, 7.0));
        // Already on the grid: no change.
        let corners = [(2.0, 5.0), (8.0, 5.0), (8.0, 9.0), (2.0, 9.0)];
        assert_eq!(snap_corner((5.0, 7.0), &corners), (5.0, 7.0));
    }

    #[test]
    fn snapping_a_turned_map_uses_its_bounding_box() {
        // The same 6 by 4 map a quarter turn later: 4 wide, 6 tall.
        // Its top-left extent is at (3.3, 3.6) and moves to (3, 4).
        let corners = [(7.3, 3.6), (7.3, 9.6), (3.3, 9.6), (3.3, 3.6)];
        assert_eq!(snap_corner((5.3, 6.6), &corners), (5.0, 7.0));
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
    fn reordering_moves_one_step_forward_or_backward() {
        assert_eq!(reorder(0, 3, true), Some(1));
        assert_eq!(reorder(1, 3, true), Some(2));
        assert_eq!(reorder(1, 3, false), Some(0));
    }

    #[test]
    fn reordering_stops_at_the_ends() {
        assert_eq!(reorder(2, 3, true), None);
        assert_eq!(reorder(0, 3, false), None);
        assert_eq!(reorder(0, 1, true), None);
    }

    #[test]
    fn the_nearest_handle_within_reach_is_picked() {
        let handles = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)];
        assert_eq!(pick_handle((103.0, 2.0), &handles, 8.0), Some(1));
        assert_eq!(pick_handle((50.0, 50.0), &handles, 8.0), None);
    }
}
