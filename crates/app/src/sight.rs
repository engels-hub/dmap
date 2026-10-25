//! What blocks sight and what gives light: walls, doors and lights.
//!
//! An import from Foundry VTT or a Universal VTT file brings these with its
//! map. They hold world inches, as a stroke does, so a drag of the group
//! they share with the map carries them along. The DM screen draws them as
//! marks over the canvas, and the TV never shows them. PLAN.md 5.1 and 6.

// Rust guideline compliant 2026-02-21

use serde::{Deserialize, Serialize};

use crate::scene::{NodeId, Shown};

/// What one wall segment is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A plain wall.
    #[default]
    Wall,
    /// A door the DM opens and closes in play. An open door lets light
    /// through. Issue #18.
    Door {
        /// Whether the door stands open.
        open: bool,
    },
    /// A door the players do not see as a door.
    Secret {
        /// Whether the door stands open.
        open: bool,
    },
}

/// One straight piece of wall, from `a` to `b`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    /// One end, in inches.
    pub a: (f64, f64),
    /// The other end, in inches.
    pub b: (f64, f64),
    /// A wall, a door, or a secret door.
    #[serde(default)]
    pub kind: Kind,
    /// Whether the wall stops light.
    #[serde(default = "yes")]
    pub blocks_light: bool,
    /// Whether the wall stops sight.
    #[serde(default = "yes")]
    pub blocks_sight: bool,
}

fn yes() -> bool {
    true
}

impl Wall {
    /// A plain wall from `a` to `b` that stops light and sight.
    pub fn new(a: (f64, f64), b: (f64, f64)) -> Self {
        Self {
            a,
            b,
            kind: Kind::Wall,
            blocks_light: true,
            blocks_sight: true,
        }
    }

    /// Whether this segment is a door of either kind.
    pub fn is_door(&self) -> bool {
        matches!(self.kind, Kind::Door { .. } | Kind::Secret { .. })
    }

    /// Whether this segment is a door that stands open.
    pub fn is_open(&self) -> bool {
        matches!(
            self.kind,
            Kind::Door { open: true } | Kind::Secret { open: true }
        )
    }

    /// How far `at` lies from this segment, in inches.
    pub fn distance(&self, at: (f64, f64)) -> f64 {
        let (dx, dy) = (self.b.0 - self.a.0, self.b.1 - self.a.1);
        let length = dx * dx + dy * dy;
        let t = if length > 0.0 {
            (((at.0 - self.a.0) * dx + (at.1 - self.a.1) * dy) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (at.0 - self.a.0 - t * dx).hypot(at.1 - self.a.1 - t * dy)
    }
}

/// Every wall of one map, as one node in the tree.
///
/// A Foundry scene holds hundreds of segments. One row for each would bury
/// the map they belong to, so the set takes one row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Walls {
    /// The name of this set in the tree.
    #[serde(default)]
    pub id: NodeId,
    /// Which screens this set draws on.
    #[serde(default)]
    pub shown: Shown,
    /// The segments.
    #[serde(default)]
    pub segments: Vec<Wall>,
}

impl Walls {
    /// The box around every segment, as `(min, max)` in inches.
    pub fn bounds(&self) -> Option<((f64, f64), (f64, f64))> {
        reach(self.segments.iter().flat_map(|wall| [wall.a, wall.b]))
    }

    /// The segment nearest `at`, if one lies within `reach` inches.
    ///
    /// `doors` keeps the search to doors, so a click near a door that
    /// meets a wall finds the door.
    pub fn nearest(&self, at: (f64, f64), reach: f64, doors: bool) -> Option<usize> {
        self.segments
            .iter()
            .enumerate()
            .filter(|(_, wall)| !doors || wall.is_door())
            .map(|(place, wall)| (place, wall.distance(at)))
            .filter(|(_, distance)| *distance <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(place, _)| place)
    }

    /// The same set, with the door at `place` opened or closed. A segment
    /// that is not a door stays as it is. Issue #18.
    pub fn toggled(&self, place: usize) -> Self {
        let mut after = self.clone();
        if let Some(wall) = after.segments.get_mut(place) {
            wall.kind = match wall.kind {
                Kind::Door { open } => Kind::Door { open: !open },
                Kind::Secret { open } => Kind::Secret { open: !open },
                Kind::Wall => Kind::Wall,
            };
        }
        after
    }

    /// The same set, every end moved by `f`.
    fn mapped(&self, f: impl Fn((f64, f64)) -> (f64, f64)) -> Self {
        Self {
            segments: self
                .segments
                .iter()
                .map(|wall| Wall {
                    a: f(wall.a),
                    b: f(wall.b),
                    ..*wall
                })
                .collect(),
            id: self.id,
            shown: self.shown,
        }
    }

    /// The same set, moved by `step` inches.
    pub fn moved(&self, step: (f64, f64)) -> Self {
        self.mapped(|at| shift(at, step))
    }

    /// The same set, grown by `factor` around `pivot`.
    pub fn scaled(&self, pivot: (f64, f64), factor: f64) -> Self {
        self.mapped(|at| grow(at, pivot, factor))
    }

    /// The same set, turned by `angle` radians around `pivot`.
    pub fn turned(&self, pivot: (f64, f64), angle: f64) -> Self {
        self.mapped(|at| turn(at, pivot, angle))
    }
}

/// An area darker than the rest of the scene, which the lights reveal.
///
/// The scene holds one darkness for all of its canvas. This holds another
/// one for the maps it lies over, such as a cellar under a sunlit yard. A
/// darkness higher in the tree wins where two of them overlap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Darkness {
    /// The name of this area in the tree.
    #[serde(default)]
    pub id: NodeId,
    /// Which screens this area darkens.
    #[serde(default)]
    pub shown: Shown,
    /// The four corners, in inches, in order around the area.
    pub corners: [(f64, f64); 4],
    /// How dark the area is where no light reaches: 0 is the map as it
    /// is, 1 shows the fill alone.
    pub strength: f64,
    /// What the area shows where no light reaches.
    #[serde(default)]
    pub fill: Fill,
}

/// What an area of darkness shows where no light reaches.
///
/// The GPU makes each pattern from noise, so the program carries no image
/// for them. A pattern holds to the canvas and moves with the camera, so
/// a cloud stays over the same part of the map.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fill {
    /// Black, or the color the scene's ambient light leaves.
    #[default]
    Black,
    /// Pale clouds, for a map seen from above.
    Clouds,
    /// Dark smoke.
    Smoke,
    /// Blue-grey mist.
    Mist,
    /// A night sky with stars.
    Stars,
}

impl Fill {
    /// Every fill, in the order the panel lists them.
    pub const ALL: [Self; 5] = [
        Self::Black,
        Self::Clouds,
        Self::Smoke,
        Self::Mist,
        Self::Stars,
    ];

    /// The number the shader knows this fill by.
    pub fn code(self) -> u32 {
        match self {
            Self::Black => 0,
            Self::Clouds => 1,
            Self::Smoke => 2,
            Self::Mist => 3,
            Self::Stars => 4,
        }
    }

    /// What the panel calls this fill.
    pub fn name(self) -> &'static str {
        match self {
            Self::Black => crate::text::fill_black(),
            Self::Clouds => crate::text::fill_clouds(),
            Self::Smoke => crate::text::fill_smoke(),
            Self::Mist => crate::text::fill_mist(),
            Self::Stars => crate::text::fill_stars(),
        }
    }
}

impl Darkness {
    /// An area that covers the box from `min` to `max`.
    pub fn covering(id: NodeId, (min, max): ((f64, f64), (f64, f64)), strength: f64) -> Self {
        Self {
            id,
            shown: Shown::default(),
            corners: [min, (max.0, min.1), max, (min.0, max.1)],
            strength,
            fill: Fill::Black,
        }
    }

    /// The box around the area, as `(min, max)` in inches.
    pub fn bounds(&self) -> ((f64, f64), (f64, f64)) {
        reach(self.corners.into_iter()).unwrap_or(((0.0, 0.0), (0.0, 0.0)))
    }

    /// How far `at` lies from the outline of the area, in inches.
    pub fn edge_distance(&self, at: (f64, f64)) -> f64 {
        (0..4)
            .map(|i| Wall::new(self.corners[i], self.corners[(i + 1) % 4]).distance(at))
            .fold(f64::INFINITY, f64::min)
    }

    fn mapped(&self, f: impl Fn((f64, f64)) -> (f64, f64)) -> Self {
        Self {
            corners: self.corners.map(f),
            ..self.clone()
        }
    }

    /// The same area, moved by `step` inches.
    pub fn moved(&self, step: (f64, f64)) -> Self {
        self.mapped(|at| shift(at, step))
    }

    /// The same area, grown by `factor` around `pivot`.
    pub fn scaled(&self, pivot: (f64, f64), factor: f64) -> Self {
        self.mapped(|at| grow(at, pivot, factor))
    }

    /// The same area, turned by `angle` radians around `pivot`.
    pub fn turned(&self, pivot: (f64, f64), angle: f64) -> Self {
        self.mapped(|at| turn(at, pivot, angle))
    }
}

/// One light on the map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Light {
    /// The name of this light in the tree.
    #[serde(default)]
    pub id: NodeId,
    /// Which screens this light draws on.
    #[serde(default)]
    pub shown: Shown,
    /// Where the light stands, in inches.
    pub center: (f64, f64),
    /// How far the bright light reaches, in inches.
    pub bright: f64,
    /// How far the dim light reaches, in inches.
    pub dim: f64,
    /// The color of the light, as red, green and blue.
    #[serde(default = "white")]
    pub color: [u8; 3],
    /// How wide the cone of light opens, in radians. A full turn is a
    /// light that shines all around.
    #[serde(default = "full_turn")]
    pub angle: f64,
    /// Where the middle of the cone points, in radians, clockwise on screen
    /// from straight down. Foundry VTT measures it the same way.
    #[serde(default)]
    pub rotation: f64,
}

fn white() -> [u8; 3] {
    [255, 255, 255]
}

fn full_turn() -> f64 {
    std::f64::consts::TAU
}

impl Light {
    /// A white light that shines all around.
    pub fn new(id: NodeId, center: (f64, f64), bright: f64, dim: f64) -> Self {
        Self {
            id,
            shown: Shown::default(),
            center,
            bright,
            dim,
            color: white(),
            angle: full_turn(),
            rotation: 0.0,
        }
    }

    /// How far the light reaches at all, in inches.
    pub fn reach(&self) -> f64 {
        self.bright.max(self.dim)
    }

    /// The box the dim light covers, as `(min, max)` in inches.
    pub fn bounds(&self) -> ((f64, f64), (f64, f64)) {
        let r = self.reach();
        (
            (self.center.0 - r, self.center.1 - r),
            (self.center.0 + r, self.center.1 + r),
        )
    }

    /// The same light, moved by `step` inches.
    pub fn moved(&self, step: (f64, f64)) -> Self {
        Self {
            center: shift(self.center, step),
            ..self.clone()
        }
    }

    /// The same light, grown by `factor` around `pivot`. The radii grow
    /// with it, so the light covers the same part of the map.
    pub fn scaled(&self, pivot: (f64, f64), factor: f64) -> Self {
        Self {
            center: grow(self.center, pivot, factor),
            bright: self.bright * factor,
            dim: self.dim * factor,
            ..self.clone()
        }
    }

    /// The same light, turned by `angle` radians around `pivot`. A cone
    /// turns with it.
    pub fn turned(&self, pivot: (f64, f64), angle: f64) -> Self {
        Self {
            center: turn(self.center, pivot, angle),
            rotation: self.rotation + angle,
            ..self.clone()
        }
    }
}

fn shift(at: (f64, f64), step: (f64, f64)) -> (f64, f64) {
    (at.0 + step.0, at.1 + step.1)
}

fn grow(at: (f64, f64), pivot: (f64, f64), factor: f64) -> (f64, f64) {
    (
        pivot.0 + (at.0 - pivot.0) * factor,
        pivot.1 + (at.1 - pivot.1) * factor,
    )
}

fn turn(at: (f64, f64), pivot: (f64, f64), angle: f64) -> (f64, f64) {
    let (sin, cos) = angle.sin_cos();
    let (dx, dy) = (at.0 - pivot.0, at.1 - pivot.1);
    (pivot.0 + dx * cos - dy * sin, pivot.1 + dx * sin + dy * cos)
}

/// The box around a set of points, as `(min, max)`.
fn reach(points: impl Iterator<Item = (f64, f64)>) -> Option<((f64, f64), (f64, f64))> {
    points.fold(None, |reach, at| {
        Some(match reach {
            None => (at, at),
            Some((min, max)) => (
                (min.0.min(at.0), min.1.min(at.1)),
                (max.0.max(at.0), max.1.max(at.1)),
            ),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{Light, Wall, Walls};

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn walls_move_turn_and_grow_as_one_piece() {
        let walls = Walls {
            id: 1,
            shown: crate::scene::Shown::default(),
            segments: vec![Wall::new((1.0, 0.0), (2.0, 0.0))],
        };
        let moved = walls.moved((1.0, 1.0));
        assert!(close(moved.segments[0].a, (2.0, 1.0)));
        let turned = walls.turned((0.0, 0.0), std::f64::consts::FRAC_PI_2);
        assert!(close(turned.segments[0].b, (0.0, 2.0)));
        let grown = walls.scaled((0.0, 0.0), 2.0);
        assert!(close(grown.segments[0].b, (4.0, 0.0)));
        assert_eq!(walls.bounds(), Some(((1.0, 0.0), (2.0, 0.0))));
    }

    #[test]
    fn a_door_near_the_click_opens_and_a_wall_does_not() {
        let mut door = Wall::new((1.0, 0.0), (2.0, 0.0));
        door.kind = super::Kind::Door { open: false };
        let walls = Walls {
            id: 1,
            shown: crate::scene::Shown::default(),
            segments: vec![Wall::new((0.0, 0.0), (1.0, 0.0)), door],
        };
        assert_eq!(walls.nearest((0.5, 0.05), 0.1, false), Some(0));
        assert_eq!(walls.nearest((0.5, 0.05), 0.1, true), None);
        assert_eq!(walls.nearest((1.5, 0.05), 0.1, true), Some(1));
        assert_eq!(walls.nearest((1.5, 1.0), 0.1, true), None);
        let opened = walls.toggled(1);
        assert!(opened.segments[1].is_open());
        assert!(!opened.toggled(1).segments[1].is_open());
        assert_eq!(walls.toggled(0), walls);
    }

    #[test]
    fn a_darkness_covers_its_box_and_moves_as_one_piece() {
        let area = super::Darkness::covering(1, ((0.0, 0.0), (4.0, 2.0)), 0.5);
        assert_eq!(area.bounds(), ((0.0, 0.0), (4.0, 2.0)));
        assert!((area.edge_distance((2.0, 0.5)) - 0.5).abs() < 1e-9);
        assert_eq!(area.moved((1.0, 1.0)).bounds(), ((1.0, 1.0), (5.0, 3.0)));
        let turned = area.turned((0.0, 0.0), std::f64::consts::FRAC_PI_2);
        assert!(close(turned.corners[1], (0.0, 4.0)));
    }

    #[test]
    fn a_grown_light_reaches_as_far_on_the_map() {
        let light = Light::new(1, (1.0, 0.0), 2.0, 4.0);
        let grown = light.scaled((0.0, 0.0), 0.5);
        assert!(close(grown.center, (0.5, 0.0)));
        assert!((grown.dim - 2.0).abs() < 1e-9);
        assert!((grown.reach() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_scene_with_walls_and_lights_reads_back_as_it_was_written() {
        let mut scene = crate::scene::Scene::default();
        let mut door = Wall::new((0.0, 0.0), (1.0, 0.0));
        door.kind = super::Kind::Door { open: true };
        scene.root.children.push(crate::scene::Node::Walls(Walls {
            id: 1,
            shown: crate::scene::Shown::default(),
            segments: vec![Wall::new((0.0, 0.0), (0.0, 1.0)), door],
        }));
        scene
            .root
            .children
            .push(crate::scene::Node::Light(Light::new(
                2,
                (3.0, 4.0),
                1.0,
                2.0,
            )));
        let back = crate::scene::Scene::from_json(&scene.to_json()).unwrap();
        assert_eq!(back, scene);
    }

    #[test]
    fn an_undo_puts_a_moved_wall_set_back() {
        use crate::command::{Command as _, SetSight};
        use crate::scene::Node;
        let walls = Walls {
            id: 1,
            shown: crate::scene::Shown::default(),
            segments: vec![Wall::new((0.0, 0.0), (1.0, 0.0))],
        };
        let mut scene = crate::scene::Scene::default();
        scene.root.children.push(Node::Walls(walls.clone()));
        let change = SetSight {
            before: vec![Node::Walls(walls.clone())],
            after: vec![Node::Walls(walls.moved((2.0, 0.0)))],
        };
        change.apply(&mut scene);
        assert_eq!(scene.root.children[0], Node::Walls(walls.moved((2.0, 0.0))));
        change.revert(&mut scene);
        assert_eq!(scene.root.children[0], Node::Walls(walls));
    }
}
