//! What is on the canvas.

// Rust guideline compliant 2026-02-21

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One map image placed on the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapObject {
    /// The image file, relative to the project file.
    pub path: PathBuf,
    /// World point in the middle of the image, in inches.
    pub center: (f64, f64),
    /// Image pixels in one grid cell. One cell is one inch on the canvas.
    pub grid_px: f64,
}

impl MapObject {
    /// Pixels per cell for a map whose grid size is not known yet.
    ///
    /// The same default as Foundry VTT, so many maps come out right at once.
    pub const DEFAULT_GRID_PX: f64 = 100.0;

    /// A map at `center` with the default grid size.
    pub fn new(path: PathBuf, center: (f64, f64)) -> Self {
        Self {
            path,
            center,
            grid_px: Self::DEFAULT_GRID_PX,
        }
    }

    /// The world rectangle, `(min, max)` in inches, of an image of `pixels` size.
    pub fn rect(&self, pixels: (u32, u32)) -> ((f64, f64), (f64, f64)) {
        let half_w = f64::from(pixels.0) / self.grid_px / 2.0;
        let half_h = f64::from(pixels.1) / self.grid_px / 2.0;
        (
            (self.center.0 - half_w, self.center.1 - half_h),
            (self.center.0 + half_w, self.center.1 + half_h),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::MapObject;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn a_map_spans_its_pixels_divided_by_pixels_per_cell() {
        let map = MapObject {
            path: PathBuf::from("crypt.png"),
            center: (10.0, 5.0),
            grid_px: 50.0,
        };
        let (min, max) = map.rect((400, 300));
        assert!(close(min, (6.0, 2.0)));
        assert!(close(max, (14.0, 8.0)));
    }

    #[test]
    fn a_new_map_uses_the_default_pixels_per_cell() {
        let map = MapObject::new(PathBuf::from("crypt.png"), (0.0, 0.0));
        assert!((map.grid_px - MapObject::DEFAULT_GRID_PX).abs() < 1e-9);
    }
}
