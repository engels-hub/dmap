//! Maps that come from other programs, with their walls and lights.
//!
//! A Foundry VTT scene and a Universal VTT file both hold a map image, its
//! grid size, its walls and doors, and its lights. Each reader turns its
//! file into an [`Imported`], in cells around the middle of the map. The
//! program then puts the map and its marks in one group, wherever the DM
//! looks. PLAN.md 5.2. Issues #16 and #17.

// Rust guideline compliant 2026-02-21

mod foundry;
mod uvtt;

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::scene::{Asset, Group, Node, NodeId, Shown};
use crate::sight::{Light, Wall, Walls};

pub use foundry::Background;

/// The file extensions of a Universal VTT file.
///
/// Dungeondraft writes `dd2vtt`, `DungeonFog` writes `df2vtt`, and the
/// format itself names `uvtt`. All three hold the same JSON.
pub const UVTT_EXTENSIONS: &[&str] = &["dd2vtt", "df2vtt", "uvtt"];

/// The file extension of a Foundry VTT scene that "Export Data" writes.
pub const FOUNDRY_EXTENSIONS: &[&str] = &["json"];

/// Every extension an importer reads.
pub fn extensions() -> Vec<&'static str> {
    UVTT_EXTENSIONS
        .iter()
        .chain(FOUNDRY_EXTENSIONS)
        .copied()
        .collect()
}

/// Where the map image of an import comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Image {
    /// A path the scene file names. Foundry VTT keeps its images apart
    /// from the scene, under its Data folder.
    Named(Background),
    /// The bytes of the image, which a Universal VTT file carries inside.
    Inside(Vec<u8>),
}

/// What one import brings, in cells around the middle of its map.
///
/// One cell is one inch on the canvas, so these numbers are inches too.
/// The program moves them to where the map lands.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The map image.
    pub image: Image,
    /// Pixels in one grid cell, measured on the image as the scene draws it.
    pub grid_px: f64,
    /// How wide the scene draws the image, in its pixels, when the scene
    /// may draw it at another size than the file holds. `None` says the
    /// file draws as it is.
    pub drawn_width: Option<f64>,
    /// The walls and doors.
    pub walls: Vec<Wall>,
    /// The lights. Their ids are not set yet.
    pub lights: Vec<Light>,
}

impl Imported {
    /// The grid size in pixels of the image file itself.
    ///
    /// A Foundry scene may draw a 4000 pixel image across 2000 pixels of
    /// scene. One cell then takes twice as many pixels of the file.
    pub fn grid_px_of(&self, file_width: u32) -> f64 {
        match self.drawn_width {
            Some(drawn) if drawn > 0.0 => self.grid_px * f64::from(file_width) / drawn,
            _ => self.grid_px,
        }
    }
}

/// The group an import joins the tree as: the map, its walls, its lights.
///
/// The map lands with its middle on `center`, and the walls and the lights
/// stand around it as the file placed them. The group takes the id
/// `first`, and the nodes in it take the ids after it. `stored` is the name
/// of the image in the scene folder, and `file_width` its width in pixels.
pub fn group(
    imported: Imported,
    name: String,
    stored: PathBuf,
    file_width: u32,
    first: NodeId,
    center: (f64, f64),
) -> Group {
    let mut group = Group::new(first, name);
    let mut asset = Asset::new(first + 1, stored, center);
    asset.grid_px = imported.grid_px_of(file_width);
    group.children.push(Node::Asset(asset));
    let mut next = first + 2;
    if !imported.walls.is_empty() {
        let walls = Walls {
            id: next,
            shown: Shown::default(),
            segments: imported.walls,
        };
        group.children.push(Node::Walls(walls.moved(center)));
        next += 1;
    }
    for light in imported.lights {
        group.children.push(Node::Light(Light {
            id: next,
            ..light.moved(center)
        }));
        next += 1;
    }
    group
}

/// Whether an importer reads `file`, by its extension.
pub fn reads(file: &Path) -> bool {
    file.extension()
        .and_then(|end| end.to_str())
        .is_some_and(|end| {
            let end = end.to_ascii_lowercase();
            extensions().contains(&end.as_str())
        })
}

/// Reads an import file.
///
/// # Errors
///
/// Returns an error that names the file when it cannot be read, when it
/// is not JSON, or when it lacks a map image or a grid size.
pub fn read(file: &Path) -> Result<Imported> {
    let text = std::fs::read_to_string(file)
        .with_context(|| format!("{}: cannot read", file.display()))?;
    let end = file
        .extension()
        .and_then(|end| end.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let read = if UVTT_EXTENSIONS.contains(&end.as_str()) {
        uvtt::read(&text)
    } else if FOUNDRY_EXTENSIONS.contains(&end.as_str()) {
        foundry::read(&text)
    } else {
        bail!("not a Foundry VTT scene or a Universal VTT file");
    };
    read.with_context(|| format!("{}", file.display()))
}

/// Where the image a Foundry scene names lies on this machine.
///
/// The scene names its image relative to the Foundry Data folder. A file
/// beside the scene file wins, so a DM who copied the scene and its map
/// into one folder needs no Data folder at all. Gives back `None` when
/// no place holds the file.
pub fn find_background(
    background: &Background,
    scene_file: &Path,
    data: Option<&Path>,
) -> Option<PathBuf> {
    let named = PathBuf::from(&background.path);
    let beside = scene_file.parent().map(|folder| {
        folder.join(
            named
                .file_name()
                .map_or_else(|| named.clone(), PathBuf::from),
        )
    });
    let candidates = [
        named.is_absolute().then(|| named.clone()),
        beside,
        data.map(|data| data.join(&named)),
    ];
    candidates.into_iter().flatten().find(|path| path.is_file())
}

/// The point at `(x, y)` in a frame of `scale` units to a cell, moved so
/// `middle` lands on zero.
fn cells(x: f64, y: f64, middle: (f64, f64), scale: f64) -> (f64, f64) {
    ((x - middle.0) / scale, (y - middle.1) / scale)
}

/// Reads a color such as `#ffaa00` or `ffffaa00`. The last six digits
/// are the red, green and blue. A Universal VTT color puts the alpha in
/// front of them.
fn hex_color(text: &str) -> Option<[u8; 3]> {
    let digits = text.trim().trim_start_matches('#');
    let rgb = digits.get(digits.len().checked_sub(6)?..)?;
    let value = u32::from_str_radix(rgb, 16).ok()?;
    let [_, r, g, b] = value.to_be_bytes();
    Some([r, g, b])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Image, Imported, group, hex_color};
    use crate::scene::Node;
    use crate::sight::{Light, Wall};

    #[test]
    fn reads_a_color_with_or_without_alpha() {
        assert_eq!(hex_color("#ff8000"), Some([255, 128, 0]));
        assert_eq!(hex_color("ffff8000"), Some([255, 128, 0]));
        assert_eq!(hex_color("nope"), None);
        assert_eq!(hex_color(""), None);
    }

    #[test]
    fn a_map_drawn_at_half_size_takes_twice_the_file_pixels() {
        let imported = Imported {
            image: Image::Inside(Vec::new()),
            grid_px: 100.0,
            drawn_width: Some(2000.0),
            walls: Vec::new(),
            lights: Vec::new(),
        };
        assert!((imported.grid_px_of(4000) - 200.0).abs() < 1e-9);
    }

    #[test]
    fn an_import_lands_as_one_group_around_the_view() {
        let imported = Imported {
            image: Image::Inside(Vec::new()),
            grid_px: 100.0,
            drawn_width: Some(1000.0),
            walls: vec![Wall::new((-1.0, 0.0), (1.0, 0.0))],
            lights: vec![Light::new(0, (0.0, 2.0), 1.0, 2.0)],
        };
        let made = group(
            imported,
            "crypt".to_owned(),
            PathBuf::from("crypt.png"),
            2000,
            7,
            (10.0, 20.0),
        );
        assert_eq!(made.id, 7);
        assert_eq!(made.name, "crypt");
        let [Node::Asset(map), Node::Walls(walls), Node::Light(light)] = made.children.as_slice()
        else {
            panic!("the group holds the map, its walls and its light");
        };
        assert_eq!((map.id, walls.id, light.id), (8, 9, 10));
        assert_eq!(map.center, (10.0, 20.0));
        assert!((map.grid_px - 200.0).abs() < 1e-9);
        assert_eq!(walls.segments[0].a, (9.0, 20.0));
        assert_eq!(light.center, (10.0, 22.0));
    }

    #[test]
    fn a_map_with_no_walls_holds_no_empty_wall_row() {
        let imported = Imported {
            image: Image::Inside(Vec::new()),
            grid_px: 100.0,
            drawn_width: None,
            walls: Vec::new(),
            lights: Vec::new(),
        };
        let made = group(
            imported,
            "m".to_owned(),
            PathBuf::from("m.png"),
            10,
            1,
            (0.0, 0.0),
        );
        assert_eq!(made.children.len(), 1);
    }
}
