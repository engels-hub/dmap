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
    /// Turn around the center in radians, clockwise on screen.
    #[serde(default)]
    pub rotation: f64,
    /// Extra size factor around the center. 1 is the size from `grid_px`.
    #[serde(default = "one")]
    pub scale: f64,
    /// Mirror the image left to right.
    #[serde(default)]
    pub flip_x: bool,
    /// Mirror the image top to bottom.
    #[serde(default)]
    pub flip_y: bool,
}

fn one() -> f64 {
    1.0
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
            rotation: 0.0,
            scale: 1.0,
            flip_x: false,
            flip_y: false,
        }
    }

    /// Half the width and height in inches, with `scale` applied.
    pub fn half_size(&self, pixels: (u32, u32)) -> (f64, f64) {
        (
            f64::from(pixels.0) / self.grid_px / 2.0 * self.scale,
            f64::from(pixels.1) / self.grid_px / 2.0 * self.scale,
        )
    }

    /// The world corners of an image of `pixels` size, turned and scaled.
    ///
    /// Order: the image's top-left, top-right, bottom-right, bottom-left.
    pub fn corners(&self, pixels: (u32, u32)) -> [(f64, f64); 4] {
        let (hw, hh) = self.half_size(pixels);
        let (sin, cos) = self.rotation.sin_cos();
        [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].map(|(x, y)| {
            (
                self.center.0 + x * cos - y * sin,
                self.center.1 + x * sin + y * cos,
            )
        })
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
            grid_px: 50.0,
            ..MapObject::new(PathBuf::from("crypt.png"), (10.0, 5.0))
        };
        let corners = map.corners((400, 300));
        assert!(close(corners[0], (6.0, 2.0)));
        assert!(close(corners[2], (14.0, 8.0)));
    }

    #[test]
    fn corners_of_an_unrotated_map_are_its_rect() {
        let map = MapObject::new(PathBuf::from("m.png"), (10.0, 5.0));
        let corners = map.corners((400, 300));
        assert!(close(corners[0], (8.0, 3.5)));
        assert!(close(corners[1], (12.0, 3.5)));
        assert!(close(corners[2], (12.0, 6.5)));
        assert!(close(corners[3], (8.0, 6.5)));
    }

    #[test]
    fn a_quarter_turn_swaps_width_and_height() {
        let mut map = MapObject::new(PathBuf::from("m.png"), (0.0, 0.0));
        map.rotation = std::f64::consts::FRAC_PI_2;
        let corners = map.corners((400, 300));
        // The image's top-left corner moves to the top-right of the turned map.
        assert!(close(corners[0], (1.5, -2.0)));
        assert!(close(corners[1], (1.5, 2.0)));
    }

    #[test]
    fn scale_grows_the_map_around_its_center() {
        let mut map = MapObject::new(PathBuf::from("m.png"), (0.0, 0.0));
        map.scale = 2.0;
        let corners = map.corners((400, 300));
        assert!(close(corners[0], (-4.0, -3.0)));
        assert!(close(corners[2], (4.0, 3.0)));
    }

    #[test]
    fn old_project_files_without_transform_fields_still_load() {
        let json = r#"{"path":"m.png","center":[1.0,2.0],"grid_px":50.0}"#;
        let map: MapObject = serde_json::from_str(json).unwrap();
        assert!((map.rotation).abs() < 1e-9);
        assert!((map.scale - 1.0).abs() < 1e-9);
        assert!(!map.flip_x && !map.flip_y);
    }

    #[test]
    fn a_new_map_uses_the_default_pixels_per_cell() {
        let map = MapObject::new(PathBuf::from("crypt.png"), (0.0, 0.0));
        assert!((map.grid_px - MapObject::DEFAULT_GRID_PX).abs() < 1e-9);
    }
}
