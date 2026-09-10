//! The program's own file: where the scenes live, and what this table has.

// Rust guideline compliant 2026-02-21

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

use crate::tvbox::DEFAULT_SNAP_PERCENT;

/// Where the TV window opens.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvPlacement {
    /// The first display that is not the DM's.
    #[default]
    Auto,
    /// A normal window on the DM's display.
    Window,
    /// Full screen on the display with this name.
    Display(String),
}

/// How many scenes of one name the scenes folder may hold.
const MAX_SAME_NAME: u32 = 1000;

/// The folder that holds the scenes, under the home folder.
const DEFAULT_SCENES: &str = "dmap/scenes";

/// What the program remembers between runs.
///
/// A scene belongs to a table full of players. This file belongs to one
/// machine, so a scene that moves to another machine leaves it behind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The folder that holds one folder for each scene.
    pub scenes_dir: PathBuf,
    /// The scene of the last run.
    ///
    /// A name is a folder inside `scenes_dir`. A whole path is a scene that
    /// lives somewhere else, such as one a DM opened from the command line.
    pub last_scene: Option<PathBuf>,
    /// Where the TV window opens.
    pub tv_display: TvPlacement,
    /// Swap the roles of the two windows instead of moving the DM window.
    pub swap_windows: bool,
    /// How close to true size the TV box must come before it snaps.
    pub snap_percent: f64,
    /// The theme the DM window draws. DESIGN.md 2.
    pub theme: crate::theme::Mode,
    /// The language the window speaks, by the name of its file.
    ///
    /// A first run takes the language of the system, and English when the
    /// program carries no file for it.
    #[serde(default = "first_language")]
    pub language: String,
    /// What every size of DESIGN.md is multiplied by. DESIGN.md 3.1.
    pub ui_scale: f64,
    /// The color the Draw view paints with, as red, green, blue, alpha.
    ///
    /// The DM picks a color and a width once, and every stroke after that
    /// takes them. Issue #12.
    #[serde(default = "default_ink_color")]
    pub ink_color: [u8; 4],
    /// How thick a stroke draws, in inches.
    #[serde(default = "default_ink_width")]
    pub ink_width: f64,
    /// Which of the six squares of the Draw panel the DM last used.
    #[serde(default)]
    pub ink_nib: crate::ui::Nib,
    /// How a ruler counts its length. Issue #12.
    #[serde(default)]
    pub ink_rule: crate::stroke::Rule,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scenes_dir: default_scenes_dir(&home()),
            last_scene: None,
            tv_display: TvPlacement::default(),
            swap_windows: true,
            snap_percent: DEFAULT_SNAP_PERCENT,
            theme: crate::theme::Mode::default(),
            language: first_language(),
            ui_scale: crate::theme::DEFAULT_SCALE,
            ink_color: default_ink_color(),
            ink_width: default_ink_width(),
            ink_nib: crate::ui::Nib::default(),
            ink_rule: crate::stroke::Rule::default(),
        }
    }
}

/// The color a first run draws with: the accent red of DESIGN.md 2.
fn default_ink_color() -> [u8; 4] {
    [0xC4, 0x3E, 0x1C, 0xFF]
}

/// How thick a stroke draws on a first run, in inches.
///
/// A tenth of an inch is a fifth of a grid cell at true size: a mark that
/// reads across the table without covering the map under it.
fn default_ink_width() -> f64 {
    0.1
}

/// The language a first run takes: the system's, or English.
///
/// The desktop puts the locale in the environment, such as `ru_RU.UTF-8`.
/// Windows leaves those unset, and a DM there picks the language in
/// Settings once. Issue #53.
fn first_language() -> String {
    for name in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        let Ok(locale) = std::env::var(name) else {
            continue;
        };
        if let Some(code) = crate::text::system_language(&locale) {
            return code.to_owned();
        }
    }
    crate::text::DEFAULT.to_owned()
}

impl Config {
    /// Serializes the config as pretty JSON, so a DM can edit it by hand.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a config has no unserializable field")
    }

    /// Parses a config from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error when `json` is not valid JSON or a field has the
    /// wrong type.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// The folder of a scene, by name or by path.
    pub fn scene_dir(&self, scene: &Path) -> PathBuf {
        if scene.is_absolute() {
            scene.to_path_buf()
        } else {
            self.scenes_dir.join(scene)
        }
    }

    /// How to remember a scene folder: by name when it sits in
    /// `scenes_dir`, and by path when it does not.
    pub fn remember(&self, dir: &Path) -> PathBuf {
        dir.strip_prefix(&self.scenes_dir)
            .map_or_else(|_| dir.to_path_buf(), Path::to_path_buf)
    }
}

/// The scenes in a folder, by name, in the order a list shows them.
///
/// Every folder is a scene. A folder with no scene file is a scene the DM
/// has not filled yet, and the program writes the file when it opens.
pub fn scene_list(scenes_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(scenes_dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

/// Whether a name can be a folder inside the scenes folder.
///
/// A name with a separator in it would put the scene somewhere else, and
/// the two dot names would climb out of the scenes folder.
pub fn valid_scene_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && trimmed != "." && trimmed != ".." && !trimmed.contains(['/', '\\'])
}

/// A scene name that no folder in `scenes_dir` uses yet.
pub fn free_scene_name(wanted: &str, taken: &[String]) -> String {
    if !taken.iter().any(|name| name == wanted) {
        return wanted.to_owned();
    }
    // A folder with this many scenes of one name is a mistake, not a table.
    (2..MAX_SAME_NAME)
        .map(|number| format!("{wanted} {number}"))
        .find(|candidate| !taken.iter().any(|name| name == candidate))
        .unwrap_or_else(|| wanted.to_owned())
}

/// Makes a new scene folder and gives back its name.
///
/// # Errors
///
/// Returns an error when the folder cannot be made.
pub fn new_scene(scenes_dir: &Path, wanted: &str) -> Result<String> {
    let name = free_scene_name(wanted, &scene_list(scenes_dir));
    let dir = scenes_dir.join(&name);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("{}: cannot make the scene", dir.display()))?;
    Ok(name)
}

/// Gives a scene folder another name.
///
/// # Errors
///
/// Returns an error when the name cannot be a folder, when another scene
/// holds it, or when the folder cannot be renamed.
pub fn rename_scene(scenes_dir: &Path, from: &str, to: &str) -> Result<String> {
    let to = to.trim();
    if !valid_scene_name(to) {
        bail!("{to}: a scene name cannot hold a slash, and cannot be empty");
    }
    if to == from {
        return Ok(to.to_owned());
    }
    let target = scenes_dir.join(to);
    if target.exists() {
        bail!("{to}: a scene of this name is already here");
    }
    std::fs::rename(scenes_dir.join(from), &target)
        .with_context(|| format!("{from}: cannot rename the scene"))?;
    Ok(to.to_owned())
}

/// Deletes a scene folder and everything in it.
///
/// # Errors
///
/// Returns an error when the name is not a scene in `scenes_dir`, or when
/// the folder cannot be deleted.
pub fn delete_scene(scenes_dir: &Path, name: &str) -> Result<()> {
    if !valid_scene_name(name) {
        bail!("{name}: this is not a scene");
    }
    let dir = scenes_dir.join(name);
    if !dir.is_dir() {
        bail!("{}: no scene here", dir.display());
    }
    std::fs::remove_dir_all(&dir).with_context(|| format!("{}: cannot delete", dir.display()))
}

/// The home folder, or the current folder when there is none.
fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// Where the scenes live when the DM has not said otherwise.
pub fn default_scenes_dir(home: &Path) -> PathBuf {
    home.join(DEFAULT_SCENES)
}

/// The file that holds the config.
///
/// A Linux desktop follows the XDG base directory rules: `XDG_CONFIG_HOME`
/// when it sets one, and `~/.config` when it does not. Windows keeps a
/// config under `APPDATA`.
pub fn config_path() -> PathBuf {
    let folder = if cfg!(windows) {
        "APPDATA"
    } else {
        "XDG_CONFIG_HOME"
    };
    let base = std::env::var_os(folder).map_or_else(|| home().join(".config"), PathBuf::from);
    base.join("dmap").join("config.json")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{Config, TvPlacement, default_scenes_dir};

    fn scenes_at(dir: &str) -> Config {
        Config {
            scenes_dir: PathBuf::from(dir),
            ..Config::default()
        }
    }

    #[test]
    fn a_new_config_points_at_the_scenes_folder_in_the_home_folder() {
        assert_eq!(
            default_scenes_dir(Path::new("/home/dm")),
            PathBuf::from("/home/dm/dmap/scenes")
        );
    }

    #[test]
    fn a_scene_sits_in_a_folder_of_its_own_name() {
        let config = scenes_at("/campaign/scenes");
        assert_eq!(
            config.scene_dir(Path::new("The Crypt")),
            PathBuf::from("/campaign/scenes/The Crypt")
        );
    }

    #[test]
    fn a_scene_somewhere_else_keeps_its_whole_path() {
        let config = scenes_at("/campaign/scenes");
        let far = Path::new("/tmp/one-off scene");
        assert_eq!(config.scene_dir(far), PathBuf::from("/tmp/one-off scene"));
    }

    #[test]
    fn a_scene_in_the_scenes_folder_is_remembered_by_name() {
        let config = scenes_at("/campaign/scenes");
        assert_eq!(
            config.remember(Path::new("/campaign/scenes/The Crypt")),
            PathBuf::from("The Crypt")
        );
        assert_eq!(
            config.remember(Path::new("/tmp/one-off scene")),
            PathBuf::from("/tmp/one-off scene")
        );
    }

    #[test]
    fn a_config_round_trips_through_json() {
        let config = Config {
            scenes_dir: PathBuf::from("/campaign/scenes"),
            last_scene: Some(PathBuf::from("The Crypt")),
            tv_display: TvPlacement::Display("HDMI-1".to_owned()),
            swap_windows: true,
            snap_percent: 12.0,
            theme: crate::theme::Mode::Dark,
            language: "ru".to_owned(),
            ui_scale: 1.25,
            ink_color: [1, 2, 3, 255],
            ink_width: 0.25,
            ink_nib: crate::ui::Nib::Ellipse,
            ink_rule: crate::stroke::Rule::Fifth,
        };
        assert_eq!(Config::from_json(&config.to_json()).unwrap(), config);
    }

    #[test]
    fn every_way_to_place_the_tv_round_trips() {
        for tv_display in [
            TvPlacement::Auto,
            TvPlacement::Window,
            TvPlacement::Display("HDMI-1".to_owned()),
        ] {
            let config = Config {
                tv_display: tv_display.clone(),
                ..Config::default()
            };
            assert_eq!(
                Config::from_json(&config.to_json()).unwrap().tv_display,
                tv_display
            );
        }
    }

    #[test]
    fn the_display_choice_reads_plainly_in_the_file() {
        let config = Config {
            tv_display: TvPlacement::Display("HDMI-1".to_owned()),
            ..Config::default()
        };
        assert!(config.to_json().contains("\"display\": \"HDMI-1\""));
    }

    #[test]
    fn a_config_from_an_older_version_takes_the_defaults() {
        let config = Config::from_json("{}").unwrap();
        assert_eq!(config.last_scene, None);
        assert!((config.snap_percent - 8.0).abs() < 1e-9);
    }

    #[test]
    fn a_new_config_opens_no_scene() {
        assert_eq!(Config::default().last_scene, None);
    }

    /// A scenes folder of its own for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dmap-scenes-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn every_folder_is_a_scene_and_the_list_reads_in_order() {
        let scenes = scratch("list");
        for name in ["The Crypt", "abbey", "Sosnovka"] {
            std::fs::create_dir_all(scenes.join(name)).unwrap();
        }
        std::fs::write(scenes.join("notes.txt"), b"not a scene").unwrap();
        assert_eq!(
            super::scene_list(&scenes),
            vec!["abbey", "Sosnovka", "The Crypt"]
        );
    }

    #[test]
    fn a_missing_scenes_folder_lists_nothing() {
        assert!(super::scene_list(Path::new("/nowhere/at/all")).is_empty());
    }

    #[test]
    fn a_scene_name_stays_inside_the_scenes_folder() {
        assert!(super::valid_scene_name("The Crypt"));
        assert!(!super::valid_scene_name(""));
        assert!(!super::valid_scene_name("   "));
        assert!(!super::valid_scene_name(".."));
        assert!(!super::valid_scene_name("../secrets"));
        assert!(!super::valid_scene_name("maps/crypt"));
    }

    #[test]
    fn a_second_scene_of_one_name_takes_a_number() {
        let taken = vec!["New scene".to_owned()];
        assert_eq!(super::free_scene_name("New scene", &taken), "New scene 2");
        assert_eq!(super::free_scene_name("The Crypt", &taken), "The Crypt");
    }

    #[test]
    fn a_new_scene_makes_a_folder() {
        let scenes = scratch("new");
        assert_eq!(super::new_scene(&scenes, "The Crypt").unwrap(), "The Crypt");
        assert_eq!(
            super::new_scene(&scenes, "The Crypt").unwrap(),
            "The Crypt 2"
        );
        assert!(scenes.join("The Crypt 2").is_dir());
    }

    #[test]
    fn a_rename_moves_the_folder_with_everything_in_it() {
        let scenes = scratch("rename");
        std::fs::create_dir_all(scenes.join("old")).unwrap();
        std::fs::write(scenes.join("old/scene.json"), b"{}").unwrap();
        assert_eq!(super::rename_scene(&scenes, "old", " new ").unwrap(), "new");
        assert!(scenes.join("new/scene.json").is_file());
        assert!(!scenes.join("old").exists());
    }

    #[test]
    fn a_rename_refuses_a_name_another_scene_holds() {
        let scenes = scratch("clash");
        for name in ["one", "two"] {
            std::fs::create_dir_all(scenes.join(name)).unwrap();
        }
        super::rename_scene(&scenes, "one", "two").unwrap_err();
        super::rename_scene(&scenes, "one", "../escape").unwrap_err();
        assert!(scenes.join("one").is_dir());
    }

    #[test]
    fn a_delete_takes_the_folder_and_leaves_the_rest() {
        let scenes = scratch("delete");
        for name in ["one", "two"] {
            std::fs::create_dir_all(scenes.join(name)).unwrap();
        }
        std::fs::write(scenes.join("one/map.png"), b"bytes").unwrap();
        super::delete_scene(&scenes, "one").unwrap();
        assert!(!scenes.join("one").exists());
        assert!(scenes.join("two").is_dir());
    }

    #[test]
    fn a_delete_cannot_climb_out_of_the_scenes_folder() {
        let scenes = scratch("escape");
        super::delete_scene(&scenes, "..").unwrap_err();
        super::delete_scene(&scenes, "no such scene").unwrap_err();
        assert!(scenes.is_dir());
    }
}
