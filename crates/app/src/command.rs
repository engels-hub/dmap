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
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::scene::{Asset, Group, Node, NodeId, Placed, Scene, Shown};
use crate::text;
use crate::tvbox::TvBox;

/// How many changes the stack keeps.
///
/// A step holds the values it rewrote, and a step that reshapes the tree
/// holds the children of the group it reshaped. A hundred of those is
/// megabytes at worst, and further back than a DM ever reaches in one
/// session.
const STEPS: usize = 100;

/// What one step says on the history list. DESIGN.md 9.8.
///
/// A change fills the first three. The history fills `ago`, because a
/// change does not know what the time is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Note {
    /// What the DM did, such as `Move` or `New group`.
    pub what: String,
    /// The file or the group it happened to.
    pub subject: String,
    /// The numbers the change wrote, such as the two spots of a move.
    pub detail: String,
    /// How long ago the DM made it, such as `4 min ago`.
    pub ago: String,
}

impl Note {
    /// A note with no time on it yet.
    fn new(what: &str, subject: String, detail: String) -> Self {
        Self {
            what: what.to_owned(),
            subject,
            detail,
            ago: String::new(),
        }
    }
}

/// One change to the scene, which the history writes and takes back.
pub trait Command: std::fmt::Debug {
    /// Writes the change into `scene`.
    fn apply(&self, scene: &mut Scene);

    /// Puts `scene` back the way it stood before [`Command::apply`].
    fn revert(&self, scene: &mut Scene);

    /// What this change says on the history list. DESIGN.md 9.8.
    fn note(&self) -> Note;
}

/// The seconds since the epoch, for the time on a step.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// How long ago a step was made, in words. DESIGN.md 9.8.
///
/// A step from a file that an older version of the program wrote carries
/// no time, and reads as `earlier`.
fn ago(at: u64, now: u64) -> String {
    if at == 0 {
        return text::history_ago_earlier().to_owned();
    }
    // A clock that went back leaves a step in the future. It happened, so
    // it reads as the newest thing that happened.
    let gap = now.saturating_sub(at);
    // Under three quarters of a minute reads as no time at all, as it
    // does in a chat window.
    if gap < 45 {
        return text::history_ago_now().to_owned();
    }
    let minutes = (gap + 30) / 60;
    if minutes < 90 {
        return text::history_ago_minutes(minutes.max(1));
    }
    let hours = (gap + 1800) / 3600;
    if hours < 24 {
        return text::history_ago_hours(hours);
    }
    // A day and a half still reads as a day. The DM wants to know which
    // session a step belongs to, not the hour of it.
    if gap < 2 * 86400 {
        return text::history_ago_day().to_owned();
    }
    text::history_ago_days(gap / 86400)
}

/// The name of a spot on the canvas, in inches.
fn spot(at: (f64, f64)) -> String {
    text::history_spot(format_args!("{:.1}", at.0), format_args!("{:.1}", at.1))
}

/// Which screens a pair of switches says yes to.
fn screens(shown: Shown) -> &'static str {
    match (shown.dm, shown.tv) {
        (true, true) => text::history_screens_both(),
        (true, false) => text::history_screens_dm(),
        (false, true) => text::history_screens_tv(),
        (false, false) => text::history_screens_none(),
    }
}

/// Whether two numbers of the scene stand apart.
///
/// The history only asks which field a step wrote, so the smallest step a
/// number can take is a change.
fn differs(was: f64, now: f64) -> bool {
    (was - now).abs() > f64::EPSILON
}

/// How a step names the assets it touched: the file, or a count of them.
fn maps(assets: &[Asset]) -> String {
    match assets {
        [] => text::history_maps_one().to_owned(),
        [one] => file_name(one),
        many => text::history_maps_many(many.len()),
    }
}

/// The file name of a map, without the folders above it.
fn file_name(asset: &Asset) -> String {
    asset.path.file_name().map_or_else(
        || asset.path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// How a step names the group lists it rewrote.
fn lists(count: usize) -> String {
    if count == 1 {
        text::history_lists_one().to_owned()
    } else {
        text::history_lists_many(count)
    }
}

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

/// One step: the change the DM made, and when they made it.
#[derive(Debug, Serialize, Deserialize)]
struct Step {
    /// What the step writes and takes back.
    change: Change,
    /// The seconds since the epoch. A file from an older version of the
    /// program carries none, and the step reads as `earlier`.
    #[serde(default)]
    at: u64,
}

/// The stack as `history.json` gives it back.
#[derive(Debug, Default, Deserialize)]
struct Read {
    /// How many of the steps stand written. The rest the DM took back.
    #[serde(default)]
    place: usize,
    /// Every step, oldest first.
    #[serde(default)]
    steps: Vec<Step>,
}

/// The same, on the way out, so no step is cloned to be written.
#[derive(Debug, Serialize)]
struct Written<'a> {
    place: usize,
    steps: Vec<&'a Step>,
}

/// The changes the DM made, and the ones they took back.
///
/// The stack keeps [`STEPS`] changes. An older one falls off the bottom.
/// It lives beside the scene in `history.json`, so the DM closes the
/// program and undoes yesterday's work tomorrow. A new scene brings its
/// own stack.
#[derive(Debug, Default)]
pub struct History {
    /// What the DM did, oldest first.
    done: VecDeque<Step>,
    /// What the DM took back, the last one last.
    undone: Vec<Step>,
    /// The change under the DM's hand, which is not a step yet.
    open: Option<Step>,
    /// Whether the stack changed since the last write to `history.json`.
    ///
    /// A step carries the values it wrote, and a step that reshaped the
    /// tree carries branches of it, so the file grows with the scene. A
    /// save that has nothing new to say writes nothing.
    unwritten: bool,
}

impl History {
    /// Writes a change that the DM is still making.
    ///
    /// The history drops the change it held and keeps this one, so a drag
    /// is one step. Every frame of the drag must carry the same `before`,
    /// because that is the state the undo goes back to.
    pub fn hold(&mut self, scene: &mut Scene, change: impl Into<Change>) {
        let change = change.into();
        change.apply(scene);
        self.undone.clear();
        // The time of a drag is the time it ends, because every frame of
        // it writes this again.
        self.open = Some(Step { change, at: now() });
        self.unwritten = true;
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
        self.unwritten = true;
    }

    /// Writes a change that is over in one frame.
    ///
    /// A change the DM still had under their hand becomes a step of its
    /// own first, so a press of a button never swallows the drag before it.
    pub fn run(&mut self, scene: &mut Scene, change: impl Into<Change>) {
        self.settle();
        self.hold(scene, change);
        self.settle();
    }

    /// Keeps a change that is in the scene already, as one step.
    ///
    /// [`reshape`] writes the tree while it reads what moved, so its
    /// result comes in here.
    pub fn kept(&mut self, change: impl Into<Change>) {
        self.settle();
        self.undone.clear();
        self.open = Some(Step {
            change: change.into(),
            at: now(),
        });
        self.settle();
    }

    /// Whether a change is under the DM's hand.
    pub fn holding(&self) -> bool {
        self.open.is_some()
    }

    /// Takes the last change back. Returns `true` when it did.
    pub fn undo(&mut self, scene: &mut Scene) -> bool {
        let Some(step) = self.done.pop_back() else {
            return false;
        };
        step.change.revert(scene);
        self.undone.push(step);
        self.unwritten = true;
        true
    }

    /// Writes the last change the DM took back. Returns `true` when it did.
    pub fn redo(&mut self, scene: &mut Scene) -> bool {
        let Some(step) = self.undone.pop() else {
            return false;
        };
        step.change.apply(scene);
        self.done.push_back(step);
        self.unwritten = true;
        true
    }

    /// What every step says, oldest first. DESIGN.md 9.8.
    ///
    /// The steps past [`History::place`] are the ones the DM took back.
    pub fn steps(&self) -> Vec<Note> {
        let now = now();
        self.done
            .iter()
            .chain(self.undone.iter().rev())
            .map(|step| Note {
                ago: ago(step.at, now),
                ..step.change.note()
            })
            .collect()
    }

    /// How many steps stand written. The rest the DM took back.
    pub fn place(&self) -> usize {
        self.done.len()
    }

    /// Walks the scene to the state after `place` steps.
    ///
    /// Returns `true` when the scene moved. A place past the end of the
    /// stack walks as far as the stack goes.
    pub fn walk_to(&mut self, scene: &mut Scene, place: usize) -> bool {
        let mut moved = false;
        while self.done.len() > place && self.undo(scene) {
            moved = true;
        }
        while self.done.len() < place && self.redo(scene) {
            moved = true;
        }
        moved
    }

    /// Whether the stack has something the file beside the scene lacks.
    pub fn unwritten(&self) -> bool {
        self.unwritten
    }

    /// Marks the stack as written, once the file holds it.
    pub fn wrote(&mut self) {
        self.unwritten = false;
    }

    /// The stack as JSON, for the file beside the scene.
    ///
    /// The change under the DM's hand is left out. It is no step yet, and
    /// the program writes the files only once the DM lets go.
    ///
    /// The JSON holds no line breaks. A step carries every value it wrote,
    /// and a step that reshaped the tree carries whole branches of it, so
    /// a hundred pretty-printed steps would make a file no one reads
    /// anyway large.
    pub fn to_json(&self) -> String {
        let file = Written {
            place: self.done.len(),
            steps: self.done.iter().chain(self.undone.iter().rev()).collect(),
        };
        serde_json::to_string(&file).expect("a change has no unserializable field")
    }

    /// Reads a stack that a run before this one wrote.
    ///
    /// # Errors
    ///
    /// Returns an error when the JSON does not hold a stack.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let file: Read = serde_json::from_str(json)?;
        let mut steps = file.steps;
        let place = file.place.min(steps.len());
        let taken_back: Vec<Step> = steps.split_off(place).into_iter().rev().collect();
        Ok(Self {
            done: steps.into(),
            undone: taken_back,
            open: None,
            unwritten: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Command as _, Deed, Grow, History, Restructure, SetAssets, SetName, SetShown, SetTvBox,
        Turn, reshape,
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
                subject: "Group 7".to_owned(),
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
                subject: "two maps".to_owned(),
                starts: starts.clone(),
                pivot: (2.0, 0.0),
                angle: 0.4,
            },
        );
        let turned = crate::scene::placed(&scene, &[1, 2]);
        history.run(
            &mut scene,
            Grow {
                subject: "two maps".to_owned(),
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
        let change = reshape(&mut scene, Deed::Ungroup, "the map".to_owned(), |scene| {
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
        let change = reshape(
            &mut scene,
            Deed::MoveInList,
            "the map".to_owned(),
            |scene| {
                crate::scene::move_into(scene, 4, 7);
            },
        )
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
        let change = reshape(&mut scene, Deed::Order, "the map".to_owned(), |scene| {
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
            reshape(
                &mut scene,
                Deed::MoveInList,
                "the map".to_owned(),
                |scene| {
                    crate::scene::move_into(scene, 7, ROOT_ID);
                }
            )
            .is_none()
        );
    }

    #[test]
    fn the_step_list_reads_oldest_first_and_keeps_what_was_taken_back() {
        let mut scene = scene();
        let mut history = History::default();
        history.run(
            &mut scene,
            SetAssets {
                before: vec![asset(1, (0.0, 0.0))],
                after: vec![asset(1, (1.0, 0.0))],
            },
        );
        history.run(
            &mut scene,
            SetName {
                id: 7,
                before: "Group 7".to_owned(),
                after: "Cave".to_owned(),
            },
        );
        let told: Vec<(String, String, String)> = history
            .steps()
            .into_iter()
            .map(|note| (note.what, note.subject, note.detail))
            .collect();
        assert_eq!(
            told,
            vec![
                (
                    "Move".to_owned(),
                    "map.png".to_owned(),
                    "0.0, 0.0 to 1.0, 0.0 in".to_owned()
                ),
                (
                    "Rename".to_owned(),
                    "Group 7".to_owned(),
                    "to Cave".to_owned()
                ),
            ]
        );
        assert_eq!(history.steps()[0].ago, "just now");
        assert_eq!(history.place(), 2);
        assert!(history.undo(&mut scene));
        // The step the DM took back stays on the list, behind the place.
        assert_eq!(history.steps().len(), 2);
        assert_eq!(history.place(), 1);
    }

    #[test]
    fn a_step_says_how_long_ago_the_dm_made_it() {
        let now = 1_000_000;
        assert_eq!(super::ago(0, now), "earlier");
        assert_eq!(super::ago(now, now), "just now");
        assert_eq!(super::ago(now - 44, now), "just now");
        assert_eq!(super::ago(now - 45, now), "1 min ago");
        assert_eq!(super::ago(now - 600, now), "10 min ago");
        assert_eq!(super::ago(now - 5400, now), "2 h ago");
        assert_eq!(super::ago(now - 8 * 3600, now), "8 h ago");
        assert_eq!(super::ago(now - 86_400, now), "a day ago");
        assert_eq!(super::ago(now - 40 * 3600, now), "a day ago");
        assert_eq!(super::ago(now - 3 * 86_400, now), "3 days ago");
        // A clock that went back leaves a step in the future.
        assert_eq!(super::ago(now + 60, now), "just now");
    }

    #[test]
    fn a_walk_goes_to_the_step_the_dm_picked() {
        let mut scene = scene();
        let start = scene.clone();
        let mut history = History::default();
        for step in 1..=4 {
            let frame = f64::from(step);
            history.run(
                &mut scene,
                SetAssets {
                    before: vec![asset(1, (frame - 1.0, 0.0))],
                    after: vec![asset(1, (frame, 0.0))],
                },
            );
        }
        let all = scene.clone();
        assert!(history.walk_to(&mut scene, 0));
        assert_eq!(scene, start);
        assert_eq!(history.place(), 0);
        assert!(history.walk_to(&mut scene, 4));
        assert_eq!(scene, all);
        // The place it stands on already asks for no walk.
        assert!(!history.walk_to(&mut scene, 4));
    }

    #[test]
    fn a_stack_that_wrote_itself_asks_for_no_second_write() {
        let mut scene = scene();
        let mut history = History::default();
        assert!(!history.unwritten());
        history.run(
            &mut scene,
            SetName {
                id: 7,
                before: "Group 7".to_owned(),
                after: "Cave".to_owned(),
            },
        );
        assert!(history.unwritten());
        history.wrote();
        assert!(!history.unwritten());
        assert!(history.undo(&mut scene));
        assert!(history.unwritten());
        // A stack that came from a file is the file.
        let back = History::from_json(&history.to_json()).unwrap();
        assert!(!back.unwritten());
    }

    #[test]
    fn a_stack_comes_back_from_its_file() {
        let mut scene = scene();
        let start = scene.clone();
        let mut history = History::default();
        history.run(
            &mut scene,
            SetAssets {
                before: vec![asset(1, (0.0, 0.0))],
                after: vec![asset(1, (2.0, 0.0))],
            },
        );
        history.run(
            &mut scene,
            SetName {
                id: 7,
                before: "Group 7".to_owned(),
                after: "Cave".to_owned(),
            },
        );
        let change = reshape(&mut scene, Deed::Ungroup, "the map".to_owned(), |scene| {
            crate::scene::take_node(scene, 2);
        })
        .unwrap();
        history.kept(change);
        // One step the DM took back must come back as one they can write
        // again, not as one that is gone.
        assert!(history.undo(&mut scene));
        let written = history.to_json();
        let saved = scene.clone();

        let mut back = History::from_json(&written).unwrap();
        assert_eq!(back.steps(), history.steps());
        assert_eq!(back.steps()[2].what, "Ungroup");
        assert_eq!(back.place(), 2);
        assert!(back.redo(&mut scene));
        assert!(back.walk_to(&mut scene, 0));
        assert_eq!(scene, start);
        assert!(back.walk_to(&mut scene, 2));
        assert_eq!(scene, saved);
    }

    #[test]
    fn a_file_that_says_more_steps_than_it_holds_stops_at_the_end() {
        let mut scene = scene();
        let history = History::from_json(r#"{"place":9,"steps":[]}"#).unwrap();
        assert_eq!(history.place(), 0);
        assert!(history.steps().is_empty());
        let mut history = history;
        assert!(!history.undo(&mut scene));
    }

    #[test]
    fn a_scene_folder_with_no_history_file_reads_as_an_empty_stack() {
        let history = History::from_json("{}").unwrap();
        assert_eq!(history.place(), 0);
        assert!(history.steps().is_empty());
    }

    #[test]
    fn a_restructure_puts_a_whole_list_back() {
        let mut scene = scene();
        let before = scene.root.children.clone();
        let change = Restructure {
            what: Deed::Ungroup,
            subject: "the map".to_owned(),
            before: vec![(ROOT_ID, before.clone())],
            after: vec![(ROOT_ID, Vec::new())],
        };
        change.apply(&mut scene);
        assert!(scene.root.children.is_empty());
        change.revert(&mut scene);
        assert_eq!(scene.root.children, before);
    }
}
