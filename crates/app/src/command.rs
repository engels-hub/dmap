//! Every change to the scene, and the stack that walks back over them.
//!
//! A tool never writes the scene. It builds a [`Command`] and hands it to
//! the [`History`], which writes it. The history keeps what it wrote, so
//! `Ctrl+Z` puts the scene back and `Ctrl+Shift+Z` writes it again.
//!
//! A change under the DM's hand, such as a drag, comes in through
//! [`History::hold`] once a frame. The history drops the one it held and
//! keeps the new one, so a drag of a hundred frames is one step. The tool
//! calls [`History::settle`] when the drag ends. A change that is over in
//! one frame, such as a key press or a click, goes through
//! [`History::run`].
//!
//! A change holds the values it writes and the values that stood there
//! before, so every revert is exact. PLAN.md 5.1.

// Rust guideline compliant 2026-02-21

use std::collections::VecDeque;

use crate::scene::{Asset, Group, Node, NodeId, Placed, Scene, Shown};
use crate::tvbox::TvBox;

/// How many changes the stack keeps.
///
/// A step holds the values it rewrote, and a step that reshapes the tree
/// holds the children of the group it reshaped. A hundred of those is
/// megabytes at worst, and further back than a DM ever reaches in one
/// session.
const STEPS: usize = 100;

/// One change to the scene, which the history writes and takes back.
pub trait Command: std::fmt::Debug {
    /// Writes the change into `scene`.
    fn apply(&self, scene: &mut Scene);

    /// Puts `scene` back the way it stood before [`Command::apply`].
    fn revert(&self, scene: &mut Scene);
}

/// Where the TV box stands, and how wide it is.
#[derive(Debug)]
pub struct SetTvBox {
    /// The box as it stood.
    pub before: TvBox,
    /// The box as the DM wants it.
    pub after: TvBox,
}

impl Command for SetTvBox {
    fn apply(&self, scene: &mut Scene) {
        scene.tv_box = self.after;
    }

    fn revert(&self, scene: &mut Scene) {
        scene.tv_box = self.before;
    }
}

/// Whole assets: a move, a turn, a size, a flip or a grid size.
///
/// One change carries every asset the DM holds, so a drag of four maps is
/// one step.
#[derive(Debug)]
pub struct SetAssets {
    /// The assets as they stood, one for each asset in `after`.
    pub before: Vec<Asset>,
    /// The assets as the DM wants them.
    pub after: Vec<Asset>,
}

impl Command for SetAssets {
    fn apply(&self, scene: &mut Scene) {
        write_assets(scene, &self.after);
    }

    fn revert(&self, scene: &mut Scene) {
        write_assets(scene, &self.before);
    }
}

fn write_assets(scene: &mut Scene, assets: &[Asset]) {
    for asset in assets {
        if let Some(place) = crate::scene::asset_mut(scene, asset.id) {
            place.clone_from(asset);
        }
    }
}

/// A set of assets grown around one point, from a drag on a corner handle.
///
/// The change holds where each asset stood, so the revert writes those
/// values back and the scene takes no rounding from the way out.
#[derive(Debug)]
pub struct Grow {
    /// Where each asset stood, at what size and at what turn.
    pub starts: Vec<Placed>,
    /// The point the set grows around, in inches.
    pub pivot: (f64, f64),
    /// What the size of each asset is multiplied by.
    pub factor: f64,
}

impl Command for Grow {
    fn apply(&self, scene: &mut Scene) {
        crate::scene::scale_about(scene, &self.starts, self.pivot, self.factor);
    }

    fn revert(&self, scene: &mut Scene) {
        write_placed(scene, &self.starts);
    }
}

/// A set of assets turned around one point, from a drag on the turn handle.
#[derive(Debug)]
pub struct Turn {
    /// Where each asset stood, at what size and at what turn.
    pub starts: Vec<Placed>,
    /// The point the set turns around, in inches.
    pub pivot: (f64, f64),
    /// How far the set turns, in radians.
    pub angle: f64,
}

impl Command for Turn {
    fn apply(&self, scene: &mut Scene) {
        crate::scene::rotate_about(scene, &self.starts, self.pivot, self.angle);
    }

    fn revert(&self, scene: &mut Scene) {
        write_placed(scene, &self.starts);
    }
}

fn write_placed(scene: &mut Scene, starts: &[Placed]) {
    for start in starts {
        if let Some(asset) = crate::scene::asset_mut(scene, start.id) {
            asset.center = start.center;
            asset.scale = start.scale;
            asset.rotation = start.rotation;
        }
    }
}

/// What the DM calls a group.
#[derive(Debug)]
pub struct SetName {
    /// The group that takes the name.
    pub id: NodeId,
    /// The name it carried.
    pub before: String,
    /// The name the DM typed.
    pub after: String,
}

impl Command for SetName {
    fn apply(&self, scene: &mut Scene) {
        if let Some(group) = crate::scene::group_mut(scene, self.id) {
            group.name.clone_from(&self.after);
        }
    }

    fn revert(&self, scene: &mut Scene) {
        if let Some(group) = crate::scene::group_mut(scene, self.id) {
            group.name.clone_from(&self.before);
        }
    }
}

/// Which screens a node draws on.
#[derive(Debug)]
pub struct SetShown {
    /// The node whose switches the DM pressed.
    pub id: NodeId,
    /// The switches as they stood.
    pub before: Shown,
    /// The switches as the DM wants them.
    pub after: Shown,
}

impl Command for SetShown {
    fn apply(&self, scene: &mut Scene) {
        if let Some(shown) = crate::scene::shown_mut(scene, self.id) {
            *shown = self.after;
        }
    }

    fn revert(&self, scene: &mut Scene) {
        if let Some(shown) = crate::scene::shown_mut(scene, self.id) {
            *shown = self.before;
        }
    }
}

/// A new shape for the tree: an add, a delete, a move, an order or a group.
///
/// The change holds the children of each group it rewrote. A group inside
/// one of those is left out, because the list of the group above already
/// carries it. Build one with [`reshape`].
#[derive(Debug)]
pub struct Restructure {
    /// The children each group carried, by group.
    pub before: Vec<(NodeId, Vec<Node>)>,
    /// The children each group carries after the change.
    pub after: Vec<(NodeId, Vec<Node>)>,
}

impl Command for Restructure {
    fn apply(&self, scene: &mut Scene) {
        write_children(scene, &self.after);
    }

    fn revert(&self, scene: &mut Scene) {
        write_children(scene, &self.before);
    }
}

fn write_children(scene: &mut Scene, lists: &[(NodeId, Vec<Node>)]) {
    for (id, children) in lists {
        if let Some(group) = crate::scene::group_mut(scene, *id) {
            group.children.clone_from(children);
        }
    }
}

/// Runs a change to the shape of the tree and writes it down.
///
/// The scene helpers move nodes around in place, and a tool reaches them
/// through this function: it keeps the tree, runs `change`, and reads the
/// groups the change rewrote out of the two trees. A `change` that moved
/// nothing gives `None`.
///
/// The change is in the scene already when this returns, so the result
/// goes to [`History::kept`], not to [`History::run`].
pub fn reshape(scene: &mut Scene, change: impl FnOnce(&mut Scene)) -> Option<Restructure> {
    let was = scene.root.clone();
    change(scene);
    let mut before = Vec::new();
    let mut after = Vec::new();
    collect_lists(&was, &scene.root, &mut before, &mut after);
    (!after.is_empty()).then_some(Restructure { before, after })
}

/// Walks two versions of one group and keeps the lists that differ.
///
/// The walk stops at the highest group whose children changed. A deeper
/// group is only visited while the lists above it match node for node.
fn collect_lists(
    was: &Group,
    now: &Group,
    before: &mut Vec<(NodeId, Vec<Node>)>,
    after: &mut Vec<(NodeId, Vec<Node>)>,
) {
    if was.children == now.children {
        return;
    }
    if same_row(&was.children, &now.children) {
        for (old, new) in was.children.iter().zip(&now.children) {
            if let (Node::Group(old), Node::Group(new)) = (old, new) {
                collect_lists(old, new, before, after);
            }
        }
        return;
    }
    before.push((now.id, was.children.clone()));
    after.push((now.id, now.children.clone()));
}

/// Whether two children lists hold the same nodes in the same order.
///
/// Two groups of one name count as the same node, however their own
/// children differ, so the walk can go on down into them. Anything else
/// must match whole.
fn same_row(was: &[Node], now: &[Node]) -> bool {
    was.len() == now.len()
        && was.iter().zip(now).all(|(old, new)| match (old, new) {
            (Node::Group(old), Node::Group(new)) => {
                old.id == new.id && old.name == new.name && old.shown == new.shown
            }
            (Node::Asset(old), Node::Asset(new)) => old == new,
            _ => false,
        })
}

/// The changes the DM made, and the ones they took back.
///
/// The stack keeps [`STEPS`] changes. An older one falls off the bottom.
/// A save does not touch the stack, so the DM saves and undoes past the
/// save. A new scene clears it with [`History::clear`].
#[derive(Debug, Default)]
pub struct History {
    /// What the DM did, oldest first.
    done: VecDeque<Box<dyn Command>>,
    /// What the DM took back, the last one last.
    undone: Vec<Box<dyn Command>>,
    /// The change under the DM's hand, which is not a step yet.
    open: Option<Box<dyn Command>>,
}

impl History {
    /// Writes a change that the DM is still making.
    ///
    /// The history drops the change it held and keeps this one, so a drag
    /// is one step. Every frame of the drag must carry the same `before`,
    /// because that is the state the undo goes back to.
    pub fn hold(&mut self, scene: &mut Scene, change: impl Command + 'static) {
        change.apply(scene);
        self.undone.clear();
        self.open = Some(Box::new(change));
    }

    /// Closes the change [`History::hold`] wrote, once the drag ends.
    pub fn settle(&mut self) {
        let Some(change) = self.open.take() else {
            return;
        };
        self.done.push_back(change);
        if self.done.len() > STEPS {
            self.done.pop_front();
        }
    }

    /// Writes a change that is over in one frame.
    ///
    /// A change the DM still had under their hand becomes a step of its
    /// own first, so a press of a button never swallows the drag before it.
    pub fn run(&mut self, scene: &mut Scene, change: impl Command + 'static) {
        self.settle();
        self.hold(scene, change);
        self.settle();
    }

    /// Keeps a change that is in the scene already, as one step.
    ///
    /// [`reshape`] writes the tree while it reads what moved, so its
    /// result comes in here.
    pub fn kept(&mut self, change: impl Command + 'static) {
        self.settle();
        self.undone.clear();
        self.open = Some(Box::new(change));
        self.settle();
    }

    /// Whether a change is under the DM's hand.
    pub fn holding(&self) -> bool {
        self.open.is_some()
    }

    /// Takes the last change back. Returns `true` when it did.
    pub fn undo(&mut self, scene: &mut Scene) -> bool {
        let Some(change) = self.done.pop_back() else {
            return false;
        };
        change.revert(scene);
        self.undone.push(change);
        true
    }

    /// Writes the last change the DM took back. Returns `true` when it did.
    pub fn redo(&mut self, scene: &mut Scene) -> bool {
        let Some(change) = self.undone.pop() else {
            return false;
        };
        change.apply(scene);
        self.done.push_back(change);
        true
    }

    /// Drops every change, for a scene that has nothing to do with them.
    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
        self.open = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Command as _, Grow, History, Restructure, SetAssets, SetName, SetShown, SetTvBox, Turn,
        reshape,
    };
    use crate::scene::{Asset, Group, Node, ROOT_ID, Scene, Shown};

    fn asset(id: u64, center: (f64, f64)) -> Asset {
        Asset {
            id,
            shown: Shown::default(),
            path: std::path::PathBuf::from("map.png"),
            center,
            grid_px: 50.0,
            rotation: 0.0,
            scale: 1.0,
            flip_x: false,
            flip_y: false,
            snap_offset: (0.0, 0.0),
        }
    }

    /// A scene with two maps in the root and one map inside a group.
    fn scene() -> Scene {
        let mut scene = Scene::default();
        let mut group = Group::new(7, "Group 7".to_owned());
        group.children.push(Node::Asset(asset(3, (9.0, 9.0))));
        scene.root.children.push(Node::Asset(asset(1, (0.0, 0.0))));
        scene.root.children.push(Node::Asset(asset(2, (4.0, 0.0))));
        scene.root.children.push(Node::Group(group));
        scene
    }

    #[test]
    fn a_move_of_two_maps_goes_back_in_one_step() {
        let mut scene = scene();
        let before = vec![asset(1, (0.0, 0.0)), asset(2, (4.0, 0.0))];
        let after = vec![asset(1, (1.0, 1.0)), asset(2, (5.0, 1.0))];
        let start = scene.clone();
        let mut history = History::default();
        history.run(&mut scene, SetAssets { before, after });
        assert_eq!(
            crate::scene::find(&scene, 1)
                .and_then(Node::asset)
                .unwrap()
                .center,
            (1.0, 1.0)
        );
        assert!(history.undo(&mut scene));
        assert_eq!(scene, start);
    }

    #[test]
    fn a_drag_of_many_frames_is_one_step() {
        let mut scene = scene();
        let start = scene.clone();
        let mut history = History::default();
        for step in 1..=5 {
            let frame = f64::from(step);
            history.hold(
                &mut scene,
                SetAssets {
                    before: vec![asset(1, (0.0, 0.0))],
                    after: vec![asset(1, (frame, 0.0))],
                },
            );
        }
        history.settle();
        assert!(history.undo(&mut scene));
        assert_eq!(scene, start);
        assert!(!history.undo(&mut scene));
    }

    #[test]
    fn redo_writes_the_change_again() {
        let mut scene = scene();
        let mut history = History::default();
        history.run(
            &mut scene,
            SetName {
                id: 7,
                before: "Group 7".to_owned(),
                after: "Cave".to_owned(),
            },
        );
        let named = scene.clone();
        assert!(history.undo(&mut scene));
        assert_ne!(scene, named);
        assert!(history.redo(&mut scene));
        assert_eq!(scene, named);
    }

    #[test]
    fn a_new_change_drops_what_the_dm_took_back() {
        let mut scene = scene();
        let mut history = History::default();
        history.run(
            &mut scene,
            SetShown {
                id: 7,
                before: Shown::default(),
                after: Shown {
                    dm: true,
                    tv: false,
                },
            },
        );
        assert!(history.undo(&mut scene));
        let box_now = scene.tv_box;
        history.run(
            &mut scene,
            SetTvBox {
                before: box_now,
                after: box_now,
            },
        );
        assert!(!history.redo(&mut scene));
    }

    #[test]
    fn the_stack_holds_a_hundred_steps_and_no_more() {
        let mut scene = scene();
        let mut history = History::default();
        for step in 1..=super::STEPS + 10 {
            let frame = step as f64;
            history.run(
                &mut scene,
                SetAssets {
                    before: vec![asset(1, (frame - 1.0, 0.0))],
                    after: vec![asset(1, (frame, 0.0))],
                },
            );
        }
        let mut steps = 0;
        while history.undo(&mut scene) {
            steps += 1;
        }
        assert_eq!(steps, super::STEPS);
    }

    #[test]
    fn a_turn_and_a_grow_put_every_asset_back_where_it_stood() {
        let mut scene = scene();
        let start = scene.clone();
        let starts = crate::scene::placed(&scene, &[1, 2]);
        let mut history = History::default();
        history.run(
            &mut scene,
            Turn {
                starts: starts.clone(),
                pivot: (2.0, 0.0),
                angle: 0.4,
            },
        );
        let turned = crate::scene::placed(&scene, &[1, 2]);
        history.run(
            &mut scene,
            Grow {
                starts: turned,
                pivot: (2.0, 0.0),
                factor: 1.5,
            },
        );
        assert_ne!(scene, start);
        assert!(history.undo(&mut scene));
        assert!(history.undo(&mut scene));
        assert_eq!(scene, start);
    }

    #[test]
    fn a_delete_in_the_root_keeps_only_the_root_list() {
        let mut scene = scene();
        let start = scene.clone();
        let change = reshape(&mut scene, |scene| {
            crate::scene::take_node(scene, 2);
        })
        .unwrap();
        assert_eq!(change.before.len(), 1);
        assert_eq!(change.before[0].0, ROOT_ID);
        let mut history = History::default();
        history.kept(change);
        assert!(history.undo(&mut scene));
        assert_eq!(scene, start);
    }

    #[test]
    fn a_move_inside_a_group_keeps_only_that_group() {
        let mut scene = scene();
        scene.root.children.push(Node::Asset(asset(4, (0.0, 6.0))));
        let start = scene.clone();
        let change = reshape(&mut scene, |scene| {
            crate::scene::move_into(scene, 4, 7);
        })
        .unwrap();
        // The root list carries the group, so one list says it all.
        assert_eq!(change.after.len(), 1);
        assert_eq!(change.after[0].0, ROOT_ID);
        let mut history = History::default();
        history.kept(change);
        assert!(history.undo(&mut scene));
        assert_eq!(scene, start);
    }

    #[test]
    fn an_order_change_inside_a_group_leaves_the_root_alone() {
        let mut scene = scene();
        if let Some(group) = crate::scene::group_mut(&mut scene, 7) {
            group.children.push(Node::Asset(asset(5, (9.0, 3.0))));
        }
        let start = scene.clone();
        let change = reshape(&mut scene, |scene| {
            crate::scene::reorder_all(scene, &[3], true);
        })
        .unwrap();
        assert_eq!(change.after.len(), 1);
        assert_eq!(change.after[0].0, 7);
        let mut history = History::default();
        history.kept(change);
        assert!(history.undo(&mut scene));
        assert_eq!(scene, start);
    }

    #[test]
    fn a_change_that_moves_nothing_is_no_step() {
        let mut scene = scene();
        // The group stands on top of the root already.
        assert!(
            reshape(&mut scene, |scene| {
                crate::scene::move_into(scene, 7, ROOT_ID);
            })
            .is_none()
        );
    }

    #[test]
    fn a_new_scene_clears_the_stack() {
        let mut scene = scene();
        let mut history = History::default();
        history.run(
            &mut scene,
            SetName {
                id: 7,
                before: "Group 7".to_owned(),
                after: "Cave".to_owned(),
            },
        );
        history.clear();
        assert!(!history.undo(&mut scene));
        assert!(!history.redo(&mut scene));
    }

    #[test]
    fn a_restructure_puts_a_whole_list_back() {
        let mut scene = scene();
        let before = scene.root.children.clone();
        let change = Restructure {
            before: vec![(ROOT_ID, before.clone())],
            after: vec![(ROOT_ID, Vec::new())],
        };
        change.apply(&mut scene);
        assert!(scene.root.children.is_empty());
        change.revert(&mut scene);
        assert_eq!(scene.root.children, before);
    }
}
