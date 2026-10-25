//! A Foundry VTT scene, from "Export Data" on the scene.
//!
//! The format changed between versions, so every field is read through a
//! lookup that tries the new name, then the old one. Version 9 kept the
//! image in `img` and the grid as a number, and version 10 moved both into
//! objects. PLAN.md 5.2 and 10.
//!
//! The scene has a padding around the map, so a wall in the file stands
//! in scene pixels from the corner of that padding and not of the map.

// Rust guideline compliant 2026-02-21

use anyhow::{Context as _, Result, bail};
use serde_json::Value;

use crate::scene::Shown;
use crate::sight::{Kind, Light, Wall};

use super::{Image, Imported, cells, hex_color};

/// The image a Foundry scene names, relative to the Foundry Data folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Background {
    /// The path, with every `%20` and the like turned back into the
    /// character it stands for.
    pub path: String,
}

/// The padding a scene takes when the file names none. Foundry's default.
const DEFAULT_PADDING: f64 = 0.25;

/// The grid size a scene takes when the file names none. Foundry's default.
const DEFAULT_GRID: f64 = 100.0;

/// The distance one cell stands for, when the file names none. Foundry's
/// default of five feet. A light reaches in these units.
const DEFAULT_DISTANCE: f64 = 5.0;

/// Reads the scene text. A `scenes.db` file holds one scene on each line,
/// and the first one counts.
pub fn read(text: &str) -> Result<Imported> {
    let scene: Value = serde_json::from_str(text).or_else(|whole| {
        text.lines()
            .find(|line| !line.trim().is_empty())
            .and_then(|line| serde_json::from_str(line).ok())
            .ok_or(whole)
    })?;
    if !scene.is_object() {
        bail!("not a Foundry VTT scene");
    }
    let image = text_at(&scene, &["background", "src"])
        .or_else(|| text_at(&scene, &["img"]))
        .context("the scene names no map image")?;
    if image.starts_with("http://") || image.starts_with("https://") {
        bail!("the map image is on the web: {image}");
    }
    let grid = number_at(&scene, &["grid", "size"])
        .or_else(|| number_at(&scene, &["grid"]))
        .filter(|size| *size > 0.0)
        .unwrap_or(DEFAULT_GRID);
    let distance = number_at(&scene, &["grid", "distance"])
        .or_else(|| number_at(&scene, &["gridDistance"]))
        .filter(|distance| *distance > 0.0)
        .unwrap_or(DEFAULT_DISTANCE);
    let width = number_at(&scene, &["width"]).context("the scene has no width")?;
    let height = number_at(&scene, &["height"]).context("the scene has no height")?;
    let padding = number_at(&scene, &["padding"]).unwrap_or(DEFAULT_PADDING);
    // Foundry rounds the padding up to whole cells, so the grid of the
    // padding lines up with the grid of the map.
    let pad = |side: f64| (side * padding / grid).ceil() * grid;
    let offset = (
        number_at(&scene, &["background", "offsetX"])
            .or_else(|| number_at(&scene, &["shiftX"]))
            .unwrap_or(0.0),
        number_at(&scene, &["background", "offsetY"])
            .or_else(|| number_at(&scene, &["shiftY"]))
            .unwrap_or(0.0),
    );
    let middle = (
        pad(width) + offset.0 + width / 2.0,
        pad(height) + offset.1 + height / 2.0,
    );
    let walls = list_at(&scene, "walls")
        .filter_map(|wall| read_wall(wall, middle, grid))
        .collect();
    let lights = list_at(&scene, "lights")
        .filter_map(|light| read_light(light, middle, grid, distance))
        .collect();
    Ok(Imported {
        image: Image::Named(Background {
            path: unescape(&image),
        }),
        grid_px: grid,
        drawn_width: Some(width),
        walls,
        lights,
    })
}

/// One wall. `c` holds both ends as `[x1, y1, x2, y2]`.
///
/// `door` is 0 for a wall, 1 for a door, 2 for a secret door. `ds` is 1
/// for an open door. A sense of 0 lets that sense through. Version 9 kept
/// sight in `sense`.
fn read_wall(wall: &Value, middle: (f64, f64), grid: f64) -> Option<Wall> {
    let ends: Vec<f64> = wall
        .get("c")?
        .as_array()?
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    let [x1, y1, x2, y2] = ends.as_slice() else {
        return None;
    };
    let open = number_at(wall, &["ds"]) == Some(1.0);
    let kind = match number_at(wall, &["door"]).unwrap_or(0.0) as i64 {
        1 => Kind::Door { open },
        2 => Kind::Secret { open },
        _ => Kind::Wall,
    };
    let blocks = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| number_at(wall, &[name]))
            .is_none_or(|sense| sense != 0.0)
    };
    Some(Wall {
        a: cells(*x1, *y1, middle, grid),
        b: cells(*x2, *y2, middle, grid),
        kind,
        blocks_light: blocks(&["light"]),
        blocks_sight: blocks(&["sight", "sense"]),
    })
}

/// One light. Version 10 moved its look into `config`.
///
/// The reach is in the units of the grid, such as feet, and a cell holds
/// `distance` of them. The angle and the turn are in degrees.
fn read_light(light: &Value, middle: (f64, f64), grid: f64, distance: f64) -> Option<Light> {
    let x = number_at(light, &["x"])?;
    let y = number_at(light, &["y"])?;
    let field =
        |name: &str| number_at(light, &["config", name]).or_else(|| number_at(light, &[name]));
    let color = text_at(light, &["config", "color"])
        .or_else(|| text_at(light, &["tintColor"]))
        .and_then(|text| hex_color(&text))
        .unwrap_or([255, 255, 255]);
    let angle = field("angle")
        .filter(|angle| *angle > 0.0 && *angle < 360.0)
        .map_or(std::f64::consts::TAU, f64::to_radians);
    Some(Light {
        id: 0,
        shown: Shown::default(),
        center: cells(x, y, middle, grid),
        bright: field("bright").unwrap_or(0.0).max(0.0) / distance,
        dim: field("dim").unwrap_or(0.0).max(0.0) / distance,
        color,
        angle,
        rotation: number_at(light, &["rotation"]).unwrap_or(0.0).to_radians(),
    })
}

/// The value down a path of keys, if every key is there.
fn at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |inside, key| inside.get(key))
}

fn number_at(value: &Value, path: &[&str]) -> Option<f64> {
    at(value, path)?.as_f64()
}

fn text_at(value: &Value, path: &[&str]) -> Option<String> {
    at(value, path)?
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Every entry of a list field, or none when the field is missing.
fn list_at<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// Turns `%20` and the like back into the bytes they stand for.
///
/// Foundry writes a path as a web address, so a space in a file name
/// comes as `%20`. A `%` with no two hex digits after it stays as it is.
fn unescape(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let code = (bytes[i] == b'%')
            .then(|| path.get(i + 1..i + 3))
            .flatten()
            .and_then(|digits| u8::from_str_radix(digits, 16).ok());
        if let Some(byte) = code {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{read, unescape};
    use crate::import::Image;
    use crate::sight::Kind;

    /// A scene in the shape version 10 to 12 export: a 2000 by 1000 map
    /// with 100 pixel cells, so the padding is 500 by 300 pixels.
    const V12: &str = r##"{
        "name": "Crypt",
        "width": 2000, "height": 1000, "padding": 0.25,
        "background": { "src": "worlds/test/maps/the%20crypt.webp", "offsetX": 0, "offsetY": 0 },
        "grid": { "type": 1, "size": 100, "distance": 5, "units": "ft" },
        "walls": [
            { "c": [500, 300, 2500, 300], "door": 0, "light": 20, "sight": 20, "move": 20 },
            { "c": [1500, 300, 1500, 800], "door": 1, "ds": 1, "light": 20, "sight": 20 },
            { "c": [1000, 800, 1100, 800], "door": 2, "ds": 0, "light": 0, "sight": 10 }
        ],
        "lights": [
            { "x": 1500, "y": 800, "rotation": 90,
              "config": { "bright": 10, "dim": 20, "color": "#ff8000", "angle": 90 } }
        ]
    }"##;

    /// The same scene in the shape version 9 exports.
    const V9: &str = r##"{
        "width": 2000, "height": 1000, "padding": 0.25,
        "img": "maps/crypt.png", "grid": 100, "gridDistance": 5,
        "walls": [ { "c": [500, 300, 2500, 300], "door": 0, "sense": 1 } ],
        "lights": [ { "x": 1500, "y": 800, "bright": 10, "dim": 20, "tintColor": "#00ff00" } ]
    }"##;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn a_new_scene_gives_its_image_grid_walls_and_lights() {
        let imported = read(V12).unwrap();
        let Image::Named(background) = &imported.image else {
            panic!("a Foundry scene names its image");
        };
        assert_eq!(background.path, "worlds/test/maps/the crypt.webp");
        assert!((imported.grid_px - 100.0).abs() < 1e-9);
        assert_eq!(imported.drawn_width, Some(2000.0));
        // The map is 20 by 10 cells, so its top edge stands 5 cells up.
        let top = imported.walls[0];
        assert!(close(top.a, (-10.0, -5.0)));
        assert!(close(top.b, (10.0, -5.0)));
        assert_eq!(top.kind, Kind::Wall);
        assert_eq!(imported.walls[1].kind, Kind::Door { open: true });
        let secret = imported.walls[2];
        assert_eq!(secret.kind, Kind::Secret { open: false });
        assert!(!secret.blocks_light);
        assert!(secret.blocks_sight);
        let light = &imported.lights[0];
        assert!(close(light.center, (0.0, 0.0)));
        assert!((light.bright - 2.0).abs() < 1e-9);
        assert!((light.dim - 4.0).abs() < 1e-9);
        assert_eq!(light.color, [255, 128, 0]);
        assert!((light.angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!((light.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    #[test]
    fn an_old_scene_reads_the_same() {
        let imported = read(V9).unwrap();
        assert_eq!(
            imported.image,
            Image::Named(super::Background {
                path: "maps/crypt.png".to_owned()
            })
        );
        assert!(close(imported.walls[0].a, (-10.0, -5.0)));
        assert!(imported.walls[0].blocks_sight);
        assert_eq!(imported.lights[0].color, [0, 255, 0]);
        assert!((imported.lights[0].angle - std::f64::consts::TAU).abs() < 1e-9);
    }

    #[test]
    fn a_scene_database_line_reads_as_a_scene() {
        let line = V9.replace('\n', " ");
        let db = format!("{line}\n{line}\n");
        assert_eq!(read(&db).unwrap(), read(V9).unwrap());
    }

    #[test]
    fn a_file_with_no_image_or_a_web_image_is_refused() {
        read(r#"{ "width": 10, "height": 10 }"#).unwrap_err();
        read(r#"{ "width": 10, "height": 10, "img": "https://x/y.png" }"#).unwrap_err();
        read("[1, 2]").unwrap_err();
        read("not json").unwrap_err();
    }

    #[test]
    fn unescapes_a_web_path() {
        assert_eq!(unescape("a%20b%2Cc"), "a b,c");
        assert_eq!(unescape("100%"), "100%");
        assert_eq!(unescape("%zz"), "%zz");
    }
}
