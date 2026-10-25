//! A Universal VTT file, from Dungeondraft and other map makers.
//!
//! The file is JSON. It carries its map as base64 text, and every point in
//! it stands in cells from the corner of the map. `line_of_sight` holds
//! the walls as runs of points, `portals` holds the doors, and `lights`
//! holds the lights. PLAN.md 5.2.

// Rust guideline compliant 2026-02-21

use anyhow::{Context as _, Result};
use base64::Engine as _;
use serde::Deserialize;

use crate::sight::{Kind, Light, Wall};

use super::{Image, Imported, cells, hex_color};

/// The parts of the file the program reads. Every other field is left.
#[derive(Debug, Deserialize)]
struct File {
    resolution: Resolution,
    #[serde(default)]
    line_of_sight: Vec<Vec<Point>>,
    /// The walls of objects, which Dungeondraft writes apart from the
    /// walls of the map.
    #[serde(default)]
    objects_line_of_sight: Vec<Vec<Point>>,
    #[serde(default)]
    portals: Vec<Portal>,
    #[serde(default)]
    lights: Vec<UvttLight>,
    image: String,
}

#[derive(Debug, Deserialize)]
struct Resolution {
    #[serde(default)]
    map_origin: Point,
    map_size: Point,
    pixels_per_grid: f64,
}

#[derive(Debug, Default, Clone, Copy, Deserialize)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Debug, Deserialize)]
struct Portal {
    bounds: Vec<Point>,
    #[serde(default = "closed")]
    closed: bool,
}

fn closed() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct UvttLight {
    position: Point,
    /// How far the light reaches, in cells.
    range: f64,
    #[serde(default)]
    color: String,
}

/// Reads the file text.
pub fn read(text: &str) -> Result<Imported> {
    let file: File = serde_json::from_str(text)?;
    let resolution = &file.resolution;
    let middle = (
        resolution.map_origin.x + resolution.map_size.x / 2.0,
        resolution.map_origin.y + resolution.map_size.y / 2.0,
    );
    let at = |point: &Point| cells(point.x, point.y, middle, 1.0);
    let mut walls: Vec<Wall> = file
        .line_of_sight
        .iter()
        .chain(&file.objects_line_of_sight)
        .flat_map(|run| {
            run.windows(2)
                .map(|pair| Wall::new(at(&pair[0]), at(&pair[1])))
        })
        .collect();
    walls.extend(file.portals.iter().filter_map(|portal| {
        let [a, b, ..] = portal.bounds.as_slice() else {
            return None;
        };
        Some(Wall {
            kind: Kind::Door {
                open: !portal.closed,
            },
            ..Wall::new(at(a), at(b))
        })
    }));
    // The format gives one reach. The bright part takes half of it, the
    // same split Foundry VTT makes when it imports these files.
    let lights = file
        .lights
        .iter()
        .map(|light| Light {
            color: hex_color(&light.color).unwrap_or([255, 255, 255]),
            ..Light::new(
                0,
                at(&light.position),
                light.range.max(0.0) / 2.0,
                light.range.max(0.0),
            )
        })
        .collect();
    let image = base64::engine::general_purpose::STANDARD
        .decode(file.image.trim())
        .context("the map image is not base64")?;
    Ok(Imported {
        image: Image::Inside(image),
        grid_px: resolution.pixels_per_grid,
        drawn_width: None,
        walls,
        lights,
    })
}

#[cfg(test)]
mod tests {
    use super::read;
    use crate::import::Image;
    use crate::sight::Kind;

    /// A map of 10 by 6 cells, at 50 pixels a cell. Its image is the four
    /// bytes "abcd" in base64.
    const FILE: &str = r#"{
        "format": 0.3,
        "resolution": {
            "map_origin": { "x": 0, "y": 0 },
            "map_size": { "x": 10, "y": 6 },
            "pixels_per_grid": 50
        },
        "line_of_sight": [
            [ { "x": 0, "y": 0 }, { "x": 10, "y": 0 }, { "x": 10, "y": 6 } ]
        ],
        "objects_line_of_sight": [],
        "portals": [
            { "position": { "x": 5, "y": 6 }, "bounds": [ { "x": 4.5, "y": 6 }, { "x": 5.5, "y": 6 } ],
              "rotation": 0, "closed": false, "freestanding": false }
        ],
        "environment": { "baked_lighting": false, "ambient_light": "ffffffff" },
        "lights": [
            { "position": { "x": 5, "y": 3 }, "range": 4, "intensity": 1, "color": "ffff8000", "shadows": true }
        ],
        "image": "YWJjZA=="
    }"#;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn a_file_gives_its_image_grid_walls_doors_and_lights() {
        let imported = read(FILE).unwrap();
        assert_eq!(imported.image, Image::Inside(b"abcd".to_vec()));
        assert!((imported.grid_px - 50.0).abs() < 1e-9);
        assert_eq!(imported.drawn_width, None);
        // A run of three points is two walls, and the door comes last.
        assert_eq!(imported.walls.len(), 3);
        assert!(close(imported.walls[0].a, (-5.0, -3.0)));
        assert!(close(imported.walls[1].b, (5.0, 3.0)));
        let door = imported.walls[2];
        assert_eq!(door.kind, Kind::Door { open: true });
        assert!(close(door.a, (-0.5, 3.0)));
        let light = &imported.lights[0];
        assert!(close(light.center, (0.0, 0.0)));
        assert!((light.dim - 4.0).abs() < 1e-9);
        assert!((light.bright - 2.0).abs() < 1e-9);
        assert_eq!(light.color, [255, 128, 0]);
    }

    #[test]
    fn a_file_with_no_grid_size_is_refused() {
        read(r#"{ "resolution": { "map_size": { "x": 1, "y": 1 } }, "image": "" }"#).unwrap_err();
        read("{}").unwrap_err();
    }
}
