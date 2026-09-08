//! The program's own file: where the scenes live, and what this table has.

// Rust guideline compliant 2026-02-21

use std::path::{Path, PathBuf};

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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scenes_dir: default_scenes_dir(&home()),
            last_scene: None,
            tv_display: TvPlacement::default(),
            swap_windows: false,
            snap_percent: DEFAULT_SNAP_PERCENT,
        }
    }
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

/// The home folder, or the current folder when there is none.
fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// Where the scenes live when the DM has not said otherwise.
pub fn default_scenes_dir(home: &Path) -> PathBuf {
    home.join(DEFAULT_SCENES)
}

/// The file that holds the config.
///
/// This follows the XDG base directory rules: `XDG_CONFIG_HOME` when the
/// desktop sets it, and `~/.config` when it does not.
pub fn config_path() -> PathBuf {
    let base =
        std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| home().join(".config"), PathBuf::from);
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
        };
        assert_eq!(Config::from_json(&config.to_json()).unwrap(), config);
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
}
