//! The Light view: walls, doors, lights, dark areas and the reveal brush.
//!
//! Everything that stops, gives or takes away light is laid here. The
//! panel picks what a click on the canvas lays, and sets the darkness of
//! the whole scene.
//!
//! A click starts a wall, and each next click lays one more length of it
//! from the last point. A double click, a right click or `Escape` ends
//! the chain, and a click on its first point closes it. A point snaps to
//! the end of a wall near it, then to a crossing of the grid. `Shift`
//! holds both snaps off. A drag of an end moves every wall that meets
//! there. A click on a wall picks it, and its panel sets what it is.
//!
//! A click puts a light down, and a drag lays a box of darkness. A drag
//! of a light or of the outline of an area moves it. The reveal brush is
//! the pen of the Draw view, and its strokes let the light of the map
//! through the darkness.
//!
//! Each length of wall, each drag and each change is one step of the
//! history, so `Ctrl+Z` takes back the last corner the DM laid.

// Rust guideline compliant 2026-02-21

use crate::command::{Deed, SetMood, SetSight, reshape};
use crate::icons::Icon;
use crate::keys::Action;
use crate::lighting::Mood;
use crate::scene::{Node, NodeId, Scene, Shown};
use crate::sight::{Darkness, End, Kind, Wall, Walls};
use crate::stroke::Mix;
use crate::text;
use crate::theme::Tokens;
use crate::widget::{self, Height};

use super::canvas::{
    add_light, add_sight, canvas_area, darkness_at, delete_held, light_at, points_in_inches,
    walls_at,
};
use super::draw::{Draw, draw_tool};
use super::panel::{darkness_properties, light_properties, nib_row, square_row, width_row};
use super::{DASH, Frame, HANDLE_SIZE, MIN_SHAPE, Nib, PANEL_SLIDER, Select, View, WALL_REACH};

/// What a click or a drag on the canvas lays in the Light view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(super) enum Build {
    /// A wall that stops light and sight.
    #[default]
    Wall,
    /// A closed door.
    Door,
    /// A light the size of a torch.
    Light,
    /// A box of darkness.
    Darkness,
    /// A stroke that reveals the map in the dark, or conceals it again.
    /// Issue #65.
    Reveal,
}

/// The shapes of the reveal brush. The eraser and the ruler leave no
/// stroke that reveals.
const REVEAL_NIBS: [Nib; 7] = [
    Nib::Pen,
    Nib::Line,
    Nib::Rect,
    Nib::Ellipse,
    Nib::Burst,
    Nib::Cone,
    Nib::Beam,
];

/// The state of the Light view between two frames.
#[derive(Debug, Default)]
pub(super) struct Gaffer {
    /// What a click lays.
    pub build: Build,
    /// The first and the last point of the chain the DM lays, if any.
    chain: Option<((f64, f64), (f64, f64))>,
    /// The wall the DM picked: its set and its place in the set.
    ///
    /// A light or an area the DM picked is in the selection of the Select
    /// view instead, so its panel is the one that view opens.
    pub picked: Option<(NodeId, usize)>,
    /// A press on the end of a wall, which becomes a drag once it moves.
    press: Option<Press>,
    /// A press on a light or on an area, which becomes a drag once it
    /// moves.
    carry: Option<Carry>,
    /// The corners of the box of darkness under the DM's hand.
    area: Option<((f64, f64), (f64, f64))>,
    /// The reveal brush, which is the pen of the Draw view.
    pub brush: Draw,
    /// Whether the brush reveals or conceals.
    pub fog: Fog,
    /// The darkness of the scene as it stood when the DM took hold of its
    /// slider, so a drag of the slider is one step.
    pub opened_mood: Option<Mood>,
}

/// What the reveal brush does to the darkness.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Fog {
    /// It cuts the stroke out of the darkness.
    #[default]
    Reveal,
    /// It paints the darkness back.
    Conceal,
}

impl Fog {
    /// How a stroke of the brush mixes with the canvas.
    fn mix(self) -> Mix {
        match self {
            Self::Reveal => Mix::Reveal,
            Self::Conceal => Mix::Conceal,
        }
    }
}

/// A press on a light or on an area of darkness.
#[derive(Debug)]
struct Carry {
    /// Where the press landed.
    from: (f64, f64),
    /// The node as it stood, for the undo.
    before: Node,
    /// Whether the pointer moved, so the press is a drag.
    moved: bool,
}

/// A press on the end of one or more walls.
#[derive(Debug)]
struct Press {
    /// Where the ends stood.
    from: (f64, f64),
    /// Each end that meets there: its set, its place and which end.
    ends: Vec<(NodeId, usize, End)>,
    /// The wall sets as they stood, for the undo.
    before: Vec<Node>,
    /// Whether the pointer moved, so the press is a drag.
    moved: bool,
}

impl Gaffer {
    /// Whether a gesture is under the DM's hand, so undo waits for it.
    pub fn busy(&self) -> bool {
        self.press.is_some() || self.carry.is_some() || self.area.is_some() || self.brush.busy()
    }

    /// Drops every gesture, so a DM who left the view does not come back
    /// to a wall that still follows the pointer.
    pub fn rest(&mut self) {
        self.chain = None;
        self.press = None;
        self.carry = None;
        self.area = None;
        if self.brush.busy() {
            self.brush = Draw::default();
        }
    }
}

/// Runs the Light view for one frame. Returns `true` when the scene
/// changed.
///
/// A new wall joins the wall set of the group `into`, the one the Objects
/// list marks. The view makes that set when the group holds none. A new
/// light and a new area join that group too.
#[expect(
    clippy::too_many_arguments,
    reason = "one view: its state, the selection it shares, the scene, the camera, the group it fills and the theme"
)]
pub(super) fn light_tool(
    ui: &mut egui::Ui,
    gaffer: &mut Gaffer,
    select: &mut Select,
    frame: &mut Frame<'_>,
    viewport: (u32, u32),
    zoom_goes_to: &mut Option<bool>,
    into: NodeId,
    tokens: Tokens,
) -> bool {
    if gaffer.build == Build::Reveal {
        gaffer.chain = None;
        gaffer.picked = None;
        let pen = (frame.settings.reveal_nib, gaffer.fog.mix());
        return draw_tool(ui, &mut gaffer.brush, frame, viewport, zoom_goes_to, pen);
    }
    if gaffer.brush.busy() {
        gaffer.brush = Draw::default();
    }
    let (rect, view, pointer) = canvas_area(ui, frame, viewport, false, zoom_goes_to);
    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    // An undo can take away the wall the DM picked.
    if gaffer
        .picked
        .is_some_and(|(id, place)| wall_of(frame.scene, id, place).is_none())
    {
        gaffer.picked = None;
    }
    let reach = points_in_inches(&view, WALL_REACH);
    let (free, ended, right) = ui.input(|i| {
        (
            i.modifiers.shift,
            i.key_pressed(egui::Key::Escape),
            i.pointer.secondary_clicked(),
        )
    });
    let skip: Vec<(NodeId, usize, End)> = gaffer
        .press
        .as_ref()
        .map(|press| press.ends.clone())
        .unwrap_or_default();
    let snap = |frame: &Frame<'_>, at: (f64, f64)| {
        if free {
            return at;
        }
        nearest_end(frame.scene, at, reach, &skip)
            .unwrap_or_else(|| frame.settings.cells().snap(at))
    };
    let cursor = pointer.pos.map(|pos| view.to_world(pos));
    let mut edited = false;
    if ended || right {
        gaffer.chain = None;
        gaffer.area = None;
    }
    let delete = ui.input_mut(|i| frame.settings.keys.pressed(i, Action::Delete));
    if delete && gaffer.chain.is_none() {
        edited |= delete_picked(gaffer, select, frame);
    }
    let live = !pointer.panning && pointer.hovered;
    // Only the wall squares take hold of a wall. A box of darkness that
    // starts on a corner would move the corner instead.
    let walling = matches!(gaffer.build, Build::Wall | Build::Door);
    if let (true, true, Some(at)) = (pointer.pressed(), live, cursor) {
        frame.history.settle();
        if let Some((first, last)) = gaffer.chain {
            let to = snap(frame, at);
            if same(to, last) {
                // The second click of a double click lands where the first
                // one did, and ends the chain.
                gaffer.chain = None;
            } else {
                edited |= lay(frame, into, last, to, gaffer.build);
                gaffer.chain = (!same(to, first)).then_some((first, to));
            }
        } else if let Some(from) = nearest_end(frame.scene, at, reach, &[]).filter(|_| walling) {
            let ends = ends_at(frame.scene, from);
            let before = sets_of(frame.scene, &ends);
            gaffer.press = Some(Press {
                from,
                ends,
                before,
                moved: false,
            });
        } else if let Some(id) =
            light_at(frame, &view, at).or_else(|| darkness_at(frame, &view, at))
        {
            gaffer.picked = None;
            select.chosen = vec![id];
            gaffer.carry = crate::scene::find(frame.scene, id).map(|node| Carry {
                from: at,
                before: node.clone(),
                moved: false,
            });
        } else if let Some(found) = walls_at(frame, &view, at, false).filter(|_| walling) {
            gaffer.picked = Some(found);
            select.chosen.clear();
        } else {
            let start = snap(frame, at);
            edited |= begin(gaffer, select, frame, into, (at, start));
        }
    }
    let to = cursor.map(|at| snap(frame, at));
    edited |= carry(gaffer, frame, to, pointer.down());
    edited |= carry_node(gaffer, frame, cursor, pointer.down());
    edited |= stretch(gaffer, select, frame, into, to, pointer.down());
    paint(ui, (rect, &view), gaffer, select, frame, to, tokens);
    edited
}

/// Deletes the wall, the light or the area the DM picked. Returns `true`
/// when the scene changed.
fn delete_picked(gaffer: &mut Gaffer, select: &mut Select, frame: &mut Frame<'_>) -> bool {
    if let Some((id, place)) = gaffer.picked.take() {
        return change(frame, id, |walls| {
            walls.segments.remove(place);
        });
    }
    holds_sight(select, frame.scene) && delete_held(select, frame)
}

/// Starts what the panel says a press on an empty spot lays, at the
/// pointer `at` or at the point `start` it snaps to. Returns `true` when
/// the scene changed.
fn begin(
    gaffer: &mut Gaffer,
    select: &mut Select,
    frame: &mut Frame<'_>,
    into: NodeId,
    (at, start): ((f64, f64), (f64, f64)),
) -> bool {
    gaffer.picked = None;
    select.chosen.clear();
    match gaffer.build {
        Build::Wall | Build::Door => gaffer.chain = Some((start, start)),
        Build::Light => return add_light(select, frame, into, at),
        Build::Darkness => gaffer.area = Some((start, start)),
        Build::Reveal => {}
    }
    false
}

/// Whether the DM holds one light or one area of darkness.
fn holds_sight(select: &Select, scene: &Scene) -> bool {
    select.only().is_some_and(|id| {
        matches!(
            crate::scene::find(scene, id),
            Some(Node::Light(_) | Node::Darkness(_))
        )
    })
}

/// Carries the ends under the DM's hand to `to` while the button stays
/// `down`, as one step. Returns `true` when the scene changed.
///
/// A press that lets go without a move starts a chain from the end, so
/// the DM goes on with a wall that is there.
fn carry(gaffer: &mut Gaffer, frame: &mut Frame<'_>, to: Option<(f64, f64)>, down: bool) -> bool {
    let Some(press) = gaffer.press.as_mut() else {
        return false;
    };
    if !down {
        if !press.moved && matches!(gaffer.build, Build::Wall | Build::Door) {
            gaffer.chain = Some((press.from, press.from));
        }
        frame.history.settle();
        gaffer.press = None;
        return false;
    }
    let to = to.unwrap_or(press.from);
    press.moved |= !same(to, press.from);
    if !press.moved {
        return false;
    }
    let after = moved(&press.before, &press.ends, to);
    frame.history.hold(
        frame.scene,
        SetSight {
            before: press.before.clone(),
            after,
        },
    );
    true
}

/// Carries the light or the area under the DM's hand along with the
/// pointer while the button stays `down`, as one step. Returns `true`
/// when the scene changed.
fn carry_node(
    gaffer: &mut Gaffer,
    frame: &mut Frame<'_>,
    cursor: Option<(f64, f64)>,
    down: bool,
) -> bool {
    let Some(carry) = gaffer.carry.as_mut() else {
        return false;
    };
    if !down {
        frame.history.settle();
        gaffer.carry = None;
        return false;
    }
    let to = cursor.unwrap_or(carry.from);
    carry.moved |= !same(to, carry.from);
    if !carry.moved {
        return false;
    }
    let step = (to.0 - carry.from.0, to.1 - carry.from.1);
    let after = match &carry.before {
        Node::Light(light) => Node::Light(light.moved(step)),
        Node::Darkness(area) => Node::Darkness(area.moved(step)),
        other => other.clone(),
    };
    frame.history.hold(
        frame.scene,
        SetSight {
            before: vec![carry.before.clone()],
            after: vec![after],
        },
    );
    true
}

/// Pulls the far corner of the box of darkness to `to` while the button
/// stays `down`, and lays the area when it goes up. Returns `true` when
/// the scene changed.
///
/// A box with no width or no height is a click, and lays nothing.
fn stretch(
    gaffer: &mut Gaffer,
    select: &mut Select,
    frame: &mut Frame<'_>,
    into: NodeId,
    to: Option<(f64, f64)>,
    down: bool,
) -> bool {
    let Some((from, last)) = gaffer.area.as_mut() else {
        return false;
    };
    if down {
        if let Some(to) = to {
            *last = to;
        }
        return false;
    }
    let (from, last) = (*from, *last);
    gaffer.area = None;
    let min = (from.0.min(last.0), from.1.min(last.1));
    let max = (from.0.max(last.0), from.1.max(last.1));
    if max.0 - min.0 < MIN_SHAPE || max.1 - min.1 < MIN_SHAPE {
        return false;
    }
    let name = text::sight_darkness().to_owned();
    add_sight(select, frame, into, (Deed::AddDarkness, name), |id| {
        Node::Darkness(Darkness::covering(id, (min, max), 1.0))
    })
}

/// The panel of the Light view. DESIGN.md 8.5.
///
/// The top row picks what a click lays. Below it come the fields of the
/// wall, the light or the area the DM picked. With none picked, the
/// fields set what a click lays and how dark the scene is. Returns `true`
/// when the scene changed.
pub(super) fn light_view_properties(
    ui: &mut egui::Ui,
    gaffer: &mut Gaffer,
    select: &mut Select,
    frame: &mut Frame<'_>,
    tokens: Tokens,
) -> bool {
    let squares = [
        (Build::Wall, Icon::BrickWall, text::wall_kind_wall()),
        (Build::Door, Icon::DoorClosed, text::wall_kind_door()),
        (Build::Light, Icon::Sun, text::sight_light()),
        (Build::Darkness, Icon::Moon, text::sight_darkness()),
        (Build::Reveal, Icon::Eye, text::panel_light_fog()),
    ];
    // The row is headed with the name of the tool that is on.
    let name = squares
        .iter()
        .find(|(one, _, _)| *one == gaffer.build)
        .map_or("", |(_, _, name)| name);
    widget::row_label(ui, name);
    let build = gaffer.build;
    square_row(ui, &mut gaffer.build, &squares, tokens);
    // A DM who picks what to lay next is done with what they held, and
    // the panel turns to the fields of the new pick.
    if gaffer.build != build {
        gaffer.picked = None;
        if holds_sight(select, frame.scene) {
            select.chosen.clear();
        }
    }
    if let Some((id, place)) = gaffer.picked {
        widget::row_label(ui, text::sight_walls());
        return wall_properties(ui, gaffer, frame, (id, place));
    }
    let picked = select
        .only()
        .and_then(|id| Some((id, crate::scene::find(frame.scene, id)?)));
    match picked {
        Some((id, Node::Light(_))) => {
            widget::row_label(ui, text::sight_light());
            return light_properties(ui, id, select, frame);
        }
        Some((id, Node::Darkness(_))) => {
            widget::row_label(ui, text::sight_darkness());
            return darkness_properties(ui, id, select, frame);
        }
        _ => {}
    }
    match gaffer.build {
        Build::Wall | Build::Door => widget::helper(ui, text::panel_walls_lay_helper()),
        Build::Light => widget::helper(ui, text::panel_light_put_helper()),
        Build::Darkness => widget::helper(ui, text::panel_light_darkness_helper()),
        Build::Reveal => {
            let settings = &mut *frame.settings;
            widget::row_label(ui, text::panel_draw_mix());
            let choices = [
                (Fog::Reveal, text::mix_reveal()),
                (Fog::Conceal, text::mix_conceal()),
            ];
            widget::segmented(ui, &mut gaffer.fog, &choices);
            widget::row_label(ui, text::panel_light_reveal_shape());
            nib_row(ui, &mut settings.reveal_nib, &REVEAL_NIBS, tokens);
            let mut snap = settings.ink_snap;
            if widget::checkbox(ui, &mut snap, text::panel_draw_snap()).clicked() {
                settings.ink_snap = snap;
            }
            width_row(ui, settings, gaffer.fog.mix());
            widget::helper(ui, text::panel_light_reveal_helper());
        }
    }
    mood_row(ui, gaffer, frame)
}

/// The Darkness row: how dark the whole scene is, as one step of the
/// history for each drag. DESIGN.md 9.3.
fn mood_row(ui: &mut egui::Ui, gaffer: &mut Gaffer, frame: &mut Frame<'_>) -> bool {
    widget::row_label(ui, text::panel_light_scene_darkness());
    let mut mood = frame.scene.light;
    let shown = text::dialog_settings_opacity_value((mood.darkness * 100.0).round());
    if !widget::slider(ui, &mut mood.darkness, 0.0..=1.0, PANEL_SLIDER, &shown).changed() {
        return false;
    }
    // The row shows whole percent, so it keeps whole percent.
    mood.darkness = (mood.darkness * 100.0).round() / 100.0;
    if mood == frame.scene.light {
        return false;
    }
    let before = *gaffer.opened_mood.get_or_insert(frame.scene.light);
    frame.history.hold(
        frame.scene,
        SetMood {
            before,
            after: mood,
        },
    );
    true
}

/// The panel of the wall the DM picked: what it is, and what it stops.
fn wall_properties(
    ui: &mut egui::Ui,
    gaffer: &mut Gaffer,
    frame: &mut Frame<'_>,
    (id, place): (NodeId, usize),
) -> bool {
    let Some(wall) = wall_of(frame.scene, id, place) else {
        return false;
    };
    let mut after = wall;
    widget::row_label(ui, text::panel_walls_kind());
    let mut kind = Shape::of(after.kind);
    let choices = [
        (Shape::Wall, text::wall_kind_wall()),
        (Shape::Door, text::wall_kind_door()),
        (Shape::Secret, text::wall_kind_secret()),
    ];
    if widget::segmented(ui, &mut kind, &choices) {
        after.kind = kind.with(after.is_open());
    }
    if after.is_door() {
        let mut open = after.is_open();
        if widget::checkbox(ui, &mut open, text::panel_walls_open()).clicked() {
            after.kind = kind.with(open);
        }
    }
    widget::checkbox(
        ui,
        &mut after.blocks_light,
        text::panel_walls_blocks_light(),
    );
    widget::checkbox(
        ui,
        &mut after.blocks_sight,
        text::panel_walls_blocks_sight(),
    );
    let mut edited = false;
    if after != wall {
        edited |= change(frame, id, |walls| walls.segments[place] = after);
    }
    let delete = text::panel_delete();
    if widget::button(ui, delete, Some(Icon::Trash), Height::Row).clicked() {
        gaffer.picked = None;
        edited |= change(frame, id, |walls| {
            walls.segments.remove(place);
        });
    }
    edited
}

/// What a wall is, apart from whether its door stands open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Wall,
    Door,
    Secret,
}

impl Shape {
    fn of(kind: Kind) -> Self {
        match kind {
            Kind::Wall => Self::Wall,
            Kind::Door { .. } => Self::Door,
            Kind::Secret { .. } => Self::Secret,
        }
    }

    /// The kind of wall this is, with a door that stands `open` or not.
    fn with(self, open: bool) -> Kind {
        match self {
            Self::Wall => Kind::Wall,
            Self::Door => Kind::Door { open },
            Self::Secret => Kind::Secret { open },
        }
    }
}

/// Whether two points are one, to the precision a click reaches.
fn same(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).hypot(a.1 - b.1) < 1e-6
}

/// The wall at `place` in the set `id`, if both are there.
fn wall_of(scene: &Scene, id: NodeId, place: usize) -> Option<Wall> {
    match crate::scene::find(scene, id)? {
        Node::Walls(walls) => walls.segments.get(place).copied(),
        _ => None,
    }
}

/// Changes the wall set `id` with `edit`, as one step. Returns `true` when
/// it did.
fn change(frame: &mut Frame<'_>, id: NodeId, edit: impl FnOnce(&mut Walls)) -> bool {
    let Some(Node::Walls(walls)) = crate::scene::find(frame.scene, id) else {
        return false;
    };
    let mut after = walls.clone();
    edit(&mut after);
    let change = SetSight {
        before: vec![Node::Walls(walls.clone())],
        after: vec![Node::Walls(after)],
    };
    frame.history.run(frame.scene, change);
    true
}

/// Lays one length of wall from `a` to `b` in the group `into`. Returns
/// `true` when it did.
///
/// The wall joins the first wall set of the group. A group with none gets
/// one, and that is one step too.
fn lay(frame: &mut Frame<'_>, into: NodeId, a: (f64, f64), b: (f64, f64), build: Build) -> bool {
    let into = if crate::scene::has_group(frame.scene, into) {
        into
    } else {
        crate::scene::ROOT_ID
    };
    let mut wall = Wall::new(a, b);
    if build == Build::Door {
        wall.kind = Kind::Door { open: false };
    }
    let set = crate::scene::children_of(frame.scene, into).and_then(|children| {
        children.iter().find_map(|node| match node {
            Node::Walls(walls) => Some(walls.id),
            _ => None,
        })
    });
    if let Some(id) = set {
        return change(frame, id, |walls| walls.segments.push(wall));
    }
    let walls = Walls {
        id: frame.scene.next_id(),
        shown: Shown::default(),
        segments: vec![wall],
    };
    let name = text::sight_walls().to_owned();
    let Some(change) = reshape(frame.scene, Deed::AddWalls, name, |scene| {
        crate::scene::push_into(scene, into, Node::Walls(walls));
    }) else {
        return false;
    };
    frame.history.kept(change);
    true
}

/// Every wall set the DM sees, in the order the scene draws them.
fn wall_sets(scene: &Scene) -> impl Iterator<Item = &Walls> {
    crate::scene::sight_order(scene, crate::scene::Audience::Dm)
        .into_iter()
        .filter_map(|node| match node {
            Node::Walls(walls) => Some(walls),
            _ => None,
        })
}

/// The end of a wall nearest `at`, within `reach`, if any. The ends in
/// `skip` do not count: they are the ones under the DM's hand.
fn nearest_end(
    scene: &Scene,
    at: (f64, f64),
    reach: f64,
    skip: &[(NodeId, usize, End)],
) -> Option<(f64, f64)> {
    wall_sets(scene)
        .flat_map(|walls| {
            walls
                .segments
                .iter()
                .enumerate()
                .flat_map(move |(place, wall)| {
                    [(End::A, wall.a), (End::B, wall.b)]
                        .map(|(end, point)| ((walls.id, place, end), point))
                })
        })
        .filter(|(key, _)| !skip.contains(key))
        .map(|(_, point)| (point, (point.0 - at.0).hypot(point.1 - at.1)))
        .filter(|(_, distance)| *distance <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(point, _)| point)
}

/// Every end of every wall that stands on `at`.
fn ends_at(scene: &Scene, at: (f64, f64)) -> Vec<(NodeId, usize, End)> {
    let mut ends = Vec::new();
    for walls in wall_sets(scene) {
        for (place, wall) in walls.segments.iter().enumerate() {
            for (end, point) in [(End::A, wall.a), (End::B, wall.b)] {
                if same(point, at) {
                    ends.push((walls.id, place, end));
                }
            }
        }
    }
    ends
}

/// The wall sets that hold `ends`, each one once.
fn sets_of(scene: &Scene, ends: &[(NodeId, usize, End)]) -> Vec<Node> {
    let mut ids: Vec<NodeId> = ends.iter().map(|(id, _, _)| *id).collect();
    ids.dedup();
    ids.iter()
        .filter_map(|id| crate::scene::find(scene, *id).cloned())
        .collect()
}

/// The wall sets `before`, with each of `ends` moved to `to`.
fn moved(before: &[Node], ends: &[(NodeId, usize, End)], to: (f64, f64)) -> Vec<Node> {
    before
        .iter()
        .map(|node| match node {
            Node::Walls(walls) => {
                let mut walls = walls.clone();
                for (_, place, end) in ends.iter().filter(|(id, _, _)| *id == walls.id) {
                    walls.segments[*place] = walls.segments[*place].with_end(*end, to);
                }
                Node::Walls(walls)
            }
            other => other.clone(),
        })
        .collect()
}

/// Paints the handles of the wall ends, what the DM picked, and the wall
/// or the box that follows the pointer.
fn paint(
    ui: &egui::Ui,
    (rect, view): (egui::Rect, &View),
    gaffer: &Gaffer,
    select: &Select,
    frame: &Frame<'_>,
    snapped: Option<(f64, f64)>,
    tokens: Tokens,
) {
    let painter = ui.painter_at(rect);
    let picked = egui::Stroke::new(HANDLE_SIZE * 0.75, tokens.accent);
    if let Some(wall) = gaffer
        .picked
        .and_then(|(id, place)| wall_of(frame.scene, id, place))
    {
        painter.line_segment([view.to_screen(wall.a), view.to_screen(wall.b)], picked);
    }
    match select
        .only()
        .and_then(|id| crate::scene::find(frame.scene, id))
    {
        Some(Node::Light(light)) => {
            let ring = egui::Stroke::new(2.0, tokens.accent);
            painter.circle_stroke(view.to_screen(light.center), HANDLE_SIZE * 2.0, ring);
        }
        Some(Node::Darkness(area)) => {
            let outline: Vec<egui::Pos2> =
                area.corners.iter().map(|at| view.to_screen(*at)).collect();
            painter.add(egui::Shape::closed_line(
                outline,
                egui::Stroke::new(2.0, tokens.accent),
            ));
        }
        _ => {}
    }
    let size = egui::vec2(HANDLE_SIZE, HANDLE_SIZE) * 0.75;
    // The handles show only where a press takes them.
    let walling = matches!(gaffer.build, Build::Wall | Build::Door);
    for walls in wall_sets(frame.scene).filter(|_| walling) {
        for wall in &walls.segments {
            for point in [wall.a, wall.b] {
                let at = view.to_screen(point);
                if rect.contains(at) {
                    let square = egui::Rect::from_center_size(at, size);
                    painter.rect_filled(square, 0.0, tokens.surface);
                    painter.rect_stroke(
                        square,
                        0.0,
                        egui::Stroke::new(1.0, tokens.ink),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
    }
    let dashes = egui::Stroke::new(2.0, tokens.accent);
    if let (Some((_, last)), Some(to)) = (gaffer.chain, snapped) {
        let line = [view.to_screen(last), view.to_screen(to)];
        painter.extend(egui::Shape::dashed_line(&line, dashes, DASH, DASH));
    }
    if let Some((from, to)) = gaffer.area {
        let corners = [from, (to.0, from.1), to, (from.0, to.1), from];
        let line: Vec<egui::Pos2> = corners.iter().map(|at| view.to_screen(*at)).collect();
        painter.extend(egui::Shape::dashed_line(&line, dashes, DASH, DASH));
    }
    // A light lands where the pointer is, so it shows no snap.
    if let (Some(to), false) = (snapped, gaffer.build == Build::Light) {
        painter.circle_stroke(
            view.to_screen(to),
            HANDLE_SIZE,
            egui::Stroke::new(1.5, tokens.accent),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Shape, ends_at, moved, nearest_end, sets_of};
    use crate::scene::{Node, Scene, Shown};
    use crate::sight::{End, Kind, Wall, Walls};

    fn room() -> Scene {
        let mut scene = Scene::default();
        scene.root.children = vec![Node::Walls(Walls {
            id: 1,
            shown: Shown::default(),
            segments: vec![
                Wall::new((0.0, 0.0), (4.0, 0.0)),
                Wall::new((4.0, 0.0), (4.0, 3.0)),
                Wall::new((9.0, 9.0), (10.0, 9.0)),
            ],
        })];
        scene
    }

    #[test]
    fn a_drag_of_a_corner_moves_both_walls_that_meet_there() {
        let scene = room();
        let ends = ends_at(&scene, (4.0, 0.0));
        assert_eq!(ends, vec![(1, 0, End::B), (1, 1, End::A)]);
        let after = moved(&sets_of(&scene, &ends), &ends, (5.0, 1.0));
        let [Node::Walls(walls)] = after.as_slice() else {
            panic!("one wall set moves");
        };
        assert_eq!(walls.segments[0].b, (5.0, 1.0));
        assert_eq!(walls.segments[1].a, (5.0, 1.0));
        // The far wall stays where it was.
        assert_eq!(walls.segments[2].a, (9.0, 9.0));
    }

    #[test]
    fn a_point_snaps_to_the_nearest_end_in_reach_and_skips_the_held_ones() {
        let scene = room();
        assert_eq!(nearest_end(&scene, (3.9, 0.1), 0.5, &[]), Some((4.0, 0.0)));
        assert_eq!(nearest_end(&scene, (2.0, 2.0), 0.5, &[]), None);
        let held = ends_at(&scene, (4.0, 0.0));
        assert_eq!(nearest_end(&scene, (3.9, 0.1), 0.5, &held), None);
    }

    #[test]
    fn a_wall_that_turns_into_a_door_keeps_the_door_open_or_shut() {
        assert_eq!(Shape::of(Kind::Door { open: true }), Shape::Door);
        assert_eq!(Shape::Secret.with(true), Kind::Secret { open: true });
        assert_eq!(Shape::Wall.with(true), Kind::Wall);
    }
}
