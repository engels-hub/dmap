//! The DM camera: which part of the canvas the DM window shows.

// Rust guideline compliant 2026-02-21

/// Where the DM window looks at the canvas.
///
/// World units are inches. Screen units are window pixels with the origin at
/// the top left and y down. The camera center sits in the middle of the view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// World point in the middle of the view, in inches.
    pub center: (f64, f64),
    /// Zoom: how many window pixels one inch takes.
    pub pixels_per_inch: f64,
}

impl Camera {
    /// Maps a window pixel to a world point for a view of `viewport` pixels.
    pub fn screen_to_world(&self, screen: (f64, f64), viewport: (u32, u32)) -> (f64, f64) {
        let (half_w, half_h) = (f64::from(viewport.0) / 2.0, f64::from(viewport.1) / 2.0);
        (
            self.center.0 + (screen.0 - half_w) / self.pixels_per_inch,
            self.center.1 + (screen.1 - half_h) / self.pixels_per_inch,
        )
    }

    /// Maps a world point to a window pixel for a view of `viewport` pixels.
    pub fn world_to_screen(&self, world: (f64, f64), viewport: (u32, u32)) -> (f64, f64) {
        let (half_w, half_h) = (f64::from(viewport.0) / 2.0, f64::from(viewport.1) / 2.0);
        (
            half_w + (world.0 - self.center.0) * self.pixels_per_inch,
            half_h + (world.1 - self.center.1) * self.pixels_per_inch,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Camera;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn the_view_center_is_the_camera_center() {
        let camera = Camera {
            center: (3.0, -2.0),
            pixels_per_inch: 50.0,
        };
        assert!(close(
            camera.screen_to_world((400.0, 300.0), (800, 600)),
            (3.0, -2.0)
        ));
        assert!(close(
            camera.world_to_screen((3.0, -2.0), (800, 600)),
            (400.0, 300.0)
        ));
    }

    #[test]
    fn one_inch_is_pixels_per_inch_pixels_and_y_points_down() {
        let camera = Camera {
            center: (0.0, 0.0),
            pixels_per_inch: 50.0,
        };
        assert!(close(
            camera.world_to_screen((1.0, 1.0), (800, 600)),
            (450.0, 350.0)
        ));
        assert!(close(
            camera.screen_to_world((350.0, 250.0), (800, 600)),
            (-1.0, -1.0)
        ));
    }

    #[test]
    fn screen_and_world_round_trip() {
        let camera = Camera {
            center: (12.5, 7.25),
            pixels_per_inch: 37.0,
        };
        let world = camera.screen_to_world((123.0, 456.0), (1440, 900));
        assert!(close(
            camera.world_to_screen(world, (1440, 900)),
            (123.0, 456.0)
        ));
    }
}
