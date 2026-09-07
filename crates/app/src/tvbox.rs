//! The TV box: the part of the canvas that the TV window shows.

// Rust guideline compliant 2026-02-21

use serde::{Deserialize, Serialize};

use crate::camera::Camera;

/// Width of a new box, in inches.
///
/// A 55 inch TV is about 48 inches wide. At 4K that gives 80 pixels to the
/// inch, which is the zoom the TV used before the box existed.
const DEFAULT_WIDTH: f64 = 48.0;

/// The smallest box, in inches. A smaller box zooms past any use.
const MIN_WIDTH: f64 = 1.0;

/// The largest box, in inches. Wider than any table.
const MAX_WIDTH: f64 = 1000.0;

/// The rectangle of the canvas that the TV shows.
///
/// The DM drags this box on the DM screen. The TV camera follows it. The
/// height is not stored: the box always has the shape of the TV.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TvBox {
    /// World point in the middle of the box, in inches.
    pub center: (f64, f64),
    /// Width of the box, in inches.
    pub width: f64,
}

impl Default for TvBox {
    fn default() -> Self {
        Self {
            center: (0.0, 0.0),
            width: DEFAULT_WIDTH,
        }
    }
}

impl TvBox {
    /// The height of the box in inches, on a TV of `viewport` pixels.
    pub fn height(&self, viewport: (u32, u32)) -> f64 {
        if viewport.0 == 0 {
            return self.width;
        }
        self.width * f64::from(viewport.1) / f64::from(viewport.0)
    }

    /// The same box, with a width the camera can divide by.
    ///
    /// A project file is text that a DM can edit by hand, so the width that
    /// comes back is not always a number this program can draw.
    pub fn clamped(self) -> Self {
        Self {
            width: clamp_width(self.width),
            ..self
        }
    }

    /// The camera that shows this box on a TV of `viewport` pixels.
    pub fn camera(&self, viewport: (u32, u32)) -> Camera {
        Camera {
            center: self.center,
            pixels_per_inch: f64::from(viewport.0) / self.width,
        }
    }

    /// The world corners of the box: top-left, top-right, bottom-right,
    /// then bottom-left.
    pub fn corners(&self, viewport: (u32, u32)) -> [(f64, f64); 4] {
        let (half_w, half_h) = (self.width / 2.0, self.height(viewport) / 2.0);
        [
            (self.center.0 - half_w, self.center.1 - half_h),
            (self.center.0 + half_w, self.center.1 - half_h),
            (self.center.0 + half_w, self.center.1 + half_h),
            (self.center.0 - half_w, self.center.1 + half_h),
        ]
    }
}

/// Holds a box width inside the range the program draws.
///
/// A width that is not a number falls back to the default, since a camera
/// cannot divide by it.
pub fn clamp_width(width: f64) -> f64 {
    if width.is_finite() {
        width.clamp(MIN_WIDTH, MAX_WIDTH)
    } else {
        DEFAULT_WIDTH
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_WIDTH, MIN_WIDTH, TvBox, clamp_width};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// A 4K TV, the display this program is built for.
    const UHD: (u32, u32) = (3840, 2160);

    #[test]
    fn the_default_box_fills_a_55_inch_tv_at_true_size() {
        // 48 inches wide over 3840 pixels is 80 pixels to the inch, which is
        // what the TV drew before the box existed.
        let camera = TvBox::default().camera(UHD);
        assert!(close(camera.pixels_per_inch, 80.0));
        assert_eq!(camera.center, (0.0, 0.0));
    }

    #[test]
    fn the_box_takes_the_shape_of_the_tv() {
        let tv_box = TvBox {
            center: (0.0, 0.0),
            width: 32.0,
        };
        assert!(close(tv_box.height(UHD), 18.0));
        // A square display gives a square box.
        assert!(close(tv_box.height((1000, 1000)), 32.0));
    }

    #[test]
    fn the_corners_sit_around_the_center() {
        let tv_box = TvBox {
            center: (10.0, 5.0),
            width: 32.0,
        };
        let corners = tv_box.corners(UHD);
        assert!(close(corners[0].0, -6.0) && close(corners[0].1, -4.0));
        assert!(close(corners[2].0, 26.0) && close(corners[2].1, 14.0));
    }

    #[test]
    fn the_camera_puts_the_corner_of_the_box_in_the_corner_of_the_tv() {
        let tv_box = TvBox {
            center: (10.0, 5.0),
            width: 32.0,
        };
        let screen = tv_box
            .camera(UHD)
            .world_to_screen(tv_box.corners(UHD)[0], UHD);
        assert!(close(screen.0, 0.0) && close(screen.1, 0.0));
    }

    #[test]
    fn a_box_cannot_shrink_to_nothing_or_grow_past_the_canvas() {
        assert!(close(clamp_width(0.0), MIN_WIDTH));
        assert!(close(clamp_width(-5.0), MIN_WIDTH));
        assert!(close(clamp_width(1e9), MAX_WIDTH));
        assert!(close(clamp_width(32.0), 32.0));
    }

    #[test]
    fn a_width_that_is_not_a_number_falls_back_to_the_default() {
        // A hand-edited project file can hold anything. A width of zero
        // would divide the camera by zero and blank the TV.
        assert!(close(clamp_width(f64::NAN), TvBox::default().width));
        assert!(close(clamp_width(f64::INFINITY), TvBox::default().width));
    }

    #[test]
    fn a_loaded_box_is_held_inside_the_range() {
        let loaded = TvBox {
            center: (2.0, 3.0),
            width: 0.0,
        };
        let fixed = loaded.clamped();
        assert!(close(fixed.width, MIN_WIDTH));
        // The center is not touched. Only the width can break the camera.
        assert_eq!(fixed.center, (2.0, 3.0));
    }

    #[test]
    fn a_box_round_trips_through_json() {
        let tv_box = TvBox {
            center: (1.5, -2.0),
            width: 30.0,
        };
        let json = serde_json::to_string(&tv_box).unwrap();
        assert_eq!(serde_json::from_str::<TvBox>(&json).unwrap(), tv_box);
    }
}
