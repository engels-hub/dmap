//! The controls of DESIGN.md 6 and the chrome recipe of DESIGN.md 7.
//!
//! Every control here paints itself from the tokens of [`crate::theme`].
//! Nothing in the UI writes a color or a size of its own.

// Rust guideline compliant 2026-02-21

use egui::{Align2, Color32, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::icon;
use crate::icons::Icon;
use crate::theme::{self, Tokens};

/// The height of a control that DESIGN.md 6 gives a fixed size.
pub const CONTROL: f32 = 28.0;

/// The width of a select. DESIGN.md 6.
pub const SELECT_WIDTH: f32 = 260.0;

/// The horizontal padding inside a button or a segment. DESIGN.md 6.
const PAD: f32 = 10.0;

/// The horizontal padding inside an input. DESIGN.md 6.
const INPUT_PAD: f32 = 8.0;

/// The width of the bar that marks an active entry. DESIGN.md 6.
pub const BAR: f32 = 3.0;

/// The size of the check inside a checkbox. DESIGN.md 6.
const CHECK: f32 = 14.0;

/// The side of the checkbox itself. DESIGN.md 6.
const BOX_SIDE: f32 = 18.0;

/// The gap between a checkbox and its label. DESIGN.md 6.
const CHECK_GAP: f32 = 8.0;

/// The size of an icon inside a button or a list row. DESIGN.md 4.
pub const SMALL_ICON: f32 = 16.0;

/// The gap between an icon and the text beside it. DESIGN.md 6.
const ICON_GAP: f32 = 6.0;

/// How tall a button is, by where it sits. DESIGN.md 6.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Height {
    /// 28 px, on the canvas or in a dialog.
    #[default]
    Full,
    /// 26 px, in a panel.
    Panel,
    /// 24 px, in a row. The floor of DESIGN.md 6.
    Row,
}

impl Height {
    /// The height in points.
    pub fn points(self) -> f32 {
        match self {
            Self::Full => CONTROL,
            Self::Panel => 26.0,
            Self::Row => 24.0,
        }
    }
}

/// Paints the box of DESIGN.md 7: `surface`, a 1 px `ink` border, a shadow.
///
/// The shadow goes down first, so the box covers its own offset.
pub fn shadow_box(ui: &Ui, rect: Rect, tokens: Tokens) {
    let painter = ui.painter();
    if let (Some(color), Some(offset)) = (tokens.shadow, tokens.shadow_offset()) {
        painter.rect_filled(rect.translate(offset), 0, color);
    }
    painter.rect(
        rect,
        0,
        tokens.surface,
        Stroke::new(1.0, tokens.ink),
        egui::StrokeKind::Inside,
    );
}

/// Paints a 1 px `rule` line along the bottom edge of `rect`.
pub fn rule_bottom(ui: &Ui, rect: Rect, tokens: Tokens) {
    ui.painter()
        .hline(rect.x_range(), rect.bottom(), tokens.hairline());
}

/// A helper line under a control: 14 px in `mute`. DESIGN.md 3.
pub fn helper(ui: &mut Ui, text: &str) {
    let tokens = theme::of(ui.ctx());
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .font(theme::font(theme::SMALL, false))
                .color(tokens.mute),
        )
        .wrap(),
    );
}

/// The label over a control in a panel: 15 px in `ink`. DESIGN.md 7.1.
pub fn row_label(ui: &mut Ui, text: &str) {
    let tokens = theme::of(ui.ctx());
    ui.label(
        egui::RichText::new(text)
            .font(theme::font(theme::BODY, false))
            .color(tokens.ink),
    );
}

/// A button of DESIGN.md 6, with the glyph it carries on the left.
pub fn button(ui: &mut Ui, text: &str, glyph: Option<Icon>, height: Height) -> Response {
    let tokens = theme::of(ui.ctx());
    let font = theme::font(theme::BODY, false);
    let width = button_width(ui, text, glyph);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height.points()), Sense::click());
    let fill = if response.is_pointer_button_down_on() || response.hovered() {
        tokens.raised
    } else {
        tokens.field
    };
    ui.painter().rect(
        rect,
        0,
        fill,
        Stroke::new(1.0, tokens.ink),
        egui::StrokeKind::Inside,
    );
    let mut left = rect.left() + PAD;
    if let Some(glyph) = glyph {
        icon::paint(
            ui.painter(),
            glyph,
            pos2(left + SMALL_ICON / 2.0, rect.center().y),
            SMALL_ICON,
            tokens.ink,
        );
        left += SMALL_ICON + ICON_GAP;
    }
    ui.painter().text(
        pos2(left, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font,
        tokens.ink,
    );
    response
}

/// A checkbox of DESIGN.md 6, with its label on the right.
///
/// A label too long for the row wraps under itself. DESIGN.md 1 puts
/// legibility first, so no word of a label goes missing.
pub fn checkbox(ui: &mut Ui, on: &mut bool, text: &str) -> Response {
    let tokens = theme::of(ui.ctx());
    let font = theme::font(theme::BODY, false);
    let room = (ui.available_width() - BOX_SIDE - CHECK_GAP).max(BOX_SIDE);
    let galley = ui
        .ctx()
        .fonts_mut(|fonts| fonts.layout(text.to_owned(), font, tokens.ink, room));
    let height = galley.size().y.max(CONTROL);
    let (rect, response) = ui.allocate_exact_size(
        vec2(BOX_SIDE + CHECK_GAP + galley.size().x, height),
        Sense::click(),
    );
    if response.clicked() {
        *on = !*on;
    }
    // The box lines up with the first line of the label, not with the
    // middle of a label that took two lines.
    let square = Rect::from_center_size(
        pos2(rect.left() + BOX_SIDE / 2.0, rect.top() + CONTROL / 2.0),
        Vec2::splat(BOX_SIDE),
    );
    let border = if *on { tokens.ink } else { tokens.rule };
    ui.painter().rect(
        square,
        0,
        tokens.field,
        Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    if *on {
        icon::paint(
            ui.painter(),
            Icon::Check,
            square.center(),
            CHECK,
            tokens.accent,
        );
    }
    ui.painter().galley(
        pos2(
            rect.left() + BOX_SIDE + CHECK_GAP,
            square.center().y - galley.size().y.min(CONTROL) / 2.0,
        ),
        galley,
        tokens.ink,
    );
    response
}

/// A segmented control of DESIGN.md 6.
///
/// Returns `true` when the click landed on another segment. The borders of
/// two segments overlap by 1 px, so the row draws one line between them.
pub fn segmented<T: Copy + PartialEq>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let tokens = theme::of(ui.ctx());
    let font = theme::font(theme::BODY, false);
    let widths: Vec<f32> = options
        .iter()
        .map(|(_, text)| text_width(ui, text, &font) + 2.0 * PAD)
        .collect();
    let total: f32 = widths.iter().sum::<f32>() - (options.len().saturating_sub(1)) as f32;
    // The id comes from the space this control took, not from where it
    // sits. Two controls that start at one x would otherwise hold one id,
    // and egui would send the click of the second to the first.
    let (rect, whole) = ui.allocate_exact_size(vec2(total, CONTROL), Sense::hover());
    let mut changed = false;
    let mut left = rect.left();
    for (index, ((choice, text), width)) in options.iter().zip(&widths).enumerate() {
        let segment = Rect::from_min_size(pos2(left, rect.top()), vec2(*width, CONTROL));
        let response = ui.interact(segment, whole.id.with(index), Sense::click());
        let picked = *value == *choice;
        if response.clicked() && !picked {
            *value = *choice;
            changed = true;
        }
        let fill = if picked {
            tokens.raised
        } else if response.hovered() {
            tokens.field
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect(
            segment,
            0,
            fill,
            tokens.hairline(),
            egui::StrokeKind::Inside,
        );
        let text_color = if picked { tokens.accent } else { tokens.ink };
        ui.painter().text(
            segment.center(),
            Align2::CENTER_CENTER,
            text,
            font.clone(),
            text_color,
        );
        if picked {
            let bar = Rect::from_min_max(
                pos2(segment.left(), segment.bottom() - BAR),
                segment.right_bottom(),
            );
            ui.painter().rect_filled(bar, 0, tokens.accent);
        }
        // The next segment starts one point back, so the two borders meet
        // on one line instead of two.
        left += width - 1.0;
    }
    changed
}

/// A number input of DESIGN.md 6, with its unit in `mute` on the right.
///
/// A drag on the field changes the value by `step` for each point.
pub fn input(
    ui: &mut Ui,
    value: &mut f64,
    unit: &str,
    width: f32,
    range: std::ops::RangeInclusive<f64>,
    step: f64,
) -> Response {
    let tokens = theme::of(ui.ctx());
    let font = theme::font(theme::BODY, false);
    let unit_width = if unit.is_empty() {
        0.0
    } else {
        ICON_GAP + text_width(ui, unit, &font)
    };
    let (whole, _) = ui.allocate_exact_size(vec2(width + unit_width, CONTROL), Sense::hover());
    let rect = Rect::from_min_size(whole.left_top(), vec2(width, CONTROL));
    // The frame goes down first. A frame painted after the value would
    // cover the number with its own fill.
    ui.painter().rect(
        rect,
        0,
        tokens.field,
        Stroke::new(1.0, tokens.rule),
        egui::StrokeKind::Inside,
    );
    let inner = rect.shrink2(vec2(INPUT_PAD, 1.0));
    let response = ui
        .scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.set_clip_rect(inner);
            let widgets = &mut ui.style_mut().visuals.widgets;
            for state in [
                &mut widgets.inactive,
                &mut widgets.hovered,
                &mut widgets.active,
            ] {
                state.weak_bg_fill = Color32::TRANSPARENT;
                state.bg_fill = Color32::TRANSPARENT;
                state.bg_stroke = Stroke::NONE;
            }
            ui.add_sized(
                inner.size(),
                egui::DragValue::new(value)
                    .speed(step)
                    .range(range)
                    .update_while_editing(false),
            )
        })
        .inner;
    // DESIGN.md 6: the border takes the accent while the input has the key.
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            0,
            Stroke::new(1.0, tokens.accent),
            egui::StrokeKind::Inside,
        );
    }
    if !unit.is_empty() {
        ui.painter().text(
            pos2(rect.right() + ICON_GAP, rect.center().y),
            egui::Align2::LEFT_CENTER,
            unit,
            font,
            tokens.mute,
        );
    }
    response
}

/// The field of a select of DESIGN.md 6, with its chevron on the right.
///
/// Returns the response, so the caller can hang the open list on it.
pub fn select_field(ui: &mut Ui, text: &str, width: f32) -> Response {
    let tokens = theme::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(width, CONTROL), Sense::click());
    let border = if response.hovered() {
        tokens.ink
    } else {
        tokens.rule
    };
    ui.painter().rect(
        rect,
        0,
        tokens.field,
        Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        pos2(rect.left() + INPUT_PAD, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        theme::font(theme::BODY, false),
        tokens.ink,
    );
    icon::paint(
        ui.painter(),
        Icon::ChevronDown,
        pos2(rect.right() - INPUT_PAD - SMALL_ICON / 2.0, rect.center().y),
        SMALL_ICON,
        tokens.ink,
    );
    response
}

/// One row of an open select. DESIGN.md 6.
pub fn select_row(ui: &mut Ui, text: &str, picked: bool) -> Response {
    let tokens = theme::of(ui.ctx());
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, CONTROL), Sense::click());
    let fill = if picked {
        tokens.raised
    } else if response.hovered() {
        tokens.field
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 0, fill);
    if picked {
        let bar = Rect::from_min_size(rect.left_top(), vec2(BAR, rect.height()));
        ui.painter().rect_filled(bar, 0, tokens.accent);
        icon::paint(
            ui.painter(),
            Icon::Check,
            pos2(rect.right() - INPUT_PAD - CHECK / 2.0, rect.center().y),
            CHECK,
            tokens.accent,
        );
    }
    let text_color = if picked { tokens.accent } else { tokens.ink };
    ui.painter().text(
        pos2(rect.left() + INPUT_PAD, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        theme::font(theme::BODY, false),
        text_color,
    );
    response
}

/// A slider of DESIGN.md 6, with its value in `mute` on the right.
///
/// The knob keeps the full range reachable: the track starts and ends half
/// a knob inside the control, so the end values sit under the pointer.
///
/// The response reads as changed on each frame of a drag that moves the
/// value, so a caller can write what the DM picked and no more.
pub fn slider(
    ui: &mut Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    width: f32,
    text: &str,
) -> Response {
    /// The side of the knob, in points. DESIGN.md 6.
    const KNOB: f32 = 14.0;
    let tokens = theme::of(ui.ctx());
    let (rect, mut response) =
        ui.allocate_exact_size(vec2(width, CONTROL), Sense::click_and_drag());
    let track = Rect::from_min_max(
        pos2(rect.left() + KNOB / 2.0, rect.center().y - 1.0),
        pos2(rect.right() - KNOB / 2.0, rect.center().y + 1.0),
    );
    let span = range.end() - range.start();
    if response.is_pointer_button_down_on()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let share = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0);
        let picked = range.start() + f64::from(share) * span;
        // A hand that holds the knob still moves no value, and a caller
        // that saves what changed must not write a file for every frame
        // of the drag.
        if (picked - *value).abs() > f64::EPSILON {
            *value = picked;
            response.mark_changed();
        }
    }
    let share = if span.abs() < f64::EPSILON {
        0.0
    } else {
        ((*value - *range.start()) / span).clamp(0.0, 1.0) as f32
    };
    let knob_x = track.left() + share * track.width();
    ui.painter().rect_filled(track, 0, tokens.rule);
    ui.painter().rect_filled(
        Rect::from_min_max(track.left_top(), pos2(knob_x, track.bottom())),
        0,
        tokens.ink,
    );
    ui.painter().rect(
        Rect::from_center_size(pos2(knob_x, rect.center().y), Vec2::splat(KNOB)),
        0,
        tokens.field,
        Stroke::new(1.0, tokens.ink),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        pos2(rect.right() + ICON_GAP, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        theme::font(theme::BODY, false),
        tokens.mute,
    );
    response
}

/// How wide `text` runs in `font`.
/// The swatch that picks the color of a stroke. DESIGN.md 8.3.
///
/// It offers a blend and no more. `egui` writes an additive color as one
/// with no alpha at all, and the ink pipeline blends on alpha, so an
/// additive color would draw nothing at all and read as a broken swatch.
/// The alpha stays above zero for the same reason: a stroke the DM cannot
/// see is a stroke they cannot find on the canvas. Issue #12.
///
/// Returns `true` when the DM changed the color.
pub fn color_swatch(ui: &mut Ui, color: &mut [u8; 4]) -> bool {
    let mut picked = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    let response = egui::color_picker::color_edit_button_srgba(
        ui,
        &mut picked,
        egui::color_picker::Alpha::OnlyBlend,
    );
    if !response.changed() {
        return false;
    }
    let mut taken = picked.to_srgba_unmultiplied();
    taken[3] = taken[3].max(MIN_ALPHA);
    *color = taken;
    true
}

/// A swatch that picks a color and takes no alpha with it. DESIGN.md 9.2.
///
/// The canvas background and the grid line are as solid as the row beside
/// them says, so an alpha in the popover would be a control that changes
/// nothing. Issue #66.
///
/// Returns `true` when the DM changed the color.
pub fn color_swatch_rgb(ui: &mut Ui, color: &mut [u8; 3]) -> bool {
    let mut picked = egui::Color32::from_rgb(color[0], color[1], color[2]);
    let response = egui::color_picker::color_edit_button_srgba(
        ui,
        &mut picked,
        egui::color_picker::Alpha::Opaque,
    );
    if !response.changed() {
        return false;
    }
    *color = [picked.r(), picked.g(), picked.b()];
    true
}

/// The faintest a stroke may draw, out of 255.
///
/// Ten is a wash the DM can still see well enough to pick the stroke up
/// again. Nothing is not a color.
const MIN_ALPHA: u8 = 10;

/// How wide a button of these words stands, in points.
///
/// A panel that has to know whether two buttons fit beside one another
/// asks this first. A language whose words run longer than the English
/// ones then takes another row instead of losing the end of a word.
pub fn button_width(ui: &Ui, text: &str, glyph: Option<Icon>) -> f32 {
    let font = theme::font(theme::BODY, false);
    text_width(ui, text, &font) + 2.0 * PAD + glyph.map_or(0.0, |_| SMALL_ICON + ICON_GAP)
}

/// How tall these words stand once they wrap to `width`, in points.
pub fn helper_height(ui: &Ui, text: &str, width: f32) -> f32 {
    ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout(
                text.to_owned(),
                theme::font(theme::SMALL, false),
                egui::Color32::PLACEHOLDER,
                width,
            )
            .size()
            .y
    })
}

fn text_width(ui: &Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font.clone(), Color32::PLACEHOLDER)
            .size()
            .x
    })
}

#[cfg(test)]
mod tests {
    use super::Height;

    #[test]
    fn a_button_takes_the_height_of_its_place() {
        // DESIGN.md 6: 28 px on the canvas, 26 px in a panel, 24 px in a row.
        assert!((Height::Full.points() - 28.0).abs() < f32::EPSILON);
        assert!((Height::Panel.points() - 26.0).abs() < f32::EPSILON);
        assert!((Height::Row.points() - 24.0).abs() < f32::EPSILON);
        // No button goes under the floor DESIGN.md 6 sets.
        for height in [Height::Full, Height::Panel, Height::Row] {
            assert!(height.points() >= 24.0, "{height:?} is under 24 px");
        }
        assert_eq!(Height::default(), Height::Full);
    }
}
