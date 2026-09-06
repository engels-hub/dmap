//! The project file: everything the DM set up for one campaign.

// Rust guideline compliant 2026-02-21

use serde::{Deserialize, Serialize};

use crate::scene::MapObject;

/// Everything the DM set up for one campaign.
///
/// Every field has a default, so a file from an older version still loads.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    /// Where the TV window opens.
    pub tv_display: TvPlacement,
    /// Swap the roles of the two windows instead of moving the DM window.
    ///
    /// Wayland does not let a program move its windows, so this is the only
    /// way to keep the DM window off the TV display there.
    pub swap_windows: bool,
    /// The maps on the canvas, in drawing order.
    pub maps: Vec<MapObject>,
}

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

impl Project {
    /// Serializes the project as pretty JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a project has no unserializable field")
    }

    /// Parses a project from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error when `json` is not valid JSON or a field has the wrong type.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Project, TvPlacement};
    use crate::scene::MapObject;

    #[test]
    fn round_trips_through_json() {
        for tv_display in [
            TvPlacement::Auto,
            TvPlacement::Window,
            TvPlacement::Display("HDMI-1".to_owned()),
        ] {
            for swap_windows in [false, true] {
                let project = Project {
                    tv_display: tv_display.clone(),
                    swap_windows,
                    ..Project::default()
                };
                let json = project.to_json();
                assert_eq!(Project::from_json(&json).unwrap(), project);
            }
        }
    }

    #[test]
    fn maps_round_trip_through_json() {
        let project = Project {
            maps: vec![MapObject {
                path: PathBuf::from("maps/crypt.png"),
                center: (1.5, -2.0),
                grid_px: 140.0,
            }],
            ..Project::default()
        };
        let json = project.to_json();
        assert_eq!(Project::from_json(&json).unwrap(), project);
    }

    #[test]
    fn swap_windows_is_off_by_default() {
        assert!(!Project::default().swap_windows);
    }

    #[test]
    fn stores_the_display_choice_readably() {
        let project = Project {
            tv_display: TvPlacement::Display("HDMI-1".to_owned()),
            ..Project::default()
        };
        assert!(project.to_json().contains("\"display\": \"HDMI-1\""));
    }

    #[test]
    fn missing_fields_take_defaults() {
        assert_eq!(Project::from_json("{}").unwrap(), Project::default());
    }

    #[test]
    fn rejects_broken_json() {
        Project::from_json("{").unwrap_err();
    }
}
