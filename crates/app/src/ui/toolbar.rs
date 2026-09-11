//! The toolbar along the window edge, and the History button. DESIGN.md 5.2.
//!
//! The toolbar holds one entry for each view, then the buttons beside
//! them. It reads the press and gives it back; it changes nothing itself.

// Rust guideline compliant 2026-02-21

use crate::icon;
use crate::icons::Icon;
use crate::text;
use crate::theme::{self, Tokens};
use crate::widget;

use super::{
    MARGIN, TOOL_GAP, TOOL_GROUP_GAP, TOOL_HEIGHT, TOOL_ICON, TOOL_PAD, TOOL_WIDTH, Tool,
    painter_dashes,
};

/// What the toolbar asked for this frame. DESIGN.md 5.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Press {
    /// Show the canvas through another view.
    View(Tool),
    /// Open the scenes dialog.
    Scenes,
    /// Put a map on the canvas.
    AddMap,
    /// Open the settings dialog.
    Settings,
}

/// The toolbar of DESIGN.md 5.2: the views, a rule, then the rest.
///
/// It floats over the canvas, centered, `MARGIN` from the bottom edge. A
/// view that is not built yet has no entry, so the toolbar never offers
/// what the program cannot do.
pub(super) fn toolbar(ui: &egui::Ui, tool: Tool, tokens: Tokens) -> Option<Press> {
    let views = [
        (
            Press::View(Tool::Select),
            text::tool_select(),
            Icon::MousePointer,
        ),
        (Press::View(Tool::Draw), text::tool_draw(), Icon::Pencil),
        (Press::View(Tool::Table), text::tool_table(), Icon::Monitor),
    ];
    let actions = [
        (Press::Scenes, text::tool_scenes(), Icon::Layers),
        (Press::AddMap, text::tool_add_map(), Icon::Plus),
        (
            Press::Settings,
            text::tool_settings(),
            Icon::SlidersHorizontal,
        ),
    ];
    let font = theme::font(theme::SMALL, false);
    let width_of = |label: &str| {
        let text = ui.ctx().fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(label.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        });
        (text + 2.0 * TOOL_PAD).max(TOOL_WIDTH).ceil()
    };
    let view_widths: Vec<f32> = views.iter().map(|(_, label, _)| width_of(label)).collect();
    let action_widths: Vec<f32> = actions
        .iter()
        .map(|(_, label, _)| width_of(label))
        .collect();
    // A segment shares its border with the segment beside it. DESIGN.md 6.
    let views_width = view_widths.iter().sum::<f32>() - (views.len() - 1) as f32;
    let actions_width = action_widths.iter().sum::<f32>();
    let width = views_width + TOOL_GROUP_GAP + actions_width;
    let screen = ui.ctx().content_rect();
    let top_left = egui::pos2(
        (screen.center().x - width / 2.0).round(),
        screen.bottom() - MARGIN - TOOL_HEIGHT,
    );
    let mut pressed = None;
    egui::Area::new(egui::Id::new("toolbar"))
        .order(egui::Order::Middle)
        .fixed_pos(top_left)
        .show(ui.ctx(), |ui| {
            let (whole, _) =
                ui.allocate_exact_size(egui::vec2(width, TOOL_HEIGHT), egui::Sense::hover());
            let views_box =
                egui::Rect::from_min_size(whole.left_top(), egui::vec2(views_width, TOOL_HEIGHT));
            let actions_box = egui::Rect::from_min_size(
                egui::pos2(views_box.right() + TOOL_GROUP_GAP, whole.top()),
                egui::vec2(actions_width, TOOL_HEIGHT),
            );
            widget::shadow_box(ui, views_box, tokens);
            widget::shadow_box(ui, actions_box, tokens);
            // Every fill stays inside the border of its box. A fill over
            // the border would rub it out under the entry that is on, and
            // the box would lose its outline there.
            let views_inside = views_box.shrink(1.0);
            let actions_inside = actions_box.shrink(1.0);
            // The views are a segmented control, and a dashed line stands
            // between each pair. A solid line would give this box the same
            // divided look as the box of buttons beside it. DESIGN.md 5.2.
            let mut left = views_box.left();
            for (index, (press, label, glyph)) in views.iter().enumerate() {
                let cell = egui::Rect::from_min_size(
                    egui::pos2(left, views_box.top()),
                    egui::vec2(view_widths[index], TOOL_HEIGHT),
                );
                let active = *press == Press::View(tool);
                if tool_entry(ui, cell, views_inside, label, *glyph, active, tokens).clicked() {
                    pressed = Some(*press);
                }
                left += view_widths[index] - 1.0;
                if index + 1 < views.len() {
                    let line = [
                        egui::pos2(left, views_inside.top()),
                        egui::pos2(left, views_inside.bottom()),
                    ];
                    // The line takes the `ink` of the box that holds it. A
                    // lighter one beside an `ink` border reads as a seam.
                    painter_dashes(ui, &line, tokens.ink);
                }
            }
            // The rest are buttons. None of them stays on, and no line
            // gathers them into one control.
            let mut left = actions_box.left();
            for (index, (press, label, glyph)) in actions.iter().enumerate() {
                let cell = egui::Rect::from_min_size(
                    egui::pos2(left, actions_box.top()),
                    egui::vec2(action_widths[index], TOOL_HEIGHT),
                );
                if tool_entry(ui, cell, actions_inside, label, *glyph, false, tokens).clicked() {
                    pressed = Some(*press);
                }
                left += action_widths[index];
            }
        });
    pressed
}

/// The History button in the corner of the window. DESIGN.md 5.7.
///
/// It takes the shape of a toolbar entry, because it does the same kind of
/// work. It stands on its own in the corner, away from the views, because
/// it belongs to no view: the history holds every change the DM made.
///
/// Returns `true` when the DM pressed it.
pub(super) fn history_button(ui: &egui::Ui, tokens: Tokens) -> bool {
    let label = text::tool_history();
    let font = theme::font(theme::SMALL, false);
    let text = ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(label.to_owned(), font, egui::Color32::PLACEHOLDER)
            .size()
            .x
    });
    let width = (text + 2.0 * TOOL_PAD).max(TOOL_WIDTH).ceil();
    let screen = ui.ctx().content_rect();
    let left_top = egui::pos2(
        screen.right() - MARGIN - width,
        screen.bottom() - MARGIN - TOOL_HEIGHT,
    );
    let mut pressed = false;
    egui::Area::new(egui::Id::new("history button"))
        .order(egui::Order::Middle)
        .fixed_pos(left_top)
        .show(ui.ctx(), |ui| {
            let (whole, _) =
                ui.allocate_exact_size(egui::vec2(width, TOOL_HEIGHT), egui::Sense::hover());
            widget::shadow_box(ui, whole, tokens);
            pressed = tool_entry(
                ui,
                whole,
                whole.shrink(1.0),
                label,
                Icon::Undo,
                false,
                tokens,
            )
            .clicked();
        });
    pressed
}

/// One entry of the toolbar: the glyph over its label. DESIGN.md 5.2.
///
/// `rect` is what the entry takes for a click, and `inside` is the box it
/// sits in, less its border. Every fill stays within the two, so the
/// border of the box runs unbroken behind the whole toolbar.
fn tool_entry(
    ui: &egui::Ui,
    rect: egui::Rect,
    inside: egui::Rect,
    label: &str,
    glyph: Icon,
    active: bool,
    tokens: Tokens,
) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(label), egui::Sense::click());
    let paint = rect.intersect(inside);
    if active {
        ui.painter().rect_filled(paint, 0, tokens.raised);
        // DESIGN.md 5.2: the bar sits on the top edge of the entry, inside
        // the border of the box.
        ui.painter().rect_filled(
            egui::Rect::from_min_size(paint.left_top(), egui::vec2(paint.width(), widget::BAR)),
            0,
            tokens.accent,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(paint, 0, tokens.field);
    }
    let color = if active { tokens.accent } else { tokens.ink };
    // The glyph and the label stand together in the middle of the entry.
    let block = TOOL_ICON + TOOL_GAP + theme::SMALL;
    let top = rect.center().y - block / 2.0;
    icon::paint(
        ui.painter(),
        glyph,
        egui::pos2(rect.center().x, top + TOOL_ICON / 2.0),
        TOOL_ICON,
        color,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, top + TOOL_ICON + TOOL_GAP),
        egui::Align2::CENTER_TOP,
        label,
        theme::font(theme::SMALL, false),
        color,
    );
    response
}
