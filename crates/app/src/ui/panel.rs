//! The two floating panels: the objects list and the properties fields.
//!
//! A panel stands over the canvas at a corner and holds no state of its
//! own. Each properties field reads what the DM picked and gives back a
//! change for the history. DESIGN.md 7 and 8.

// Rust guideline compliant 2026-02-21

use crate::command::{Deed, SetAssets, SetStrokes, SetTvBox, reshape};
use crate::icon;
use crate::icons::Icon;
use crate::scene::{Asset, Node, NodeId, ROOT_ID, Scene};
use crate::stroke::{Ink, Rule, Stroke};
use crate::text;
use crate::theme::{self, Tokens};
use crate::transform::{MAX_GRID_PX, MIN_GRID_PX};
use crate::tvbox::{TV_WIDTH_INCHES, TvBox, clamp_width};
use crate::widget::{self, Height};

use super::draw::measure_overlay;
use super::tree::{Asked, Tree, act_on, row_name, scoped_rows};
use super::{
    Frame, MARGIN, MAX_INK_WIDTH, MAX_PERCENT, MAX_REACH, MAX_ZOOM_PERCENT, MIN_INK_WIDTH,
    MIN_PERCENT, MIN_REACH, MIN_ZOOM_PERCENT, Measure, NIB_SQUARE, Nib, PANEL_HEADER, PANEL_MAX,
    PANEL_PAD, PANEL_SLIDER, PANEL_WIDTH, PATH_GLYPH, PATH_HEIGHT, PATH_PARTS, ROW_HEIGHT, Select,
    Settings, TOOL_HEIGHT, Table, Tool, painter_dashes,
};

/// Where a floating panel stands, and how big it is. DESIGN.md 7.1.
#[derive(Debug, Clone, Copy)]
struct Place {
    /// The top-left corner of the panel, in points.
    left_top: egui::Pos2,
    /// The width of the panel, in points.
    width: f32,
    /// The height, or `None` to take the height the body asks for.
    height: Option<f32>,
}

/// The frame of a floating panel: the box, the header and the body.
///
/// `body` draws inside the padded body. DESIGN.md 7.1. A panel with a
/// `height` of `None` takes the height its body asks for, so no row of a
/// property panel falls off its bottom edge.
fn panel(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    place: Place,
    tokens: Tokens,
    body: impl FnOnce(&mut egui::Ui),
) -> egui::Rect {
    let Place {
        left_top,
        width,
        height,
    } = place;
    let mut rect = egui::Rect::from_min_size(left_top, egui::vec2(width, height.unwrap_or(0.0)));
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Middle)
        .fixed_pos(left_top)
        .show(ctx, |ui| {
            // The frame cannot go down before the body, because a panel
            // that sizes itself only knows its height afterwards. So two
            // shapes wait here and take their place at the end.
            let shadow = ui.painter().add(egui::Shape::Noop);
            let box_shape = ui.painter().add(egui::Shape::Noop);
            let header = egui::Rect::from_min_size(left_top, egui::vec2(width, PANEL_HEADER));
            // A file name is often longer than the panel. It ends in an
            // ellipsis, and the whole name comes up under the pointer.
            let room = width - 2.0 * PANEL_PAD;
            let galley = ui.ctx().fonts_mut(|fonts| {
                let mut job = egui::text::LayoutJob::simple_singleline(
                    title.to_owned(),
                    theme::font(theme::PANEL_TITLE, true),
                    tokens.ink,
                );
                job.wrap.max_width = room;
                job.wrap.max_rows = 1;
                job.wrap.break_anywhere = true;
                job.wrap.overflow_character = Some('\u{2026}');
                fonts.layout_job(job)
            });
            let cut = galley.size().x >= room;
            // The clip is the guarantee. `epaint` hangs the ellipsis past
            // the width it was given, and a title that reached over the
            // border would sit on the canvas.
            let inside = egui::Rect::from_min_max(
                egui::pos2(header.left() + PANEL_PAD, header.top()),
                egui::pos2(header.right() - PANEL_PAD, header.bottom()),
            );
            ui.painter().with_clip_rect(inside).galley(
                egui::pos2(inside.left(), header.center().y - galley.size().y / 2.0),
                galley,
                tokens.ink,
            );
            if cut {
                ui.interact(header, ui.id().with("title"), egui::Sense::hover())
                    .on_hover_text(title);
            }
            widget::rule_bottom(ui, header, tokens);
            let bottom = height.map_or(f32::INFINITY, |tall| left_top.y + tall - PANEL_PAD);
            let inner = egui::Rect::from_min_max(
                egui::pos2(left_top.x + PANEL_PAD, header.bottom() + PANEL_PAD),
                egui::pos2(left_top.x + width - PANEL_PAD, bottom),
            );
            let mut body_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            if height.is_some() {
                body_ui.set_clip_rect(inner);
            }
            body_ui.spacing_mut().item_spacing.y = PANEL_PAD;
            body(&mut body_ui);
            let tall =
                height.unwrap_or_else(|| body_ui.min_rect().bottom() + PANEL_PAD - left_top.y);
            rect = egui::Rect::from_min_size(left_top, egui::vec2(width, tall));
            if let (Some(color), Some(offset)) = (tokens.shadow, tokens.shadow_offset()) {
                ui.painter().set(
                    shadow,
                    egui::Shape::rect_filled(rect.translate(offset), 0, color),
                );
            }
            // The surface and the border go under everything the body
            // drew, because both wait at an index the body never reached.
            ui.painter().set(
                box_shape,
                egui::Shape::Vec(vec![
                    egui::Shape::rect_filled(rect, 0, tokens.surface),
                    egui::Shape::rect_stroke(
                        rect,
                        0,
                        egui::Stroke::new(1.0, tokens.ink),
                        egui::StrokeKind::Inside,
                    ),
                ]),
            );
            // The rect goes down last, because the panel only knows its
            // height once the body has drawn. It senses hover and nothing
            // more: a rect that took the click would win over every button
            // and every row inside it, since the later widget wins where
            // two overlap. The `Area` still stands over the canvas, so no
            // click on the panel reaches a map behind it.
            ui.allocate_rect(rect, egui::Sense::hover());
        });
    rect
}

/// The objects list of DESIGN.md 8.4, docked on the left.
///
/// Returns `true` when the DM changed the scene.
pub(super) fn objects_panel(
    ctx: &egui::Context,
    frame: &mut Frame<'_>,
    select: &mut Select,
    tree: &mut Tree,
    tokens: Tokens,
) -> bool {
    let scene = &mut *frame.scene;
    // A group the DM marked can go, by Ungroup or by a hand-edited file.
    // The root is always there to take a new asset.
    if !crate::scene::has_group(scene, tree.active) {
        tree.active = ROOT_ID;
    }
    // The group the list shows can go the same way, and then the list
    // falls back to the root.
    if !crate::scene::has_group(scene, tree.scope) {
        tree.scope = ROOT_ID;
    }
    // A node the DM picked on the canvas opens the groups above it, so the
    // list shows the row without a hunt. A node outside the group the list
    // shows takes the list back to the root, or the row would have nowhere
    // to appear.
    if tree.shown != select.only() {
        tree.shown = select.only();
        if let Some(id) = select.only() {
            let above = crate::scene::ancestors(scene, id);
            if tree.scope != ROOT_ID && !above.contains(&tree.scope) && id != tree.scope {
                tree.scope = ROOT_ID;
            }
            tree.open.extend(above);
        }
    }
    let screen = ctx.content_rect();
    let tall = screen.height() - 2.0 * MARGIN - TOOL_HEIGHT - MARGIN;
    let rect = egui::Rect::from_min_size(egui::pos2(MARGIN, MARGIN), egui::vec2(tree.width, tall));
    let mut edited = false;
    let mut asked = Asked::default();
    panel(
        ctx,
        "objects",
        text::panel_objects_title(),
        Place {
            left_top: rect.min,
            width: tree.width,
            height: Some(tall),
        },
        tokens,
        |ui| {
            // DESIGN.md 8.4: the path names the group the list shows, and each
            // part of it takes a click and goes back up.
            let path = crate::scene::path_to(scene, tree.scope);
            if let Some(up) = path_line(ui, &path, tokens) {
                tree.scope = up;
                tree.open.insert(up);
            }
            let footer = footer_height(ui, &select.note);
            let list = ui.available_height() - footer;
            egui::ScrollArea::vertical()
                .max_height(list.max(ROW_HEIGHT))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    scoped_rows(ui, scene, select, tree, &mut asked, tokens);
                });
            // DESIGN.md 7.1: a rule runs the whole width above the footer, so
            // it reaches past the padding of the body.
            widget::rule_bottom(
                ui,
                egui::Rect::from_min_size(
                    egui::pos2(rect.left(), ui.cursor().top() - PANEL_PAD / 2.0),
                    egui::vec2(rect.width(), 0.0),
                ),
                tokens,
            );
            // A language whose words run long takes a second row instead
            // of losing the end of one. `footer_height` reads the same
            // widths, so the list above leaves the room for it.
            ui.horizontal_wrapped(|ui| {
                // The DM can only take apart the one group they hold.
                let group = select.only().filter(|id| *id != ROOT_ID).filter(|id| {
                    crate::scene::find(scene, *id)
                        .and_then(Node::group)
                        .is_some()
                });
                if widget::button(
                    ui,
                    text::panel_objects_new_group(),
                    Some(Icon::Plus),
                    Height::Panel,
                )
                .clicked()
                {
                    asked.new_group = Some(scene.next_id());
                }
                let ungroup =
                    widget::button(ui, text::panel_objects_ungroup(), None, Height::Panel);
                if group.is_some() && ungroup.clicked() {
                    asked.ungroup = group;
                }
            });
            if select.note.is_empty() {
                widget::helper(ui, text::panel_objects_helper());
            } else {
                ui.label(
                    egui::RichText::new(&select.note)
                        .font(theme::font(theme::SMALL, false))
                        .color(tokens.accent),
                );
            }
        },
    );
    edited |= act_on(frame, select, tree, asked);
    edited |= drag_panel_edge(ctx, rect, &mut tree.width, tokens);
    edited
}

/// How much room the foot of the objects list needs, in points.
///
/// The two buttons take one row when they fit beside one another and two
/// when they do not. The line under them holds the note, or the helper
/// when there is no note, and wraps to the width of the panel.
fn footer_height(ui: &egui::Ui, note: &str) -> f32 {
    let width = ui.available_width();
    let buttons = widget::button_width(ui, text::panel_objects_new_group(), Some(Icon::Plus))
        + ui.spacing().item_spacing.x
        + widget::button_width(ui, text::panel_objects_ungroup(), None);
    let rows = if buttons <= width { 1.0 } else { 2.0 };
    let words = if note.is_empty() {
        text::panel_objects_helper()
    } else {
        note
    };
    // Each row of buttons carries the gap under it, and the words below
    // them take as many lines as they need.
    let step = Height::Panel.points() + ui.spacing().item_spacing.y;
    rows * step + widget::helper_height(ui, words, width) + 3.0 * PANEL_PAD
}

/// The path over the objects list. DESIGN.md 8.4.
///
/// Each part names a group on the way down to the one the list shows, and
/// a click on a part takes the list back up to it. The last part is the
/// group the list shows, so it takes no click. A path of more than four
/// parts drops its middle to an ellipsis.
///
/// Returns the group the DM asked to go back to.
fn path_line(ui: &mut egui::Ui, path: &[(NodeId, String)], tokens: Tokens) -> Option<NodeId> {
    // The root shows itself at the top of the list already, so a path of
    // one part would say the same thing twice.
    if path.len() < 2 {
        return None;
    }
    let font = theme::font(theme::SMALL, false);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), PATH_HEIGHT),
        egui::Sense::hover(),
    );
    let mut asked = None;
    let mut left = rect.left();
    for (index, part) in shortened(path).into_iter().enumerate() {
        if index > 0 {
            icon::paint(
                ui.painter(),
                Icon::ChevronRight,
                egui::pos2(left + PATH_GLYPH / 2.0, rect.center().y),
                PATH_GLYPH,
                tokens.mute,
            );
            left += PATH_GLYPH + 2.0;
        }
        let Some((id, name)) = part else {
            ui.painter().text(
                egui::pos2(left, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "\u{2026}",
                font.clone(),
                tokens.mute,
            );
            left += 12.0;
            continue;
        };
        let width = ui.ctx().fonts_mut(|fonts| {
            fonts
                .layout_no_wrap((*name).to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        });
        let part_rect =
            egui::Rect::from_min_size(egui::pos2(left, rect.top()), egui::vec2(width, PATH_HEIGHT));
        // The last part is where the list already stands, so it is a label.
        let last = index + 1 == shortened(path).len();
        let color = if last {
            tokens.ink
        } else {
            let response = ui.interact(part_rect, ui.id().with(("path", id)), egui::Sense::click());
            if response.clicked() {
                asked = Some(id);
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                ui.painter().hline(
                    part_rect.x_range(),
                    part_rect.bottom() - 5.0,
                    egui::Stroke::new(1.0, tokens.accent),
                );
                tokens.accent
            } else {
                tokens.mute
            }
        };
        ui.painter().text(
            egui::pos2(left, rect.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            font.clone(),
            color,
        );
        left += width;
    }
    asked
}

/// The parts a path shows. `None` stands for the middle it dropped.
///
/// DESIGN.md 8.4: a path of more than four parts keeps its first part and
/// its last two, so the DM still sees where the list stands and how to
/// reach the root.
fn shortened(path: &[(NodeId, String)]) -> Vec<Option<(NodeId, &str)>> {
    fn named(part: &(NodeId, String)) -> (NodeId, &str) {
        (part.0, part.1.as_str())
    }
    if path.len() <= PATH_PARTS {
        return path.iter().map(|part| Some(named(part))).collect();
    }
    let mut parts = vec![path.first().map(named), None];
    parts.extend(path[path.len() - 2..].iter().map(|part| Some(named(part))));
    parts
}

/// The grip on the right edge that widens the objects list. DESIGN.md 8.4.
///
/// Returns `false`: the width is a view setting and never a scene change.
fn drag_panel_edge(ctx: &egui::Context, rect: egui::Rect, width: &mut f32, tokens: Tokens) -> bool {
    /// How wide the grip is for the pointer, in points.
    const GRIP: f32 = 5.0;
    let edge = egui::Rect::from_min_max(
        egui::pos2(rect.right() - GRIP, rect.top()),
        egui::pos2(rect.right(), rect.bottom()),
    );
    egui::Area::new(egui::Id::new("objects-edge"))
        .order(egui::Order::Middle)
        .fixed_pos(edge.min)
        .show(ctx, |ui| {
            let response = ui.allocate_rect(edge, egui::Sense::drag());
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                ui.painter().vline(
                    edge.center().x,
                    edge.y_range().shrink(PANEL_PAD),
                    egui::Stroke::new(1.0, tokens.mute),
                );
            }
            if response.dragged() {
                *width = (*width + response.drag_delta().x).clamp(PANEL_WIDTH, PANEL_MAX);
            }
        });
    false
}

/// The properties of the view or of the selection. DESIGN.md 8.1 and 8.2.
///
/// The title names what the DM holds: the file of a map, or the name of a
/// group. A panel headed "Map" over a group said nothing about it.
pub(super) fn properties_panel(
    ctx: &egui::Context,
    frame: &mut Frame<'_>,
    views: &mut Views<'_>,
    tool: Tool,
    frame_box: &mut bool,
    tokens: Tokens,
) -> bool {
    let Views { select, table } = views;
    let screen = ctx.content_rect();
    let held = match tool {
        Tool::Table => Held::TvBox,
        Tool::Draw => Held::Draw,
        Tool::Select => match select
            .only()
            .map(|id| (id, crate::scene::find(frame.scene, id)))
        {
            Some((id, Some(Node::Asset(asset)))) => {
                Held::Map(id, asset.path.to_string_lossy().into_owned())
            }
            // A stroke opens a panel of its own, so the DM changes what
            // they drew after they drew it. Issue #12.
            Some((id, Some(Node::Stroke(stroke)))) => {
                Held::Stroke(id, stroke.ink.name().to_owned())
            }
            // A group has no panel. Its name and its two switches sit on
            // its row, and it carries no size and no turn of its own, so a
            // panel over it would hold nothing the list does not say.
            _ => return false,
        },
    };
    let left_top = egui::pos2(screen.right() - MARGIN - PANEL_WIDTH, MARGIN);
    let mut edited = false;
    let mut box_asked = None;
    panel(
        ctx,
        "properties",
        held.title(),
        Place {
            left_top,
            width: PANEL_WIDTH,
            height: None,
        },
        tokens,
        |ui| match &held {
            Held::TvBox => {
                // Another tool does not run the measure, so the panel must
                // not leave a measure armed behind it.
                select.measure = None;
                box_asked = box_properties(ui, frame.scene.tv_box, frame_box, tokens);
            }
            Held::Draw => draw_properties(ui, frame.settings, tokens),
            Held::Map(id, _) => edited = map_properties(ui, *id, select, frame, tokens),
            Held::Stroke(id, _) => edited = stroke_properties(ui, *id, select, frame),
        },
    );
    if let Some(after) = box_asked {
        let before = *table.opened.get_or_insert(frame.scene.tv_box);
        frame.history.hold(frame.scene, SetTvBox { before, after });
        edited = true;
    }
    edited
}

/// What the properties panel is about.
#[derive(Debug, Clone)]
enum Held {
    /// The box that decides what the TV shows. DESIGN.md 8.2.
    TvBox,
    /// The pen, the shapes, the eraser and the ruler. DESIGN.md 8.3.
    Draw,
    /// One stroke the DM drew, by name, with what it is called.
    Stroke(NodeId, String),
    /// One map, by id, with the name of its file. DESIGN.md 8.1.
    Map(NodeId, String),
}

impl Held {
    /// The title the panel takes.
    fn title(&self) -> &str {
        match self {
            Self::TvBox => text::panel_box_title(),
            Self::Draw => text::panel_draw_title(),
            Self::Stroke(_, name) | Self::Map(_, name) => name,
        }
    }
}

/// The properties of the TV box. DESIGN.md 8.2.
///
/// The DM drags a corner handle to reach a zoom by eye. This field reaches
/// an exact one, such as 50 percent for a map twice the size of the table.
///
/// `frame_box` comes back `true` when the DM asked to see the whole box.
fn box_properties(
    ui: &mut egui::Ui,
    tv_box: TvBox,
    frame_box: &mut bool,
    tokens: Tokens,
) -> Option<TvBox> {
    let _ = tokens;
    let mut percent = tv_box.zoom(TV_WIDTH_INCHES) * 100.0;
    let mut asked = None;
    widget::row_label(ui, text::panel_box_zoom());
    if widget::input(
        ui,
        &mut percent,
        text::unit_percent(),
        100.0,
        MIN_ZOOM_PERCENT..=MAX_ZOOM_PERCENT,
        0.1,
    )
    .changed()
    {
        let mut after = tv_box;
        after.width = clamp_width(TV_WIDTH_INCHES / (percent / 100.0));
        asked = Some(after);
    }
    widget::helper(ui, text::panel_box_zoom_helper());
    widget::row_label(ui, text::panel_box_move());
    widget::helper(ui, text::panel_box_move_helper());
    widget::row_label(ui, text::panel_box_frame());
    *frame_box = widget::button(ui, text::panel_box_frame_button(), None, Height::Panel).clicked();
    asked
}

/// The Group row of the map panel: which group holds this map.
///
/// Returns the group the DM picked, when they picked one.
fn group_field(ui: &mut egui::Ui, id: NodeId, scene: &Scene) -> Option<NodeId> {
    let names = crate::scene::group_names(scene);
    let holder = crate::scene::parent_of(scene, id).map(|(group, _)| group);
    let open = holder
        .and_then(|group| names.iter().find(|(other, ..)| *other == group))
        .map_or("", |(_, group_name, _)| group_name.as_str());
    let field = widget::select_field(ui, open, ui.available_width());
    let mut picked = None;
    egui::Popup::menu(&field)
        .gap(-1.0)
        .width(ui.available_width())
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (group, group_name, depth) in &names {
                let label = format!("{}{group_name}", "  ".repeat(*depth));
                if widget::select_row(ui, &label, holder == Some(*group)).clicked() {
                    picked = Some(*group);
                }
            }
        });
    picked
}

/// The state of the views the panel draws for.
///
/// The Draw view keeps its choices in the settings, so it brings nothing
/// of its own here.
#[derive(Debug)]
pub(super) struct Views<'a> {
    pub(super) select: &'a mut Select,
    pub(super) table: &'a mut Table,
}

/// Writes what every measure and every area of effect says about itself.
///
/// It draws in each view, because a shape the DM laid over the map keeps
/// its number whatever they do next. The TV says the same over its own
/// canvas, from the same code. Issue #12.
pub(super) fn say_lengths(
    ui: &egui::Ui,
    frame: &Frame<'_>,
    live: Option<&Stroke>,
    viewport: (u32, u32),
    tokens: Tokens,
) {
    let mut ink = crate::scene::ink_order(frame.scene, crate::scene::Audience::Dm);
    if let Some(live) = live {
        ink.push(live);
    }
    let painter = ui.ctx().layer_painter(egui::LayerId::background());
    let ppp = f64::from(ui.ctx().pixels_per_point());
    measure_overlay(
        &painter,
        &ink,
        frame.camera,
        viewport,
        ppp,
        frame.settings.cells(),
        tokens,
        theme::SMALL,
    );
}

/// The Draw panel: the six squares, the color and the width. DESIGN.md 8.3.
fn draw_properties(ui: &mut egui::Ui, settings: &mut Settings, tokens: Tokens) {
    nib_row(
        ui,
        &mut settings.ink_nib,
        &[
            Nib::Pen,
            Nib::Line,
            Nib::Rect,
            Nib::Ellipse,
            Nib::Eraser,
            Nib::Ruler,
        ],
        tokens,
    );
    // The areas of effect take a row of their own. They lie over the map
    // instead of marking it, and a row of nine squares outgrows the panel.
    widget::row_label(ui, text::panel_draw_effects());
    nib_row(
        ui,
        &mut settings.ink_nib,
        &[Nib::Burst, Nib::Cone, Nib::Beam],
        tokens,
    );
    widget::row_label(ui, text::panel_draw_color());
    widget::color_swatch(ui, &mut settings.ink_color);
    if settings.ink_nib == Nib::Ruler {
        widget::row_label(ui, text::ruler_rule());
        let field = widget::select_field(ui, settings.ink_rule.name(), ui.available_width());
        egui::Popup::menu(&field)
            .gap(-1.0)
            .width(ui.available_width())
            .show(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for rule in Rule::all() {
                    if widget::select_row(ui, rule.name(), rule == settings.ink_rule).clicked() {
                        settings.ink_rule = rule;
                    }
                }
            });
    }
    let mut snap = settings.ink_snap;
    if widget::checkbox(ui, &mut snap, text::panel_draw_snap()).clicked() {
        settings.ink_snap = snap;
    }
    widget::row_label(ui, text::panel_draw_width());
    let mut width = settings.ink_width;
    let shown = text::panel_draw_width_value(format_args!("{width:.2}"));
    if widget::slider(
        ui,
        &mut width,
        MIN_INK_WIDTH..=MAX_INK_WIDTH,
        PANEL_SLIDER,
        &shown,
    )
    .is_pointer_button_down_on()
    {
        settings.ink_width = width;
    }
}

/// The row of six squares that says what a drag draws. DESIGN.md 8.3.
fn nib_row(ui: &mut egui::Ui, nib: &mut Nib, nibs: &[Nib], tokens: Tokens) {
    // One border holds the whole row and a dashed line stands between
    // each pair, as the views do in the toolbar. DESIGN.md 8.3 and 5.2.
    let width = NIB_SQUARE * nibs.len() as f32;
    let (whole, _) = ui.allocate_exact_size(egui::vec2(width, NIB_SQUARE), egui::Sense::hover());
    let inside = whole.shrink(1.0);
    for (place, one) in nibs.iter().copied().enumerate() {
        let square = egui::Rect::from_min_size(
            egui::pos2(whole.left() + place as f32 * NIB_SQUARE, whole.top()),
            egui::Vec2::splat(NIB_SQUARE),
        )
        .intersect(inside);
        // The name of the nib, not its place, or the same square in two
        // rows would take the same id.
        let response = ui.interact(square, ui.id().with(("nib", one)), egui::Sense::click());
        let on = *nib == one;
        if on {
            ui.painter().rect_filled(square, 0, tokens.raised);
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    square.left_top(),
                    egui::vec2(square.width(), widget::BAR),
                ),
                0,
                tokens.accent,
            );
        } else if response.hovered() {
            ui.painter().rect_filled(square, 0, tokens.field);
        }
        let response = response.on_hover_text(one.name());
        icon::paint(
            ui.painter(),
            one.glyph(),
            square.center(),
            widget::SMALL_ICON,
            if on { tokens.accent } else { tokens.ink },
        );
        if response.clicked() {
            *nib = one;
        }
        // The dash tells one square from the next without cutting the row
        // into six controls. DESIGN.md 5.2.
        if place + 1 < nibs.len() {
            let edge = whole.left() + (place + 1) as f32 * NIB_SQUARE;
            let line = [
                egui::pos2(edge, inside.top()),
                egui::pos2(edge, inside.bottom()),
            ];
            painter_dashes(ui, &line, tokens.rule);
        }
    }
    ui.painter().rect(
        whole,
        0,
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.0, tokens.rule),
        egui::StrokeKind::Inside,
    );
}

/// The panel of one stroke: what it is drawn in, and how far it reaches.
///
/// The DM changes what they drew after they drew it, and every field
/// goes through the history as one step. Issue #12.
fn stroke_properties(
    ui: &mut egui::Ui,
    id: NodeId,
    select: &mut Select,
    frame: &mut Frame<'_>,
) -> bool {
    let Some(mark) = crate::scene::find(frame.scene, id)
        .and_then(Node::stroke)
        .cloned()
    else {
        return false;
    };
    let mut after = mark.clone();
    let mut changed = false;
    widget::row_label(ui, text::panel_draw_color());
    if widget::color_swatch(ui, &mut after.color) {
        changed = true;
    }
    widget::row_label(ui, text::panel_draw_width());
    let mut width = mark.width;
    let shown = text::panel_draw_width_value(format_args!("{width:.2}"));
    if widget::slider(
        ui,
        &mut width,
        MIN_INK_WIDTH..=MAX_INK_WIDTH,
        PANEL_SLIDER,
        &shown,
    )
    .is_pointer_button_down_on()
    {
        after.width = width;
        changed = true;
    }
    changed |= reach_row(ui, &mark, &mut after);
    if mark.ink == Ink::Measure {
        widget::row_label(ui, text::ruler_rule());
        let field = widget::select_field(ui, mark.rule.name(), ui.available_width());
        egui::Popup::menu(&field)
            .gap(-1.0)
            .width(ui.available_width())
            .show(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for rule in Rule::all() {
                    if widget::select_row(ui, rule.name(), rule == mark.rule).clicked() {
                        after.rule = rule;
                        changed = true;
                    }
                }
            });
    }
    if !changed {
        return false;
    }
    let before = select
        .opened_ink
        .clone()
        .filter(|held| held.id == id)
        .unwrap_or(mark);
    select.opened_ink = Some(before.clone());
    frame.history.hold(
        frame.scene,
        SetStrokes {
            before: vec![before],
            after: vec![after],
        },
    );
    true
}

/// The rows that say how far an area of effect reaches. DESIGN.md 8.3.
///
/// A reach moves the far point along the line the shape already runs on,
/// so the shape keeps its heading and takes a new size.
fn reach_row(ui: &mut egui::Ui, mark: &Stroke, after: &mut Stroke) -> bool {
    let Some(reach) = mark.ink.reach(&mark.points) else {
        return false;
    };
    let mut changed = false;
    widget::row_label(ui, text::panel_stroke_reach());
    let mut cells = reach;
    if widget::input(
        ui,
        &mut cells,
        text::unit_cells(),
        100.0,
        MIN_REACH..=MAX_REACH,
        0.5,
    )
    .changed()
    {
        after.points = mark.reached(cells);
        changed = true;
    }
    if mark.ink.spans() {
        widget::row_label(ui, text::panel_stroke_across());
        let mut span = if mark.span > 0.0 { mark.span } else { 1.0 };
        if widget::input(
            ui,
            &mut span,
            text::unit_cells(),
            100.0,
            MIN_REACH..=MAX_REACH,
            0.5,
        )
        .changed()
        {
            after.span = span;
            changed = true;
        }
    }
    changed
}

/// The properties of the selected map. DESIGN.md 8.1.
///
/// The grid size decides the true size of the map: one grid cell is one
/// inch on the canvas. Only a Foundry or a Universal VTT file carries that
/// number, so for a plain PNG or JPEG the DM types it or measures it.
fn map_properties(
    ui: &mut egui::Ui,
    id: NodeId,
    select: &mut Select,
    frame: &mut Frame<'_>,
    tokens: Tokens,
) -> bool {
    let mut edited = false;
    // DESIGN.md 8.1 gives the file name a row of its own. The title of the
    // panel carries it now, so the row would say it twice.
    widget::row_label(ui, text::panel_map_group());
    let move_to = group_field(ui, id, frame.scene);
    if let Some(group) = move_to
        && let Some(change) = reshape(
            frame.scene,
            Deed::AnotherGroup,
            crate::scene::name_of(frame.scene, id),
            |scene| {
                crate::scene::move_into(scene, id, group);
            },
        )
    {
        frame.history.kept(change);
        edited = true;
    }
    let Some(map) = crate::scene::find(frame.scene, id)
        .and_then(Node::asset)
        .cloned()
    else {
        return edited;
    };
    // The widgets write into this copy. One change at the end of the panel
    // carries whatever they wrote.
    let mut after = map.clone();
    let mut changed = false;
    changed |= map_numbers(ui, &map, &mut after);
    if widget::button(
        ui,
        text::panel_map_measure(),
        Some(Icon::Ruler),
        Height::Panel,
    )
    .clicked()
    {
        select.measure = Some(Measure::Start);
    }
    if select.measure.is_some() {
        widget::helper(ui, text::panel_map_measure_helper());
    }
    // DESIGN.md 8.1 puts the turn and the flip in the footer of the panel.
    ui.horizontal_wrapped(|ui| {
        if widget::button(
            ui,
            text::panel_map_turn_button(),
            Some(Icon::RotateCw),
            Height::Row,
        )
        .clicked()
        {
            after.rotation += std::f64::consts::FRAC_PI_2;
            changed = true;
        }
        if widget::button(
            ui,
            text::panel_map_flip(),
            Some(Icon::FlipHorizontal2),
            Height::Row,
        )
        .clicked()
        {
            after.flip_x = !map.flip_x;
            changed = true;
        }
    });
    widget::helper(ui, text::panel_map_flip_helper());
    let _ = tokens;
    if changed {
        // A drag of a number runs over many frames. Every one of them goes
        // back to the map as it stood when the drag began, so the whole
        // drag is one step. `DmUi::run` closes it when the DM lets go.
        let before = select
            .opened
            .clone()
            .filter(|held| held.id == id)
            .unwrap_or(map);
        select.opened = Some(before.clone());
        frame.history.hold(
            frame.scene,
            SetAssets {
                before: vec![before],
                after: vec![after],
            },
        );
        edited = true;
    }
    edited
}

/// The grid size, the size and the turn of a map. DESIGN.md 8.1.
///
/// The three write into `after`, which the panel hands to the history as
/// one change. Returns `true` when the DM moved one of them.
fn map_numbers(ui: &mut egui::Ui, map: &Asset, after: &mut Asset) -> bool {
    let mut changed = false;
    widget::row_label(ui, text::panel_map_grid_px());
    let mut grid_px = map.grid_px;
    if widget::input(
        ui,
        &mut grid_px,
        text::unit_px(),
        100.0,
        MIN_GRID_PX..=MAX_GRID_PX,
        1.0,
    )
    .changed()
    {
        after.grid_px = grid_px;
        changed = true;
    }
    // The size sits next to the grid size because the two multiply: a map
    // with the right grid size draws at true size only at 100 percent.
    let mut percent = map.scale * 100.0;
    widget::row_label(ui, text::panel_map_size());
    if widget::input(
        ui,
        &mut percent,
        text::unit_percent(),
        100.0,
        MIN_PERCENT..=MAX_PERCENT,
        0.5,
    )
    .changed()
    {
        after.scale = percent / 100.0;
        changed = true;
    }
    let mut degrees = map.rotation.to_degrees();
    widget::row_label(ui, text::panel_map_turn());
    if widget::input(
        ui,
        &mut degrees,
        text::unit_degrees(),
        100.0,
        -360.0..=360.0,
        0.5,
    )
    .changed()
    {
        after.rotation = degrees.to_radians();
        changed = true;
    }
    changed
}

/// The list of what a band drag picked, and the button that groups it.
///
/// It floats at the top of the canvas, centered. The canvas fills the
/// window now, so its top-left corner sits under the objects list, and a
/// popup there would hide behind the panel.
pub(super) fn selection_popup(
    ctx: &egui::Context,
    select: &mut Select,
    frame: &mut Frame<'_>,
    tokens: Tokens,
) -> bool {
    if !select.popup || select.chosen.is_empty() {
        return false;
    }
    let held = crate::scene::normalize(frame.scene, &select.chosen);
    let mut edited = false;
    let mut group_them = false;
    let screen = ctx.content_rect();
    let left_top = egui::pos2((screen.center().x - PANEL_WIDTH / 2.0).round(), MARGIN);
    panel(
        ctx,
        "selection",
        &text::panel_picked_title(held.len()),
        Place {
            left_top,
            width: PANEL_WIDTH,
            height: None,
        },
        tokens,
        |ui| {
            for id in &held {
                let name = match crate::scene::find(frame.scene, *id) {
                    Some(Node::Group(group)) => group.name.clone(),
                    Some(Node::Asset(asset)) => asset.path.to_string_lossy().into_owned(),
                    Some(Node::Stroke(stroke)) => stroke.ink.name().to_owned(),
                    None => continue,
                };
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), ROW_HEIGHT),
                    egui::Sense::hover(),
                );
                row_name(ui, rect, &name, false, tokens);
            }
            ui.horizontal(|ui| {
                if widget::button(
                    ui,
                    text::panel_picked_group(),
                    Some(Icon::Plus),
                    Height::Panel,
                )
                .clicked()
                {
                    group_them = true;
                }
                if widget::button(ui, text::panel_picked_close(), None, Height::Panel).clicked() {
                    select.popup = false;
                }
            });
        },
    );
    if group_them {
        let name = text::panel_objects_group_name(frame.scene.next_id());
        let chosen = select.chosen.clone();
        let mut made = None;
        let subject = name.clone();
        let change = reshape(frame.scene, Deed::Group, subject, |scene| {
            made = crate::scene::group_selection(scene, &chosen, name);
        });
        if let (Some(change), Some(id)) = (change, made) {
            frame.history.kept(change);
            select.chosen = vec![id];
            select.popup = false;
            edited = true;
        }
    }
    edited
}
