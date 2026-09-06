//! The map layer: images drawn on the canvas.

// Rust guideline compliant 2026-02-21

use std::path::{Path, PathBuf};

use crate::camera::Camera;

/// One vertex: clip x, clip y, texture u, texture v.
pub type MapVertex = [f32; 4];

/// The two triangles that show a map's world `rect` through `camera`.
pub fn map_quad(
    rect: ((f64, f64), (f64, f64)),
    camera: &Camera,
    viewport: (u32, u32),
) -> [MapVertex; 6] {
    let (min, max) = rect;
    let corner = |u: f32, v: f32| -> MapVertex {
        let world = (
            if u == 0.0 { min.0 } else { max.0 },
            if v == 0.0 { min.1 } else { max.1 },
        );
        let (sx, sy) = camera.world_to_screen(world, viewport);
        let clip_x = (sx / f64::from(viewport.0)) * 2.0 - 1.0;
        let clip_y = 1.0 - (sy / f64::from(viewport.1)) * 2.0;
        [clip_x as f32, clip_y as f32, u, v]
    };
    [
        corner(0.0, 0.0),
        corner(1.0, 0.0),
        corner(0.0, 1.0),
        corner(0.0, 1.0),
        corner(1.0, 0.0),
        corner(1.0, 1.0),
    ]
}

/// The path to store in the project: relative when the file is inside
/// `project_dir`, otherwise as given.
pub fn relative_path(project_dir: &Path, file: &Path) -> PathBuf {
    file.strip_prefix(project_dir).unwrap_or(file).to_path_buf()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{map_quad, relative_path};
    use crate::camera::Camera;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn a_map_rect_becomes_a_clip_quad_with_texture_coordinates() {
        let camera = Camera {
            center: (0.0, 0.0),
            pixels_per_inch: 100.0,
        };
        // A 2 by 1 inch map centered on the origin in a 400 by 200 view.
        let quad = map_quad(((-1.0, -0.5), (1.0, 0.5)), &camera, (400, 200));
        assert_eq!(quad.len(), 6);
        let top_left = quad
            .iter()
            .find(|v| close(v[2], 0.0) && close(v[3], 0.0))
            .unwrap();
        assert!(close(top_left[0], -0.5) && close(top_left[1], 0.5));
        let bottom_right = quad
            .iter()
            .find(|v| close(v[2], 1.0) && close(v[3], 1.0))
            .unwrap();
        assert!(close(bottom_right[0], 0.5) && close(bottom_right[1], -0.5));
    }

    #[test]
    fn a_map_file_inside_the_project_folder_is_stored_relative() {
        let project = Path::new("/home/dm/campaign");
        assert_eq!(
            relative_path(project, Path::new("/home/dm/campaign/maps/crypt.png")),
            Path::new("maps/crypt.png")
        );
    }

    #[test]
    fn a_map_file_elsewhere_keeps_its_absolute_path() {
        let project = Path::new("/home/dm/campaign");
        assert_eq!(
            relative_path(project, Path::new("/mnt/maps/crypt.png")),
            Path::new("/mnt/maps/crypt.png")
        );
    }
}
