//! What is on the canvas.

// Rust guideline compliant 2026-02-21

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::tvbox::TvBox;

/// Who is looking at the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// The DM window.
    Dm,
    /// The TV the players watch.
    Tv,
}

/// A layer of the scene.
///
/// Every map sits on a layer. A layer shows or hides on each screen by
/// itself, so the DM keeps a layer of notes that the TV never draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    /// What the DM calls this layer.
    pub name: String,
    /// Draw this layer on the DM screen.
    pub show_dm: bool,
    /// Draw this layer on the TV.
    pub show_tv: bool,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            name: "Layer".to_owned(),
            show_dm: true,
            show_tv: true,
        }
    }
}

impl Layer {
    /// A layer with a name of its own, shown on both screens.
    pub fn new(name: String) -> Self {
        Self {
            name,
            ..Self::default()
        }
    }

    /// Whether this layer draws for `audience`.
    pub fn shows(&self, audience: Audience) -> bool {
        match audience {
            Audience::Dm => self.show_dm,
            Audience::Tv => self.show_tv,
        }
    }
}

/// Where an index lands once a layer moves from `from` to `to`.
///
/// A layer that moves takes every map on it along, and the layers it steps
/// over move by one to make room.
pub fn index_after_move(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < to && index > from && index <= to {
        index - 1
    } else if from > to && index >= to && index < from {
        index + 1
    } else {
        index
    }
}

/// Moves a layer to another place in the pile.
///
/// Every map keeps the layer it sits on, so the maps move with it. An index
/// outside the pile leaves everything alone.
pub fn move_layer(layers: &mut Vec<Layer>, maps: &mut [MapObject], from: usize, to: usize) {
    if from == to || from >= layers.len() || to >= layers.len() {
        return;
    }
    let layer = layers.remove(from);
    layers.insert(to, layer);
    for map in maps {
        map.layer = index_after_move(map.layer, from, to);
    }
}

/// Takes a layer out of the pile, with every map that sits on it.
///
/// The last layer stays: a scene with no layer has nowhere to put a map.
/// A map on a layer above the one that goes steps down to keep its place.
pub fn delete_layer(layers: &mut Vec<Layer>, maps: &mut Vec<MapObject>, index: usize) {
    if index >= layers.len() || layers.len() == 1 {
        return;
    }
    layers.remove(index);
    maps.retain(|map| map.layer != index);
    for map in maps {
        if map.layer > index {
            map.layer -= 1;
        }
    }
}

/// How many maps sit on one layer.
pub fn maps_on(maps: &[MapObject], index: usize) -> usize {
    maps.iter().filter(|map| map.layer == index).count()
}

/// The maps one screen draws, in the order they draw.
///
/// A layer draws over the layers under it. Inside a layer, a later map
/// draws over an earlier one. A layer that is off for this screen puts
/// nothing on it, and a map on no layer at all draws nowhere.
pub fn draw_order<'a>(
    maps: &'a [MapObject],
    layers: &[Layer],
    audience: Audience,
) -> Vec<&'a MapObject> {
    layers
        .iter()
        .enumerate()
        .filter(|(_, layer)| layer.shows(audience))
        .flat_map(|(index, _)| maps.iter().filter(move |map| map.layer == index))
        .collect()
}

/// Everything one scene holds.
///
/// A scene lives in a folder of its own. The folder holds this file, under
/// the name `scene.json`, and the images the maps point at. Every path in
/// it is the name of a file in that folder, so the whole folder moves to
/// another machine and still opens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scene {
    /// The maps on the canvas, in drawing order inside their layer.
    pub maps: Vec<MapObject>,
    /// The part of the canvas the TV shows.
    pub tv_box: TvBox,
    /// The layers, bottom one first.
    pub layers: Vec<Layer>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            maps: Vec::new(),
            tv_box: TvBox::default(),
            layers: vec![Layer::new("Maps".to_owned())],
        }
    }
}

impl Scene {
    /// Serializes the scene as pretty JSON, so a DM can read it.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a scene has no unserializable field")
    }

    /// Parses a scene from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error when `json` is not valid JSON or a field has the
    /// wrong type.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        Ok(Self::repaired(serde_json::from_str(json)?))
    }

    /// The same scene, with a layer for every map to stand on.
    ///
    /// A scene file is text a DM can edit, and a scene from before layers
    /// existed has none at all. A map that points at no layer joins the
    /// bottom one, where the DM can see it and move it.
    fn repaired(mut self) -> Self {
        if self.layers.is_empty() {
            self.layers = Self::default().layers;
        }
        for map in &mut self.maps {
            if map.layer >= self.layers.len() {
                map.layer = 0;
            }
        }
        self
    }
}

/// How many maps of one name a scene folder may hold.
const MAX_SAME_NAME: u32 = 1000;

/// A file name for `source` inside a scene folder that nothing else uses.
///
/// `taken` says whether a name is in the way. A file that is already there
/// under the same name, with the same bytes, is the same map, so it is not
/// in the way and the scene points at the copy it holds. A different file
/// with the same name gets a number.
pub fn free_name(source: &Path, taken: impl Fn(&str) -> bool) -> String {
    let name = source.file_name().map_or_else(
        || "map".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    if !taken(&name) {
        return name;
    }
    let stem = source.file_stem().map_or_else(
        || "map".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let suffix = source
        .extension()
        .map_or_else(String::new, |end| format!(".{}", end.to_string_lossy()));
    // A folder with this many maps of one name is a mistake, not a scene.
    (2..MAX_SAME_NAME)
        .map(|number| format!("{stem} {number}{suffix}"))
        .find(|candidate| !taken(candidate))
        .unwrap_or(name)
}

/// Copies an image into a scene folder and gives back its name there.
///
/// The folder then holds everything the scene needs, so the whole folder
/// moves to another machine and still opens. A file that is already in the
/// folder stays where it is. The same image added twice keeps one copy.
///
/// # Errors
///
/// Returns an error when the file cannot be read, when it has no name, or
/// when the copy cannot be written.
pub fn copy_into_scene(scene_dir: &Path, file: &Path) -> Result<PathBuf> {
    let name = file.file_name().context("a map file needs a name")?;
    if file.parent() == Some(scene_dir) {
        return Ok(PathBuf::from(name));
    }
    let bytes = std::fs::read(file).with_context(|| format!("{}: cannot read", file.display()))?;
    let name = free_name(file, |candidate| {
        std::fs::read(scene_dir.join(candidate)).is_ok_and(|there| there != bytes)
    });
    let target = scene_dir.join(&name);
    if !target.exists() {
        std::fs::write(&target, &bytes)
            .with_context(|| format!("{}: cannot copy the map", target.display()))?;
    }
    Ok(PathBuf::from(name))
}

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
    /// The layer this map sits on, as a place in the scene's layers.
    #[serde(default)]
    pub layer: usize,
    /// Where the grid this map snaps to starts, in inches.
    ///
    /// Zero is the canvas grid. A free move, with Ctrl held, writes the
    /// spot the DM chose here, so a later snapped move steps by whole
    /// inches from that spot and never pulls the map back.
    #[serde(default)]
    pub snap_offset: (f64, f64),
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
            layer: 0,
            snap_offset: (0.0, 0.0),
        }
    }

    /// Half the width and height in inches, with `scale` applied.
    fn half_size(&self, pixels: (u32, u32)) -> (f64, f64) {
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
    use std::path::{Path, PathBuf};

    use super::{
        Audience, Layer, MapObject, Scene, copy_into_scene, delete_layer, draw_order, free_name,
        index_after_move, maps_on, move_layer,
    };

    /// A folder of its own for one test, under the system's temp folder.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dmap-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    use crate::tvbox::TvBox;

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
        // A map from before this field snaps to the canvas grid, as it did.
        assert_eq!(map.snap_offset, (0.0, 0.0));
    }

    #[test]
    fn a_new_map_uses_the_default_pixels_per_cell() {
        let map = MapObject::new(PathBuf::from("crypt.png"), (0.0, 0.0));
        assert!((map.grid_px - MapObject::DEFAULT_GRID_PX).abs() < 1e-9);
    }

    #[test]
    fn a_scene_round_trips_through_json() {
        let scene = Scene {
            maps: vec![MapObject {
                grid_px: 140.0,
                rotation: 0.5,
                scale: 1.5,
                flip_x: true,
                snap_offset: (0.25, 0.75),
                ..MapObject::new(PathBuf::from("crypt.png"), (1.5, -2.0))
            }],
            tv_box: TvBox {
                center: (3.0, -1.5),
                width: 36.0,
            },
            ..Scene::default()
        };
        assert_eq!(Scene::from_json(&scene.to_json()).unwrap(), scene);
    }

    #[test]
    fn an_empty_scene_file_gives_an_empty_scene() {
        assert_eq!(Scene::from_json("{}").unwrap(), Scene::default());
    }

    #[test]
    fn a_scene_holds_the_names_of_files_beside_it() {
        let scene = Scene {
            maps: vec![MapObject::new(PathBuf::from("crypt.png"), (0.0, 0.0))],
            ..Scene::default()
        };
        assert!(scene.to_json().contains("\"crypt.png\""));
    }

    #[test]
    fn a_map_keeps_its_own_name_in_an_empty_folder() {
        assert_eq!(
            free_name(Path::new("/maps/crypt.png"), |_| false),
            "crypt.png"
        );
    }

    #[test]
    fn another_file_of_the_same_name_gets_a_number() {
        let taken = |name: &str| name == "crypt.png";
        assert_eq!(
            free_name(Path::new("/maps/crypt.png"), taken),
            "crypt 2.png"
        );
        let taken = |name: &str| name == "crypt.png" || name == "crypt 2.png";
        assert_eq!(
            free_name(Path::new("/maps/crypt.png"), taken),
            "crypt 3.png"
        );
    }

    #[test]
    fn a_file_with_no_extension_still_gets_a_name() {
        let taken = |name: &str| name == "crypt";
        assert_eq!(free_name(Path::new("/maps/crypt"), taken), "crypt 2");
    }

    #[test]
    fn a_map_from_elsewhere_is_copied_into_the_scene() {
        let scene = scratch("copy");
        let outside = scene.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let source = outside.join("crypt.png");
        std::fs::write(&source, b"first map").unwrap();

        let name = copy_into_scene(&scene, &source).unwrap();
        assert_eq!(name, PathBuf::from("crypt.png"));
        assert_eq!(
            std::fs::read(scene.join("crypt.png")).unwrap(),
            b"first map"
        );
    }

    #[test]
    fn the_same_map_twice_keeps_one_copy() {
        let scene = scratch("twice");
        let source = scene.join("far/crypt.png");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"first map").unwrap();

        assert_eq!(
            copy_into_scene(&scene, &source).unwrap(),
            PathBuf::from("crypt.png")
        );
        assert_eq!(
            copy_into_scene(&scene, &source).unwrap(),
            PathBuf::from("crypt.png")
        );
        let copies = std::fs::read_dir(&scene)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|end| end == "png"))
            .count();
        assert_eq!(copies, 1);
    }

    #[test]
    fn a_different_map_of_the_same_name_keeps_both() {
        let scene = scratch("clash");
        let one = scene.join("a/crypt.png");
        let two = scene.join("b/crypt.png");
        for (path, bytes) in [(&one, b"first map"), (&two, b"other map")] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        assert_eq!(
            copy_into_scene(&scene, &one).unwrap(),
            PathBuf::from("crypt.png")
        );
        assert_eq!(
            copy_into_scene(&scene, &two).unwrap(),
            PathBuf::from("crypt 2.png")
        );
        assert_eq!(
            std::fs::read(scene.join("crypt.png")).unwrap(),
            b"first map"
        );
        assert_eq!(
            std::fs::read(scene.join("crypt 2.png")).unwrap(),
            b"other map"
        );
    }

    #[test]
    fn a_map_already_in_the_scene_folder_is_left_alone() {
        let scene = scratch("inside");
        let source = scene.join("crypt.png");
        std::fs::write(&source, b"first map").unwrap();
        assert_eq!(
            copy_into_scene(&scene, &source).unwrap(),
            PathBuf::from("crypt.png")
        );
        assert_eq!(std::fs::read_dir(&scene).unwrap().count(), 1);
    }

    /// Three maps, one on each of three layers.
    fn stack() -> (Vec<MapObject>, Vec<Layer>) {
        let maps = (0..3)
            .map(|layer| MapObject {
                layer,
                ..MapObject::new(PathBuf::from(format!("{layer}.png")), (0.0, 0.0))
            })
            .collect();
        let layers = (0..3)
            .map(|number| Layer::new(format!("Layer {number}")))
            .collect();
        (maps, layers)
    }

    #[test]
    fn a_layer_draws_over_the_layers_under_it() {
        let (maps, layers) = stack();
        let order: Vec<_> = draw_order(&maps, &layers, Audience::Dm)
            .iter()
            .map(|map| map.path.clone())
            .collect();
        assert_eq!(
            order,
            vec![
                PathBuf::from("0.png"),
                PathBuf::from("1.png"),
                PathBuf::from("2.png")
            ]
        );
    }

    #[test]
    fn a_layer_that_is_off_draws_nothing() {
        let (maps, mut layers) = stack();
        layers[1].show_dm = false;
        let order = draw_order(&maps, &layers, Audience::Dm);
        assert_eq!(order.len(), 2);
        assert!(order.iter().all(|map| map.path != Path::new("1.png")));
    }

    #[test]
    fn the_two_screens_do_not_have_to_agree() {
        let (maps, mut layers) = stack();
        layers[2].show_tv = false;
        layers[0].show_dm = false;
        assert_eq!(draw_order(&maps, &layers, Audience::Dm).len(), 2);
        assert_eq!(draw_order(&maps, &layers, Audience::Tv).len(), 2);
        assert_eq!(
            draw_order(&maps, &layers, Audience::Tv)[0].path,
            PathBuf::from("0.png")
        );
    }

    #[test]
    fn a_map_draws_over_the_maps_before_it_on_its_layer() {
        let layers = vec![Layer::new("one".to_owned())];
        let maps: Vec<MapObject> = ["a.png", "b.png"]
            .iter()
            .map(|name| MapObject::new(PathBuf::from(*name), (0.0, 0.0)))
            .collect();
        let order = draw_order(&maps, &layers, Audience::Dm);
        assert_eq!(order[0].path, PathBuf::from("a.png"));
        assert_eq!(order[1].path, PathBuf::from("b.png"));
    }

    #[test]
    fn a_scene_from_before_layers_opens_with_one() {
        let json = r#"{"maps":[{"path":"m.png","center":[0.0,0.0],"grid_px":50.0}]}"#;
        let scene = Scene::from_json(json).unwrap();
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.maps[0].layer, 0);
    }

    #[test]
    fn a_map_that_points_past_the_layers_joins_the_bottom_one() {
        let json = r#"{"maps":[{"path":"m.png","center":[0.0,0.0],"grid_px":50.0,"layer":7}],
                       "layers":[{"name":"one","show_dm":true,"show_tv":true}]}"#;
        let scene = Scene::from_json(json).unwrap();
        assert_eq!(scene.maps[0].layer, 0);
    }

    #[test]
    fn the_layers_round_trip_through_json() {
        let scene = Scene {
            layers: vec![
                Layer::new("Maps".to_owned()),
                Layer {
                    name: "My notes".to_owned(),
                    show_dm: true,
                    show_tv: false,
                },
            ],
            maps: vec![MapObject {
                layer: 1,
                ..MapObject::new(PathBuf::from("note.png"), (0.0, 0.0))
            }],
            ..Scene::default()
        };
        assert_eq!(Scene::from_json(&scene.to_json()).unwrap(), scene);
    }

    #[test]
    fn a_layer_that_moves_up_takes_its_maps_with_it() {
        let (mut maps, mut layers) = stack();
        move_layer(&mut layers, &mut maps, 0, 2);
        assert_eq!(layers[2].name, "Layer 0");
        // The map that sat on layer 0 sits on layer 2 now.
        assert_eq!(maps[0].layer, 2);
        assert_eq!(maps[1].layer, 0);
        assert_eq!(maps[2].layer, 1);
    }

    #[test]
    fn a_layer_that_moves_down_takes_its_maps_with_it() {
        let (mut maps, mut layers) = stack();
        move_layer(&mut layers, &mut maps, 2, 0);
        assert_eq!(layers[0].name, "Layer 2");
        assert_eq!(maps[2].layer, 0);
        assert_eq!(maps[0].layer, 1);
        assert_eq!(maps[1].layer, 2);
    }

    #[test]
    fn a_move_that_goes_nowhere_changes_nothing() {
        let (mut maps, mut layers) = stack();
        let before = (maps.clone(), layers.clone());
        move_layer(&mut layers, &mut maps, 1, 1);
        move_layer(&mut layers, &mut maps, 0, 9);
        move_layer(&mut layers, &mut maps, 9, 0);
        assert_eq!((maps, layers), before);
    }

    #[test]
    fn every_index_lands_somewhere_of_its_own() {
        for (from, to) in [(0_usize, 2_usize), (2, 0), (1, 2), (2, 1)] {
            let landed: Vec<usize> = (0..3).map(|i| index_after_move(i, from, to)).collect();
            let mut sorted = landed.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, vec![0, 1, 2], "from {from} to {to} gave {landed:?}");
        }
    }

    #[test]
    fn a_layer_that_goes_takes_its_maps_with_it() {
        let (mut maps, mut layers) = stack();
        delete_layer(&mut layers, &mut maps, 1);
        assert_eq!(layers.len(), 2);
        assert_eq!(maps.len(), 2);
        // The map above the one that went steps down to keep its place.
        assert_eq!(maps[0].layer, 0);
        assert_eq!(maps[1].layer, 1);
        assert_eq!(layers[1].name, "Layer 2");
    }

    #[test]
    fn the_last_layer_stays() {
        let mut layers = vec![Layer::new("only".to_owned())];
        let mut maps = vec![MapObject::new(PathBuf::from("m.png"), (0.0, 0.0))];
        delete_layer(&mut layers, &mut maps, 0);
        assert_eq!(layers.len(), 1);
        assert_eq!(maps.len(), 1);
    }

    #[test]
    fn a_layer_that_is_not_there_takes_nothing() {
        let (mut maps, mut layers) = stack();
        delete_layer(&mut layers, &mut maps, 9);
        assert_eq!((layers.len(), maps.len()), (3, 3));
    }

    #[test]
    fn a_layer_knows_how_many_maps_stand_on_it() {
        let (mut maps, _) = stack();
        maps.push(MapObject {
            layer: 1,
            ..MapObject::new(PathBuf::from("extra.png"), (0.0, 0.0))
        });
        assert_eq!(maps_on(&maps, 1), 2);
        assert_eq!(maps_on(&maps, 2), 1);
    }
}
