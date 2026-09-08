//! The DM camera: which part of the canvas the DM window shows.

// Rust guideline compliant 2026-02-21

/// The closest the DM camera may look. Below this a map is a few pixels.
pub const MIN_PIXELS_PER_INCH: f64 = 2.0;

/// The furthest the DM camera may look in. One inch fills a small window.
pub const MAX_PIXELS_PER_INCH: f64 = 400.0;

/// A rectangle of the window, in pixels.
///
/// The rail and the settings panel take their share of the window, so the
/// part of it that shows the canvas is not the whole of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    /// The top-left corner of the rectangle.
    pub min: (f64, f64),
    /// The width and the height of the rectangle.
    pub size: (f64, f64),
}

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

    /// The camera after the pointer dragged the canvas by `delta` pixels.
    ///
    /// The world goes with the pointer, so the camera looks the other way.
    pub fn panned(self, delta: (f64, f64)) -> Self {
        Self {
            center: (
                self.center.0 - delta.0 / self.pixels_per_inch,
                self.center.1 - delta.1 / self.pixels_per_inch,
            ),
            ..self
        }
    }

    /// The camera after one zoom step under the pointer at `screen`.
    ///
    /// The world point under the pointer stays under it, so the DM zooms
    /// into what the pointer is on. The zoom stops at the ends of its range.
    pub fn zoomed_at(self, screen: (f64, f64), factor: f64, viewport: (u32, u32)) -> Self {
        let world = self.screen_to_world(screen, viewport);
        let pixels_per_inch =
            (self.pixels_per_inch * factor).clamp(MIN_PIXELS_PER_INCH, MAX_PIXELS_PER_INCH);
        let (half_w, half_h) = (f64::from(viewport.0) / 2.0, f64::from(viewport.1) / 2.0);
        Self {
            center: (
                world.0 - (screen.0 - half_w) / pixels_per_inch,
                world.1 - (screen.1 - half_h) / pixels_per_inch,
            ),
            pixels_per_inch,
        }
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

/// The camera that puts a world rectangle in the middle of `area`.
///
/// `world_size` is the width and the height of the rectangle in inches.
/// `margin` is the share of `area` the rectangle takes, so 0.9 leaves a
/// tenth of it around the edge. The side that runs out first decides.
pub fn fit(
    world_center: (f64, f64),
    world_size: (f64, f64),
    area: Area,
    viewport: (u32, u32),
    margin: f64,
) -> Camera {
    let fits = |along: f64, across: f64| {
        if across > 0.0 {
            along * margin / across
        } else {
            MAX_PIXELS_PER_INCH
        }
    };
    let pixels_per_inch = fits(area.size.0, world_size.0)
        .min(fits(area.size.1, world_size.1))
        .clamp(MIN_PIXELS_PER_INCH, MAX_PIXELS_PER_INCH);
    // The camera holds its center in the middle of the window, and the
    // middle of the area is somewhere else, so the center moves by the
    // distance between the two.
    let (half_w, half_h) = (f64::from(viewport.0) / 2.0, f64::from(viewport.1) / 2.0);
    let middle = (
        area.min.0 + area.size.0 / 2.0,
        area.min.1 + area.size.1 / 2.0,
    );
    Camera {
        center: (
            world_center.0 - (middle.0 - half_w) / pixels_per_inch,
            world_center.1 - (middle.1 - half_h) / pixels_per_inch,
        ),
        pixels_per_inch,
    }
}

#[cfg(test)]
mod tests {
    use super::{Area, Camera, MAX_PIXELS_PER_INCH, MIN_PIXELS_PER_INCH, fit};

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

    /// The whole of a 1000 by 800 window.
    const WHOLE: Area = Area {
        min: (0.0, 0.0),
        size: (1000.0, 800.0),
    };

    #[test]
    fn a_pan_carries_the_world_with_the_pointer() {
        let camera = Camera {
            center: (0.0, 0.0),
            pixels_per_inch: 50.0,
        };
        // The pointer goes 100 pixels right, so the world goes right with it
        // and the camera looks 2 inches further left.
        let panned = camera.panned((100.0, 0.0));
        assert!(close(panned.center, (-2.0, 0.0)));
        assert!((panned.pixels_per_inch - 50.0).abs() < 1e-9);
    }

    #[test]
    fn a_zoom_holds_the_world_under_the_pointer_still() {
        let camera = Camera {
            center: (3.0, -2.0),
            pixels_per_inch: 50.0,
        };
        let viewport = (1000, 800);
        let under = camera.screen_to_world((200.0, 700.0), viewport);
        let zoomed = camera.zoomed_at((200.0, 700.0), 1.4, viewport);
        assert!(close(
            zoomed.screen_to_world((200.0, 700.0), viewport),
            under
        ));
        assert!((zoomed.pixels_per_inch - 70.0).abs() < 1e-9);
    }

    #[test]
    fn a_zoom_stops_at_the_ends_of_its_range() {
        let camera = Camera {
            center: (0.0, 0.0),
            pixels_per_inch: 50.0,
        };
        let far = camera.zoomed_at((500.0, 400.0), 0.000_01, (1000, 800));
        assert!((far.pixels_per_inch - MIN_PIXELS_PER_INCH).abs() < 1e-9);
        let near = camera.zoomed_at((500.0, 400.0), 100_000.0, (1000, 800));
        assert!((near.pixels_per_inch - MAX_PIXELS_PER_INCH).abs() < 1e-9);
    }

    #[test]
    fn a_fit_puts_the_whole_rectangle_on_the_screen() {
        // A 20 by 10 inch box in a 1000 by 800 window, with a tenth of the
        // window left over: the width decides, at 45 pixels to the inch.
        let camera = fit((5.0, 5.0), (20.0, 10.0), WHOLE, (1000, 800), 0.9);
        assert!((camera.pixels_per_inch - 45.0).abs() < 1e-9);
        assert!(close(camera.center, (5.0, 5.0)));
        let corner = camera.world_to_screen((-5.0, 0.0), (1000, 800));
        assert!(corner.0 > 0.0 && corner.1 > 0.0);
    }

    #[test]
    fn a_fit_centers_on_the_area_it_is_given() {
        // The canvas sits right of a 200 pixel rail. The box must land in
        // the middle of the canvas, not in the middle of the window.
        let canvas = Area {
            min: (200.0, 0.0),
            size: (800.0, 800.0),
        };
        let camera = fit((5.0, 5.0), (20.0, 10.0), canvas, (1000, 800), 0.9);
        let middle = camera.world_to_screen((5.0, 5.0), (1000, 800));
        assert!(close(middle, (600.0, 400.0)));
    }
}
