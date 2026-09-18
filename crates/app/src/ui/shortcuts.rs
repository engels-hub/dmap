//! The Shortcuts tab of Settings: every control, its key, and a way to
//! give it another. DESIGN.md 9.4. Issue #36.

// Rust guideline compliant 2026-02-21

use crate::icon;
use crate::icons::Icon;
use crate::keys::{ACTIONS, Action, Chord, Group, Keys, Taken, key_name};
use crate::text;
use crate::theme::{self, Tokens};
use crate::widget::{self, Height};

use super::Tool;

/// The height of one row of the list, in points. DESIGN.md 9.4.
const ROW: f32 = 34.0;

/// The width of the column that names a control, in points.
///
/// DESIGN.md 9.4 asks for 270, but the body of the dialog is 452 wide,
/// and 270 leaves no room for `Ctrl + Shift + Z` and the Change button
/// beside it. A name that does not fit wraps inside the column.
const ACTION_COLUMN: f32 = 200.0;

/// The room the Change buttons leave for the scroll bar, in points.
///
/// egui draws the bar over the right edge of the list, and a button under
/// it takes clicks meant for the bar.
const SCROLL_GAP: f32 = 12.0;

/// The space inside a key chip, left and right of its text, in points.
const CHIP_PAD: f32 = 6.0;

/// What the tab holds between frames.
#[derive(Debug, Default)]
pub(super) struct Shortcuts {
    /// The control that waits for its new key.
    waiting: Option<Action>,
    /// The control whose key the program just refused, and what holds it.
    refused: Option<(Action, Taken)>,
}

impl Shortcuts {
    /// Stops the wait for a key, as a dialog that closes does.
    pub(super) fn stop(&mut self) {
        *self = Self::default();
    }
}

/// The size of a glyph inside a chip, in points.
const CHIP_GLYPH: f32 = 14.0;

/// The gap between two parts of a chip, in points.
const PART_GAP: f32 = 4.0;

/// One part of a key chip: words, or a glyph.
#[derive(Debug, Clone, Copy)]
enum Part {
    /// Words, such as `Ctrl` or `drag`.
    Words(&'static str),
    /// A glyph, such as the left button of a mouse.
    Glyph(Icon),
}

/// One chip: the parts of one key, or of one gesture.
type Chip = Vec<Part>;

/// `Ctrl`. Lucide has no glyph for it, and the word is the same in every
/// language the program carries.
const CTRL: Part = Part::Words("Ctrl");

/// `Alt`, for the same reason as `Ctrl`.
const ALT: Part = Part::Words("Alt");

/// `Shift`, as the arrow a keyboard prints on the key.
const SHIFT: Part = Part::Glyph(Icon::ArrowBigUp);

/// Joins the groups of a chip with a `+`, such as `Ctrl + Z`.
fn chip(groups: &[&[Part]]) -> Chip {
    let mut out = Vec::new();
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            out.push(Part::Words("+"));
        }
        out.extend_from_slice(group);
    }
    out
}

/// The chip of one key the DM can change.
fn chord_chip(chord: Chord) -> Chip {
    let key = match chord.key {
        egui::Key::Space => Part::Glyph(Icon::Space),
        egui::Key::Enter => Part::Glyph(Icon::CornerDownLeft),
        egui::Key::Backspace => Part::Glyph(Icon::Delete),
        egui::Key::ArrowLeft => Part::Glyph(Icon::ArrowLeft),
        egui::Key::ArrowRight => Part::Glyph(Icon::ArrowRight),
        egui::Key::ArrowUp => Part::Glyph(Icon::ArrowUp),
        egui::Key::ArrowDown => Part::Glyph(Icon::ArrowDown),
        other => Part::Words(key_name(other)),
    };
    let mut groups: Vec<&[Part]> = Vec::new();
    if chord.ctrl {
        groups.push(&[CTRL]);
    }
    if chord.shift {
        groups.push(&[SHIFT]);
    }
    if chord.alt {
        groups.push(&[ALT]);
    }
    let key = [key];
    groups.push(&key);
    chip(&groups)
}

/// A control that rides on a gesture, and keeps its key.
struct Fixed {
    /// Where the control works.
    group: Group,
    /// What the control does.
    label: &'static str,
    /// One chip for each way to do it.
    chips: Vec<Chip>,
}

/// Every control whose key the DM cannot change, in the order the list
/// shows them.
fn fixed() -> [Fixed; 12] {
    let left = Part::Glyph(Icon::MouseLeft);
    let right = Part::Glyph(Icon::MouseRight);
    // Lucide draws the wheel in the middle of `mouse`, so the one glyph
    // stands for the wheel and for the middle button.
    let wheel = Part::Glyph(Icon::Mouse);
    let drag = Part::Words(text::keys_word_drag());
    let click = Part::Words(text::keys_word_click());
    let roll = Part::Words(text::keys_word_wheel());
    let release = Part::Words(text::keys_word_release());
    let space = Part::Glyph(Icon::Space);
    let row = |group, label, chips| Fixed {
        group,
        label,
        chips,
    };
    [
        row(
            Group::Every,
            text::keys_fixed_pan(),
            vec![
                chip(&[&[space], &[left, drag]]),
                chip(&[&[wheel, drag]]),
                chip(&[&[wheel, roll]]),
            ],
        ),
        row(
            Group::Every,
            text::keys_fixed_pan_sideways(),
            vec![chip(&[&[SHIFT], &[wheel, roll]])],
        ),
        row(
            Group::Every,
            text::keys_fixed_zoom(),
            vec![chip(&[&[CTRL], &[wheel, roll]])],
        ),
        row(
            Group::Every,
            text::keys_fixed_escape(),
            vec![chip(&[&[Part::Words("Escape")]])],
        ),
        row(
            Group::Select,
            text::keys_fixed_move_free(),
            vec![chip(&[&[CTRL], &[left, drag]])],
        ),
        row(
            Group::Select,
            text::keys_fixed_add(),
            vec![chip(&[&[CTRL], &[left, click]])],
        ),
        row(
            Group::Draw,
            text::keys_fixed_off_grid(),
            vec![chip(&[&[SHIFT], &[left, drag]])],
        ),
        row(
            Group::Draw,
            text::keys_fixed_ruler_off_grid(),
            vec![chip(&[&[ALT], &[left, drag]])],
        ),
        row(
            Group::Draw,
            text::keys_fixed_keep(),
            vec![chip(&[&[SHIFT], &[left, release]])],
        ),
        row(
            Group::Draw,
            text::keys_fixed_bend(),
            vec![chip(&[&[right, click]])],
        ),
        row(
            Group::Table,
            text::keys_fixed_zoom_box(),
            vec![chip(&[&[CTRL], &[ALT], &[wheel, roll]])],
        ),
        row(
            Group::Table,
            text::keys_fixed_keep_size(),
            vec![chip(&[&[ALT], &[left, release]])],
        ),
    ]
}

/// Takes the next key press for the control that waits for one.
///
/// Every key event of the frame goes, so the press reaches no control of
/// the canvas and no close of the dialog. `Escape` gives up the change.
/// Returns `true` when a control took a new key.
pub(super) fn listen(ctx: &egui::Context, keys: &mut Keys, state: &mut Shortcuts) -> bool {
    let Some(action) = state.waiting else {
        return false;
    };
    let mut press = None;
    ctx.input_mut(|input| {
        input.events.retain(|event| match event {
            egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } => {
                if *pressed && !*repeat && press.is_none() {
                    press = Some((*key, *modifiers));
                }
                false
            }
            egui::Event::Text(_) => false,
            _ => true,
        });
    });
    let Some((key, modifiers)) = press else {
        return false;
    };
    if key == egui::Key::Escape {
        state.stop();
        return false;
    }
    match keys.set(action, Chord::of(key, modifiers)) {
        Ok(()) => {
            state.stop();
            true
        }
        // The row keeps its wait, so the DM tries another key at once.
        Err(taken) => {
            state.refused = Some((action, taken));
            false
        }
    }
}

/// The list of controls. DESIGN.md 9.4.
///
/// The group of the view the DM works in comes first, then the controls
/// of every view, then the rest.
pub(super) fn shortcuts_tab(
    ui: &mut egui::Ui,
    keys: &Keys,
    state: &mut Shortcuts,
    tool: Tool,
    tokens: Tokens,
) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let here = match tool {
        Tool::Select => Group::Select,
        Tool::Draw => Group::Draw,
        Tool::Table => Group::Table,
    };
    let mut groups = vec![here, Group::Every];
    groups.extend(
        [Group::Select, Group::Draw, Group::Table]
            .into_iter()
            .filter(|group| *group != here),
    );
    let fixed = fixed();
    for group in groups {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(group.label())
                .font(theme::font(theme::BODY, true))
                .color(tokens.ink),
        );
        ui.add_space(4.0);
        for action in ACTIONS.into_iter().filter(|a| a.group() == group) {
            let waiting = state.waiting == Some(action);
            let chips: Vec<Chip> = if waiting {
                vec![vec![Part::Words(text::keys_waiting())]]
            } else {
                keys.of(action).into_iter().map(chord_chip).collect()
            };
            if control_row(ui, action.label(), &chips, waiting, true, tokens) {
                state.refused = None;
                state.waiting = if waiting { None } else { Some(action) };
            }
            if let Some((_, taken)) = state.refused.filter(|(a, _)| *a == action) {
                ui.label(
                    egui::RichText::new(text::keys_taken(taken.holder()))
                        .font(theme::font(theme::SMALL, false))
                        .color(tokens.accent),
                );
            }
        }
        for row in fixed.iter().filter(|f| f.group == group) {
            control_row(ui, row.label, &row.chips, false, false, tokens);
        }
    }
    ui.add_space(12.0);
    widget::helper(ui, text::keys_fixed_helper());
}

/// One row: what the control does, its keys, and the Change button.
///
/// Returns `true` when the DM pressed Change.
fn control_row(
    ui: &mut egui::Ui,
    label: &str,
    chips: &[Chip],
    waiting: bool,
    changes: bool,
    tokens: Tokens,
) -> bool {
    let mut pressed = false;
    let width = ui.available_width();
    let row = ui.horizontal(|ui| {
        // Every row takes the whole width, so the rules line up whether a
        // row has a Change button or not.
        ui.set_min_size(egui::vec2(width, ROW));
        ui.allocate_ui_with_layout(
            egui::vec2(ACTION_COLUMN, ROW),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(ACTION_COLUMN);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(label)
                            .font(theme::font(theme::BODY, false))
                            .color(tokens.ink),
                    )
                    .wrap(),
                );
            },
        );
        let change = text::keys_change();
        let button = widget::button_width(ui, change, None);
        let room = (ui.available_width() - button - SCROLL_GAP - 8.0).max(0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(room, ROW),
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
            |ui| {
                ui.set_width(room);
                for chip in chips {
                    key_chip(ui, chip, waiting, tokens);
                }
            },
        );
        if changes {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(SCROLL_GAP);
                pressed = widget::button(ui, change, None, Height::Row).clicked();
            });
        }
    });
    widget::rule_bottom(ui, row.response.rect, tokens);
    pressed
}

/// A key, drawn as a chip. The chip of a row that waits takes the accent.
fn key_chip(ui: &mut egui::Ui, parts: &[Part], waiting: bool, tokens: Tokens) {
    let color = if waiting { tokens.accent } else { tokens.ink };
    let galleys: Vec<Option<std::sync::Arc<egui::Galley>>> = parts
        .iter()
        .map(|part| match part {
            Part::Words(words) => Some(ui.ctx().fonts_mut(|fonts| {
                fonts.layout_no_wrap((*words).to_owned(), theme::font(theme::SMALL, false), color)
            })),
            Part::Glyph(_) => None,
        })
        .collect();
    let inner: f32 = galleys
        .iter()
        .map(|galley| galley.as_ref().map_or(CHIP_GLYPH, |g| g.size().x))
        .sum::<f32>()
        + PART_GAP * parts.len().saturating_sub(1) as f32;
    let size = egui::vec2(inner + 2.0 * CHIP_PAD, Height::Row.points() - 2.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let border = if waiting {
        egui::Stroke::new(1.0, tokens.accent)
    } else {
        tokens.hairline()
    };
    ui.painter()
        .rect(rect, 0, tokens.field, border, egui::StrokeKind::Inside);
    let mut left = rect.left() + CHIP_PAD;
    for (part, galley) in parts.iter().zip(galleys) {
        match (part, galley) {
            (_, Some(galley)) => {
                let width = galley.size().x;
                let top = rect.center().y - galley.size().y / 2.0;
                ui.painter().galley(egui::pos2(left, top), galley, color);
                left += width;
            }
            (Part::Glyph(glyph), None) => {
                let at = egui::pos2(left + CHIP_GLYPH / 2.0, rect.center().y);
                icon::paint(ui.painter(), *glyph, at, CHIP_GLYPH, color);
                left += CHIP_GLYPH;
            }
            (Part::Words(_), None) => {}
        }
        left += PART_GAP;
    }
}

/// The footer of the tab: the default keys on the left, Close on the
/// right. DESIGN.md 9.4.
///
/// Returns whether the keys changed, and whether the DM asked to close.
pub(super) fn footer(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    keys: &mut Keys,
    state: &mut Shortcuts,
    tokens: Tokens,
) -> (bool, bool) {
    ui.painter()
        .hline(rect.x_range(), rect.top(), tokens.hairline());
    let inner = rect.shrink2(egui::vec2(20.0, 0.0));
    let mut edited = false;
    let mut close = false;
    let mut footer_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    footer_ui.add_enabled_ui(!keys.is_default(), |ui| {
        if widget::button(ui, text::keys_reset(), None, Height::Full).clicked() {
            keys.reset();
            state.stop();
            edited = true;
        }
    });
    footer_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        close = widget::button(ui, text::keys_close(), None, Height::Full).clicked();
    });
    (edited, close)
}
