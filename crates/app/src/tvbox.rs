//! The TV box: the part of the canvas that the TV window shows.

// Rust guideline compliant 2026-02-21

use serde::{Deserialize, Serialize};

use crate::camera::Camera;

/// The width of the TV screen in inches, until the DM enters its size.
///
/// A 55 inch 16:9 TV is about 48 inches wide. A box this wide draws the
/// world at true size, so this is also the width of a new box. Issue #6
/// lets the DM enter the size of the real TV in its place.
///
/// The pixels of the TV do not appear here on purpose. The TV spreads the
/// box across every pixel it has, so a 1080p TV and a 4K TV of the same
/// size agree on what true size is.
pub const TV_WIDTH_INCHES: f64 = 48.0;

/// The smallest box, in inches. A smaller box zooms past any use.
const MIN_WIDTH: f64 = 1.0;

/// The largest box, in inches. Wider than any table.
const MAX_WIDTH: f64 = 1000.0;

/// The snap window a new project uses, in percent. PLAN.md section 3.2.
pub const DEFAULT_SNAP_PERCENT: f64 = 8.0;

/// The widest snap window a project may hold, in percent. Half of that
/// already reaches from 50 to 150 percent.
pub const MAX_SNAP_PERCENT: f64 = 50.0;

/// Which number the DM gave for the size of the TV. Issue #6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeBy {
    /// The diagonal of the screen, in inches, as the box of the TV says.
    #[default]
    Diagonal,
    /// How many pixels of the screen make one inch.
    PixelsPerInch,
    /// The width of the screen, in inches, as a tape measure says.
    Width,
}

/// The size of the real TV, as the DM entered it. Issue #6.
///
/// The number stays as the DM gave it. A diagonal then still holds when
/// the TV window moves to a screen of another resolution.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TvSize {
    /// Which number `value` is.
    pub by: SizeBy,
    /// The number, in inches or in pixels per inch.
    pub value: f64,
}

impl TvSize {
    /// How many pixels make one inch on a screen of `screen` pixels.
    ///
    /// Gives back `None` for a number the program cannot draw by, such as
    /// a diagonal of zero.
    pub fn pixels_per_inch(self, screen: (u32, u32)) -> Option<f64> {
        let (width, height) = (f64::from(screen.0), f64::from(screen.1));
        let ppi = match self.by {
            SizeBy::Diagonal => width.hypot(height) / self.value,
            SizeBy::PixelsPerInch => self.value,
            SizeBy::Width => width / self.value,
        };
        (ppi.is_finite() && ppi > 0.0 && width > 0.0).then_some(ppi)
    }

    /// The size that gives `ppi` on a screen of `screen` pixels, said as
    /// the number `by` names.
    pub fn of(by: SizeBy, ppi: f64, screen: (u32, u32)) -> Self {
        let (width, height) = (f64::from(screen.0), f64::from(screen.1));
        let value = match by {
            SizeBy::Diagonal => width.hypot(height) / ppi,
            SizeBy::PixelsPerInch => ppi,
            SizeBy::Width => width / ppi,
        };
        Self { by, value }
    }
}

/// How many pixels make one inch on the TV.
///
/// `screen` is the resolution of the TV, and `viewport` the pixels of the
/// TV window. With no size entered, the window is [`TV_WIDTH_INCHES`]
/// wide.
pub fn tv_pixels_per_inch(size: Option<TvSize>, screen: (u32, u32), viewport: (u32, u32)) -> f64 {
    size.and_then(|size| size.pixels_per_inch(screen))
        .unwrap_or_else(|| f64::from(viewport.0.max(1)) / TV_WIDTH_INCHES)
}

/// The width of the TV window in inches: the width of a box at true size.
pub fn tv_width(size: Option<TvSize>, screen: (u32, u32), viewport: (u32, u32)) -> f64 {
    if size.is_none() || viewport.0 == 0 {
        return TV_WIDTH_INCHES;
    }
    clamp_width(f64::from(viewport.0) / tv_pixels_per_inch(size, screen, viewport))
}

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
            width: TV_WIDTH_INCHES,
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

    /// The zoom of the box. 1.0 draws the world at true size on the TV.
    ///
    /// A wider box shows more of the canvas, so its zoom is below 1.0.
    /// `tv_width` is the width of the TV screen in inches.
    pub fn zoom(&self, tv_width: f64) -> f64 {
        tv_width / self.width
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

    /// The same box on a TV `after` inches wide, when it stood at true size
    /// on a TV `before` inches wide. A box at another zoom stays as it is.
    ///
    /// A new size for the TV moves true size. A box at 100 % then stays at
    /// 100 %, so one cell is still one inch on the glass. Issue #6.
    pub fn resized_tv(self, before: f64, after: f64) -> Self {
        if at_true_size(self.zoom(before)) {
            Self {
                width: clamp_width(after),
                ..self
            }
        } else {
            self
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

/// Whether a zoom stands at true size, so the label can say so.
pub fn at_true_size(zoom: f64) -> bool {
    (zoom - 1.0).abs() < 1e-9
}

/// The box width once the snap to true size has had its say.
///
/// A box within `percent` of a 100 percent zoom takes true size exactly, so
/// that a miniature on the table covers the cell it stands in. A box outside
/// that window keeps the width the DM gave it.
pub fn snap_to_true_size(width: f64, tv_width: f64, percent: f64) -> f64 {
    let box_at = TvBox {
        center: (0.0, 0.0),
        width,
    };
    if (box_at.zoom(tv_width) - 1.0).abs() * 100.0 <= percent {
        tv_width
    } else {
        width
    }
}

/// Holds a snap window inside the range the settings offer.
///
/// A window that is not a number falls back to the default.
pub fn clamp_snap_percent(percent: f64) -> f64 {
    if percent.is_finite() {
        percent.clamp(0.0, MAX_SNAP_PERCENT)
    } else {
        DEFAULT_SNAP_PERCENT
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
        TV_WIDTH_INCHES
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_SNAP_PERCENT, MAX_SNAP_PERCENT, MAX_WIDTH, MIN_WIDTH, SizeBy, TV_WIDTH_INCHES,
        TvBox, TvSize, at_true_size, clamp_snap_percent, clamp_width, snap_to_true_size, tv_width,
    };

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
    fn a_box_as_wide_as_the_tv_screen_is_at_100_percent() {
        let tv_box = TvBox {
            center: (0.0, 0.0),
            width: TV_WIDTH_INCHES,
        };
        assert!(close(tv_box.zoom(TV_WIDTH_INCHES), 1.0));
        assert!(at_true_size(tv_box.zoom(TV_WIDTH_INCHES)));
    }

    #[test]
    fn the_zoom_does_not_follow_the_pixels_of_the_tv() {
        // The TV shows the box across every pixel it has, whatever that
        // count is, so only the inches of the screen decide true size. A
        // 1080p TV and a 4K TV of the same size agree on 100 %.
        let tv_box = TvBox {
            center: (0.0, 0.0),
            width: 48.0,
        };
        assert!(close(tv_box.zoom(48.0), 1.0));
        assert!(close(tv_box.camera(UHD).pixels_per_inch, 80.0));
        assert!(close(tv_box.camera((1920, 1080)).pixels_per_inch, 40.0));
    }

    #[test]
    fn a_wider_box_shows_more_world_at_less_zoom() {
        let tv_box = TvBox {
            center: (0.0, 0.0),
            width: 96.0,
        };
        assert!(close(tv_box.zoom(48.0), 0.5));
        assert!(!at_true_size(tv_box.zoom(48.0)));
    }

    #[test]
    fn a_size_inside_the_snap_window_takes_true_size() {
        // 50 inches is a zoom of 96 %, which is inside 8 % of 100 %.
        assert!(close(snap_to_true_size(50.0, 48.0, 8.0), 48.0));
        // 44 inches is 109 %, which is outside it.
        assert!(close(snap_to_true_size(44.0, 48.0, 8.0), 44.0));
    }

    #[test]
    fn a_snap_window_of_zero_leaves_every_size_alone() {
        assert!(close(snap_to_true_size(47.9, 48.0, 0.0), 47.9));
    }

    #[test]
    fn a_box_already_at_true_size_does_not_move() {
        assert!(close(snap_to_true_size(48.0, 48.0, 8.0), 48.0));
    }

    #[test]
    fn a_snap_window_from_a_file_is_held_inside_the_range() {
        assert!(close(clamp_snap_percent(-5.0), 0.0));
        assert!(close(clamp_snap_percent(1e9), MAX_SNAP_PERCENT));
        assert!(close(clamp_snap_percent(f64::NAN), DEFAULT_SNAP_PERCENT));
        assert!(close(clamp_snap_percent(12.0), 12.0));
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

    #[test]
    fn a_box_at_true_size_follows_a_new_tv_size() {
        let at_true = TvBox::default();
        assert!(close(at_true.resized_tv(48.0, 56.0).width, 56.0));
        let zoomed = TvBox {
            center: (0.0, 0.0),
            width: 30.0,
        };
        assert!(close(zoomed.resized_tv(48.0, 56.0).width, 30.0));
    }

    #[test]
    fn a_65_inch_4k_tv_is_about_57_inches_wide() {
        let size = TvSize {
            by: SizeBy::Diagonal,
            value: 65.0,
        };
        let ppi = size.pixels_per_inch(UHD).unwrap();
        assert!((ppi - 67.78).abs() < 0.01);
        assert!((tv_width(Some(size), UHD, UHD) - 56.65).abs() < 0.01);
    }

    #[test]
    fn the_three_numbers_give_the_same_tv() {
        let ppi = 80.0;
        for by in [SizeBy::Diagonal, SizeBy::PixelsPerInch, SizeBy::Width] {
            let size = TvSize::of(by, ppi, UHD);
            assert!(close(size.pixels_per_inch(UHD).unwrap(), ppi));
        }
        assert!(close(TvSize::of(SizeBy::Width, ppi, UHD).value, 48.0));
    }

    #[test]
    fn a_window_smaller_than_the_tv_is_narrower_in_inches() {
        let size = TvSize {
            by: SizeBy::PixelsPerInch,
            value: 80.0,
        };
        assert!(close(tv_width(Some(size), UHD, (1920, 1080)), 24.0));
    }

    #[test]
    fn no_size_or_a_size_of_nothing_keeps_the_old_width() {
        assert!(close(tv_width(None, UHD, UHD), TV_WIDTH_INCHES));
        let nothing = TvSize {
            by: SizeBy::Diagonal,
            value: 0.0,
        };
        assert_eq!(nothing.pixels_per_inch(UHD), None);
        assert!(close(tv_width(Some(nothing), UHD, UHD), TV_WIDTH_INCHES));
    }
}
