//! What is on the canvas.

// Rust guideline compliant 2026-02-21

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

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
}

impl Node {
    /// The name of this node in the tree.
    pub fn id(&self) -> NodeId {
        match self {
            Self::Group(group) => group.id,
            Self::Asset(asset) => asset.id,
        }
    }

    /// Whether this node draws for `audience`, on its own account.
    ///
    /// A group above it can still keep it off the screen.
    pub fn shows(&self, audience: Audience) -> bool {
        match self {
            Self::Group(group) => group.shows(audience),
            Self::Asset(asset) => asset.shows(audience),
        }
    }

    /// The group this node holds, if it is one.
    pub fn group(&self) -> Option<&Group> {
        match self {
            Self::Group(group) => Some(group),
            Self::Asset(_) => None,
        }
    }

    /// The asset this node holds, if it is one.
    pub fn asset(&self) -> Option<&Asset> {
        match self {
            Self::Asset(asset) => Some(asset),
            Self::Group(_) => None,
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

/// The assets one screen draws, in the order they draw.
///
/// The walk starts at the root and takes each child in turn, so a group
/// takes one place in the order of its parent and its children draw inside
/// that place. A node draws only when it and every group above it show for
/// this screen.
pub fn draw_order(scene: &Scene, audience: Audience) -> Vec<&Asset> {
    let mut drawn = Vec::new();
    collect_drawn(&scene.root.children, audience, &mut drawn);
    drawn
}

fn collect_drawn<'a>(nodes: &'a [Node], audience: Audience, drawn: &mut Vec<&'a Asset>) {
    for node in nodes {
        if !node.shows(audience) {
            continue;
        }
        match node {
            Node::Asset(asset) => drawn.push(asset),
            Node::Group(group) => collect_drawn(&group.children, audience, drawn),
        }
    }
}

/// Every asset in the scene, in the order they draw for the DM.
pub fn assets(scene: &Scene) -> Vec<&Asset> {
    draw_order(scene, Audience::Dm)
}

/// The assets the DM screen draws, each with whether the TV shows it too.
///
/// A group that is off for the TV takes every asset under it off the TV,
/// whatever the asset says. The DM screen draws the ones the TV misses at
/// half strength. DESIGN.md 5.6.
pub fn dm_draw_order(scene: &Scene) -> Vec<(&Asset, bool)> {
    let mut drawn = Vec::new();
    collect_for_dm(
        &scene.root.children,
        scene.root.shows(Audience::Tv),
        &mut drawn,
    );
    drawn
}

fn collect_for_dm<'a>(nodes: &'a [Node], on_tv: bool, drawn: &mut Vec<(&'a Asset, bool)>) {
    for node in nodes {
        if !node.shows(Audience::Dm) {
            continue;
        }
        let reaches_tv = on_tv && node.shows(Audience::Tv);
        match node {
            Node::Asset(asset) => drawn.push((asset, reaches_tv)),
            Node::Group(group) => collect_for_dm(&group.children, reaches_tv, drawn),
        }
    }
}

/// The groups from the root down to `group`, with the name of each.
///
/// The last pair is `group` itself. The list is empty when the scene holds
/// no group of that id, so a caller can fall back to the root. DESIGN.md
/// 8.4 draws this as the path line over the objects list.
pub fn path_to(scene: &Scene, group: NodeId) -> Vec<(NodeId, String)> {
    if group == ROOT_ID {
        return vec![(ROOT_ID, scene.root.name.clone())];
    }
    if !has_group(scene, group) {
        return Vec::new();
    }
    let mut walk = ancestors(scene, group);
    walk.reverse();
    walk.push(group);
    walk.into_iter()
        .map(|id| {
            let name = if id == ROOT_ID {
                scene.root.name.clone()
            } else {
                find(scene, id)
                    .and_then(Node::group)
                    .map_or_else(String::new, |held| held.name.clone())
            };
            (id, name)
        })
        .collect()
}

/// The group that holds this node, and where in it the node sits.
pub fn parent_of(scene: &Scene, id: NodeId) -> Option<(NodeId, usize)> {
    parent_in(&scene.root, id)
}

fn parent_in(group: &Group, id: NodeId) -> Option<(NodeId, usize)> {
    if let Some(place) = group.children.iter().position(|node| node.id() == id) {
        return Some((group.id, place));
    }
    group
        .children
        .iter()
        .filter_map(Node::group)
        .find_map(|inside| parent_in(inside, id))
}

/// Every group above a node, the nearest one first.
///
/// A list that shows this node must open all of them.
pub fn ancestors(scene: &Scene, id: NodeId) -> Vec<NodeId> {
    let mut above = Vec::new();
    let mut walk = id;
    while let Some((parent, _)) = parent_of(scene, walk) {
        above.push(parent);
        if parent == ROOT_ID {
            break;
        }
        walk = parent;
    }
    above
}

/// Where a node sits: the place it takes at each level from the root.
///
/// The order of two paths is the order the two nodes draw in.
pub fn path_of(scene: &Scene, id: NodeId) -> Option<Vec<usize>> {
    let mut path = Vec::new();
    path_in(&scene.root, id, &mut path).then_some(path)
}

fn path_in(group: &Group, id: NodeId, path: &mut Vec<usize>) -> bool {
    for (place, node) in group.children.iter().enumerate() {
        if node.id() == id {
            path.push(place);
            return true;
        }
        if let Node::Group(inside) = node {
            path.push(place);
            if path_in(inside, id, path) {
                return true;
            }
            path.pop();
        }
    }
    false
}

/// The selection with nothing in it twice, and no node a group in it holds.
///
/// A group already carries its children, so a selection of a group and one
/// of its children is a selection of the group.
pub fn normalize(scene: &Scene, ids: &[NodeId]) -> Vec<NodeId> {
    let mut kept: Vec<NodeId> = Vec::new();
    for id in ids {
        let held = ancestors(scene, *id)
            .iter()
            .any(|above| ids.contains(above));
        if !held && !kept.contains(id) {
            kept.push(*id);
        }
    }
    kept
}

/// Takes a node out of the tree and hands it over.
pub fn take_node(scene: &mut Scene, id: NodeId) -> Option<Node> {
    take_in(&mut scene.root, id)
}

fn take_in(group: &mut Group, id: NodeId) -> Option<Node> {
    if let Some(place) = group.children.iter().position(|node| node.id() == id) {
        return Some(group.children.remove(place));
    }
    for child in &mut group.children {
        if let Node::Group(inside) = child
            && let Some(taken) = take_in(inside, id)
        {
            return Some(taken);
        }
    }
    None
}

/// The group that holds every node in a selection, when one does.
///
/// Order lives inside one group, so a move in order asks this first.
pub fn share_parent(scene: &Scene, ids: &[NodeId]) -> Option<NodeId> {
    let mut parents = ids
        .iter()
        .map(|id| parent_of(scene, *id).map(|(group, _)| group));
    let first = parents.next()??;
    parents.all(|parent| parent == Some(first)).then_some(first)
}

/// Moves a selection one place among its brothers and sisters.
///
/// The whole selection moves, or none of it does. A set that has reached
/// the end stays where it is, so two nodes in it never step over one
/// another and the order they draw in holds.
pub fn reorder_all(scene: &mut Scene, ids: &[NodeId], toward_top: bool) -> bool {
    let Some(parent) = share_parent(scene, ids) else {
        return false;
    };
    let Some(group) = group_mut(scene, parent) else {
        return false;
    };
    let mut places: Vec<usize> = ids
        .iter()
        .filter_map(|id| group.children.iter().position(|node| node.id() == *id))
        .collect();
    places.sort_unstable();
    let (Some(first), Some(last)) = (places.first(), places.last()) else {
        return false;
    };
    if toward_top {
        if last + 1 >= group.children.len() {
            return false;
        }
        for place in places.iter().rev() {
            group.children.swap(*place, place + 1);
        }
    } else {
        if *first == 0 {
            return false;
        }
        for place in &places {
            group.children.swap(*place, place - 1);
        }
    }
    true
}

/// Moves a node to where another node stands.
///
/// The node joins the group that holds `target`, and takes the place
/// `target` had, so it lands under `target` in the list. A group cannot go
/// into itself, or into anything it holds. Returns `true` when it moved.
pub fn move_above(scene: &mut Scene, id: NodeId, target: NodeId) -> bool {
    if id == target || target == ROOT_ID || ancestors(scene, target).contains(&id) {
        return false;
    }
    let Some(node) = take_node(scene, id) else {
        return false;
    };
    // The tree is one node shorter now, so the place of the target is the
    // place the node takes.
    let Some((parent, place)) = parent_of(scene, target) else {
        scene.root.children.push(node);
        return false;
    };
    let Some(group) = group_mut(scene, parent) else {
        scene.root.children.push(node);
        return false;
    };
    let place = place.min(group.children.len());
    group.children.insert(place, node);
    true
}

/// Moves a node into a group, on top of what the group holds.
///
/// A group cannot go into itself, or into anything it holds. Returns
/// `true` when it moved.
pub fn move_into(scene: &mut Scene, id: NodeId, parent: NodeId) -> bool {
    if id == parent || ancestors(scene, parent).contains(&id) {
        return false;
    }
    // A node already on top of that group has nowhere to go.
    if children_of(scene, parent)
        .and_then(<[Node]>::last)
        .map(Node::id)
        == Some(id)
    {
        return false;
    }
    let Some(node) = take_node(scene, id) else {
        return false;
    };
    let Some(group) = group_mut(scene, parent) else {
        scene.root.children.push(node);
        return false;
    };
    group.children.push(node);
    true
}

/// Takes a group out of the tree and leaves its children in its place.
///
/// The root stays: the tree needs a root. Returns `true` when it went.
pub fn ungroup(scene: &mut Scene, id: NodeId) -> bool {
    if id == ROOT_ID || find(scene, id).and_then(Node::group).is_none() {
        return false;
    }
    let Some((parent, place)) = parent_of(scene, id) else {
        return false;
    };
    let Some(Node::Group(group)) = take_node(scene, id) else {
        return false;
    };
    let Some(holder) = group_mut(scene, parent) else {
        return false;
    };
    for (step, child) in group.children.into_iter().enumerate() {
        let place = (place + step).min(holder.children.len());
        holder.children.insert(place, child);
    }
    true
}

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

/// What a group holds, the root included.
pub fn children_of(scene: &Scene, id: NodeId) -> Option<&[Node]> {
    if id == ROOT_ID {
        return Some(&scene.root.children);
    }
    Some(find(scene, id)?.group()?.children.as_slice())
}

/// Whether the tree still holds this group.
///
/// A group the DM marked can go while the mark stays behind, and a caller
/// that puts something in it must know.
pub fn has_group(scene: &Scene, id: NodeId) -> bool {
    children_of(scene, id).is_some()
}

/// Every group in the scene, the root first, each with how deep it sits.
///
/// A list of groups for the DM to choose from reads better with the depth.
pub fn group_names(scene: &Scene) -> Vec<(NodeId, String, usize)> {
    let mut found = vec![(ROOT_ID, scene.root.name.clone(), 0)];
    name_groups(&scene.root.children, 1, &mut found);
    found
}

fn name_groups(nodes: &[Node], depth: usize, found: &mut Vec<(NodeId, String, usize)>) {
    for group in nodes.iter().filter_map(Node::group) {
        found.push((group.id, group.name.clone(), depth));
        name_groups(&group.children, depth + 1, found);
    }
}

/// Every asset a node holds: the asset itself, or all under a group.
pub fn assets_of(scene: &Scene, id: NodeId) -> Vec<NodeId> {
    match find(scene, id) {
        Some(Node::Asset(asset)) => vec![asset.id],
        Some(Node::Group(group)) => under(group).iter().map(|asset| asset.id).collect(),
        None => Vec::new(),
    }
}

/// Puts a selection into a new group, and gives back its name.
///
/// The new group lands in the lowest group that holds every member, and
/// sits over what that group already holds. Every member leaves the group
/// it sat in, and they keep the order they drew in.
pub fn group_selection(scene: &mut Scene, ids: &[NodeId], name: String) -> Option<NodeId> {
    let ids = normalize(scene, ids);
    if ids.is_empty() {
        return None;
    }
    let mut members: Vec<(Vec<usize>, NodeId)> = ids
        .iter()
        .filter_map(|id| Some((path_of(scene, *id)?, *id)))
        .collect();
    if members.is_empty() {
        return None;
    }
    members.sort();
    // The name comes first. A node that has left the tree is not there to
    // count, so a name taken later could be one of theirs.
    let id = scene.next_id();
    let depth = shared_depth(&members);
    let holder = group_at(scene, &members[0].0[..depth])?;
    // The new group takes the place of the highest member. Every member
    // that sat in the holder itself leaves a gap, so the place moves down.
    let highest = members.iter().map(|(path, _)| path[depth]).max()?;
    let gaps = members
        .iter()
        .filter(|(path, _)| path.len() == depth + 1 && path[depth] <= highest)
        .count();
    let place = highest + 1 - gaps;
    let taken: Vec<Node> = members
        .iter()
        .filter_map(|(_, id)| take_node(scene, *id))
        .collect();
    let mut group = Group::new(id, name);
    group.children = taken;
    let holder = group_mut(scene, holder)?;
    let place = place.min(holder.children.len());
    holder.children.insert(place, Node::Group(group));
    Some(id)
}

/// How deep the paths of a selection run together.
fn shared_depth(members: &[(Vec<usize>, NodeId)]) -> usize {
    let first = &members[0].0;
    let mut depth = 0;
    while depth + 1 < first.len()
        && members
            .iter()
            .all(|(path, _)| path.len() > depth + 1 && path[depth] == first[depth])
    {
        depth += 1;
    }
    depth
}

/// The group at a path from the root.
fn group_at(scene: &Scene, path: &[usize]) -> Option<NodeId> {
    let mut group = &scene.root;
    for place in path {
        group = group.children.get(*place)?.group()?;
    }
    Some(group.id)
}

/// Puts a node into a group, on top of what the group already holds.
pub fn push_into(scene: &mut Scene, parent: NodeId, node: Node) -> bool {
    let Some(group) = group_mut(scene, parent) else {
        return false;
    };
    group.children.push(node);
    true
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

/// Every asset under a group, however deep.
pub fn under(group: &Group) -> Vec<&Asset> {
    let mut found = Vec::new();
    collect_assets(&group.children, &mut found);
    found
}

fn collect_assets<'a>(nodes: &'a [Node], found: &mut Vec<&'a Asset>) {
    for node in nodes {
        match node {
            Node::Asset(asset) => found.push(asset),
            Node::Group(group) => collect_assets(&group.children, found),
        }
    }
}

/// Every group in the scene that draws for the DM, with the root last.
///
/// The root has no box around it, so a caller that draws boxes skips it.
pub fn groups(scene: &Scene) -> Vec<&Group> {
    let mut found = Vec::new();
    collect_groups(&scene.root.children, &mut found);
    found
}

fn collect_groups<'a>(nodes: &'a [Node], found: &mut Vec<&'a Group>) {
    for group in nodes.iter().filter_map(Node::group) {
        found.push(group);
        collect_groups(&group.children, found);
    }
}

/// The node of this name, wherever it sits.
pub fn find(scene: &Scene, id: NodeId) -> Option<&Node> {
    find_in(&scene.root.children, id)
}

fn find_in(nodes: &[Node], id: NodeId) -> Option<&Node> {
    for node in nodes {
        if node.id() == id {
            return Some(node);
        }
        if let Node::Group(group) = node
            && let Some(found) = find_in(&group.children, id)
        {
            return Some(found);
        }
    }
    None
}

/// What the DM sees a node called: a group name, or a file name.
///
/// The history list and the panels both name a node this way, so a step
/// reads as the row the DM pressed.
pub fn name_of(scene: &Scene, id: NodeId) -> String {
    match find(scene, id) {
        Some(Node::Group(group)) => group.name.clone(),
        Some(Node::Asset(asset)) => asset.path.file_name().map_or_else(
            || asset.path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
        None => String::new(),
    }
}

/// The asset of this name, ready to change.
pub fn asset_mut(scene: &mut Scene, id: NodeId) -> Option<&mut Asset> {
    find_asset(&mut scene.root.children, id)
}

fn find_asset(nodes: &mut [Node], id: NodeId) -> Option<&mut Asset> {
    for node in nodes {
        match node {
            Node::Asset(asset) if asset.id == id => return Some(asset),
            Node::Group(group) => {
                if let Some(found) = find_asset(&mut group.children, id) {
                    return Some(found);
                }
            }
            Node::Asset(_) => {}
        }
    }
    None
}

/// The screens of this node, ready to change.
///
/// A group and an asset both carry the pair, so a switch in the list
/// reaches either one through this.
pub fn shown_mut(scene: &mut Scene, id: NodeId) -> Option<&mut Shown> {
    if has_group(scene, id) {
        return group_mut(scene, id).map(|group| &mut group.shown);
    }
    asset_mut(scene, id).map(|asset| &mut asset.shown)
}

/// The group of this name, ready to change.
pub fn group_mut(scene: &mut Scene, id: NodeId) -> Option<&mut Group> {
    if id == ROOT_ID {
        return Some(&mut scene.root);
    }
    find_group(&mut scene.root.children, id)
}

fn find_group(nodes: &mut [Node], id: NodeId) -> Option<&mut Group> {
    for node in nodes {
        if let Node::Group(group) = node {
            if group.id == id {
                return Some(group);
            }
            if let Some(found) = find_group(&mut group.children, id) {
                return Some(found);
            }
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        Asset, Audience, Group, Node, NodeId, ROOT_ID, Scene, assets, assets_of, copy_into_scene,
        dm_draw_order, draw_order, find, free_name, group_selection, move_above, normalize,
        parent_of, path_of, path_to,
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

    /// A scene with one map loose under the root and one inside a group.
    fn scene_with_a_group() -> Scene {
        let mut scene = Scene::default();
        scene.root.children.push(Node::Asset(Asset::new(
            1,
            PathBuf::from("loose.png"),
            (0.0, 0.0),
        )));
        let mut notes = Group::new(2, "Notes".to_owned());
        notes.children.push(Node::Asset(Asset::new(
            3,
            PathBuf::from("secret.png"),
            (0.0, 0.0),
        )));
        scene.root.children.push(Node::Group(notes));
        scene
    }

    /// The name of each asset the DM draws, with whether the TV shows it.
    fn dm_names(scene: &Scene) -> Vec<(String, bool)> {
        dm_draw_order(scene)
            .into_iter()
            .map(|(asset, on_tv)| (asset.path.to_string_lossy().into_owned(), on_tv))
            .collect()
    }

    #[test]
    fn the_path_to_the_root_is_the_root_alone() {
        let scene = scene_with_a_group();
        let path = path_to(&scene, ROOT_ID);
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].0, ROOT_ID);
    }

    #[test]
    fn the_path_to_a_group_starts_at_the_root_and_ends_at_the_group() {
        let scene = scene_with_a_group();
        let names: Vec<String> = path_to(&scene, 2).into_iter().map(|(_, n)| n).collect();
        assert_eq!(names.len(), 2);
        assert_eq!(names[1], "Notes");
        assert_eq!(path_to(&scene, 2)[0].0, ROOT_ID);
    }

    #[test]
    fn a_path_to_a_group_that_went_is_empty() {
        // The panel falls back to the root when its group is gone.
        let scene = scene_with_a_group();
        assert!(path_to(&scene, 99).is_empty());
        // An asset is not a group, so it names no path either.
        assert!(path_to(&scene, 3).is_empty());
    }

    #[test]
    fn the_dm_sees_every_map_and_which_ones_reach_the_tv() {
        let scene = scene_with_a_group();
        assert_eq!(
            dm_names(&scene),
            vec![
                ("loose.png".to_owned(), true),
                ("secret.png".to_owned(), true)
            ]
        );
    }

    #[test]
    fn a_map_off_the_tv_still_draws_for_the_dm() {
        // DESIGN.md 5.6: the DM keeps it, at half strength.
        let mut scene = scene_with_a_group();
        let Some(Node::Asset(loose)) = scene.root.children.first_mut() else {
            panic!("the first child is the loose map");
        };
        loose.shown.tv = false;
        assert_eq!(
            dm_names(&scene),
            vec![
                ("loose.png".to_owned(), false),
                ("secret.png".to_owned(), true)
            ]
        );
    }

    #[test]
    fn a_group_off_the_tv_takes_its_children_off_the_tv() {
        let mut scene = scene_with_a_group();
        let Some(Node::Group(notes)) = scene.root.children.get_mut(1) else {
            panic!("the second child is the group");
        };
        notes.shown.tv = false;
        // The asset inside says yes, and the group over it still wins.
        assert_eq!(
            dm_names(&scene),
            vec![
                ("loose.png".to_owned(), true),
                ("secret.png".to_owned(), false)
            ]
        );
    }

    #[test]
    fn a_map_off_the_dm_screen_leaves_the_dm_order() {
        // Half strength belongs to the TV switch. The DM switch still hides.
        let mut scene = scene_with_a_group();
        let Some(Node::Asset(loose)) = scene.root.children.first_mut() else {
            panic!("the first child is the loose map");
        };
        loose.shown.dm = false;
        assert_eq!(dm_names(&scene), vec![("secret.png".to_owned(), true)]);
    }

    #[test]
    fn the_dm_order_and_the_tv_order_hold_the_same_maps_when_both_show() {
        let scene = scene_with_a_group();
        let tv: Vec<String> = draw_order(&scene, Audience::Tv)
            .into_iter()
            .map(|asset| asset.path.to_string_lossy().into_owned())
            .collect();
        let dm: Vec<String> = dm_names(&scene).into_iter().map(|(name, _)| name).collect();
        assert_eq!(dm, tv);
    }

    #[test]
    fn a_map_spans_its_pixels_divided_by_pixels_per_cell() {
        let map = Asset {
            grid_px: 50.0,
            ..Asset::new(0, PathBuf::from("crypt.png"), (10.0, 5.0))
        };
        let corners = map.corners((400, 300));
        assert!(close(corners[0], (6.0, 2.0)));
        assert!(close(corners[2], (14.0, 8.0)));
    }

    #[test]
    fn corners_of_an_unrotated_map_are_its_rect() {
        let map = Asset::new(0, PathBuf::from("m.png"), (10.0, 5.0));
        let corners = map.corners((400, 300));
        assert!(close(corners[0], (8.0, 3.5)));
        assert!(close(corners[1], (12.0, 3.5)));
        assert!(close(corners[2], (12.0, 6.5)));
        assert!(close(corners[3], (8.0, 6.5)));
    }

    #[test]
    fn a_quarter_turn_swaps_width_and_height() {
        let mut map = Asset::new(0, PathBuf::from("m.png"), (0.0, 0.0));
        map.rotation = std::f64::consts::FRAC_PI_2;
        let corners = map.corners((400, 300));
        // The image's top-left corner moves to the top-right of the turned map.
        assert!(close(corners[0], (1.5, -2.0)));
        assert!(close(corners[1], (1.5, 2.0)));
    }

    #[test]
    fn scale_grows_the_map_around_its_center() {
        let mut map = Asset::new(0, PathBuf::from("m.png"), (0.0, 0.0));
        map.scale = 2.0;
        let corners = map.corners((400, 300));
        assert!(close(corners[0], (-4.0, -3.0)));
        assert!(close(corners[2], (4.0, 3.0)));
    }

    #[test]
    fn old_project_files_without_transform_fields_still_load() {
        let json = r#"{"path":"m.png","center":[1.0,2.0],"grid_px":50.0}"#;
        let map: Asset = serde_json::from_str(json).unwrap();
        assert!((map.rotation).abs() < 1e-9);
        assert!((map.scale - 1.0).abs() < 1e-9);
        assert!(!map.flip_x && !map.flip_y);
        // A map from before this field snaps to the canvas grid, as it did.
        assert_eq!(map.snap_offset, (0.0, 0.0));
    }

    #[test]
    fn a_new_map_uses_the_default_pixels_per_cell() {
        let map = Asset::new(0, PathBuf::from("crypt.png"), (0.0, 0.0));
        assert!((map.grid_px - Asset::DEFAULT_GRID_PX).abs() < 1e-9);
    }

    #[test]
    fn a_scene_round_trips_through_json() {
        let mut scene = Scene {
            tv_box: TvBox {
                center: (3.0, -1.5),
                width: 36.0,
            },
            ..Scene::default()
        };
        let asset = Asset {
            grid_px: 140.0,
            rotation: 0.5,
            scale: 1.5,
            flip_x: true,
            snap_offset: (0.25, 0.75),
            ..Asset::new(1, PathBuf::from("crypt.png"), (1.5, -2.0))
        };
        let mut notes = Group::new(2, "Notes".to_owned());
        notes.shown.tv = false;
        notes.children.push(Node::Asset(Asset::new(
            3,
            PathBuf::from("note.png"),
            (0.0, 0.0),
        )));
        scene.root.children.push(Node::Asset(asset));
        scene.root.children.push(Node::Group(notes));
        assert_eq!(Scene::from_json(&scene.to_json()).unwrap(), scene);
    }

    #[test]
    fn an_empty_scene_file_gives_an_empty_scene() {
        assert_eq!(Scene::from_json("{}").unwrap(), Scene::default());
    }

    #[test]
    fn a_scene_holds_the_names_of_files_beside_it() {
        let mut scene = Scene::default();
        scene.root.children.push(Node::Asset(Asset::new(
            1,
            PathBuf::from("crypt.png"),
            (0.0, 0.0),
        )));
        assert!(scene.to_json().contains("\"crypt.png\""));
        // The file says what each node is, so a DM can read the tree.
        assert!(scene.to_json().contains("\"type\": \"asset\""));
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

    /// A root with an asset, a group, and an asset inside the group.
    fn nested() -> Scene {
        let mut scene = Scene::default();
        let mut notes = Group::new(2, "Notes".to_owned());
        notes.children.push(Node::Asset(Asset::new(
            3,
            PathBuf::from("note.png"),
            (0.0, 0.0),
        )));
        scene.root.children.push(Node::Asset(Asset::new(
            1,
            PathBuf::from("floor.png"),
            (0.0, 0.0),
        )));
        scene.root.children.push(Node::Group(notes));
        scene
    }

    #[test]
    fn a_group_draws_inside_the_place_it_holds() {
        let scene = nested();
        let order: Vec<_> = draw_order(&scene, Audience::Dm)
            .iter()
            .map(|asset| asset.id)
            .collect();
        assert_eq!(order, vec![1, 3]);
    }

    #[test]
    fn a_group_that_is_off_takes_its_children_off() {
        let mut scene = nested();
        let Some(Node::Group(notes)) = scene.root.children.get_mut(1) else {
            panic!("the second child is the group");
        };
        notes.shown.tv = false;
        assert_eq!(draw_order(&scene, Audience::Dm).len(), 2);
        assert_eq!(draw_order(&scene, Audience::Tv).len(), 1);
    }

    #[test]
    fn an_asset_can_hide_on_its_own() {
        let mut scene = nested();
        let Some(Node::Asset(floor)) = scene.root.children.get_mut(0) else {
            panic!("the first child is the asset");
        };
        floor.shown.tv = false;
        assert_eq!(draw_order(&scene, Audience::Tv)[0].id, 3);
    }

    #[test]
    fn a_scene_from_before_the_tree_opens_under_the_root() {
        let json = r#"{"maps":[
            {"path":"a.png","center":[0.0,0.0],"grid_px":50.0},
            {"path":"b.png","center":[1.0,1.0],"grid_px":50.0}],
            "tv_box":{"center":[0.0,0.0],"width":40.0}}"#;
        let scene = Scene::from_json(json).unwrap();
        assert_eq!(scene.root.children.len(), 2);
        assert_eq!(assets(&scene).len(), 2);
        assert!((scene.tv_box.width - 40.0).abs() < 1e-9);
        // Every asset took a name of its own.
        let ids: Vec<_> = assets(&scene).iter().map(|asset| asset.id).collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn a_file_with_two_nodes_of_one_name_gets_them_apart() {
        let json = r#"{"root":{"id":0,"name":"Scene","shown":{"dm":true,"tv":true},"children":[
            {"type":"asset","id":5,"path":"a.png","center":[0.0,0.0],"grid_px":50.0},
            {"type":"asset","id":5,"path":"b.png","center":[0.0,0.0],"grid_px":50.0}]}}"#;
        let scene = Scene::from_json(json).unwrap();
        let ids: Vec<_> = assets(&scene).iter().map(|asset| asset.id).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn the_root_keeps_its_name_whatever_the_file_says() {
        let json = r#"{"root":{"id":9,"name":"Not the root","shown":{"dm":false,"tv":false},
            "children":[]}}"#;
        let scene = Scene::from_json(json).unwrap();
        assert_eq!(scene.root.id, ROOT_ID);
        assert_eq!(scene.root.name, "Scene");
        assert!(scene.root.shown.dm && scene.root.shown.tv);
    }

    #[test]
    fn a_new_name_is_one_no_node_holds() {
        let scene = nested();
        assert_eq!(scene.next_id(), 4);
    }

    #[test]
    fn a_node_moves_among_the_nodes_it_sits_with() {
        let mut scene = nested();
        // The asset at the bottom of the root moves over the group.
        assert!(super::reorder_all(&mut scene, &[1], true));
        assert_eq!(scene.root.children[0].id(), 2);
        assert_eq!(scene.root.children[1].id(), 1);
        // It cannot go any further.
        assert!(!super::reorder_all(&mut scene, &[1], true));
    }

    #[test]
    fn a_node_inside_a_group_moves_inside_that_group() {
        let mut scene = nested();
        // The only child of the group has nowhere to go.
        assert!(!super::reorder_all(&mut scene, &[3], true));
        assert!(!super::reorder_all(&mut scene, &[3], false));
        assert_eq!(parent_of(&scene, 3), Some((2, 0)));
    }

    #[test]
    fn the_tree_says_which_group_holds_a_node() {
        let scene = nested();
        assert_eq!(parent_of(&scene, 1), Some((ROOT_ID, 0)));
        assert_eq!(parent_of(&scene, 2), Some((ROOT_ID, 1)));
        assert_eq!(parent_of(&scene, 3), Some((2, 0)));
        assert_eq!(parent_of(&scene, 9), None);
        assert!(find(&scene, 3).is_some());
        assert!(find(&scene, 9).is_none());
    }

    /// A root with two assets, and a group holding two more.
    ///
    /// ```text
    /// root
    ///   1 floor.png
    ///   2 wall.png
    ///   3 Notes
    ///       4 note.png
    ///       5 pin.png
    /// ```
    fn family() -> Scene {
        let mut scene = Scene::default();
        let mut notes = Group::new(3, "Notes".to_owned());
        for (id, name) in [(4, "note.png"), (5, "pin.png")] {
            notes
                .children
                .push(Node::Asset(Asset::new(id, PathBuf::from(name), (0.0, 0.0))));
        }
        for (id, name) in [(1, "floor.png"), (2, "wall.png")] {
            scene
                .root
                .children
                .push(Node::Asset(Asset::new(id, PathBuf::from(name), (0.0, 0.0))));
        }
        scene.root.children.push(Node::Group(notes));
        scene
    }

    #[test]
    fn a_path_says_where_a_node_sits() {
        let scene = family();
        assert_eq!(path_of(&scene, 1), Some(vec![0]));
        assert_eq!(path_of(&scene, 3), Some(vec![2]));
        assert_eq!(path_of(&scene, 5), Some(vec![2, 1]));
        assert_eq!(path_of(&scene, 9), None);
    }

    #[test]
    fn a_selection_drops_what_a_group_in_it_already_holds() {
        let scene = family();
        // The group and one of its children: the group carries it.
        assert_eq!(normalize(&scene, &[3, 4]), vec![3]);
        assert_eq!(normalize(&scene, &[4, 3]), vec![3]);
        // Two nodes that hold each other, and one that holds neither.
        assert_eq!(normalize(&scene, &[1, 3, 5]), vec![1, 3]);
        // The same node twice is one node.
        assert_eq!(normalize(&scene, &[1, 1]), vec![1]);
    }

    #[test]
    fn a_group_holds_every_asset_under_it() {
        let scene = family();
        assert_eq!(assets_of(&scene, 1), vec![1]);
        assert_eq!(assets_of(&scene, 3), vec![4, 5]);
        assert!(assets_of(&scene, 9).is_empty());
    }

    #[test]
    fn two_brothers_group_where_they_stood() {
        let mut scene = family();
        let id = group_selection(&mut scene, &[1, 2], "New".to_owned()).unwrap();
        // The root holds the new group and the old group, nothing else.
        assert_eq!(scene.root.children.len(), 2);
        assert_eq!(scene.root.children[0].id(), id);
        assert_eq!(scene.root.children[1].id(), 3);
        let Some(Node::Group(new)) = scene.root.children.first() else {
            panic!("the first child is the new group");
        };
        // They keep the order they drew in.
        assert_eq!(
            new.children.iter().map(Node::id).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn an_uncle_and_a_nephew_group_in_the_root() {
        let mut scene = family();
        // floor.png sits in the root, pin.png sits in Notes.
        let id = group_selection(&mut scene, &[1, 5], "New".to_owned()).unwrap();
        assert_eq!(
            path_of(&scene, 1),
            Some(vec![path_of(&scene, id).unwrap()[0], 0])
        );
        // The nephew left Notes, which keeps its other child.
        let Some(Node::Group(notes)) = scene.root.children.iter().find(|n| n.id() == 3) else {
            panic!("Notes is still in the root");
        };
        assert_eq!(notes.children.len(), 1);
        assert_eq!(notes.children[0].id(), 4);
    }

    #[test]
    fn two_children_of_one_group_stay_in_that_group() {
        let mut scene = family();
        let id = group_selection(&mut scene, &[4, 5], "New".to_owned()).unwrap();
        // The new group sits inside Notes, since Notes holds them both.
        assert_eq!(path_of(&scene, id), Some(vec![2, 0]));
        assert_eq!(assets_of(&scene, id), vec![4, 5]);
    }

    #[test]
    fn a_group_and_its_own_child_group_as_one() {
        let mut scene = family();
        // The group carries the child, so this groups the group alone.
        let id = group_selection(&mut scene, &[3, 4], "New".to_owned()).unwrap();
        assert_eq!(assets_of(&scene, id), vec![4, 5]);
        assert_eq!(scene.root.children.len(), 3);
    }

    #[test]
    fn a_group_of_nothing_is_no_group() {
        let mut scene = family();
        assert_eq!(group_selection(&mut scene, &[], "New".to_owned()), None);
        assert_eq!(group_selection(&mut scene, &[9], "New".to_owned()), None);
    }

    #[test]
    fn a_group_draws_where_its_highest_member_drew() {
        let mut scene = family();
        // wall.png draws over floor.png, and Notes draws over both.
        let id = group_selection(&mut scene, &[2, 3], "New".to_owned()).unwrap();
        // The new group takes the place Notes had, over floor.png.
        assert_eq!(
            scene.root.children.iter().map(Node::id).collect::<Vec<_>>(),
            vec![1, id]
        );
    }

    #[test]
    fn a_new_group_takes_a_name_no_member_holds() {
        let mut scene = family();
        let id = group_selection(&mut scene, &[1, 2], "New".to_owned()).unwrap();
        // The members leave the tree while the group is made, so a name
        // counted then would land on one of them.
        assert!(!assets_of(&scene, id).contains(&id));
        let mut names: Vec<NodeId> = Vec::new();
        let mut walk: Vec<&Node> = scene.root.children.iter().collect();
        while let Some(node) = walk.pop() {
            names.push(node.id());
            if let Node::Group(group) = node {
                walk.extend(group.children.iter());
            }
        }
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            names.len(),
            "two nodes share a name: {names:?}"
        );
    }

    #[test]
    fn a_node_moves_to_where_another_node_stands() {
        let mut scene = family();
        // floor.png joins Notes, under note.png.
        assert!(move_above(&mut scene, 1, 4));
        assert_eq!(path_of(&scene, 1), Some(vec![1, 0]));
        assert_eq!(assets_of(&scene, 3), vec![1, 4, 5]);
    }

    #[test]
    fn a_node_moves_among_the_nodes_it_already_sits_with() {
        let mut scene = family();
        // wall.png drops to where floor.png stands.
        assert!(move_above(&mut scene, 2, 1));
        assert_eq!(
            scene.root.children.iter().map(Node::id).collect::<Vec<_>>(),
            vec![2, 1, 3]
        );
    }

    #[test]
    fn a_group_cannot_go_inside_itself() {
        let mut scene = family();
        assert!(!move_above(&mut scene, 3, 4));
        assert!(!move_above(&mut scene, 3, 3));
        // Nothing moved, and nothing was lost.
        assert_eq!(assets_of(&scene, 3), vec![4, 5]);
        assert_eq!(scene.root.children.len(), 3);
    }

    #[test]
    fn a_move_in_order_asks_whether_the_nodes_sit_together() {
        let scene = family();
        assert_eq!(super::share_parent(&scene, &[1, 2]), Some(ROOT_ID));
        assert_eq!(super::share_parent(&scene, &[4, 5]), Some(3));
        // floor.png sits in the root, pin.png sits in Notes.
        assert_eq!(super::share_parent(&scene, &[1, 5]), None);
        assert_eq!(super::share_parent(&scene, &[]), None);
    }

    #[test]
    fn brothers_and_sisters_move_in_order_and_keep_theirs() {
        let mut scene = family();
        // floor.png and wall.png stand under Notes. Both step up.
        assert!(super::reorder_all(&mut scene, &[1, 2], true));
        assert_eq!(
            scene.root.children.iter().map(Node::id).collect::<Vec<_>>(),
            vec![3, 1, 2]
        );
        // They cannot climb past the top.
        assert!(!super::reorder_all(&mut scene, &[1, 2], true));
    }

    #[test]
    fn a_node_moves_into_a_group_that_holds_nothing() {
        let mut scene = family();
        scene
            .root
            .children
            .push(Node::Group(Group::new(6, "Empty".to_owned())));
        assert!(super::move_into(&mut scene, 1, 6));
        assert_eq!(assets_of(&scene, 6), vec![1]);
    }

    #[test]
    fn a_node_moves_into_a_group_on_top_of_what_it_holds() {
        let mut scene = family();
        assert!(super::move_into(&mut scene, 1, 3));
        assert_eq!(assets_of(&scene, 3), vec![4, 5, 1]);
        assert_eq!(scene.root.children.len(), 2);
    }

    #[test]
    fn a_group_cannot_move_into_what_it_holds() {
        let mut scene = family();
        assert!(!super::move_into(&mut scene, 3, 3));
        assert_eq!(scene.root.children.len(), 3);
    }

    #[test]
    fn the_list_of_groups_starts_at_the_root() {
        let scene = family();
        let names: Vec<_> = super::group_names(&scene)
            .iter()
            .map(|(id, name, depth)| (*id, name.clone(), *depth))
            .collect();
        assert_eq!(
            names,
            vec![(ROOT_ID, "Scene".to_owned(), 0), (3, "Notes".to_owned(), 1)]
        );
    }

    #[test]
    fn a_group_that_goes_leaves_its_children_where_it_stood() {
        let mut scene = family();
        assert!(super::ungroup(&mut scene, 3));
        assert_eq!(
            scene.root.children.iter().map(Node::id).collect::<Vec<_>>(),
            vec![1, 2, 4, 5]
        );
        // Every asset is still here, in the order it drew in.
        assert_eq!(
            assets(&scene)
                .iter()
                .map(|asset| asset.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 4, 5]
        );
    }

    #[test]
    fn a_group_that_goes_leaves_every_transform_alone() {
        let mut scene = family();
        // Turn and grow the group first, so its children stand apart.
        let starts = super::placed(&scene, &[3]);
        super::rotate_about(&mut scene, &starts, (1.0, 1.0), 0.7);
        let starts = super::placed(&scene, &[3]);
        super::scale_about(&mut scene, &starts, (1.0, 1.0), 1.4);
        let before = super::placed(&scene, &[3]);

        assert!(super::ungroup(&mut scene, 3));

        // An asset carries where it stands, so nothing about it changes
        // when the group around it goes.
        let after = super::placed(&scene, &[4, 5]);
        assert_eq!(before, after);
    }

    #[test]
    fn the_root_never_goes() {
        let mut scene = family();
        assert!(!super::ungroup(&mut scene, ROOT_ID));
        // An asset is not a group, so it does not go either.
        assert!(!super::ungroup(&mut scene, 1));
        assert_eq!(scene.root.children.len(), 3);
    }

    #[test]
    fn a_set_grows_around_a_point_and_holds_its_shape() {
        let mut scene = family();
        let starts = super::placed(&scene, &[3]);
        super::scale_about(&mut scene, &starts, (0.0, 0.0), 2.0);
        // Both assets sat at the middle, so both stay there, twice as big.
        for id in [4, 5] {
            let asset = find(&scene, id).unwrap().asset().unwrap();
            assert!((asset.scale - 2.0).abs() < 1e-9);
        }
    }

    #[test]
    fn a_set_turns_around_a_point_and_holds_its_shape() {
        let mut scene = family();
        // pin.png stands two inches to the right of the middle.
        let Some(Node::Group(notes)) = scene.root.children.get_mut(2) else {
            panic!("Notes is the third child");
        };
        let Some(Node::Asset(pin)) = notes.children.get_mut(1) else {
            panic!("pin.png is the second child of Notes");
        };
        pin.center = (2.0, 0.0);

        let starts = super::placed(&scene, &[5]);
        super::rotate_about(&mut scene, &starts, (0.0, 0.0), std::f64::consts::FRAC_PI_2);

        let pin = find(&scene, 5).unwrap().asset().unwrap();
        // A quarter turn takes it to two inches below the middle.
        assert!(pin.center.0.abs() < 1e-9 && (pin.center.1 - 2.0).abs() < 1e-9);
        assert!((pin.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    #[test]
    fn a_node_on_top_of_a_group_stays_where_it_is() {
        let mut scene = family();
        // pin.png is the top child of Notes, so a drop on Notes is a no-op.
        assert!(!super::move_into(&mut scene, 5, 3));
        // note.png is under it, and has somewhere to go.
        assert!(super::move_into(&mut scene, 4, 3));
        assert_eq!(assets_of(&scene, 3), vec![5, 4]);
    }

    #[test]
    fn a_group_that_went_is_no_longer_there_to_fill() {
        let mut scene = family();
        assert!(super::has_group(&scene, 3));
        assert!(super::has_group(&scene, ROOT_ID));
        // An asset is not a group, and neither is a name nothing holds.
        assert!(!super::has_group(&scene, 1));
        super::ungroup(&mut scene, 3);
        assert!(!super::has_group(&scene, 3));
    }
}
