//! The project file: everything the DM set up for one campaign.

// Rust guideline compliant 2026-02-21

use serde::{Deserialize, Serialize};

/// Everything the DM set up for one campaign.
///
/// Every field has a default, so a file from an older version still loads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    /// Name of the display that shows the TV window, or `None` for a window.
    pub tv_display: Option<String>,
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
    use super::Project;

    #[test]
    fn round_trips_through_json() {
        let project = Project {
            tv_display: Some("HDMI-1".to_owned()),
        };
        let json = project.to_json();
        assert_eq!(Project::from_json(&json).unwrap(), project);
    }

    #[test]
    fn missing_fields_take_defaults() {
        assert_eq!(Project::from_json("{}").unwrap(), Project::default());
    }

    #[test]
    fn rejects_broken_json() {
        assert!(matches!(Project::from_json("{"), Err(_)));
    }
}
