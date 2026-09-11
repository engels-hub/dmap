//! One image on the canvas, and where a set of them stands.
//!
//! An asset holds its center, its turn and its scale, so a drag writes
//! the few numbers a map needs. `Placed` keeps those numbers as they
//! stood when a drag began, so every revert is exact.

// Rust guideline compliant 2026-02-21

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Audience, Group, NodeId, Scene, Shown, asset_mut, assets_of, find, normalize, under};

/// Where an asset stood when a drag began.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placed {
    /// The asset this belongs to.
    pub id: NodeId,
    /// Where the asset stood, in inches.
    pub center: (f64, f64),
    /// The size factor it had.
    pub scale: f64,
    /// The turn it had, in radians.
    pub rotation: f64,
}

/// Where every asset under a selection stands now.
pub fn placed(scene: &Scene, ids: &[NodeId]) -> Vec<Placed> {
    normalize(scene, ids)
        .iter()
        .flat_map(|id| assets_of(scene, *id))
        .filter_map(|id| {
            let asset = find(scene, id)?.asset()?;
            Some(Placed {
                id,
                center: asset.center,
                scale: asset.scale,
                rotation: asset.rotation,
            })
        })
        .collect()
}

/// Grows or shrinks a set of assets around one point.
///
/// Each asset keeps its distance from the point in proportion, so the set
/// holds its shape.
pub fn scale_about(scene: &mut Scene, starts: &[Placed], pivot: (f64, f64), factor: f64) {
    for start in starts {
        let Some(asset) = asset_mut(scene, start.id) else {
            continue;
        };
        asset.scale = start.scale * factor;
        asset.center = (
            pivot.0 + (start.center.0 - pivot.0) * factor,
            pivot.1 + (start.center.1 - pivot.1) * factor,
        );
    }
}

/// Turns a set of assets around one point.
///
/// Each asset turns on its own as well, so the set holds its shape.
pub fn rotate_about(scene: &mut Scene, starts: &[Placed], pivot: (f64, f64), angle: f64) {
    let (sin, cos) = angle.sin_cos();
    for start in starts {
        let Some(asset) = asset_mut(scene, start.id) else {
            continue;
        };
        let (dx, dy) = (start.center.0 - pivot.0, start.center.1 - pivot.1);
        asset.center = (pivot.0 + dx * cos - dy * sin, pivot.1 + dx * sin + dy * cos);
        asset.rotation = start.rotation + angle;
    }
}

/// The world rectangle a group covers, as `(min, max)` in inches.
///
/// The box holds every asset under the group, however deep. A group with
/// no asset yet, or one whose images have not loaded, covers nothing.
pub fn bounds(
    group: &Group,
    size_of: &dyn Fn(&Path) -> Option<(u32, u32)>,
) -> Option<((f64, f64), (f64, f64))> {
    let mut reach: Option<((f64, f64), (f64, f64))> = None;
    for asset in under(group) {
        let Some(size) = size_of(&asset.path) else {
            continue;
        };
        for corner in asset.corners(size) {
            reach = Some(match reach {
                None => (corner, corner),
                Some((min, max)) => (
                    (min.0.min(corner.0), min.1.min(corner.1)),
                    (max.0.max(corner.0), max.1.max(corner.1)),
                ),
            });
        }
    }
    reach
}

/// One image placed on the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    /// The name of this asset in the tree.
    #[serde(default)]
    pub id: NodeId,
    /// Which screens this asset draws on.
    #[serde(default)]
    pub shown: Shown,
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

impl Asset {
    /// Pixels per cell for a map whose grid size is not known yet.
    ///
    /// The same default as Foundry VTT, so many maps come out right at once.
    pub const DEFAULT_GRID_PX: f64 = 100.0;

    /// An asset at `center` with the default grid size.
    pub fn new(id: NodeId, path: PathBuf, center: (f64, f64)) -> Self {
        Self {
            id,
            shown: Shown::default(),
            path,
            center,
            grid_px: Self::DEFAULT_GRID_PX,
            rotation: 0.0,
            scale: 1.0,
            flip_x: false,
            flip_y: false,
            snap_offset: (0.0, 0.0),
        }
    }

    /// Whether this asset draws for `audience`, on its own account.
    pub fn shows(&self, audience: Audience) -> bool {
        self.shown.says(audience)
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
