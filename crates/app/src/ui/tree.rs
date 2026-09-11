//! The objects list: the scene tree, its rows and what a row asks for.
//!
//! The list draws one row for each node under the group it shows. A row
//! holds no history, so what it asks for reaches the scene through
//! `act_on` once the borrow on the tree is over.

// Rust guideline compliant 2026-02-21

use crate::command::{Deed, SetName, SetShown, reshape};
use crate::icon;
use crate::icons::Icon;
use crate::scene::{Group, Node, NodeId, ROOT_ID, Scene, Shown};
use crate::stroke::Stroke;
use crate::text;
use crate::theme::{self, Tokens};
use crate::widget;

use super::{Frame, PANEL_WIDTH, ROW_HEIGHT, ROW_INDENT, SWITCH, Select, TWIST};

/// Turns what the objects list asked for into changes. DESIGN.md 8.4.
///
/// The rows hold the tree while they draw, so nothing there can reach the
/// history. This runs once the borrow is over. Returns `true` when the
/// scene changed.
pub(super) fn act_on(
    frame: &mut Frame<'_>,
    select: &mut Select,
    tree: &mut Tree,
    asked: Asked,
) -> bool {
    let mut edited = false;
    if let Some(id) = asked.new_group {
        let new = Group::new(id, text::panel_objects_group_name(id));
        let name = new.name.clone();
        let into = tree.active;
        if let Some(change) = reshape(frame.scene, Deed::NewGroup, name, |scene| {
            crate::scene::push_into(scene, into, Node::Group(new));
        }) {
            frame.history.kept(change);
            tree.open.insert(id);
            tree.active = id;
            edited = true;
        }
    }
    if let Some(id) = asked.ungroup {
        // What was in the group stands where it stood.
        let freed = crate::scene::assets_of(frame.scene, id);
        let name = crate::scene::name_of(frame.scene, id);
        if let Some(change) = reshape(frame.scene, Deed::Ungroup, name, |scene| {
            crate::scene::ungroup(scene, id);
        }) {
            frame.history.kept(change);
            select.chosen = freed;
            edited = true;
        }
    }
    if let Some((node, target, into)) = asked.moved
        && let Some(change) = reshape(
            frame.scene,
            Deed::MoveInList,
            crate::scene::name_of(frame.scene, node),
            |scene| {
                if into {
                    crate::scene::move_into(scene, node, target);
                } else {
                    crate::scene::move_above(scene, node, target);
                }
            },
        )
    {
        frame.history.kept(change);
        edited = true;
    }
    if let Some((id, before, after)) = asked.shown {
        let subject = crate::scene::name_of(frame.scene, id);
        frame.history.run(
            frame.scene,
            SetShown {
                id,
                subject,
                before,
                after,
            },
        );
        edited = true;
    }
    if let (Some(done), Some(naming)) = (asked.named, tree.renaming.clone()) {
        if naming.before != naming.draft {
            frame.history.hold(
                frame.scene,
                SetName {
                    id: naming.id,
                    before: naming.before,
                    after: naming.draft,
                },
            );
            edited = true;
        }
        if done {
            frame.history.settle();
            tree.renaming = None;
        }
    }
    edited
}

/// The group the list shows, and every row under it. DESIGN.md 8.4.
///
/// The group stands at the top, so the DM sees where the list is before
/// they read a single child.
pub(super) fn scoped_rows(
    ui: &mut egui::Ui,
    scene: &mut Scene,
    select: &mut Select,
    tree: &mut Tree,
    asked: &mut Asked,
    tokens: Tokens,
) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let scope = tree.scope;
    let open = tree.open.contains(&scope);
    let picked = scope != ROOT_ID && select.holds(scope);
    let naming = tree
        .renaming
        .clone()
        .filter(|naming| naming.id == scope && scope != ROOT_ID);
    let Some(group) = crate::scene::group_mut(scene, scope) else {
        return;
    };
    let row = tree_row(
        ui,
        RowLook {
            depth: 0,
            picked,
            twist: Some(open),
            glyph: Icon::Folder,
            accent_glyph: tree.active == scope,
            tokens,
        },
    );
    if let Some(mut naming) = naming {
        let (_, done) = rename_field(ui, row.name, &mut naming.draft, tokens);
        tree.renaming = Some(naming);
        asked.named = Some(done);
    } else {
        row_name(ui, row.name, &group.name, picked, tokens);
    }
    if row.body.double_clicked() && scope != ROOT_ID {
        tree.renaming = Some(Renaming {
            id: scope,
            before: group.name.clone(),
            draft: group.name.clone(),
        });
    }
    // DESIGN.md 8.4: the root is always visible and carries no switches. A
    // group the DM went into carries its own.
    if scope != ROOT_ID
        && let Some(after) = switches(ui, row.switches, group.shown, tokens)
    {
        asked.shown = Some((scope, group.shown, after));
    }
    if row.twist.is_some_and(|twist| twist.clicked()) {
        flip(&mut tree.open, scope);
    }
    if row.body.clicked() {
        tree.active = scope;
        if scope != ROOT_ID {
            select.take(scope, ui.input(|i| i.modifiers.ctrl));
        }
    }
    if open {
        tree_rows(ui, &mut group.children, select, tree, 1, asked, tokens);
    }
}

/// What the tree list holds between frames.
#[derive(Debug)]
pub(super) struct Tree {
    /// The group a new asset joins.
    pub(super) active: NodeId,
    /// The groups whose children the list shows.
    pub(super) open: std::collections::HashSet<NodeId>,
    /// The node the list opened its groups for.
    pub(super) shown: Option<NodeId>,
    /// How wide the panel stands, in points. DESIGN.md 8.4.
    pub(super) width: f32,
    /// The group whose name the DM is writing, from a double click.
    renaming: Option<Renaming>,
    /// The group the list shows. Its children are the rows under it.
    ///
    /// DESIGN.md 8.4: the list never shows the whole tree from the root,
    /// so the indent of a deep scene has no room to run away.
    pub(super) scope: NodeId,
}

/// What the objects list asks for, to act on once the tree is free.
///
/// A row reads and writes the tree through one borrow, so nothing in it
/// can reach the history. Each row leaves its ask here instead, and the
/// panel turns it into a change when the borrow is over.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Asked {
    /// A node to move: the node, where it goes, and whether it goes inside.
    moved: Option<(NodeId, NodeId, bool)>,
    /// New switches for one node: the node, what it had, what it takes.
    shown: Option<(NodeId, Shown, Shown)>,
    /// The DM typed a name this frame. `true` when they finished.
    named: Option<bool>,
    /// The name of a new group the DM asked for.
    pub(super) new_group: Option<NodeId>,
    /// A group the DM asked to take apart.
    pub(super) ungroup: Option<NodeId>,
}

/// A group name the DM is typing.
///
/// The draft lives here, not in the scene, so every keystroke reaches the
/// tree as one change the DM can take back whole.
#[derive(Debug, Clone)]
struct Renaming {
    /// The group that takes the name.
    id: NodeId,
    /// The name the group carried before the first keystroke.
    before: String,
    /// The name as it stands now.
    draft: String,
}

impl Default for Tree {
    fn default() -> Self {
        Self {
            active: ROOT_ID,
            open: std::collections::HashSet::from([ROOT_ID]),
            shown: None,
            renaming: None,
            width: PANEL_WIDTH,
            scope: ROOT_ID,
        }
    }
}
/// The rows under one group. What the DM pressed lands in `asked`.
///
/// DESIGN.md 8.4 gives every row the same shape: a twist, the glyph that
/// says what the row is, the name, then the two switches.
pub(super) fn tree_rows(
    ui: &mut egui::Ui,
    nodes: &mut [Node],
    select: &mut Select,
    tree: &mut Tree,
    depth: usize,
    asked: &mut Asked,
    tokens: Tokens,
) {
    // The list reads from the top down, and the last node draws over the
    // rest, so the last node comes first.
    for node in nodes.iter_mut().rev() {
        match node {
            Node::Group(group) => {
                let id = group.id;
                let open = tree.open.contains(&id);
                let picked = select.holds(id);
                let row = tree_row(
                    ui,
                    RowLook {
                        depth,
                        picked,
                        twist: Some(open),
                        glyph: Icon::Folder,
                        accent_glyph: tree.active == id,
                        tokens,
                    },
                );
                row_name(ui, row.name, &group.name, picked, tokens);
                if row.twist.is_some_and(|twist| twist.clicked()) {
                    flip(&mut tree.open, id);
                }
                if row.body.clicked() {
                    let add = ui.input(|i| i.modifiers.ctrl);
                    select.take(id, add);
                    tree.active = id;
                    // DESIGN.md 8.4: a click into a group puts that group
                    // at the top of the list. Ctrl gathers a selection
                    // instead, so it leaves the list where it stands.
                    if !add {
                        tree.scope = id;
                        tree.open.insert(id);
                        // A name half written on another row does not
                        // follow the list into a new group.
                        tree.renaming = None;
                    }
                }
                if row.body.drag_started() {
                    egui::DragAndDrop::set_payload(ui.ctx(), id);
                }
                if let Some(after) = switches(ui, row.switches, group.shown, tokens) {
                    asked.shown = Some((id, group.shown, after));
                }
                dropped_on(ui, &row.whole, id, true, &mut asked.moved, tokens);
                if open {
                    tree_rows(
                        ui,
                        &mut group.children,
                        select,
                        tree,
                        depth + 1,
                        asked,
                        tokens,
                    );
                }
            }
            Node::Stroke(stroke) => stroke_row(ui, stroke, select, depth, asked, tokens),
            Node::Asset(asset) => {
                let id = asset.id;
                let picked = select.holds(id);
                let row = tree_row(
                    ui,
                    RowLook {
                        depth,
                        picked,
                        twist: None,
                        glyph: Icon::Image,
                        accent_glyph: false,
                        tokens,
                    },
                );
                if row.body.clicked() {
                    select.take(id, ui.input(|i| i.modifiers.ctrl));
                }
                if row.body.drag_started() {
                    egui::DragAndDrop::set_payload(ui.ctx(), id);
                }
                let name = asset.path.to_string_lossy().into_owned();
                row_name(ui, row.name, &name, picked, tokens);
                if let Some(after) = switches(ui, row.switches, asset.shown, tokens) {
                    asked.shown = Some((id, asset.shown, after));
                }
                dropped_on(ui, &row.whole, id, false, &mut asked.moved, tokens);
            }
        }
    }
}

/// One row of the objects list for a stroke. DESIGN.md 8.4.
fn stroke_row(
    ui: &mut egui::Ui,
    stroke: &Stroke,
    select: &mut Select,
    depth: usize,
    asked: &mut Asked,
    tokens: Tokens,
) {
    let id = stroke.id;
    let picked = select.holds(id);
    let row = tree_row(
        ui,
        RowLook {
            depth,
            picked,
            twist: None,
            glyph: stroke.ink.glyph(),
            accent_glyph: false,
            tokens,
        },
    );
    if row.body.clicked() {
        select.take(id, ui.input(|i| i.modifiers.ctrl));
    }
    if row.body.drag_started() {
        egui::DragAndDrop::set_payload(ui.ctx(), id);
    }
    row_name(ui, row.name, stroke.ink.name(), picked, tokens);
    if let Some(after) = switches(ui, row.switches, stroke.shown, tokens) {
        asked.shown = Some((id, stroke.shown, after));
    }
    dropped_on(ui, &row.whole, id, false, &mut asked.moved, tokens);
}

/// What one row of the objects list looks like. DESIGN.md 8.4.
#[derive(Debug, Clone, Copy)]
struct RowLook {
    /// How deep the row sits under the root.
    depth: usize,
    /// Whether the DM holds this row.
    picked: bool,
    /// `Some(true)` for an open group, `None` for an asset.
    twist: Option<bool>,
    /// The glyph that says what the row is.
    glyph: Icon,
    /// Whether the glyph takes the accent, for the group a new asset joins.
    accent_glyph: bool,
    /// The colors of the theme.
    tokens: Tokens,
}

/// The parts of a drawn row that the caller still has to fill.
#[derive(Debug)]
struct Row {
    /// The whole row, for a drop.
    whole: egui::Response,
    /// The glyph and the name, for a click and a drag.
    body: egui::Response,
    /// Where the name goes.
    name: egui::Rect,
    /// Where the two switches go.
    switches: egui::Rect,
    /// The twist of a group.
    twist: Option<egui::Response>,
}

/// Where each part of one row of the objects list sits. DESIGN.md 8.4.
#[derive(Debug, Clone, Copy)]
struct RowParts {
    /// The square that opens and closes a group, or `None` for an asset.
    twist: Option<egui::Rect>,
    /// Where the glyph that says what the row is starts.
    glyph_left: f32,
    /// The part of the row that takes a click and starts a drag.
    body: egui::Rect,
    /// Where the name goes.
    name: egui::Rect,
    /// Where the two switches go.
    switches: egui::Rect,
}

/// Cuts one row into its parts.
///
/// The twist and the body never cover the same point. `egui` gives a point
/// that two widgets cover to the one that came later, and the body comes
/// later, so an overlap would take every press away from the twist. That
/// is the bug where the arrow opened the root and no group under it.
fn row_parts(rect: egui::Rect, depth: usize, has_twist: bool) -> RowParts {
    let indent = rect.left() + widget::BAR + 3.0 + depth as f32 * ROW_INDENT;
    let twist = has_twist.then(|| {
        egui::Rect::from_center_size(
            egui::pos2(indent + TWIST / 2.0, rect.center().y),
            egui::Vec2::splat(TWIST + 6.0),
        )
    });
    // An asset carries no twist, and its glyph still lines up with the
    // glyph of a group beside it.
    let glyph_left = indent + TWIST + 5.0;
    let switches = egui::Rect::from_min_max(
        egui::pos2(rect.right() - 2.0 * SWITCH, rect.top()),
        rect.right_bottom(),
    );
    let name = egui::Rect::from_min_max(
        egui::pos2(glyph_left + widget::SMALL_ICON + 6.0, rect.top()),
        egui::pos2(switches.left() - 5.0, rect.bottom()),
    );
    // The body starts a point past the twist. Two rects that share an edge
    // both sit at distance zero from a pointer on it, and the body would
    // win that tie.
    let body = egui::Rect::from_min_max(
        egui::pos2(
            twist.map_or(rect.left(), |square| square.right() + 1.0),
            rect.top(),
        ),
        egui::pos2(name.right(), rect.bottom()),
    );
    RowParts {
        twist,
        glyph_left,
        body,
        name,
        switches,
    }
}

/// Paints the frame of one row and hands back the space that is left.
///
/// The twist and the body of the row never cover the same point. Two
/// widgets over one point leave it to `egui` which of them takes the
/// press, and the twist would lose: it is the small one inside the large
/// one. So the body of a group starts where its twist ends.
fn tree_row(ui: &mut egui::Ui, look: RowLook) -> Row {
    let tokens = look.tokens;
    let (rect, whole) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_HEIGHT),
        egui::Sense::hover(),
    );
    let parts = row_parts(rect, look.depth, look.twist.is_some());
    let RowParts {
        twist: square,
        glyph_left,
        body: body_rect,
        name,
        switches,
    } = parts;
    let twist = look.twist.zip(square).map(|(open, square)| {
        let response = ui.interact(
            square,
            ui.id().with(("twist", rect.top() as i32, look.depth)),
            egui::Sense::click(),
        );
        (response, square, open)
    });
    let body = ui.interact(
        body_rect,
        ui.id().with(("row", rect.top() as i32, look.depth)),
        egui::Sense::click_and_drag(),
    );
    if look.picked {
        ui.painter().rect_filled(rect, 0, tokens.raised);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(widget::BAR, ROW_HEIGHT)),
            0,
            tokens.accent,
        );
    } else if body.hovered() {
        ui.painter().rect_filled(rect, 0, tokens.field);
    }
    if let Some((response, square, open)) = &twist {
        let color = if response.hovered() {
            tokens.ink
        } else {
            tokens.mute
        };
        icon::paint(
            ui.painter(),
            if *open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            square.center(),
            TWIST,
            color,
        );
    }
    // The accent says two things on one glyph: the DM holds this row, or a
    // new asset joins this group.
    let glyph_color = if look.accent_glyph || look.picked {
        tokens.accent
    } else {
        tokens.ink
    };
    icon::paint(
        ui.painter(),
        look.glyph,
        egui::pos2(glyph_left + widget::SMALL_ICON / 2.0, rect.center().y),
        widget::SMALL_ICON,
        glyph_color,
    );
    Row {
        whole,
        body,
        name,
        switches,
        twist: twist.map(|(response, ..)| response),
    }
}

/// The name of a row, cut with an ellipsis when it runs too long.
///
/// DESIGN.md 8.4: the whole name comes up under the pointer.
pub(super) fn row_name(ui: &egui::Ui, rect: egui::Rect, name: &str, picked: bool, tokens: Tokens) {
    let color = if picked { tokens.accent } else { tokens.ink };
    let font = theme::font(theme::BODY, false);
    let galley = ui.ctx().fonts_mut(|fonts| {
        let mut job = egui::text::LayoutJob::simple_singleline(name.to_owned(), font, color);
        job.wrap.max_width = rect.width();
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        fonts.layout_job(job)
    });
    let cut = galley.rows.first().is_some_and(|row| row.ends_with_newline)
        || galley.size().x >= rect.width();
    ui.painter().with_clip_rect(rect).galley(
        egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    if cut {
        ui.interact(
            rect,
            ui.id().with(("name", rect.top() as i32)),
            egui::Sense::hover(),
        )
        .on_hover_text(name);
    }
}

/// A rename in place, from a double click on the name. DESIGN.md 10.
///
/// A group carries no panel, so its row is where its name is written. Only
/// the row at the top of the list takes this, because a click on any other
/// row takes the list into that group and every row moves. The second
/// click of a double click would then land on a row that was not there
/// when the first one went down.
///
/// The field takes the keyboard the frame it appears, because the click
/// that opened it is over by then. `Enter` and `Escape` both give it up,
/// and so does a click anywhere else.
///
/// Returns whether the name changed, and whether the DM is done with it.
fn rename_field(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    name: &mut String,
    tokens: Tokens,
) -> (bool, bool) {
    let mut field = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    field.set_clip_rect(rect);
    let visuals = &mut field.style_mut().visuals;
    visuals.extreme_bg_color = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, tokens.accent);
    let response = field.add(
        egui::TextEdit::singleline(name)
            .desired_width(rect.width())
            .font(theme::font(theme::BODY, false))
            .text_color(tokens.accent)
            .margin(egui::Margin::ZERO),
    );
    if !response.has_focus() && !response.lost_focus() {
        response.request_focus();
    }
    let key = field.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape));
    (response.changed(), key || response.lost_focus())
}

/// The two switches on a row: the DM screen, then the TV. DESIGN.md 8.4.
///
/// Returns the pair the DM asked for, when they pressed one of the two.
fn switches(ui: &mut egui::Ui, rect: egui::Rect, shown: Shown, tokens: Tokens) -> Option<Shown> {
    let mut asked = None;
    for (index, (on, glyph, off_glyph)) in [
        (shown.dm, Icon::Eye, Icon::EyeOff),
        (shown.tv, Icon::Monitor, Icon::MonitorOff),
    ]
    .into_iter()
    .enumerate()
    {
        let square = egui::Rect::from_min_size(
            egui::pos2(rect.left() + index as f32 * SWITCH, rect.top()),
            egui::vec2(SWITCH, rect.height()),
        );
        let response = ui.interact(
            square,
            ui.id().with(("switch", index, rect.top() as i32)),
            egui::Sense::click(),
        );
        if response.clicked() {
            let mut after = shown;
            if index == 0 {
                after.dm = !shown.dm;
            } else {
                after.tv = !shown.tv;
            }
            asked = Some(after);
        }
        let (glyph, color) = if on {
            (glyph, tokens.ink)
        } else {
            (off_glyph, tokens.mute)
        };
        icon::paint(
            ui.painter(),
            glyph,
            square.center(),
            widget::SMALL_ICON,
            color,
        );
    }
    asked
}

/// Marks where a row on its way through the tree would land.
///
/// A drop on a group goes into that group. A drop on an asset takes the
/// place of that asset, in the group that holds it.
fn dropped_on(
    ui: &egui::Ui,
    row: &egui::Response,
    id: NodeId,
    is_group: bool,
    moved: &mut Option<(NodeId, NodeId, bool)>,
    tokens: Tokens,
) {
    if row.dnd_hover_payload::<NodeId>().is_some() {
        let rect = row.rect;
        ui.painter().hline(
            rect.x_range(),
            rect.top(),
            egui::Stroke::new(2.0, tokens.accent),
        );
    }
    if let Some(dragged) = row.dnd_release_payload::<NodeId>() {
        *moved = Some((*dragged, id, is_group));
    }
}

/// Opens a closed group, and closes an open one.
fn flip(open: &mut std::collections::HashSet<NodeId>, id: NodeId) {
    if !open.remove(&id) {
        open.insert(id);
    }
}

#[cfg(test)]
mod tests {
    use super::{ROW_HEIGHT, row_parts};

    /// One row of the objects list, the size the panel gives it.
    fn row() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(22.0, 80.0), egui::vec2(218.0, ROW_HEIGHT))
    }

    #[test]
    fn the_twist_and_the_body_of_a_row_never_meet() {
        // `egui` gives a point that two widgets cover to the one that came
        // later. The body comes later, so an overlap would take every
        // press away from the twist, and no group would open.
        for depth in 1..6 {
            let parts = row_parts(row(), depth, true);
            let twist = parts.twist.expect("a group carries a twist");
            assert!(
                twist.right() < parts.body.left(),
                "depth {depth}: the twist reaches into the body"
            );
            assert!(!twist.intersects(parts.body), "depth {depth}: the two meet");
        }
    }

    #[test]
    fn a_row_without_a_twist_gives_its_whole_width_to_the_body() {
        let parts = row_parts(row(), 1, false);
        assert!(parts.twist.is_none());
        assert!((parts.body.left() - row().left()).abs() < f32::EPSILON);
    }

    #[test]
    fn the_parts_of_a_row_stay_in_order() {
        let parts = row_parts(row(), 1, true);
        let twist = parts.twist.expect("a group carries a twist");
        assert!(twist.right() <= parts.glyph_left);
        assert!(parts.glyph_left < parts.name.left());
        assert!(parts.name.right() <= parts.switches.left());
        assert!((parts.switches.right() - row().right()).abs() < f32::EPSILON);
    }

    #[test]
    fn a_deeper_row_indents_and_keeps_its_switches() {
        // DESIGN.md 8.4: a child row indents from its parent, and the two
        // switches stay on the right edge whatever the depth.
        let shallow = row_parts(row(), 1, true);
        let deep = row_parts(row(), 2, true);
        let step = deep.glyph_left - shallow.glyph_left;
        assert!((step - super::ROW_INDENT).abs() < f32::EPSILON);
        assert_eq!(shallow.switches, deep.switches);
    }
}
