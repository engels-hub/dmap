//! The scene on disk: its JSON, and the maps beside it.
//!
//! A scene folder holds `scene.json` and every map the scene points at.
//! A map from elsewhere is copied in, under a name nothing else uses.

// Rust guideline compliant 2026-02-21

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

use super::Scene;

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
        serde_json::from_str(json)
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
