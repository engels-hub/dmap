//! What is on the canvas.
//!
//! This module holds the tree itself: the nodes, the groups and the root.
//! One submodule stands for each thing the program asks of it. `tree`
//! walks it and changes its shape, `draw` says what draws and in what
//! order, `asset` holds one map and where a set of them stands, and
//! `files` reads and writes the scene folder.

// Rust guideline compliant 2026-02-21

mod asset;
mod draw;
mod files;
mod tree;

pub use asset::{Asset, Placed, bounds, placed, rotate_about, scale_about};
pub use draw::{assets, dm_draw_order, draw_order, groups, ink_order, under};
pub use files::copy_into_scene;
pub use tree::{
    ancestors, asset_mut, assets_of, find, group_mut, group_names, group_selection, has_group,
    ink_group, move_above, move_into, name_of, normalize, parent_of, path_to, push_into,
    reorder_all, share_parent, shown_mut, stroke_mut, take_node, ungroup,
};

use serde::{Deserialize, Serialize};

use crate::stroke::Stroke;
use crate::tvbox::TvBox;

/// The name of every node in one scene.
///
/// Two assets may point at one image file, so a file name cannot say which
/// asset the DM means. A hex crawl holds five hex textures and a hundred
/// assets that point at them.
pub type NodeId = u64;

/// Who is looking at the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// The DM window.
    Dm,
    /// The TV the players watch.
    Tv,
}

/// Which screens a node draws on, when the groups above it allow it.
///
/// The DM and the players do not have to agree. A group of notes shows for
/// the DM and stays off the TV.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shown {
    /// Draw on the DM screen.
    pub dm: bool,
    /// Draw on the TV.
    pub tv: bool,
}

impl Default for Shown {
    fn default() -> Self {
        Self { dm: true, tv: true }
    }
}

impl Shown {
    /// Whether this pair says yes to one screen.
    pub fn says(self, audience: Audience) -> bool {
        match audience {
            Audience::Dm => self.dm,
            Audience::Tv => self.tv,
        }
    }
}

/// One thing in the tree: a group, or an asset on the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    /// A group of other nodes.
    Group(Group),
    /// One image on the canvas.
    Asset(Asset),
    /// One thing the DM drew, over the images. See [`crate::stroke`].
    Stroke(Stroke),
}

impl Node {
    /// The name of this node in the tree.
    pub fn id(&self) -> NodeId {
        match self {
            Self::Group(group) => group.id,
            Self::Asset(asset) => asset.id,
            Self::Stroke(stroke) => stroke.id,
        }
    }

    /// Whether this node draws for `audience`, on its own account.
    ///
    /// A group above it can still keep it off the screen.
    pub fn shows(&self, audience: Audience) -> bool {
        match self {
            Self::Group(group) => group.shows(audience),
            Self::Asset(asset) => asset.shows(audience),
            Self::Stroke(stroke) => stroke.shown.says(audience),
        }
    }

    /// The group this node holds, if it is one.
    pub fn group(&self) -> Option<&Group> {
        match self {
            Self::Group(group) => Some(group),
            _ => None,
        }
    }

    /// The asset this node holds, if it is one.
    pub fn asset(&self) -> Option<&Asset> {
        match self {
            Self::Asset(asset) => Some(asset),
            _ => None,
        }
    }

    /// The stroke this node holds, if it is one.
    pub fn stroke(&self) -> Option<&Stroke> {
        match self {
            Self::Stroke(stroke) => Some(stroke),
            _ => None,
        }
    }
}

/// A group of nodes, which the DM moves and hides as one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    /// The name of this group in the tree.
    #[serde(default)]
    pub id: NodeId,
    /// What the DM calls this group.
    #[serde(default = "group_name")]
    pub name: String,
    /// Which screens this group draws on.
    #[serde(default)]
    pub shown: Shown,
    /// What sits in this group, bottom one first.
    #[serde(default)]
    pub children: Vec<Node>,
    /// Whether a new stroke joins this group.
    ///
    /// The Draw view keeps its strokes together, and it finds the group
    /// by this mark and not by its name, so the group holds whatever the
    /// DM renamed it to and reads the same in every language.
    #[serde(default)]
    pub ink: bool,
}

fn group_name() -> String {
    "Group".to_owned()
}

impl Group {
    /// An empty group with a name of its own, shown on both screens.
    pub fn new(id: NodeId, name: String) -> Self {
        Self {
            id,
            name,
            shown: Shown::default(),
            children: Vec::new(),
            ink: false,
        }
    }

    /// Whether this group draws for `audience`.
    pub fn shows(&self, audience: Audience) -> bool {
        self.shown.says(audience)
    }
}

/// Everything one scene holds.
///
/// A scene lives in a folder of its own. The folder holds this file, under
/// the name `scene.json`, and the images the assets point at. Every path in
/// it is the name of a file in that folder, so the whole folder moves to
/// another machine and still opens.
///
/// The scene is one tree. The root group holds every other node, keeps its
/// name, and always shows on both screens.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scene {
    /// The group that holds everything else.
    pub root: Group,
    /// The part of the canvas the TV shows.
    pub tv_box: TvBox,
}

/// The name of the root group. The DM cannot change it.
pub const ROOT_NAME: &str = "Scene";

/// The id of the root group. Every other node takes a larger one.
pub const ROOT_ID: NodeId = 0;

impl Default for Scene {
    fn default() -> Self {
        Self {
            root: Group::new(ROOT_ID, ROOT_NAME.to_owned()),
            tv_box: TvBox::default(),
        }
    }
}

/// What a scene file may hold.
///
/// A file from before the tree held a flat list of maps under `maps`. Both
/// shapes read into a scene, so an older scene opens.
#[derive(Default, Deserialize)]
#[serde(default)]
struct SceneFile {
    root: Option<Group>,
    tv_box: TvBox,
    maps: Vec<Asset>,
}

impl<'de> Deserialize<'de> for Scene {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let file = SceneFile::deserialize(deserializer)?;
        let mut scene = Self {
            root: file
                .root
                .unwrap_or_else(|| Group::new(ROOT_ID, ROOT_NAME.to_owned())),
            tv_box: file.tv_box,
        };
        // A scene from before the tree kept its maps in a flat list. They
        // join the root, in the order they had.
        scene
            .root
            .children
            .extend(file.maps.into_iter().map(Node::Asset));
        Ok(scene.repaired())
    }
}

impl Scene {
    /// The same scene, with a root that follows the rules and no id twice.
    ///
    /// A scene file is text a DM can edit, so it may hold a root with
    /// another name, or two nodes of one id. Every node needs a name of its
    /// own, since a selection points at nodes by id.
    fn repaired(mut self) -> Self {
        self.root.id = ROOT_ID;
        ROOT_NAME.clone_into(&mut self.root.name);
        self.root.shown = Shown::default();
        let mut taken = std::collections::HashSet::from([ROOT_ID]);
        let mut next = 1;
        rename_nodes(&mut self.root.children, &mut taken, &mut next);
        self
    }

    /// A name no node in this scene holds.
    pub fn next_id(&self) -> NodeId {
        let mut highest = ROOT_ID;
        for_each_node(&self.root.children, &mut |node| {
            highest = highest.max(node.id());
        });
        highest + 1
    }
}

/// Gives every node a name of its own, deepest last.
fn rename_nodes(
    nodes: &mut [Node],
    taken: &mut std::collections::HashSet<NodeId>,
    next: &mut NodeId,
) {
    for node in nodes {
        let id = match node {
            Node::Group(group) => &mut group.id,
            Node::Asset(asset) => &mut asset.id,
            Node::Stroke(stroke) => &mut stroke.id,
        };
        if *id == ROOT_ID || !taken.insert(*id) {
            while !taken.insert(*next) {
                *next += 1;
            }
            *id = *next;
        }
        if let Node::Group(group) = node {
            rename_nodes(&mut group.children, taken, next);
        }
    }
}

/// Walks every node under `nodes`, a parent before its children.
fn for_each_node(nodes: &[Node], visit: &mut impl FnMut(&Node)) {
    for node in nodes {
        visit(node);
        if let Node::Group(group) = node {
            for_each_node(&group.children, visit);
        }
    }
}

#[cfg(test)]
mod tests;
