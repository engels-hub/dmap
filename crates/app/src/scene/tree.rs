//! Walking the tree, and the edits that change its shape.
//!
//! A node sits under one group, and the path to it says which. Every edit
//! here moves nodes between those lists; none of them touches what a node
//! holds. DESIGN.md 8.4.

// Rust guideline compliant 2026-02-21

use crate::stroke::Stroke;

use super::{Asset, Group, Node, NodeId, ROOT_ID, Scene, Shown, groups, under};

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
        Some(Node::Stroke(_)) | None => Vec::new(),
    }
}

/// Every stroke a node holds: the stroke itself, or all under a group.
///
/// A drag carries the drawings of a selection as it carries its maps, so
/// a group of drawings moves with one hand. Issue #69.
pub fn strokes_of(scene: &Scene, id: NodeId) -> Vec<NodeId> {
    match find(scene, id) {
        Some(Node::Stroke(stroke)) => vec![stroke.id],
        Some(Node::Group(group)) => super::ink_under(group)
            .iter()
            .map(|stroke| stroke.id)
            .collect(),
        Some(Node::Asset(_)) | None => Vec::new(),
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
        Some(Node::Stroke(stroke)) => stroke.ink.name().to_owned(),
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
            Node::Asset(_) | Node::Stroke(_) => {}
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
    if has_stroke(scene, id) {
        return stroke_mut(scene, id).map(|stroke| &mut stroke.shown);
    }
    asset_mut(scene, id).map(|asset| &mut asset.shown)
}

/// The group a new stroke joins, made when there is none.
///
/// Issue #12: the strokes keep to a group of their own, so a hundred pen
/// marks do not flood the objects list. The group starts closed.
pub fn ink_group(scene: &mut Scene, name: String) -> NodeId {
    if let Some(group) = groups(scene).iter().find(|group| group.ink) {
        return group.id;
    }
    let id = scene.next_id();
    let mut group = Group::new(id, name);
    group.ink = true;
    scene.root.children.push(Node::Group(group));
    id
}

/// Whether the tree holds a stroke of this name.
pub fn has_stroke(scene: &Scene, id: NodeId) -> bool {
    find(scene, id).and_then(Node::stroke).is_some()
}

/// The stroke of this name, ready to change.
pub fn stroke_mut(scene: &mut Scene, id: NodeId) -> Option<&mut Stroke> {
    find_stroke(&mut scene.root.children, id)
}

fn find_stroke(nodes: &mut [Node], id: NodeId) -> Option<&mut Stroke> {
    for node in nodes {
        match node {
            Node::Stroke(stroke) if stroke.id == id => return Some(stroke),
            Node::Group(group) => {
                if let Some(found) = find_stroke(&mut group.children, id) {
                    return Some(found);
                }
            }
            Node::Stroke(_) | Node::Asset(_) => {}
        }
    }
    None
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
