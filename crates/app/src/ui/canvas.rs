//! The canvas: the Select tool, the Table tool and what a press does.
//!
//! The canvas fills the window under the panels. It takes the press, the
//! drag and the keys, and it paints the maps, the grid and the TV box.
//! DESIGN.md 6.

// Rust guideline compliant 2026-02-21

use crate::camera::{Area, Camera, DEFAULT_PIXELS_PER_INCH, fit};
use crate::command::{Deed, Grow, SetAssets, SetTvBox, Turn, reshape};
use crate::scene::{Asset, Node, NodeId, Placed, Scene};
use crate::text;
use crate::theme::{self, Tokens};
use crate::transform::{
    corner_offset, edge_midpoint, grid_px_from_measure, hit_test, pick_handle, rotation_from_drag,
    rotation_handle, scale_from_drag, snap_corner, step_scale,
};
use crate::tvbox::{TV_WIDTH_INCHES, TvBox, at_true_size, clamp_width, snap_to_true_size};

use super::panel::selection_popup;
use super::{
    BoxDrag, Button, CELL, DASH, Drag, FRAME_MARGIN, Frame, GROUP_MARGIN, GROUP_REACH,
    HANDLE_REACH, HANDLE_SIZE, HANDLE_STANDOFF, KEY_ZOOM_STEP, Measure, PICK_REACH, Pointer, REDO,
    REDO_Y, ROTATION_HANDLE_OFFSET, Select, Table, UNDO, View, ZOOM_IN, ZOOM_IN_EQUALS,
    ZOOM_LABEL_GAP, ZOOM_LABEL_SIZE, ZOOM_OUT, ZOOM_RESET,
};

/// The Select tool on the canvas: pick, move, scale, turn and flip maps.
///
/// Returns `true` when a map changed.
pub(super) fn select_tool(
    ui: &mut egui::Ui,
    select: &mut Select,
    frame: &mut Frame<'_>,
    viewport: (u32, u32),
    zoom_goes_to: &mut Option<bool>,
    tokens: Tokens,
) -> bool {
    let (rect, view, pointer) = canvas_area(ui, frame.camera, viewport, false, zoom_goes_to);
    let snap = !ui.input(|i| i.modifiers.ctrl);

    // The measure tool takes the canvas for two clicks. Escape gives up.
    if select.measure.is_some() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            select.measure = None;
            return false;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        let mut edited = false;
        if let (true, true, Some(pos)) = (pointer.pressed(), pointer.hovered, pointer.pos) {
            edited = measure_click(select, frame, view.to_world(pos));
        }
        // A line from the first corner to the cursor, so the DM sees the
        // cell that is being measured.
        if let (Some(Measure::From(first)), Some(pos)) = (select.measure, pointer.pos) {
            ui.painter_at(rect).line_segment(
                [view.to_screen(first), pos],
                egui::Stroke::new(2.0, tokens.accent),
            );
        }
        return edited;
    }

    // Handles of the selected map, in points: four corners, then rotation.
    let handles: Vec<egui::Pos2> = held_corners(select, frame)
        .map(|corners| {
            let mut handles: Vec<egui::Pos2> = corners.iter().map(|&c| view.to_screen(c)).collect();
            let top_left = (f64::from(handles[0].x), f64::from(handles[0].y));
            let top_right = (f64::from(handles[1].x), f64::from(handles[1].y));
            let (rx, ry) = rotation_handle(top_left, top_right, ROTATION_HANDLE_OFFSET);
            handles.push(egui::pos2(rx as f32, ry as f32));
            handles
        })
        .unwrap_or_default();

    // A press selects: a handle of the selected map, else the topmost map
    // under the cursor. The same press starts a drag that moves, scales or
    // turns until the button goes up.
    if let (true, true, Some(pos)) = (pointer.pressed(), pointer.hovered, pointer.pos) {
        // Ctrl adds to the selection, as it does in a file manager.
        let add = ui.input(|i| i.modifiers.ctrl);
        press(select, frame, &handles, pos, view.to_world(pos), &view, add);
    }
    let mut edited = false;
    if let (true, Some(pos)) = (pointer.down(), pointer.pos) {
        edited |= apply_drag(select, frame, view.to_world(pos), snap);
    } else if let Some(dropped) = select.drag.take() {
        // The drag is over, so what it wrote becomes one step.
        frame.history.settle();
        // The band is over: what it covered is what the DM now holds.
        if let (Drag::Band { start_cursor }, Some(pos)) = (dropped, pointer.pos) {
            for id in band_covers(frame, start_cursor, view.to_world(pos)) {
                if !select.holds(id) {
                    select.chosen.push(id);
                }
            }
            select.popup = !select.chosen.is_empty();
        }
    }
    let icon = select.drag.as_ref().and_then(|drag| match drag {
        Drag::Scale { .. } => Some(egui::CursorIcon::ResizeNwSe),
        Drag::Rotate { .. } => Some(egui::CursorIcon::Grabbing),
        Drag::Move { .. } | Drag::Band { .. } => None,
    });
    set_cursor(ui, select.drag.is_some(), icon, pointer, &handles);

    edited |= keys(ui, select, frame);
    edited |= selection_popup(ui.ctx(), select, frame, tokens);

    if let (Some(Drag::Band { start_cursor }), Some(pos)) = (select.drag.as_ref(), pointer.pos) {
        let band = egui::Rect::from_two_pos(view.to_screen(*start_cursor), pos);
        let on_canvas = ui.painter_at(rect);
        on_canvas.rect_filled(band, 0.0, tokens.dim);
        on_canvas.rect_stroke(
            band,
            0.0,
            egui::Stroke::new(1.0, tokens.accent),
            egui::StrokeKind::Inside,
        );
    }
    draw_group_boxes(&ui.painter_at(rect), frame, &select.chosen, &view, tokens);
    if handles.len() == 5 {
        draw_selection(&ui.painter_at(rect), &handles, tokens);
    }
    edited
}

/// The dashed box around every group the DM can see.
///
/// The root has no box: it holds the whole scene, so a box around it says
/// nothing. The group the DM picked draws its box solid.
fn draw_group_boxes(
    painter: &egui::Painter,
    frame: &Frame<'_>,
    picked: &[NodeId],
    view: &View,
    tokens: Tokens,
) {
    for group in crate::scene::groups(frame.scene) {
        if !group.shown.dm {
            continue;
        }
        let Some((min, max)) = crate::scene::bounds(group, frame.size_of) else {
            continue;
        };
        let box_rect =
            egui::Rect::from_two_pos(view.to_screen(min), view.to_screen(max)).expand(GROUP_MARGIN);
        let stroke = egui::Stroke::new(1.0, tokens.accent);
        let corners = [
            box_rect.left_top(),
            box_rect.right_top(),
            box_rect.right_bottom(),
            box_rect.left_bottom(),
            box_rect.left_top(),
        ];
        if picked.contains(&group.id) {
            painter.add(egui::Shape::line(corners.to_vec(), stroke));
        } else {
            painter.add(egui::Shape::dashed_line(&corners, stroke, DASH, DASH));
        }
    }
}

fn band_covers(frame: &Frame<'_>, start: (f64, f64), end: (f64, f64)) -> Vec<NodeId> {
    let (low, high) = (
        (start.0.min(end.0), start.1.min(end.1)),
        (start.0.max(end.0), start.1.max(end.1)),
    );
    crate::scene::draw_order(frame.scene, crate::scene::Audience::Dm)
        .iter()
        .filter(|asset| {
            let middle = asset.center;
            middle.0 >= low.0 && middle.0 <= high.0 && middle.1 >= low.1 && middle.1 <= high.1
        })
        .map(|asset| asset.id)
        .collect()
}

/// The group whose box holds `pos`, if the DM aimed at one.
///
/// The smallest box wins, so a group inside another takes the click.
fn group_at(frame: &Frame<'_>, view: &View, pos: egui::Pos2) -> Option<NodeId> {
    crate::scene::groups(frame.scene)
        .into_iter()
        .filter(|group| group.shown.dm)
        .filter_map(|group| {
            let (min, max) = crate::scene::bounds(group, frame.size_of)?;
            let rect = egui::Rect::from_two_pos(view.to_screen(min), view.to_screen(max))
                .expand(GROUP_MARGIN);
            // The box itself takes the click, not the middle of it, so a
            // click inside still reaches the asset under the pointer.
            let inside = rect.shrink(GROUP_REACH);
            (rect.contains(pos) && !inside.contains(pos)).then_some((group.id, rect.area()))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// The Table tool: the box that decides what the TV shows.
///
/// A drag inside the box moves it. A drag on a corner handle changes its
/// size, and the box keeps the shape of the TV. The arrow keys move the box
/// one grid cell. Returns `true` when the box changed.
pub(super) fn table_tool(
    ui: &mut egui::Ui,
    table: &mut Table,
    frame: &mut Frame<'_>,
    viewport: (u32, u32),
    zoom_goes_to: &mut Option<bool>,
    tokens: Tokens,
) -> bool {
    let (rect, view, pointer) = canvas_area(ui, frame.camera, viewport, true, zoom_goes_to);
    let corners = frame.scene.tv_box.corners(frame.tv_viewport);
    let handles: Vec<egui::Pos2> = corners.iter().map(|&c| view.to_screen(c)).collect();
    let handle_points: Vec<(f64, f64)> = handles
        .iter()
        .map(|h| (f64::from(h.x), f64::from(h.y)))
        .collect();

    if let (true, true, Some(pos)) = (pointer.pressed(), pointer.hovered, pointer.pos) {
        let point = (f64::from(pos.x), f64::from(pos.y));
        let cursor = view.to_world(pos);
        table.drag = if pick_handle(point, &handle_points, HANDLE_REACH).is_some() {
            Some(BoxDrag::Resize {
                start_width: frame.scene.tv_box.width,
                start_cursor: cursor,
            })
        } else if hit_test(cursor, &corners) {
            Some(BoxDrag::Move {
                start_center: frame.scene.tv_box.center,
                start_cursor: cursor,
            })
        } else {
            None
        };
    }

    let mut edited = false;
    let alt = ui.input(|i| i.modifiers.alt);
    // The box this frame works on. What the drag, the keys and the wheel
    // write goes into one change at the end, so the DM can take it back.
    let mut box_now = frame.scene.tv_box;
    match (pointer.down(), pointer.pos, table.drag) {
        (true, Some(pos), Some(drag)) => {
            edited = drag_box(&mut box_now, drag, view.to_world(pos));
        }
        (true, _, _) => {}
        // The drag is over. A size that came close to true size takes it
        // exactly, so that a miniature covers the cell it stands in. Alt
        // keeps the size the DM dragged. PLAN.md section 3.2.
        _ => {
            if let (Some(BoxDrag::Resize { start_width, .. }), false) = (table.drag.take(), alt) {
                // A click on a handle that never moved is not a resize. It
                // must not pull a size the DM chose with Alt back to true
                // size.
                let resized = (box_now.width - start_width).abs() > f64::EPSILON;
                let snapped =
                    snap_to_true_size(box_now.width, TV_WIDTH_INCHES, frame.settings.snap_percent);
                if resized && (snapped - box_now.width).abs() > f64::EPSILON {
                    box_now.width = snapped;
                    edited = true;
                }
            }
        }
    }
    // The keys wait for the drag to end. A drag rewrites the box from its
    // start state every frame, so a key press in the middle of one is lost.
    if table.drag.is_none() {
        edited |= arrow_keys(ui, &mut box_now);
    }

    // Ctrl and Alt with the wheel reach a zoom without a drag on a handle.
    // Figma has no gesture that resizes an object with the wheel, so Alt
    // marks this one as ours. The canvas already gave the gesture up.
    if let (Some(pinch), true) = (pointer.box_zoom, pointer.hovered) {
        let zoom = box_now.zoom(TV_WIDTH_INCHES) * f64::from(pinch);
        box_now.width = clamp_width(TV_WIDTH_INCHES / zoom);
        edited = true;
    }
    if box_now != frame.scene.tv_box {
        // The first frame of the gesture keeps the box the undo goes back
        // to. `DmUi::run` closes the step when the DM lets go.
        let before = *table.opened.get_or_insert(frame.scene.tv_box);
        frame.history.hold(
            frame.scene,
            SetTvBox {
                before,
                after: box_now,
            },
        );
    }

    let icon = table.drag.map(|_| egui::CursorIcon::ResizeNwSe);
    set_cursor(ui, table.drag.is_some(), icon, pointer, &handles);
    let zoom = frame.scene.tv_box.zoom(TV_WIDTH_INCHES);
    draw_tv_box(&ui.painter_at(rect), rect, &handles, zoom, tokens);
    edited
}

/// Applies the drag in progress to the box. Returns `true` when it changed.
///
/// A press that does not move picks the box up and nothing more. It must
/// not count as an edit, or every click would redraw the TV and save.
fn drag_box(tv_box: &mut TvBox, drag: BoxDrag, cursor: (f64, f64)) -> bool {
    match drag {
        BoxDrag::Move {
            start_center,
            start_cursor,
        } => {
            if cursor == start_cursor {
                return false;
            }
            tv_box.center = (
                start_center.0 + cursor.0 - start_cursor.0,
                start_center.1 + cursor.1 - start_cursor.1,
            );
        }
        BoxDrag::Resize {
            start_width,
            start_cursor,
        } => {
            if cursor == start_cursor {
                return false;
            }
            let factor = scale_from_drag(tv_box.center, start_cursor, cursor);
            tv_box.width = clamp_width(start_width * factor);
        }
    }
    true
}

/// The arrow keys move the box one grid cell. Held keys repeat, so the DM
/// can walk the box across the canvas.
fn arrow_keys(ui: &egui::Ui, tv_box: &mut TvBox) -> bool {
    // A number in the panel takes the keyboard first. egui leaves the left
    // and right keys to us, so the box would walk while the DM types.
    if ui.ctx().egui_wants_keyboard_input() {
        return false;
    }
    let mut moved = false;
    ui.input(|input| {
        for event in &input.events {
            let egui::Event::Key {
                key, pressed: true, ..
            } = event
            else {
                continue;
            };
            let (dx, dy) = match key {
                egui::Key::ArrowLeft => (-CELL, 0.0),
                egui::Key::ArrowRight => (CELL, 0.0),
                egui::Key::ArrowUp => (0.0, -CELL),
                egui::Key::ArrowDown => (0.0, CELL),
                _ => continue,
            };
            tv_box.center = (tv_box.center.0 + dx, tv_box.center.1 + dy);
            moved = true;
        }
    });
    moved
}

/// The wash outside the box, the outline of the box and its handles.
fn draw_tv_box(
    painter: &egui::Painter,
    canvas: egui::Rect,
    handles: &[egui::Pos2],
    zoom: f64,
    tokens: Tokens,
) {
    let inside = egui::Rect::from_two_pos(handles[0], handles[2]);
    // Four rectangles around the box, so the box itself stays clear.
    let (top, bottom) = (inside.top(), inside.bottom());
    for wash in [
        egui::Rect::from_min_max(canvas.left_top(), egui::pos2(canvas.right(), top)),
        egui::Rect::from_min_max(egui::pos2(canvas.left(), bottom), canvas.right_bottom()),
        egui::Rect::from_min_max(
            egui::pos2(canvas.left(), top),
            egui::pos2(inside.left(), bottom),
        ),
        egui::Rect::from_min_max(
            egui::pos2(inside.right(), top),
            egui::pos2(canvas.right(), bottom),
        ),
    ] {
        painter.rect_filled(wash.intersect(canvas), 0.0, tokens.dim);
    }
    let stroke = egui::Stroke::new(2.0, tokens.accent);
    painter.add(egui::Shape::closed_line(handles.to_vec(), stroke));
    // DESIGN.md 5.4: a handle stands 4 points outside the box, so the
    // outline stays whole under it.
    let middle = handles
        .iter()
        .fold(egui::Vec2::ZERO, |sum, at| sum + at.to_vec2())
        / handles.len() as f32;
    for handle in handles {
        let away = (handle.to_vec2() - middle).normalized() * HANDLE_STANDOFF;
        painter.rect_filled(
            egui::Rect::from_center_size(*handle + away, egui::vec2(HANDLE_SIZE, HANDLE_SIZE)),
            0.0,
            tokens.accent,
        );
    }
    // DESIGN.md 5.4: the zoom stands above the top-right corner, and takes
    // the accent while the box is at true size. A box wider than the canvas
    // keeps its label on screen, since the corner it belongs to is not.
    let corner = egui::pos2(handles[1].x, handles[1].y - ZOOM_LABEL_GAP);
    painter.text(
        canvas.shrink(ZOOM_LABEL_GAP).clamp(corner),
        egui::Align2::RIGHT_BOTTOM,
        format!("{} %", (zoom * 100.0).round()),
        theme::font(ZOOM_LABEL_SIZE, true),
        if at_true_size(zoom) {
            tokens.accent
        } else {
            tokens.ink
        },
    );
}

/// The world corners of what the DM holds, when they hold one thing.
///
/// An asset gives its own four corners, turned as it draws. A group gives
/// the corners of the box around everything in it.
fn held_corners(select: &Select, frame: &Frame<'_>) -> Option<[(f64, f64); 4]> {
    let id = select.only()?;
    match crate::scene::find(frame.scene, id)? {
        Node::Asset(asset) => {
            let size = (frame.size_of)(&asset.path)?;
            Some(asset.corners(size))
        }
        Node::Group(group) => {
            let (min, max) = crate::scene::bounds(group, frame.size_of)?;
            Some([min, (max.0, min.1), max, (min.0, max.1)])
        }
        // A stroke turns and grows with no handles of its own yet, so the
        // box says where it stands and nothing more.
        Node::Stroke(stroke) => {
            let (min, max) = stroke.bounds()?;
            Some([min, (max.0, min.1), max, (min.0, max.1)])
        }
    }
}

/// The point a turn or a growth happens around: the middle of what the DM
/// holds.
fn pivot_of(corners: &[(f64, f64); 4]) -> (f64, f64) {
    (
        f64::midpoint(corners[0].0, corners[2].0),
        f64::midpoint(corners[0].1, corners[2].1),
    )
}

/// Starts the drag for a press at `pos` (points) and `cursor` (world).
fn press(
    select: &mut Select,
    frame: &mut Frame<'_>,
    handles: &[egui::Pos2],
    pos: egui::Pos2,
    cursor: (f64, f64),
    view: &View,
    add: bool,
) {
    let handle_points: Vec<(f64, f64)> = handles
        .iter()
        .map(|h| (f64::from(h.x), f64::from(h.y)))
        .collect();
    let hit_handle = pick_handle(
        (f64::from(pos.x), f64::from(pos.y)),
        &handle_points,
        HANDLE_REACH,
    );
    // A press starts a gesture, so whatever the DM had under their hand
    // becomes a step of its own.
    frame.history.settle();
    if let (Some(handle), Some(corners)) = (hit_handle, held_corners(select, frame)) {
        let starts = crate::scene::placed(frame.scene, &select.chosen);
        let pivot = pivot_of(&corners);
        select.drag = Some(if handle == 4 {
            Drag::Rotate {
                starts,
                pivot,
                start_cursor: cursor,
            }
        } else {
            Drag::Scale {
                starts,
                pivot,
                start_cursor: cursor,
            }
        });
        return;
    }
    // The box of a group takes the press first, then a stroke, because a
    // stroke draws over every map. Inside the box, the press goes on to
    // the asset under the pointer.
    let aimed = group_at(frame, view, pos)
        .or_else(|| ink_at(frame, cursor))
        .or_else(|| {
            // The topmost asset under the pointer takes the press. The tree
            // draws the bottom one first, so the search runs the other way.
            crate::scene::assets(frame.scene)
                .iter()
                .rev()
                .find(|asset| {
                    (frame.size_of)(&asset.path)
                        .is_some_and(|size| hit_test(cursor, &asset.corners(size)))
                })
                .map(|asset| asset.id)
        });
    let Some(id) = aimed else {
        // A press on bare canvas starts a band that picks what it covers.
        if !add {
            select.chosen.clear();
        }
        select.popup = false;
        select.drag = Some(Drag::Band {
            start_cursor: cursor,
        });
        return;
    };
    if !select.holds(id) || add {
        select.take(id, add);
    }
    select.drag = Some(Drag::Move {
        was: standing(frame.scene, &select.chosen),
        start_cursor: cursor,
    });
}

/// The stroke under the pointer, if the pointer is on one.
///
/// The strokes draw over the maps, and the topmost one takes the press,
/// so the DM picks what they see. Issue #12.
fn ink_at(frame: &Frame<'_>, cursor: (f64, f64)) -> Option<NodeId> {
    crate::scene::ink_order(frame.scene, crate::scene::Audience::Dm)
        .iter()
        .rev()
        .find(|stroke| stroke.touches(cursor, PICK_REACH))
        .map(|stroke| stroke.id)
}

/// Every asset the DM holds, as it stands now.
///
/// A group in the selection hands over every asset under it, so a drag on
/// a group moves all of it and nothing loses its place inside. The whole
/// asset comes along, because the change that ends the drag puts every
/// field of it back.
fn standing(scene: &Scene, chosen: &[NodeId]) -> Vec<Asset> {
    crate::scene::normalize(scene, chosen)
        .iter()
        .flat_map(|id| crate::scene::assets_of(scene, *id))
        .filter_map(|id| crate::scene::find(scene, id).and_then(Node::asset).cloned())
        .collect()
}

/// What the keyboard does to the selected map. Returns `true` when it
/// changed one.
fn keys(ui: &egui::Ui, select: &mut Select, frame: &mut Frame<'_>) -> bool {
    // A number in the panel takes the keyboard first, or `R` and `F` would
    // turn and flip the map while the DM types.
    if ui.ctx().egui_wants_keyboard_input() {
        return false;
    }
    let mut edited = false;
    // Keys act on the selection when no drag is in progress, since a drag
    // rewrites the map from its start state every frame. Held keys do not
    // repeat: one press is one turn, one flip, or one step in the stack.
    if select.drag.is_some() || select.chosen.is_empty() {
        return edited;
    }
    let held = crate::scene::normalize(frame.scene, &select.chosen);
    let one = select.only();
    ui.input(|input| {
        for event in &input.events {
            let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            // Order lives inside one group, so these two keys move nodes
            // past their brothers and sisters and never leave the parent.
            if matches!(key, egui::Key::PageUp | egui::Key::PageDown) {
                if crate::scene::share_parent(frame.scene, &held).is_none() {
                    text::panel_objects_one_group().clone_into(&mut select.note);
                    continue;
                }
                select.note.clear();
                let toward_top = *key == egui::Key::PageUp;
                let subject = held
                    .first()
                    .map(|id| crate::scene::name_of(frame.scene, *id))
                    .unwrap_or_default();
                if let Some(change) = reshape(frame.scene, Deed::Order, subject, |scene| {
                    crate::scene::reorder_all(scene, &held, toward_top);
                }) {
                    frame.history.kept(change);
                    edited = true;
                }
                continue;
            }
            let Some(before) = one
                .and_then(|id| crate::scene::find(frame.scene, id))
                .and_then(Node::asset)
                .cloned()
            else {
                continue;
            };
            let mut after = before.clone();
            match key {
                egui::Key::R => after.rotation += std::f64::consts::FRAC_PI_2,
                egui::Key::F if modifiers.shift => after.flip_y = !before.flip_y,
                egui::Key::F => after.flip_x = !before.flip_x,
                // Plus is the numpad key; Equals is the shared "=/+" main
                // row key, which egui reports without needing Shift.
                egui::Key::Plus | egui::Key::Equals => {
                    after.scale = step_scale(before.scale, true);
                }
                egui::Key::Minus => after.scale = step_scale(before.scale, false),
                _ => continue,
            }
            frame.history.run(
                frame.scene,
                SetAssets {
                    before: vec![before],
                    after: vec![after],
                },
            );
            edited = true;
        }
    });
    edited
}

/// `Ctrl+Z` takes the last change back. `Ctrl+Shift+Z` writes it again.
///
/// A field in a panel takes the keyboard first, because egui keeps an undo
/// of its own for the text in it.
///
/// A drag waits, `dragging`, and so does a change the DM still holds. A
/// drag rewrites the scene from the state it started in, every frame, so
/// it would write over whatever the undo put back. The press that starts
/// one writes nothing until the pointer moves, so the history alone does
/// not say that a hand is on the canvas.
///
/// Returns `true` when the scene changed.
pub(super) fn undo_keys(ui: &egui::Ui, frame: &mut Frame<'_>, dragging: bool) -> bool {
    if dragging || ui.ctx().egui_wants_keyboard_input() || frame.history.holding() {
        return false;
    }
    // Redo goes first. Ctrl and Shift with Z would answer to the undo
    // shortcut as well, and the one that reads the event first takes it.
    let redo =
        ui.input_mut(|input| input.consume_shortcut(&REDO) || input.consume_shortcut(&REDO_Y));
    if redo {
        return frame.history.redo(frame.scene);
    }
    if ui.input_mut(|input| input.consume_shortcut(&UNDO)) {
        return frame.history.undo(frame.scene);
    }
    false
}

/// One click of the measure tool. Returns `true` when it set the grid size.
///
/// The first click stores a cell corner. The second reads the grid size out
/// of the distance between the two, and puts the map back to true size.
fn measure_click(select: &mut Select, frame: &mut Frame<'_>, cursor: (f64, f64)) -> bool {
    let Some(Measure::From(first)) = select.measure else {
        select.measure = Some(Measure::From(cursor));
        return false;
    };
    select.measure = None;
    let Some(before) = select
        .only()
        .and_then(|id| crate::scene::find(frame.scene, id))
        .and_then(Node::asset)
        .cloned()
    else {
        return false;
    };
    let Some(grid_px) = grid_px_from_measure(first, cursor, before.grid_px, before.scale) else {
        return false;
    };
    let mut after = before.clone();
    after.grid_px = grid_px;
    // A measured map draws straight from its grid size: one cell, one inch.
    after.scale = 1.0;
    frame.history.run(
        frame.scene,
        SetAssets {
            before: vec![before],
            after: vec![after],
        },
    );
    true
}

/// The canvas area, and how this frame reads the pointer over it.
pub(super) fn canvas_area(
    ui: &mut egui::Ui,
    camera: &mut Camera,
    viewport: (u32, u32),
    box_takes_zoom: bool,
    zoom_goes_to: &mut Option<bool>,
) -> (egui::Rect, View, Pointer) {
    let rect = ui.available_rect_before_wrap();
    let response = ui.interact(rect, ui.id().with("canvas"), egui::Sense::click_and_drag());
    let ppp = f64::from(ui.ctx().pixels_per_point());
    let (pressed, down, pos, space, middle, drag, scroll, zoom, shift, alt) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.interact_pos(),
            i.key_down(egui::Key::Space),
            i.pointer.middle_down(),
            i.pointer.delta(),
            i.smooth_scroll_delta,
            i.zoom_delta(),
            i.modifiers.shift,
            i.modifiers.alt,
        )
    });

    // The canvas takes its controls from Figma. Space with a drag and the
    // middle button pan. The wheel pans, and Shift with the wheel pans
    // sideways. Ctrl with the wheel zooms. Alt marks the one gesture Figma
    // has no answer for, the zoom of the TV box, so the camera leaves that
    // one alone. PLAN.md section 3.1.
    let panning = (space && down) || middle;
    // egui smooths one wheel notch into a stream of small factors over
    // many frames. Whoever the gesture started with keeps it to the end,
    // or a DM who lets Alt go too early would hand the rest of a box zoom
    // to the camera.
    if zoomed(zoom) {
        zoom_goes_to.get_or_insert(box_takes_zoom && alt);
    } else {
        *zoom_goes_to = None;
    }
    let box_zoom = (*zoom_goes_to == Some(true)).then_some(zoom);
    if panning {
        *camera = camera.panned((f64::from(drag.x) * ppp, f64::from(drag.y) * ppp));
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    if response.hovered() {
        // Some platforms turn Shift and the wheel into a sideways scroll
        // before egui sees it, and some leave that to the program.
        let wheel = if shift && scroll.x == 0.0 {
            egui::vec2(scroll.y, 0.0)
        } else {
            scroll
        };
        if wheel != egui::Vec2::ZERO {
            *camera = camera.panned((f64::from(wheel.x) * ppp, f64::from(wheel.y) * ppp));
        }
        if let (true, None, Some(at)) = (zoomed(zoom), box_zoom, pos) {
            let screen = (f64::from(at.x) * ppp, f64::from(at.y) * ppp);
            *camera = camera.zoomed_at(screen, f64::from(zoom), viewport);
        }
    }
    key_zoom(
        ui,
        camera,
        pos.map(|at| (f64::from(at.x) * ppp, f64::from(at.y) * ppp)),
        viewport,
    );

    let view = View::new(ui, *camera, viewport);
    // A tool must not move a map while the DM moves the camera, so the
    // button reads as up for as long as the pan runs.
    let button = match (panning, pressed, down) {
        (false, true, _) => Button::Pressed,
        (false, false, true) => Button::Held,
        _ => Button::Up,
    };
    let pointer = Pointer {
        button,
        pos,
        hovered: response.hovered(),
        panning,
        box_zoom,
    };
    (rect, view, pointer)
}

/// The zoom keys of a browser, on the DM camera.
///
/// Ctrl with plus or minus steps the zoom, and Ctrl with zero goes back to
/// the zoom a new project opens with. The point under the pointer stays
/// where it is, as it does for the wheel.
fn key_zoom(ui: &egui::Ui, camera: &mut Camera, pointer: Option<(f64, f64)>, viewport: (u32, u32)) {
    let (steps, reset) = ui.input_mut(|input| {
        let in_ = input.consume_shortcut(&ZOOM_IN) || input.consume_shortcut(&ZOOM_IN_EQUALS);
        let out = input.consume_shortcut(&ZOOM_OUT);
        (
            i32::from(in_) - i32::from(out),
            input.consume_shortcut(&ZOOM_RESET),
        )
    });
    if steps == 0 && !reset {
        return;
    }
    // Without a pointer, hold the middle of the window instead.
    let at = pointer.unwrap_or((f64::from(viewport.0) / 2.0, f64::from(viewport.1) / 2.0));
    let factor = if reset {
        DEFAULT_PIXELS_PER_INCH / camera.pixels_per_inch
    } else {
        KEY_ZOOM_STEP.powi(steps)
    };
    *camera = camera.zoomed_at(at, factor, viewport);
}

/// Whether egui reported a zoom gesture this frame.
///
/// egui reads Ctrl with the wheel as a zoom factor, and gives 1.0 when no
/// gesture happened. A pinch arrives the same way, but only from macOS and
/// iOS: winit 0.30 never sends `PinchGesture` from X11 or from Wayland.
fn zoomed(factor: f32) -> bool {
    (factor - 1.0).abs() > f32::EPSILON
}

/// `T` puts the whole TV box on the DM screen. PLAN.md section 5.4.
pub(super) fn frame_tv_box(
    ui: &egui::Ui,
    frame: &mut Frame<'_>,
    rect: egui::Rect,
    viewport: (u32, u32),
    asked: bool,
) {
    let asked = asked
        || (ui.input(|i| i.key_pressed(egui::Key::T)) && !ui.ctx().egui_wants_keyboard_input());
    if !asked {
        return;
    }
    let ppp = f64::from(ui.ctx().pixels_per_point());
    let area = Area {
        min: (f64::from(rect.min.x) * ppp, f64::from(rect.min.y) * ppp),
        size: (
            f64::from(rect.width()) * ppp,
            f64::from(rect.height()) * ppp,
        ),
    };
    let size = (
        frame.scene.tv_box.width,
        frame.scene.tv_box.height(frame.tv_viewport),
    );
    *frame.camera = fit(
        frame.scene.tv_box.center,
        size,
        area,
        viewport,
        FRAME_MARGIN,
    );
}

/// Shows the cursor of the drag while one runs, and the cursor of the
/// handle under the pointer while none does.
///
/// `Pointer::pos` is `interact_pos`, which egui gives even without a button
/// down, so hover feedback works before the DM commits to a drag.
fn set_cursor(
    ui: &egui::Ui,
    dragging: bool,
    drag_icon: Option<egui::CursorIcon>,
    pointer: Pointer,
    handles: &[egui::Pos2],
) {
    if pointer.panning {
        return;
    }
    let icon = if dragging {
        drag_icon
    } else {
        pointer
            .hovered
            .then(|| pointer.pos.and_then(|pos| hovered_handle(pos, handles)))
            .flatten()
    };
    if let Some(icon) = icon {
        ui.ctx().set_cursor_icon(icon);
    }
}

/// The cursor for the handle nearest `pos`, if any is within reach.
fn hovered_handle(pos: egui::Pos2, handles: &[egui::Pos2]) -> Option<egui::CursorIcon> {
    let points: Vec<(f64, f64)> = handles
        .iter()
        .map(|h| (f64::from(h.x), f64::from(h.y)))
        .collect();
    let hit = pick_handle((f64::from(pos.x), f64::from(pos.y)), &points, HANDLE_REACH)?;
    Some(if hit == 4 {
        egui::CursorIcon::Grab
    } else {
        egui::CursorIcon::ResizeNwSe
    })
}

/// Applies the drag in progress for the cursor at `cursor` (world).
///
/// Returns `true` when the map changed.
fn apply_drag(select: &mut Select, frame: &mut Frame<'_>, cursor: (f64, f64), snap: bool) -> bool {
    // The drag is read where it lies. A frame of a move drag would
    // otherwise clone every asset the DM holds twice: once to read the
    // spots it started from, and once for the change it builds.
    let Some(drag) = select.drag.as_ref() else {
        return false;
    };
    match drag {
        // The band picks nothing until the DM lets go of it.
        Drag::Band { .. } => false,
        Drag::Move { was, start_cursor } => {
            // A press without motion picks and nothing more: a snap would
            // shift a map that the DM placed off the grid.
            if cursor == *start_cursor || was.is_empty() {
                return false;
            }
            let step = (cursor.0 - start_cursor.0, cursor.1 - start_cursor.1);
            // One asset snaps to its own grid. A selection moves as one
            // piece, so the first asset snaps and the rest follow it.
            let lead = snap.then(|| snap_step(frame, &was[0], step)).flatten();
            let step = lead.unwrap_or(step);
            let after: Vec<Asset> = was
                .iter()
                .map(|start| {
                    let mut asset = start.clone();
                    asset.center = (start.center.0 + step.0, start.center.1 + step.1);
                    if !snap && let Some(size) = (frame.size_of)(&asset.path) {
                        let corners = asset.corners(size);
                        asset.snap_offset = corner_offset(frame.settings.cells(), &corners);
                    }
                    asset
                })
                .collect();
            let before = was.clone();
            frame.history.hold(frame.scene, SetAssets { before, after });
            true
        }
        Drag::Scale {
            starts,
            pivot,
            start_cursor,
        } => {
            let factor = scale_from_drag(*pivot, *start_cursor, cursor);
            frame.history.hold(
                frame.scene,
                Grow {
                    subject: held_names(frame.scene, starts),
                    starts: starts.clone(),
                    pivot: *pivot,
                    factor,
                },
            );
            true
        }
        Drag::Rotate {
            starts,
            pivot,
            start_cursor,
        } => {
            // The snap lands the turn of the first asset on a step, not
            // the sweep of the drag, so an asset that starts off a step
            // can get back on one. A group turns as one piece, so every
            // asset in it takes the same angle.
            let base = starts.first().map_or(0.0, |first| first.rotation);
            let turned = rotation_from_drag(base, *pivot, *start_cursor, cursor, snap);
            frame.history.hold(
                frame.scene,
                Turn {
                    subject: held_names(frame.scene, starts),
                    starts: starts.clone(),
                    pivot: *pivot,
                    angle: turned - base,
                },
            );
            true
        }
    }
}

/// What the history calls the assets of a drag: the file, or a count.
fn held_names(scene: &Scene, starts: &[Placed]) -> String {
    match starts {
        [] => String::new(),
        [one] => crate::scene::name_of(scene, one.id),
        many => text::history_maps_many(many.len()),
    }
}

/// The step a move takes once the leading asset snaps to its own grid.
fn snap_step(frame: &Frame<'_>, lead: &Asset, step: (f64, f64)) -> Option<(f64, f64)> {
    let size = (frame.size_of)(&lead.path)?;
    let start = lead.center;
    let moved = (start.0 + step.0, start.1 + step.1);
    let mut settled = lead.clone();
    settled.center = moved;
    let corners = settled.corners(size);
    let snapped = snap_corner(frame.settings.cells(), moved, &corners, lead.snap_offset);
    Some((snapped.0 - start.0, snapped.1 - start.1))
}

/// The outline, the corner handles and the rotation handle of the selection.
fn draw_selection(painter: &egui::Painter, handles: &[egui::Pos2], tokens: Tokens) {
    let stroke = egui::Stroke::new(2.0, tokens.accent);
    painter.add(egui::Shape::closed_line(handles[..4].to_vec(), stroke));
    // The same function that placed the rotation handle, so the line always
    // starts exactly where the handle's offset is measured from.
    let (mid_x, mid_y) = edge_midpoint(
        (f64::from(handles[0].x), f64::from(handles[0].y)),
        (f64::from(handles[1].x), f64::from(handles[1].y)),
    );
    let top_mid = egui::pos2(mid_x as f32, mid_y as f32);
    painter.line_segment([top_mid, handles[4]], stroke);
    for handle in &handles[..4] {
        painter.rect_filled(
            egui::Rect::from_center_size(*handle, egui::vec2(HANDLE_SIZE, HANDLE_SIZE)),
            0.0,
            tokens.accent,
        );
    }
    painter.circle_filled(handles[4], HANDLE_SIZE / 2.0, tokens.accent);
}
