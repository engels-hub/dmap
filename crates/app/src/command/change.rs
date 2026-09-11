//! Every change a tool can make to the scene.
//!
//! Each change holds the values it writes and the values that stood there
//! before, so every revert is exact. `Change` wraps them all, because the
//! history keeps one list of steps. PLAN.md 5.1.

// Rust guideline compliant 2026-02-21

use serde::{Deserialize, Serialize};

use crate::scene::{Asset, Group, Node, NodeId, Placed, Scene, Shown};
use crate::stroke::Stroke;
use crate::text;
use crate::tvbox::TvBox;

use super::{Command, Note, differs, lists, maps, screens, spot};

/// Where the TV box stands, and how wide it is.
#[derive(Debug, Serialize, Deserialize)]
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

    fn note(&self) -> Note {
        Note::new(
            text::history_deed_tv_box(),
            String::new(),
            text::history_detail_tv_box(
                format_args!("{:.1}", self.after.width),
                spot(self.after.center),
            ),
        )
    }
}

/// Whole assets: a move, a turn, a size, a flip or a grid size.
///
/// One change carries every asset the DM holds, so a drag of four maps is
/// one step.
#[derive(Debug, Serialize, Deserialize)]
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

    fn note(&self) -> Note {
        let subject = maps(&self.after);
        let Some((was, now)) = self
            .before
            .iter()
            .zip(&self.after)
            .find(|(was, now)| was != now)
        else {
            return Note::new(text::history_deed_change(), subject, String::new());
        };
        let (what, detail) = if was.center != now.center {
            (
                text::history_deed_move(),
                text::history_detail_move(spot(was.center), spot(now.center)),
            )
        } else if differs(was.rotation, now.rotation) {
            (
                text::history_deed_turn(),
                text::history_detail_turn(
                    format_args!("{:.0}", was.rotation.to_degrees()),
                    format_args!("{:.0}", now.rotation.to_degrees()),
                ),
            )
        } else if differs(was.scale, now.scale) {
            (
                text::history_deed_size(),
                text::history_detail_size(
                    format_args!("{:.0}", was.scale * 100.0),
                    format_args!("{:.0}", now.scale * 100.0),
                ),
            )
        } else if was.flip_x != now.flip_x {
            (
                text::history_deed_flip(),
                text::history_detail_flip_x().to_owned(),
            )
        } else if was.flip_y != now.flip_y {
            (
                text::history_deed_flip(),
                text::history_detail_flip_y().to_owned(),
            )
        } else if differs(was.grid_px, now.grid_px) {
            (
                text::history_deed_grid_px(),
                text::history_detail_grid_px(
                    format_args!("{:.0}", was.grid_px),
                    format_args!("{:.0}", now.grid_px),
                ),
            )
        } else {
            (text::history_deed_change(), String::new())
        };
        Note::new(what, subject, detail)
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
#[derive(Debug, Serialize, Deserialize)]
pub struct Grow {
    /// The files the drag holds, for the history list.
    pub subject: String,
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

    fn note(&self) -> Note {
        let was = self.starts.first().map_or(1.0, |first| first.scale);
        Note::new(
            text::history_deed_size(),
            self.subject.clone(),
            text::history_detail_size(
                format_args!("{:.0}", was * 100.0),
                format_args!("{:.0}", was * self.factor * 100.0),
            ),
        )
    }
}

/// A set of assets turned around one point, from a drag on the turn handle.
#[derive(Debug, Serialize, Deserialize)]
pub struct Turn {
    /// The files the drag holds, for the history list.
    pub subject: String,
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

    fn note(&self) -> Note {
        let was = self.starts.first().map_or(0.0, |first| first.rotation);
        Note::new(
            text::history_deed_turn(),
            self.subject.clone(),
            text::history_detail_turn(
                format_args!("{:.0}", was.to_degrees()),
                format_args!("{:.0}", (was + self.angle).to_degrees()),
            ),
        )
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

/// Whole strokes: a color, a width, a reach, or the rule of a measure.
///
/// The DM changes what they drew in the panel of the stroke, and one
/// change carries every field of it. Issue #12.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetStrokes {
    /// The strokes as they stood, one for each stroke in `after`.
    pub before: Vec<Stroke>,
    /// The strokes as the DM wants them.
    pub after: Vec<Stroke>,
}

impl Command for SetStrokes {
    fn apply(&self, scene: &mut Scene) {
        write_strokes(scene, &self.after);
    }

    fn revert(&self, scene: &mut Scene) {
        write_strokes(scene, &self.before);
    }

    fn note(&self) -> Note {
        let subject = self
            .after
            .first()
            .map(|stroke| stroke.ink.name().to_owned())
            .unwrap_or_default();
        Note::new(text::history_deed_change(), subject, String::new())
    }
}

fn write_strokes(scene: &mut Scene, strokes: &[Stroke]) {
    for stroke in strokes {
        if let Some(place) = crate::scene::stroke_mut(scene, stroke.id) {
            place.clone_from(stroke);
        }
    }
}

/// What the DM calls a group.
#[derive(Debug, Serialize, Deserialize)]
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

    fn note(&self) -> Note {
        Note::new(
            text::history_deed_rename(),
            self.before.clone(),
            text::history_detail_rename(&self.after),
        )
    }
}

/// Which screens a node draws on.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetShown {
    /// The node whose switches the DM pressed.
    pub id: NodeId,
    /// What that node is called, for the history list.
    pub subject: String,
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

    fn note(&self) -> Note {
        Note::new(
            text::history_deed_show(),
            self.subject.clone(),
            text::history_detail_screens(screens(self.before), screens(self.after)),
        )
    }
}

/// What a change to the shape of the tree did.
///
/// The step keeps the deed and not the word for it. The word comes from
/// the language the DM reads, and `history.json` holds the same file
/// whichever language wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Deed {
    /// A map joined the scene.
    AddMap,
    /// A group joined the tree.
    NewGroup,
    /// A group went, and what it held stayed.
    Ungroup,
    /// A node took another place in the list.
    MoveInList,
    /// A map went into another group, from its panel.
    AnotherGroup,
    /// A selection became a group.
    Group,
    /// A node moved up or down the stack.
    Order,
    /// The DM drew something.
    Draw,
    /// The eraser took a bite out of what the DM drew.
    Erase,
}

impl Deed {
    /// What the history list calls this deed.
    fn word(self) -> &'static str {
        match self {
            Self::AddMap => text::history_deed_add_map(),
            Self::NewGroup => text::history_deed_new_group(),
            Self::Ungroup => text::history_deed_ungroup(),
            Self::MoveInList => text::history_deed_move_in_list(),
            Self::AnotherGroup => text::history_deed_another_group(),
            Self::Group => text::history_deed_group(),
            Self::Order => text::history_deed_order(),
            Self::Draw => text::history_deed_draw(),
            Self::Erase => text::history_deed_erase(),
        }
    }
}

/// A new shape for the tree: an add, a delete, a move, an order or a group.
///
/// The change holds the children of each group it rewrote. A group inside
/// one of those is left out, because the list of the group above already
/// carries it. Build one with [`reshape`].
#[derive(Debug, Serialize, Deserialize)]
pub struct Restructure {
    /// What the DM did, for the history dialog.
    pub what: Deed,
    /// The file or the group it happened to.
    pub subject: String,
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

    fn note(&self) -> Note {
        Note::new(
            self.what.word(),
            self.subject.clone(),
            text::history_detail_tree(lists(self.after.len())),
        )
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
/// nothing gives `None`. The words `what` and `subject` go to the history
/// dialog, which reads "New group" over "Cave".
///
/// The change is in the scene already when this returns, so the result
/// goes to [`History::kept`], not to [`History::run`].
pub fn reshape(
    scene: &mut Scene,
    what: Deed,
    subject: String,
    change: impl FnOnce(&mut Scene),
) -> Option<Restructure> {
    let was = scene.root.clone();
    change(scene);
    let mut before = Vec::new();
    let mut after = Vec::new();
    collect_lists(&was, &scene.root, &mut before, &mut after);
    (!after.is_empty()).then_some(Restructure {
        what,
        subject,
        before,
        after,
    })
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

/// One step of the history, in the form the project file holds.
///
/// The stack keeps this and not a boxed [`Command`], because a step has to
/// go into `history.json` and come back. Every kind of change turns into
/// one with `into`, so a tool still hands the history the change it built.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum Change {
    /// See [`SetTvBox`].
    TvBox(SetTvBox),
    /// See [`SetAssets`].
    Assets(SetAssets),
    /// See [`SetStrokes`].
    Strokes(SetStrokes),
    /// See [`Grow`].
    Grow(Grow),
    /// See [`Turn`].
    Turn(Turn),
    /// See [`SetName`].
    Name(SetName),
    /// See [`SetShown`].
    Shown(SetShown),
    /// See [`Restructure`].
    Restructure(Restructure),
}

impl Command for Change {
    fn apply(&self, scene: &mut Scene) {
        match self {
            Self::TvBox(change) => change.apply(scene),
            Self::Assets(change) => change.apply(scene),
            Self::Strokes(change) => change.apply(scene),
            Self::Grow(change) => change.apply(scene),
            Self::Turn(change) => change.apply(scene),
            Self::Name(change) => change.apply(scene),
            Self::Shown(change) => change.apply(scene),
            Self::Restructure(change) => change.apply(scene),
        }
    }

    fn revert(&self, scene: &mut Scene) {
        match self {
            Self::TvBox(change) => change.revert(scene),
            Self::Assets(change) => change.revert(scene),
            Self::Strokes(change) => change.revert(scene),
            Self::Grow(change) => change.revert(scene),
            Self::Turn(change) => change.revert(scene),
            Self::Name(change) => change.revert(scene),
            Self::Shown(change) => change.revert(scene),
            Self::Restructure(change) => change.revert(scene),
        }
    }

    fn note(&self) -> Note {
        match self {
            Self::TvBox(change) => change.note(),
            Self::Assets(change) => change.note(),
            Self::Strokes(change) => change.note(),
            Self::Grow(change) => change.note(),
            Self::Turn(change) => change.note(),
            Self::Name(change) => change.note(),
            Self::Shown(change) => change.note(),
            Self::Restructure(change) => change.note(),
        }
    }
}

impl From<SetTvBox> for Change {
    fn from(change: SetTvBox) -> Self {
        Self::TvBox(change)
    }
}

impl From<SetAssets> for Change {
    fn from(change: SetAssets) -> Self {
        Self::Assets(change)
    }
}

impl From<SetStrokes> for Change {
    fn from(change: SetStrokes) -> Self {
        Self::Strokes(change)
    }
}

impl From<Grow> for Change {
    fn from(change: Grow) -> Self {
        Self::Grow(change)
    }
}

impl From<Turn> for Change {
    fn from(change: Turn) -> Self {
        Self::Turn(change)
    }
}

impl From<SetName> for Change {
    fn from(change: SetName) -> Self {
        Self::Name(change)
    }
}

impl From<SetShown> for Change {
    fn from(change: SetShown) -> Self {
        Self::Shown(change)
    }
}

impl From<Restructure> for Change {
    fn from(change: Restructure) -> Self {
        Self::Restructure(change)
    }
}
