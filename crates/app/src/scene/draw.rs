//! Which nodes draw, and in what order.
//!
//! A node draws only when it and every group above it show for that
//! audience. The tree holds the topmost node first, and the painter wants
//! it last, so every order here comes back reversed. PLAN.md 5.3.

// Rust guideline compliant 2026-02-21

use crate::stroke::Stroke;

use super::{Asset, Audience, Group, Node, Scene};

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
            Node::Stroke(_) => {}
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
            Node::Stroke(_) => {}
        }
    }
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
            Node::Stroke(_) => {}
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

/// Every stroke in the scene, in the order they draw for `audience`.
///
/// A stroke draws over every map, so the strokes come as their own list
/// and not in the order the tree holds them. PLAN.md 5.3.
pub fn ink_order(scene: &Scene, audience: Audience) -> Vec<&Stroke> {
    let mut drawn = Vec::new();
    collect_ink(&scene.root.children, audience, &mut drawn);
    drawn
}

fn collect_ink<'a>(nodes: &'a [Node], audience: Audience, drawn: &mut Vec<&'a Stroke>) {
    for node in nodes {
        if !node.shows(audience) {
            continue;
        }
        match node {
            Node::Stroke(stroke) => drawn.push(stroke),
            Node::Group(group) => collect_ink(&group.children, audience, drawn),
            Node::Asset(_) => {}
        }
    }
}
