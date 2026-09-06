//! The pointer disc drawn on the TV window instead of the system pointer.

// Rust guideline compliant 2026-02-21

/// One vertex: clip x, clip y, then local x and y in `-1..=1` across the disc.
pub type DiscVertex = [f32; 4];

/// The two triangles that cover a disc of `radius` pixels around `center`.
///
/// `center` is in window pixels with the origin at the top left. The clip
/// coordinates put the origin in the middle with y up, as the GPU expects.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "drawn by the pointer pass in the next commit")
)]
pub fn disc_quad(center: (f32, f32), radius: f32, viewport: (u32, u32)) -> [DiscVertex; 6] {
    let (width, height) = (viewport.0 as f32, viewport.1 as f32);
    let corner = |lx: f32, ly: f32| -> DiscVertex {
        let px = center.0 + lx * radius;
        let py = center.1 + ly * radius;
        [px / width * 2.0 - 1.0, 1.0 - py / height * 2.0, lx, ly]
    };
    [
        corner(-1.0, -1.0),
        corner(1.0, -1.0),
        corner(-1.0, 1.0),
        corner(-1.0, 1.0),
        corner(1.0, -1.0),
        corner(1.0, 1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::disc_quad;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn maps_the_top_left_pixel_to_the_top_left_clip_corner() {
        let quad = disc_quad((0.0, 0.0), 10.0, (100, 50));
        // The quad's top-left vertex is one radius up and left of the center.
        let top_left = quad.iter().find(|v| v[2] < 0.0 && v[3] < 0.0).unwrap();
        assert!(close(top_left[0], -1.0 - 0.2), "x {}", top_left[0]);
        assert!(close(top_left[1], 1.0 + 0.4), "y {}", top_left[1]);
    }

    #[test]
    fn centers_the_quad_on_the_pixel() {
        let quad = disc_quad((50.0, 25.0), 5.0, (100, 50));
        let (mut sx, mut sy) = (0.0, 0.0);
        for v in &quad {
            sx += v[0];
            sy += v[1];
        }
        assert!(close(sx / 6.0, 0.0));
        assert!(close(sy / 6.0, 0.0));
    }

    #[test]
    fn spans_two_triangles_with_unit_local_coordinates() {
        let quad = disc_quad((50.0, 25.0), 5.0, (100, 50));
        assert_eq!(quad.len(), 6);
        assert!(quad.iter().all(|v| v[2].abs() == 1.0 && v[3].abs() == 1.0));
        // All four corners are present.
        for corner in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            assert!(quad.iter().any(|v| (v[2], v[3]) == corner));
        }
    }
}
