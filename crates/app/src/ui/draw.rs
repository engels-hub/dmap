//! The Draw tool: the pen, the shapes, the ruler and the eraser.
//!
//! One stroke stands under the hand while the DM drags. It joins the
//! scene as one step when the button goes up. The eraser writes the
//! scene as it moves, and the whole drag becomes one step. DESIGN.md 8.3.

// Rust guideline compliant 2026-02-21

use crate::camera::Camera;
use crate::command::{Deed, Restructure, reshape};
use crate::grid::Cells;
use crate::scene::{Group, Node, NodeId, ROOT_ID, Scene};
use crate::stroke::{Ink, Stroke};
use crate::text;
use crate::theme::{self, Tokens};

use super::{
    ERASER_REACH, FEET_PER_CELL, Frame, LABEL_GAP, LABEL_PAD, MIN_SHAPE, PEN_STEP, Pointer, View,
    canvas_area,
};

/// The Draw view's state between frames.
#[derive(Debug, Default)]
pub(super) struct Draw {
    /// Whether the DM is on the second drag, the one that sets the width.
    ///
    /// A beam takes two drags: one for how far it runs, one for how wide.
    /// Neither drag asks for a button and a move at the same time.
    /// Issue #12.
    spanning: bool,
    /// Whether that second drag has begun.
    pulled: bool,
    /// The stroke under the DM's hand, which is in no scene yet.
    pub(super) live: Option<Stroke>,
    /// Where the eraser stood last frame, so a fast drag bites nothing
    /// between one frame and the next.
    last: Option<(f64, f64)>,
    /// The tree as it stood when the eraser went down.
    ///
    /// The eraser writes the scene while it moves, so the DM watches the
    /// line go. The whole drag becomes one step out of this.
    before: Option<Vec<Node>>,
}

impl Draw {
    /// Whether the DM has something under their hand.
    pub(super) fn busy(&self) -> bool {
        self.live.is_some() || self.before.is_some()
    }
}

/// The Draw view on the canvas: the pen and the shapes. DESIGN.md 8.3.
///
/// Returns `true` when the scene changed.
pub(super) fn draw_tool(
    ui: &mut egui::Ui,
    draw: &mut Draw,
    frame: &mut Frame<'_>,
    viewport: (u32, u32),
    zoom_goes_to: &mut Option<bool>,
) -> bool {
    let (_, view, pointer) = canvas_area(ui, frame.camera, viewport, false, zoom_goes_to);
    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    let at = pointer.pos.map(|pos| view.to_world(pos));
    let nib = frame.settings.ink_nib;
    // A DM who picks another square in the middle of a gesture leaves the
    // last one behind. Without this the stroke stands under their hand for
    // ever: it draws on both screens, and the history waits on a hand that
    // has let go. Issue #12.
    if draw
        .live
        .as_ref()
        .is_some_and(|live| nib.ink() != Some(live.ink))
    {
        draw.live = None;
        draw.spanning = false;
        draw.pulled = false;
    }
    let Some(ink) = nib.ink() else {
        return erase(draw, frame, at, pointer);
    };
    // A shape starts on a crossing of the grid, because a spell lands on
    // a cell. `Shift` holds that off for one drag. The ruler takes `Alt`
    // instead, because `Shift` there keeps the measure. Issue #12.
    let off = ui.input(|i| {
        if ink == Ink::Measure {
            i.modifiers.alt
        } else {
            i.modifiers.shift
        }
    });
    let snap = frame.settings.ink_snap != off && ink != Ink::Pen;
    // A measure lands on the grid at both ends. Every other shape starts
    // on it and reaches wherever the DM pulls.
    let at = at.map(|at| {
        if snap && ink == Ink::Measure {
            on_grid(frame.settings.cells(), at)
        } else {
            at
        }
    });
    if ink == Ink::Measure {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            draw.live = None;
            return false;
        }
        // The second button bends the ruler where the pointer stands.
        if let (true, Some(at), Some(live)) = (
            ui.input(|i| i.pointer.secondary_pressed()),
            at,
            draw.live.as_mut(),
        ) {
            live.points.push(at);
        }
    }
    // The second drag of a beam says how wide it runs. The width follows
    // the pointer until the DM takes hold of it, and the beam goes into
    // the scene when they let go, so neither drag ever asks for a button
    // and a move at the same time. Issue #12.
    if draw.spanning {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            draw.spanning = false;
            draw.pulled = false;
            draw.live = None;
            return false;
        }
        if let (Some(at), Some(live)) = (at, draw.live.as_mut()) {
            live.span = span_of(live, at);
        }
        if pointer.down() {
            draw.pulled = true;
            return false;
        }
        if draw.pulled {
            draw.spanning = false;
            draw.pulled = false;
            return keep(draw.live.take(), frame);
        }
        return false;
    }
    if let (true, true, Some(at)) = (pointer.pressed(), pointer.hovered, at) {
        frame.history.settle();
        draw.live = Some(Stroke {
            // The name comes at the end, in `join_scene`, because a map
            // the DM adds while they draw would take the same one.
            id: ROOT_ID,
            shown: crate::scene::Shown::default(),
            ink,
            points: vec![if snap {
                on_grid(frame.settings.cells(), at)
            } else {
                at
            }],
            color: frame.settings.ink_color,
            width: frame.settings.ink_width,
            span: 0.0,
            rule: frame.settings.ink_rule,
        });
    }
    if let (true, Some(at), Some(live)) = (pointer.down(), at, draw.live.as_mut()) {
        grow(live, at);
        return false;
    }
    // A cone and a beam are not done: the hand lets go, and the next
    // drag says how wide they are.
    if draw.live.as_ref().is_some_and(|live| live.ink.spans()) {
        draw.spanning = true;
        return false;
    }
    let live = draw.live.take();
    // A measure is gone when the DM lets go, unless they held Shift.
    if live.as_ref().is_some_and(|live| live.ink == Ink::Measure)
        && !ui.input(|i| i.modifiers.shift)
    {
        return false;
    }
    keep(live, frame)
}

/// Puts the stroke the DM finished into the scene. Issue #12.
///
/// A press that never moved leaves a dot for the pen and nothing for a
/// shape, which would have no size at all.
fn keep(live: Option<Stroke>, frame: &mut Frame<'_>) -> bool {
    let Some(live) = live else {
        return false;
    };
    if live.ink != Ink::Pen {
        // A twitch of the hand between the press and the release is not a
        // shape. A shape has to reach at least a little way.
        let (Some(from), Some(to)) = (live.points.first(), live.points.last()) else {
            return false;
        };
        if (to.0 - from.0).hypot(to.1 - from.1) < MIN_SHAPE {
            return false;
        }
    }
    let Some(change) = join_scene(frame.scene, live) else {
        return false;
    };
    frame.history.kept(change);
    true
}

/// Puts a finished stroke in the group the Draw view fills, as one change.
///
/// The group is made inside the change and not before it, or the first
/// stroke of a scene would leave the group behind when the DM takes that
/// stroke back. The stroke takes its name here as well, so a map added
/// while the DM drew cannot have taken the same one. Issue #12.
fn join_scene(scene: &mut Scene, live: Stroke) -> Option<Restructure> {
    let name = live.ink.name().to_owned();
    reshape(scene, Deed::Draw, name, |scene| {
        let into = crate::scene::ink_group(scene, text::panel_objects_drawings().to_owned());
        let mut live = live;
        live.id = scene.next_id();
        crate::scene::push_into(scene, into, Node::Stroke(live));
    })
}

/// How wide the pointer makes a cone or a beam, in cells.
///
/// The width is twice the distance from the pointer to the line the
/// shape runs along, so the shape grows to either side as the DM pulls
/// away from it.
fn span_of(live: &Stroke, at: (f64, f64)) -> f64 {
    let (Some(from), Some(to)) = (live.points.first(), live.points.last()) else {
        return 0.0;
    };
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy);
    if length <= f64::EPSILON {
        return 0.0;
    }
    // The distance from a point to the line, by the cross product.
    let away = ((at.0 - from.0) * dy - (at.1 - from.1) * dx).abs() / length;
    2.0 * away
}

/// Draws the labels of every measure the screen shows. Issue #12.
///
/// A kept measure carries its length as the live one does, so the DM and
/// the players read the same numbers off the same line.
#[expect(
    clippy::too_many_arguments,
    reason = "one overlay of labels: what it labels, where, on which grid, and how"
)]
pub fn measure_overlay(
    painter: &egui::Painter,
    strokes: &[&Stroke],
    camera: &Camera,
    viewport: (u32, u32),
    ppp: f64,
    cells: Cells,
    tokens: Tokens,
    size: f32,
) {
    let view = View {
        camera: *camera,
        viewport,
        ppp,
    };
    for stroke in strokes {
        if stroke.ink == Ink::Measure {
            measure_labels(painter, stroke, &view, cells, tokens, size);
        } else if let (Some(reach), Some(last)) =
            (stroke.ink.reach(&stroke.points), stroke.points.last())
        {
            // A stroke holds inches, and a label says cells. One cell is
            // one inch no longer, so both numbers turn. Issue #15.
            let says = if stroke.span > 0.0 {
                // The width says cells alone. The length beside it
                // already carries the feet.
                let across = text::ruler_cells(format_args!("{:.1}", cells.in_cells(stroke.span)));
                text::ruler_span(length(cells.in_cells(reach)), across)
            } else {
                length(cells.in_cells(reach))
            };
            label(painter, view.to_screen(*last), &says, tokens, true, size);
        }
    }
}

/// Draws the labels of a measure, one for each leg and one for the whole.
///
/// The label sits beside the middle of its leg, and the total stands at
/// the end, where the pointer is. Issue #12.
fn measure_labels(
    painter: &egui::Painter,
    live: &Stroke,
    view: &View,
    cells: Cells,
    tokens: Tokens,
    size: f32,
) {
    let mut whole = 0.0;
    for pair in live.points.windows(2) {
        let leg = live.rule.cells(cells, pair[0], pair[1]);
        whole += leg;
        let middle = (
            f64::midpoint(pair[0].0, pair[1].0),
            f64::midpoint(pair[0].1, pair[1].1),
        );
        label(
            painter,
            view.to_screen(middle),
            &length(leg),
            tokens,
            false,
            size,
        );
    }
    if live.points.len() > 2
        && let Some(last) = live.points.last()
    {
        label(
            painter,
            view.to_screen(*last),
            &length(whole),
            tokens,
            true,
            size,
        );
    }
}

/// One label of the ruler, in a box that reads over any map.
fn label(
    painter: &egui::Painter,
    at: egui::Pos2,
    text: &str,
    tokens: Tokens,
    whole: bool,
    size: f32,
) {
    let font = theme::font(size, whole);
    let galley = painter.layout_no_wrap(text.to_owned(), font, tokens.ink);
    let box_rect = egui::Rect::from_min_size(
        at + egui::vec2(LABEL_GAP, -galley.size().y - LABEL_GAP),
        galley.size() + egui::Vec2::splat(2.0 * LABEL_PAD),
    );
    painter.rect(
        box_rect,
        0,
        tokens.surface,
        egui::Stroke::new(1.0, tokens.ink),
        egui::StrokeKind::Inside,
    );
    painter.galley(
        box_rect.min + egui::Vec2::splat(LABEL_PAD),
        galley,
        tokens.ink,
    );
}

/// How far a leg runs, in cells and in feet. Issue #12.
fn length(cells: f64) -> String {
    text::ruler_length(
        format_args!("{cells:.1}"),
        format_args!("{:.0}", cells * FEET_PER_CELL),
    )
}

/// The nearest point of the grid, in inches.
///
/// A square grid gives the nearest crossing. A hex grid gives the middle
/// of the nearest hex, because a spell lands on a cell, not on a corner.
/// Issue #15.
fn on_grid(cells: Cells, at: (f64, f64)) -> (f64, f64) {
    cells.snap(at)
}

/// Takes a bite out of every stroke the eraser passes over.
///
/// The scene changes while the eraser moves, so the DM watches the line
/// go. The whole drag becomes one step when the hand lets go.
fn erase(draw: &mut Draw, frame: &mut Frame<'_>, at: Option<(f64, f64)>, pointer: Pointer) -> bool {
    if let (true, true, Some(at)) = (pointer.pressed(), pointer.hovered, at) {
        frame.history.settle();
        draw.before = Some(frame.scene.root.children.clone());
        draw.last = Some(at);
    }
    if let (true, Some(at), true) = (pointer.down(), at, draw.before.is_some()) {
        // The pointer jumps from frame to frame, and a stroke between two
        // of those places would come through a drag untouched. So the
        // eraser bites its way along the path it took.
        let from = draw.last.unwrap_or(at);
        draw.last = Some(at);
        let far = (at.0 - from.0).hypot(at.1 - from.1);
        let steps = (far / ERASER_REACH).ceil().max(1.0);
        let mut cut_one = false;
        for step in 0..=(steps as usize) {
            let part = step as f64 / steps;
            let here = (
                from.0 + (at.0 - from.0) * part,
                from.1 + (at.1 - from.1) * part,
            );
            cut_one |= bite(frame.scene, here, ERASER_REACH);
        }
        return cut_one;
    }
    draw.last = None;
    let Some(before) = draw.before.take() else {
        return false;
    };
    if before == frame.scene.root.children {
        return false;
    }
    frame.history.kept(Restructure {
        what: Deed::Erase,
        subject: text::stroke_eraser().to_owned(),
        before: vec![(ROOT_ID, before)],
        after: vec![(ROOT_ID, frame.scene.root.children.clone())],
    });
    true
}

/// Cuts every stroke a disc reaches. Returns `true` when one gave way.
///
/// An area of effect and a kept measure come away whole: a piece of
/// either one says nothing. Paint comes off in parts, and the parts of
/// one stroke join a group of their own, so the objects list holds them
/// together and the DM moves or hides them as one. Issue #12.
fn bite(scene: &mut Scene, at: (f64, f64), radius: f64) -> bool {
    let hit: Vec<NodeId> = crate::scene::ink_order(scene, crate::scene::Audience::Dm)
        .into_iter()
        .filter(|stroke| stroke.touches(at, radius))
        .map(|stroke| stroke.id)
        .collect();
    let mut cut_one = false;
    for id in hit {
        let Some((parent, place)) = crate::scene::parent_of(scene, id) else {
            continue;
        };
        let Some(Node::Stroke(stroke)) = crate::scene::take_node(scene, id) else {
            continue;
        };
        cut_one = true;
        if stroke.ink.whole() {
            // It is gone, and nothing goes back in its place.
            continue;
        }
        let runs = crate::stroke::cut(&stroke.polyline(), at, radius + stroke.width / 2.0);
        let Some(runs) = runs else {
            // The disc reached the stroke and cut nothing out of it, so
            // the stroke goes back where it was.
            put_back(scene, parent, place, Node::Stroke(stroke));
            continue;
        };
        // A stroke that came apart before is already in a group of its
        // own, and the new pieces stay in it. Only the first cut makes
        // one, or a long rub would build a tower of groups.
        let holder = if runs.len() > 1 && !pieces_group(scene, parent) {
            let id = scene.next_id();
            let mut group = Group::new(id, text::panel_objects_pieces().to_owned());
            group.shown = stroke.shown;
            put_back(scene, parent, place, Node::Group(group));
            id
        } else {
            parent
        };
        for (step, run) in runs.into_iter().enumerate() {
            let id = scene.next_id();
            let piece = Stroke {
                id,
                // What is left of a shape is a free line: a box with a
                // bite out of it is no longer a box.
                ink: Ink::Pen,
                points: run,
                ..stroke.clone()
            };
            let place = if holder == parent { place + step } else { step };
            put_back(scene, holder, place, Node::Stroke(piece));
        }
    }
    cut_one
}

/// Puts a node back into a group, at the place it had or at the end.
fn put_back(scene: &mut Scene, parent: NodeId, place: usize, node: Node) {
    if let Some(group) = crate::scene::group_mut(scene, parent) {
        let place = place.min(group.children.len());
        group.children.insert(place, node);
    }
}

/// Whether this group already holds the pieces of a stroke.
///
/// The group the Draw view fills is not one of those, and neither is the
/// root, so the first cut inside either one makes a group.
fn pieces_group(scene: &Scene, id: NodeId) -> bool {
    id != ROOT_ID
        && crate::scene::find(scene, id)
            .and_then(Node::group)
            .is_some_and(|group| !group.ink)
}

/// Takes the stroke under the DM's hand to where the pointer stands.
fn grow(live: &mut Stroke, at: (f64, f64)) {
    if live.ink == Ink::Pen {
        // A point that lands on the one before it says nothing, and a pen
        // drags out thousands of them.
        let far = live
            .points
            .last()
            .is_none_or(|last| (last.0 - at.0).hypot(last.1 - at.1) > PEN_STEP);
        if far {
            live.points.push(at);
        }
        return;
    }
    if live.ink == Ink::Measure {
        // Every waypoint stays. The last point follows the pointer.
        if live.points.len() < 2 {
            live.points.push(at);
        } else if let Some(last) = live.points.last_mut() {
            *last = at;
        }
        return;
    }
    // A shape stands between the press and the pointer.
    live.points.truncate(1);
    live.points.push(at);
}

#[cfg(test)]
mod tests {
    use super::{bite, join_scene};
    use crate::command::Command as _;
    use crate::scene::{Group, Node, Scene, Shown};
    use crate::stroke::{Ink, Rule, Stroke};

    /// A stroke of this ink, through these points.
    fn stroke(id: u64, ink: Ink, points: &[(f64, f64)]) -> Stroke {
        Stroke {
            id,
            shown: Shown::default(),
            ink,
            points: points.to_vec(),
            color: [0, 0, 0, 255],
            width: 0.1,
            span: 0.0,
            rule: Rule::default(),
        }
    }

    /// A scene whose Drawings group holds these strokes.
    fn drawn(strokes: Vec<Stroke>) -> Scene {
        let mut scene = Scene::default();
        let mut group = Group::new(9, "Drawings".to_owned());
        group.ink = true;
        group.children = strokes.into_iter().map(Node::Stroke).collect();
        scene.root.children.push(Node::Group(group));
        scene
    }

    /// What the Drawings group holds now, as names and depths.
    fn inside(scene: &Scene) -> Vec<(usize, String)> {
        fn walk(nodes: &[Node], depth: usize, found: &mut Vec<(usize, String)>) {
            for node in nodes {
                match node {
                    Node::Group(group) => {
                        found.push((depth, group.name.clone()));
                        walk(&group.children, depth + 1, found);
                    }
                    Node::Stroke(stroke) => found.push((depth, format!("{:?}", stroke.ink))),
                    Node::Asset(_) => {}
                }
            }
        }
        let mut found = Vec::new();
        walk(&scene.root.children, 0, &mut found);
        found
    }

    #[test]
    fn taking_back_the_first_stroke_takes_its_group_with_it() {
        let mut scene = Scene::default();
        let mark = stroke(0, Ink::Pen, &[(0.0, 0.0), (1.0, 0.0)]);
        let change = join_scene(&mut scene, mark).expect("the stroke joins the tree");
        assert_eq!(
            inside(&scene),
            vec![(0, "Drawings".to_owned()), (1, "Pen".to_owned())]
        );
        // The group was made inside the change, so the change takes it
        // back as well and no empty group stands behind.
        change.revert(&mut scene);
        assert!(inside(&scene).is_empty(), "{:?}", inside(&scene));
    }

    #[test]
    fn a_stroke_takes_its_name_where_it_joins_the_tree() {
        let mut scene = drawn(vec![stroke(9000, Ink::Pen, &[(0.0, 0.0), (1.0, 0.0)])]);
        // The stroke comes in with the name a press would have left on
        // it, which is no name at all.
        let mark = stroke(crate::scene::ROOT_ID, Ink::Line, &[(2.0, 2.0), (3.0, 3.0)]);
        join_scene(&mut scene, mark).expect("the stroke joins the tree");
        let names: Vec<u64> = crate::scene::ink_order(&scene, crate::scene::Audience::Dm)
            .iter()
            .map(|stroke| stroke.id)
            .collect();
        assert_eq!(names.len(), 2);
        assert_ne!(names[0], names[1], "two strokes took one name");
        assert!(names.iter().all(|id| *id != crate::scene::ROOT_ID));
    }

    #[test]
    fn the_eraser_takes_a_whole_effect_and_leaves_no_line_behind() {
        let mut scene = drawn(vec![stroke(1, Ink::Beam, &[(0.0, 0.0), (10.0, 0.0)])]);
        assert!(bite(&mut scene, (5.0, 0.0), 0.2));
        // The beam is gone, and no piece of its outline stands in for it.
        assert_eq!(inside(&scene), vec![(0, "Drawings".to_owned())]);
    }

    #[test]
    fn the_eraser_takes_a_kept_measure_whole_as_well() {
        let mut scene = drawn(vec![stroke(1, Ink::Measure, &[(0.0, 0.0), (10.0, 0.0)])]);
        assert!(bite(&mut scene, (5.0, 0.0), 0.2));
        assert_eq!(inside(&scene), vec![(0, "Drawings".to_owned())]);
    }

    #[test]
    fn the_pieces_of_a_cut_line_join_a_group() {
        let mut scene = drawn(vec![stroke(1, Ink::Pen, &[(0.0, 0.0), (10.0, 0.0)])]);
        assert!(bite(&mut scene, (5.0, 0.0), 1.0));
        assert_eq!(
            inside(&scene),
            vec![
                (0, "Drawings".to_owned()),
                (1, "Pieces".to_owned()),
                (2, "Pen".to_owned()),
                (2, "Pen".to_owned()),
            ]
        );
    }

    #[test]
    fn a_second_cut_stays_in_the_group_the_first_one_made() {
        let mut scene = drawn(vec![stroke(1, Ink::Pen, &[(0.0, 0.0), (20.0, 0.0)])]);
        assert!(bite(&mut scene, (5.0, 0.0), 1.0));
        assert!(bite(&mut scene, (12.0, 0.0), 1.0));
        // Three pieces, and one group holding all of them.
        let held = inside(&scene);
        let groups = held.iter().filter(|(_, name)| name == "Pieces").count();
        let pieces = held.iter().filter(|(_, name)| name == "Pen").count();
        assert_eq!((groups, pieces), (1, 3), "{held:?}");
        assert!(held.iter().all(|(depth, _)| *depth <= 2), "{held:?}");
    }

    #[test]
    fn a_bite_that_cuts_nothing_leaves_the_stroke_where_it_stood() {
        let mut scene = drawn(vec![stroke(1, Ink::Pen, &[(0.0, 0.0), (10.0, 0.0)])]);
        // The disc reaches the line but takes no part of it away.
        assert!(!bite(&mut scene, (5.0, 4.0), 0.2));
        assert_eq!(
            inside(&scene),
            vec![(0, "Drawings".to_owned()), (1, "Pen".to_owned())]
        );
    }
}
