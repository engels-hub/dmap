//! The settings dialog, the history list and the scene picker.
//!
//! Each of these is a window the DM opens over the canvas. They share
//! the frame of `dialog_frame`, and none of them paints the canvas.

// Rust guideline compliant 2026-02-21

use crate::command::Note;
use crate::config::{MAX_GRID_WIDTH, MIN_GRID_WIDTH};
use crate::icon;
use crate::icons::Icon;
use crate::text;
use crate::theme::{self, Tokens};
use crate::tv::display_label;
use crate::tvbox::MAX_SNAP_PERCENT;
use crate::widget::{self, Height};

use super::{
    DIALOG, DIALOG_HEADER, DIALOG_NAV, FOOTER, Frame, LABEL_COLUMN, MARGIN, PANEL_PAD, PATH_WIDTH,
    ROW_GAP, ROW_HEIGHT, SCENE_ROW, STEP_LINE, STEP_PAD, STEP_ROW, SceneCommand, Scenes, Settings,
};

use super::tree::row_name;

/// A tab of the settings dialog. DESIGN.md 9.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tab {
    /// The TV, its size and the snap. DESIGN.md 9.1.
    #[default]
    Table,
    /// The canvas grid. DESIGN.md 9.2.
    Grid,
    /// The light of the scene. DESIGN.md 9.3.
    Light,
    /// Every control and its key. DESIGN.md 9.4.
    Shortcuts,
    /// What dmap is built from, and under what terms. DESIGN.md 9.5.
    About,
}

impl Tab {
    /// The name and the glyph of the navigation entry.
    fn entry(self) -> (&'static str, Icon) {
        match self {
            Self::Table => (text::dialog_settings_tab_table(), Icon::Monitor),
            Self::Grid => (text::dialog_settings_tab_grid(), Icon::Grid3x3),
            Self::Light => (text::dialog_settings_tab_light(), Icon::Sun),
            Self::Shortcuts => (text::dialog_settings_tab_shortcuts(), Icon::Keyboard),
            Self::About => (text::dialog_settings_tab_about(), Icon::Info),
        }
    }
}

/// What the settings dialog holds between frames.
#[derive(Debug, Default)]
pub(super) struct Dialog {
    /// Whether the dialog stands over the canvas.
    pub(super) open: bool,
    /// The tab the navigation column marks.
    pub(super) tab: Tab,
    /// The scale the DM is dragging toward, until the button goes up.
    scale_drag: Option<f64>,
    /// The license the About tab shows.
    license: License,
}

impl Dialog {
    /// A dialog that stands closed, and opens on this tab.
    pub(super) fn on_tab(tab: Tab) -> Self {
        Self {
            tab,
            ..Self::default()
        }
    }
}

/// The box of a dialog: the scrim, the frame, the header. DESIGN.md 9.
///
/// `body` draws inside the rest of the frame. Returns `true` when the DM
/// asked to close the dialog.
fn dialog_frame(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: egui::Vec2,
    tokens: Tokens,
    body: impl FnOnce(&mut egui::Ui, egui::Rect),
) -> bool {
    let screen = ctx.content_rect();
    // A dialog never outgrows the window. At a large interface scale the
    // size of DESIGN.md 9 does not fit, and a dialog whose close button
    // sits off the screen is a dialog no one can leave.
    let size = size.min(screen.size() - egui::Vec2::splat(2.0 * MARGIN));
    let rect = egui::Rect::from_center_size(screen.center(), size);
    let mut close = false;
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            // The scrim takes every click that misses the dialog, so
            // nothing behind it moves while it stands.
            let behind = ui.allocate_rect(screen, egui::Sense::click_and_drag());
            ui.painter().rect_filled(screen, 0, tokens.scrim);
            if behind.clicked() {
                close = true;
            }
            ui.allocate_rect(rect, egui::Sense::click_and_drag());
            if let Some(color) = tokens.shadow {
                // DESIGN.md 9: a 6 by 8 offset at a quarter of the value.
                ui.painter().rect_filled(
                    rect.translate(egui::vec2(6.0, 8.0)),
                    0,
                    color.gamma_multiply(0.25),
                );
            }
            ui.painter().rect(
                rect,
                0,
                tokens.surface,
                egui::Stroke::new(1.0, tokens.ink),
                egui::StrokeKind::Inside,
            );
            let header =
                egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), DIALOG_HEADER));
            ui.painter().text(
                egui::pos2(header.left() + 20.0, header.center().y),
                egui::Align2::LEFT_CENTER,
                title,
                theme::font(theme::TITLE, true),
                tokens.ink,
            );
            let close_rect = egui::Rect::from_center_size(
                egui::pos2(header.right() - 12.0 - 14.0, header.center().y),
                egui::Vec2::splat(28.0),
            );
            let button = ui.interact(close_rect, ui.id().with("close"), egui::Sense::click());
            if button.hovered() {
                ui.painter().rect_filled(close_rect, 0, tokens.raised);
            }
            icon::paint(ui.painter(), Icon::X, close_rect.center(), 18.0, tokens.ink);
            close |= button.clicked();
            widget::rule_bottom(ui, header, tokens);
            body(
                ui,
                egui::Rect::from_min_max(egui::pos2(rect.left(), header.bottom()), rect.max),
            );
        });
    close || ctx.input(|i| i.key_pressed(egui::Key::Escape))
}

/// The settings dialog of DESIGN.md 9. Returns `true` when a value changed.
pub(super) fn settings_dialog(
    ctx: &egui::Context,
    frame: &mut Frame<'_>,
    dialog: &mut Dialog,
    tokens: Tokens,
) -> bool {
    if !dialog.open {
        return false;
    }
    let mut edited = false;
    let title = text::dialog_settings_title();
    let close = dialog_frame(ctx, "settings", title, DIALOG, tokens, |ui, rest| {
        let nav = egui::Rect::from_min_size(rest.left_top(), egui::vec2(DIALOG_NAV, rest.height()));
        dialog_nav(ui, nav, &mut dialog.tab, tokens);
        let body = egui::Rect::from_min_max(
            egui::pos2(nav.right() + 20.0, rest.top() + 18.0),
            egui::pos2(rest.right() - 20.0, rest.bottom() - 18.0),
        );
        let mut body_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(body)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        body_ui.set_clip_rect(body);
        body_ui.spacing_mut().item_spacing.y = ROW_GAP;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(&mut body_ui, |body_ui| {
                body_ui.spacing_mut().item_spacing.y = ROW_GAP;
                match dialog.tab {
                    Tab::Table => {
                        edited = table_tab(body_ui, frame, &mut dialog.scale_drag, tokens);
                    }
                    Tab::Grid => {
                        edited = grid_tab(body_ui, frame.settings);
                    }
                    Tab::Light => {
                        not_built(body_ui, text::dialog_settings_light_soon());
                    }
                    Tab::About => about_tab(body_ui, &mut dialog.license, tokens),
                    Tab::Shortcuts => {
                        not_built(body_ui, text::dialog_settings_shortcuts_soon());
                    }
                }
            });
    });
    if close {
        dialog.open = false;
    }
    edited
}

/// The navigation column of a dialog. DESIGN.md 9.
fn dialog_nav(ui: &egui::Ui, rect: egui::Rect, tab: &mut Tab, tokens: Tokens) {
    /// The vertical padding of the column, in points. DESIGN.md 9.
    const NAV_PAD: f32 = 10.0;
    /// The height of one entry, in points: 14 px text and 8 px above and
    /// below it, rounded up to a whole point. DESIGN.md 9.
    const ENTRY: f32 = 32.0;
    ui.painter()
        .vline(rect.right(), rect.y_range(), tokens.hairline());
    let mut top = rect.top() + NAV_PAD;
    for choice in [
        Tab::Table,
        Tab::Grid,
        Tab::Light,
        Tab::Shortcuts,
        Tab::About,
    ] {
        let (label, glyph) = choice.entry();
        let entry = egui::Rect::from_min_size(
            egui::pos2(rect.left(), top),
            egui::vec2(rect.width(), ENTRY),
        );
        let response = ui.interact(entry, ui.id().with(label), egui::Sense::click());
        if response.clicked() {
            *tab = choice;
        }
        let active = *tab == choice;
        if active {
            ui.painter().rect_filled(entry, 0, tokens.raised);
            ui.painter().rect_filled(
                egui::Rect::from_min_size(entry.left_top(), egui::vec2(widget::BAR, ENTRY)),
                0,
                tokens.accent,
            );
        } else if response.hovered() {
            ui.painter().rect_filled(entry, 0, tokens.field);
        }
        let color = if active { tokens.accent } else { tokens.ink };
        icon::paint(
            ui.painter(),
            glyph,
            egui::pos2(entry.left() + 12.0 + 9.0, entry.center().y),
            18.0,
            color,
        );
        ui.painter().text(
            egui::pos2(entry.left() + 12.0 + 18.0 + 8.0, entry.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            theme::font(theme::BODY, false),
            color,
        );
        top += ENTRY;
    }
}

/// One row of a dialog body: the label column, then the controls.
///
/// DESIGN.md 9 gives the label column 170 points and stacks the controls
/// of a row with an 8 point gap.
fn dialog_row(ui: &mut egui::Ui, label: &str, controls: impl FnOnce(&mut egui::Ui)) {
    let tokens = theme::of(ui.ctx());
    ui.horizontal_top(|ui| {
        // DESIGN.md 9 gives the label a column 140 points wide. A language
        // whose word does not fit that column wraps inside it and the row
        // grows, so a label never runs into the control beside it.
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_COLUMN, widget::CONTROL),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                // The column keeps its width whatever the label needs, so
                // the controls of every row line up.
                ui.set_min_width(LABEL_COLUMN);
                // DESIGN.md 9: the label sits 6 points below the top.
                ui.add_space(6.0);
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
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            controls(ui);
        });
    });
}

/// The Table tab of DESIGN.md 9.1. Returns `true` when a value changed.
fn table_tab(
    ui: &mut egui::Ui,
    frame: &mut Frame<'_>,
    scale_drag: &mut Option<f64>,
    tokens: Tokens,
) -> bool {
    let mut edited = false;
    let displays = frame.displays;
    let label = |i: usize| {
        let display = &displays[i];
        let size = display.size();
        display_label(display.name().as_deref(), size.width, size.height)
    };
    dialog_row(ui, text::dialog_settings_display(), |ui| {
        let shown = frame
            .settings
            .tv_display
            .map_or_else(|| text::dialog_settings_display_window().to_owned(), &label);
        let field = widget::select_field(ui, &shown, widget::SELECT_WIDTH);
        let popup = egui::Popup::menu(&field)
            .gap(-1.0)
            .width(widget::SELECT_WIDTH);
        popup.show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let picked = frame.settings.tv_display;
            if widget::select_row(ui, text::dialog_settings_display_window(), picked.is_none())
                .clicked()
            {
                frame.settings.tv_display = None;
                edited = true;
            }
            for i in 0..displays.len() {
                if widget::select_row(ui, &label(i), picked == Some(i)).clicked() {
                    frame.settings.tv_display = Some(i);
                    edited = true;
                }
            }
        });
    });
    dialog_row(ui, text::dialog_settings_size(), |ui| {
        widget::helper(ui, text::dialog_settings_size_helper());
    });
    dialog_row(ui, text::dialog_settings_snap(), |ui| {
        let mut percent = frame.settings.snap_percent;
        if widget::input(
            ui,
            &mut percent,
            text::unit_percent(),
            90.0,
            0.0..=MAX_SNAP_PERCENT,
            0.1,
        )
        .changed()
        {
            frame.settings.snap_percent = percent;
            edited = true;
        }
        widget::helper(ui, text::dialog_settings_snap_helper());
    });
    dialog_row(ui, text::dialog_settings_windows(), |ui| {
        let mut swap = frame.settings.swap_windows;
        if widget::checkbox(ui, &mut swap, text::dialog_settings_swap()).clicked() {
            frame.settings.swap_windows = swap;
            edited = true;
        }
    });
    dialog_row(ui, text::dialog_settings_theme(), |ui| {
        // DESIGN.md 2 gives two themes and no place to pick one, so the
        // choice sits here, beside the other settings about the screens.
        let mut mode = frame.settings.theme;
        let choices = [
            (theme::Mode::Light, theme::Mode::Light.label()),
            (theme::Mode::Dark, theme::Mode::Dark.label()),
        ];
        if widget::segmented(ui, &mut mode, &choices) {
            frame.settings.theme = mode;
            edited = true;
        }
        let _ = tokens;
    });
    dialog_row(ui, text::dialog_settings_language(), |ui| {
        edited |= language_field(ui, &mut frame.settings.language);
    });
    dialog_row(ui, text::dialog_settings_scale(), |ui| {
        let mut scale = scale_drag.unwrap_or(frame.settings.ui_scale);
        let percent = text::dialog_settings_scale_value((scale * 100.0).round());
        let response = widget::slider(
            ui,
            &mut scale,
            theme::MIN_SCALE..=theme::MAX_SCALE,
            180.0,
            &percent,
        );
        // The window keeps its size while the button is down. A window that
        // rescaled under the hand would move the slider away from the
        // pointer, and the value would run to one end on its own. The
        // number beside the track follows the drag, so the DM still sees
        // where the knob stands.
        if response.is_pointer_button_down_on() {
            // A step of five percent, so the value stays a round number.
            *scale_drag = Some((scale * 20.0).round() / 20.0);
        } else if let Some(picked) = scale_drag.take() {
            frame.settings.ui_scale = picked;
            edited = true;
        }
        widget::helper(ui, text::dialog_settings_scale_helper());
    });
    edited
}

/// The Grid tab of Settings: the colors of the canvas and the grid.
///
/// Every choice here belongs to the theme the window draws, so a DM who
/// works in the dark theme and shows the light one keeps a grid they can
/// see in both. Returns `true` when the DM changed something. DESIGN.md
/// 9.2. Issue #66.
fn grid_tab(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mode = settings.theme;
    let mut edited = false;
    let paper = settings.paper_mut();
    dialog_row(ui, text::dialog_settings_canvas(), |ui| {
        let mut color = rgb_of(paper.canvas(mode));
        ui.horizontal(|ui| {
            if widget::color_swatch_rgb(ui, &mut color) {
                paper.canvas = Some(color);
                edited = true;
            }
            if reset(ui, paper.canvas.is_some()) {
                paper.canvas = None;
                edited = true;
            }
        });
        widget::helper(ui, text::dialog_settings_canvas_helper());
    });
    dialog_row(ui, text::dialog_settings_line(), |ui| {
        let mut automatic = paper.automatic;
        let choices = [
            (true, text::dialog_settings_line_auto()),
            (false, text::dialog_settings_line_chosen()),
        ];
        if widget::segmented(ui, &mut automatic, &choices) {
            paper.automatic = automatic;
            edited = true;
        }
        widget::helper(ui, text::dialog_settings_line_helper());
    });
    // A chosen color has nothing to say while the line takes its own, so
    // the row goes away instead of standing there greyed out.
    if !paper.automatic {
        dialog_row(ui, text::dialog_settings_line_color(), |ui| {
            let token = mode.tokens().grid.0;
            let mut color = paper.line.unwrap_or([
                (token >> 16) as u8,
                (token >> 8) as u8,
                u8::try_from(token & 0xff).unwrap_or(u8::MAX),
            ]);
            ui.horizontal(|ui| {
                if widget::color_swatch_rgb(ui, &mut color) {
                    paper.line = Some(color);
                    edited = true;
                }
                if reset(ui, paper.line.is_some()) {
                    paper.line = None;
                    edited = true;
                }
            });
        });
    }
    dialog_row(ui, text::dialog_settings_line_width(), |ui| {
        let mut width = f64::from(paper.width_of());
        if widget::input(
            ui,
            &mut width,
            text::unit_points(),
            90.0,
            MIN_GRID_WIDTH..=MAX_GRID_WIDTH,
            0.1,
        )
        .changed()
        {
            paper.width = Some(width as f32);
            edited = true;
        }
    });
    dialog_row(ui, text::dialog_settings_line_opacity(), |ui| {
        let mut opacity = f64::from(paper.opacity_of(mode));
        let shown = text::dialog_settings_opacity_value((opacity * 100.0).round());
        if widget::slider(ui, &mut opacity, 0.0..=1.0, 180.0, &shown).changed() {
            // The row shows whole percent, so it keeps whole percent. A
            // config file a DM opens holds 0.77, not 0.7713873.
            paper.opacity = Some((opacity * 100.0).round() as f32 / 100.0);
            edited = true;
        }
    });
    not_built(ui, text::dialog_settings_grid_soon());
    edited
}

/// The Reset button of a Grid tab row. It gives the token back.
///
/// The button stands in every row, so the rows keep one shape, and it
/// takes no click while the row already shows the token.
fn reset(ui: &mut egui::Ui, live: bool) -> bool {
    ui.add_enabled_ui(live, |ui| {
        widget::button(ui, text::dialog_settings_reset(), None, widget::Height::Row).clicked()
    })
    .inner
}

/// The red, green and blue of a color, without its alpha.
fn rgb_of(color: egui::Color32) -> [u8; 3] {
    [color.r(), color.g(), color.b()]
}

/// The Language row of the Table tab. DESIGN.md 9.1.
///
/// Every language the program carries names itself in its own words, so a
/// DM finds their own without reading English first. Returns `true` when
/// the DM picked another one.
fn language_field(ui: &mut egui::Ui, picked: &mut String) -> bool {
    let shown = text::language(picked).map_or(text::DEFAULT, |language| language.name);
    let field = widget::select_field(ui, shown, widget::SELECT_WIDTH);
    let mut changed = false;
    egui::Popup::menu(&field)
        .gap(-1.0)
        .width(widget::SELECT_WIDTH)
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for language in text::LANGUAGES {
                let here = language.code == picked;
                if widget::select_row(ui, language.name, here).clicked() {
                    language.code.clone_into(picked);
                    changed = true;
                }
            }
        });
    changed
}

/// What dmap carries, and the terms it comes under. DESIGN.md 9.5.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum License {
    /// The program itself.
    #[default]
    Program,
    /// Atkinson Hyperlegible, which DESIGN.md 3 gives the window.
    Font,
    /// Fira Sans, which holds the letters Atkinson lacks. DESIGN.md 3.
    Fallback,
    /// The Lucide glyphs of DESIGN.md 4.
    Icons,
}

/// The text of the GPL, which the program comes under.
const GPL: &str = include_str!("../../../../LICENSE");

/// The text of the SIL Open Font License, which the font comes under.
const OFL: &str = include_str!("../../assets/fonts/LICENSE-OFL.txt");

/// The same license again, as the second font carries its own copy.
const OFL_FALLBACK: &str = include_str!("../../assets/fonts/LICENSE-OFL-FiraSans.txt");

/// The text of the ISC license, which the glyphs come under.
const ISC: &str = include_str!("../../assets/icons/LICENSE-ISC.txt");

/// The people who built dmap, one to a line.
///
/// A line that starts with a hash is a note in the file, not a name.
const CONTRIBUTORS: &str = include_str!("../../../../CONTRIBUTORS");

/// Where a DM takes a bug, a story or a patch.
const ISSUES: &str = "https://github.com/engels-hub/dmap/issues";

impl License {
    /// The name of the thing, what it comes under, and where it lives.
    fn about(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Program => ("dmap", "GPL-3.0-only", "https://github.com/engels-hub/dmap"),
            Self::Font => (
                "Atkinson Hyperlegible",
                "SIL Open Font License 1.1",
                "https://github.com/googlefonts/atkinson-hyperlegible",
            ),
            Self::Fallback => (
                "Fira Sans",
                "SIL Open Font License 1.1",
                "https://github.com/mozilla/Fira",
            ),
            Self::Icons => (
                "Lucide",
                "ISC License",
                "https://github.com/lucide-icons/lucide",
            ),
        }
    }

    /// The whole text of the license.
    fn text(self) -> &'static str {
        match self {
            Self::Program => GPL,
            Self::Font => OFL,
            Self::Fallback => OFL_FALLBACK,
            Self::Icons => ISC,
        }
    }
}

/// The About tab of DESIGN.md 9.5.
///
/// The text of every license is built into the program. A link alone
/// would not do: the GPL asks that a copy reach every person who gets the
/// program, and the font and the glyphs ask that their notice travel with
/// them. The font is compiled in, so its license has nowhere else to go.
fn about_tab(ui: &mut egui::Ui, picked: &mut License, tokens: Tokens) {
    // This tab is a page to read, not a row of controls to fill in, so its
    // lines sit closer together than the `ROW_GAP` of DESIGN.md 9. The
    // whole of it has to fit over the license text.
    ui.spacing_mut().item_spacing.y = 6.0;
    widget::row_label(ui, &text::dialog_about_version(env!("CARGO_PKG_VERSION")));
    widget::helper(ui, text::dialog_about_tagline());
    for one in [
        License::Program,
        License::Font,
        License::Fallback,
        License::Icons,
    ] {
        let (name, terms, url) = one.about();
        // A language whose words run long takes the link to the next
        // line instead of past the edge of the dialog.
        ui.horizontal_wrapped(|ui| {
            widget::row_label(ui, &text::dialog_about_licensed(name, terms));
            ui.hyperlink_to(
                egui::RichText::new(text::dialog_about_source())
                    .font(theme::font(theme::SMALL, false))
                    .color(tokens.accent),
                url,
            );
        });
    }
    ui.add_space(4.0);
    widget::row_label(ui, text::dialog_about_built_by());
    let names: Vec<&str> = CONTRIBUTORS
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    widget::helper(ui, &names.join(", "));
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        widget::helper(ui, text::dialog_about_free());
        ui.hyperlink_to(
            egui::RichText::new(text::dialog_about_bring())
                .font(theme::font(theme::SMALL, false))
                .color(tokens.accent),
            ISSUES,
        );
    });
    ui.add_space(4.0);
    // Every one of these is a name, and a name reads the same in every
    // language, so no key stands behind them.
    let choices = [
        (License::Program, "dmap"),
        (License::Font, "Atkinson"),
        (License::Fallback, "Fira"),
        (License::Icons, "Lucide"),
    ];
    widget::segmented(ui, picked, &choices);
    // The text takes the room that is left, so the tab needs no scroll of
    // its own around the one the text already has. Two scrolls in a
    // column leave a DM guessing which one a wheel turns.
    let room = (ui.available_height() - PANEL_PAD).max(ROW_HEIGHT * 3.0);
    egui::ScrollArea::vertical()
        .max_height(room)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(picked.text())
                    .font(theme::font(theme::SMALL, false))
                    .color(tokens.mute),
            );
        });
}

/// One line that says a tab waits for its story. DESIGN.md 9.
fn not_built(ui: &mut egui::Ui, line: &str) {
    widget::helper(ui, line);
}

/// The scenes dialog of DESIGN.md 9.6.
///
/// Returns what the DM asked for. The program does the work, so an error
/// on the disk has one place to go.
pub(super) fn scenes_dialog(
    ui: &egui::Ui,
    scenes: &mut Scenes,
    frame: &Frame<'_>,
    tokens: Tokens,
) -> Option<SceneCommand> {
    if !scenes.open {
        return None;
    }
    let mut command = None;
    let close = dialog_frame(
        ui.ctx(),
        "scenes",
        text::dialog_scenes_title(),
        egui::vec2(660.0, 440.0),
        tokens,
        |ui, rest| {
            let footer = egui::Rect::from_min_size(
                egui::pos2(rest.left(), rest.bottom() - FOOTER),
                egui::vec2(rest.width(), FOOTER),
            );
            let body = egui::Rect::from_min_max(
                egui::pos2(rest.left() + 20.0, rest.top() + 18.0),
                egui::pos2(rest.right() - 20.0, footer.top() - 18.0),
            );
            let mut body_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(body)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            body_ui.set_clip_rect(body);
            body_ui.spacing_mut().item_spacing.y = 10.0;
            body_ui.horizontal(|ui| {
                widget::row_label(ui, text::dialog_scenes_folder());
                let path = frame.scenes_dir.display().to_string();
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(PATH_WIDTH, widget::CONTROL),
                    egui::Sense::hover(),
                );
                ui.painter().rect(
                    rect,
                    0,
                    tokens.field,
                    tokens.hairline(),
                    egui::StrokeKind::Inside,
                );
                row_name(ui, rect.shrink2(egui::vec2(8.0, 0.0)), &path, false, tokens);
                let _ = response;
                if widget::button(ui, text::dialog_scenes_change(), None, Height::Full).clicked() {
                    command = Some(SceneCommand::ScenesFolder);
                }
            });
            if !frame.scene_error.is_empty() {
                ui.label(
                    egui::RichText::new(frame.scene_error)
                        .font(theme::font(theme::SMALL, false))
                        .color(tokens.accent),
                );
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(&mut body_ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for name in (frame.list_scenes)() {
                        // Two scenes of one name can live in two folders, so
                        // the row that stands out is the one whose folder is
                        // open.
                        let open = frame.scenes_dir.join(&name) == *frame.scene_dir;
                        scene_row(ui, scenes, open, &name, &mut command, tokens, SCENE_ROW);
                    }
                });
            let mut foot = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(footer.shrink2(egui::vec2(20.0, 0.0)))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            widget::rule_bottom(
                ui,
                egui::Rect::from_min_size(
                    egui::pos2(footer.left(), footer.top()),
                    egui::vec2(footer.width(), 0.0),
                ),
                tokens,
            );
            if widget::button(
                &mut foot,
                text::dialog_scenes_new(),
                Some(Icon::Plus),
                Height::Full,
            )
            .clicked()
            {
                command = Some(SceneCommand::New);
            }
        },
    );
    if close {
        scenes.open = false;
    }
    // A half-typed name, or a question no one answered, does not wait for
    // the next time the dialog opens.
    if command.is_some() || !scenes.open {
        scenes.renaming = None;
        scenes.deleting = None;
    }
    command
}

/// One scene in the dialog: its name, and what the DM can do to it.
///
/// DESIGN.md 9.6. Caution: Delete takes the scene folder and every map in
/// it, so the row asks the question on itself before it goes.
fn scene_row(
    ui: &mut egui::Ui,
    scenes: &mut Scenes,
    open: bool,
    name: &str,
    command: &mut Option<SceneCommand>,
    tokens: Tokens,
    height: f32,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    widget::rule_bottom(ui, rect, tokens);
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    if scenes.deleting.as_deref() == Some(name) {
        row.label(
            egui::RichText::new(text::dialog_scenes_delete_ask(name))
                .font(theme::font(theme::BODY, false))
                .color(tokens.accent),
        );
        if widget::button(
            &mut row,
            text::dialog_scenes_delete(),
            Some(Icon::Trash),
            Height::Row,
        )
        .clicked()
        {
            *command = Some(SceneCommand::Delete(name.to_owned()));
        }
        if widget::button(&mut row, text::dialog_scenes_keep(), None, Height::Row).clicked() {
            scenes.deleting = None;
        }
        return;
    }
    if let Some((from, typed)) = scenes.renaming.as_mut().filter(|(from, _)| from == name) {
        let field = row.add(
            egui::TextEdit::singleline(typed)
                .desired_width(190.0)
                .font(theme::font(theme::BODY, false)),
        );
        let done = field.lost_focus() && row.input(|i| i.key_pressed(egui::Key::Enter));
        field.request_focus();
        if done || widget::button(&mut row, text::dialog_scenes_save(), None, Height::Row).clicked()
        {
            *command = Some(SceneCommand::Rename {
                from: from.clone(),
                to: typed.clone(),
            });
        }
        if widget::button(&mut row, text::dialog_scenes_cancel(), None, Height::Row).clicked() {
            scenes.renaming = None;
        }
        return;
    }
    let color = if open { tokens.accent } else { tokens.ink };
    icon::paint(
        row.painter(),
        Icon::Folder,
        egui::pos2(rect.left() + widget::SMALL_ICON / 2.0, rect.center().y),
        widget::SMALL_ICON,
        color,
    );
    row.add_space(widget::SMALL_ICON + 8.0);
    row.label(
        egui::RichText::new(name)
            .font(theme::font(theme::BODY, open))
            .color(color),
    );
    row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |row| {
        if widget::button(row, text::dialog_scenes_delete(), None, Height::Row).clicked() {
            scenes.deleting = Some(name.to_owned());
        }
        if widget::button(row, text::dialog_scenes_reveal(), None, Height::Row).clicked() {
            *command = Some(SceneCommand::Reveal(name.to_owned()));
        }
        if widget::button(row, text::dialog_scenes_rename(), None, Height::Row).clicked() {
            scenes.renaming = Some((name.to_owned(), name.to_owned()));
        }
        if open {
            row.label(
                egui::RichText::new(text::dialog_scenes_open_now())
                    .font(theme::font(theme::BODY, false))
                    .color(tokens.mute),
            );
        } else if widget::button(row, text::dialog_scenes_open(), None, Height::Row).clicked() {
            *command = Some(SceneCommand::Open(name.to_owned()));
        }
    });
}

/// Every change the DM made, newest first. DESIGN.md 9.8.
///
/// A click on a step takes the scene to the state after that step. The
/// steps the DM took back stay on the list in `mute`, so a walk forward
/// is a click as well. Returns `true` when the scene moved.
pub(super) fn history_dialog(
    ui: &egui::Ui,
    open: &mut bool,
    frame: &mut Frame<'_>,
    tokens: Tokens,
) -> bool {
    if !*open {
        return false;
    }
    // A step the DM is still making is no step yet, and the walk would
    // write over it.
    frame.history.settle();
    let steps = frame.history.steps();
    let place = frame.history.place();
    let mut go_to = None;
    let close = dialog_frame(
        ui.ctx(),
        "history",
        text::dialog_history_title(),
        egui::vec2(520.0, 460.0),
        tokens,
        |ui, rest| {
            let footer = egui::Rect::from_min_size(
                egui::pos2(rest.left(), rest.bottom() - FOOTER),
                egui::vec2(rest.width(), FOOTER),
            );
            let body = egui::Rect::from_min_max(
                egui::pos2(rest.left() + 20.0, rest.top() + 18.0),
                egui::pos2(rest.right() - 20.0, footer.top() - 18.0),
            );
            let mut body_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(body)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            body_ui.set_clip_rect(body);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(&mut body_ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    // The newest change stands at the top, as the objects
                    // list puts the top of the pile first.
                    for (index, note) in steps.iter().enumerate().rev() {
                        let step = index + 1;
                        if step_row(ui, note, step == place, step > place, tokens) {
                            go_to = Some(step);
                        }
                    }
                    let first = Note {
                        what: text::dialog_history_start().to_owned(),
                        ..Note::default()
                    };
                    if step_row(ui, &first, place == 0, false, tokens) {
                        go_to = Some(0);
                    }
                });
            let mut foot = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(footer.shrink2(egui::vec2(20.0, 0.0)))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            widget::rule_bottom(
                ui,
                egui::Rect::from_min_size(
                    egui::pos2(footer.left(), footer.top()),
                    egui::vec2(footer.width(), 0.0),
                ),
                tokens,
            );
            widget::helper(&mut foot, text::dialog_history_helper());
        },
    );
    if close {
        *open = false;
    }
    match go_to {
        Some(step) => frame.history.walk_to(frame.scene, step),
        None => false,
    }
}

/// One step on the history list. Returns `true` when the DM clicked it.
///
/// The row holds two lines: what the DM did and what it happened to, then
/// the numbers the step wrote. The time stands on the right of the first
/// line. DESIGN.md 9.8.
///
/// The step the scene stands on takes the `accent` and the `raised`
/// background, as a picked row does. A step the DM took back draws in
/// `mute`, because it says what a walk forward would write again.
fn step_row(ui: &mut egui::Ui, note: &Note, here: bool, undone: bool, tokens: Tokens) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), STEP_ROW),
        egui::Sense::click(),
    );
    if here {
        ui.painter().rect_filled(rect, 0, tokens.raised);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(widget::BAR, rect.height())),
            0,
            tokens.accent,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0, tokens.field);
    }
    widget::rule_bottom(ui, rect, tokens);
    let color = if here {
        tokens.accent
    } else if undone {
        tokens.mute
    } else {
        tokens.ink
    };
    // The two lines sit around the middle of the row, so a row with no
    // second line still reads as one block.
    let painter = ui.painter_at(rect);
    let left = rect.left() + STEP_PAD;
    let middle = rect.center().y;
    let headline = if note.subject.is_empty() {
        note.what.clone()
    } else {
        text::history_row(&note.what, &note.subject)
    };
    painter.text(
        egui::pos2(left, middle - STEP_LINE),
        egui::Align2::LEFT_BOTTOM,
        &headline,
        theme::font(theme::BODY, here),
        color,
    );
    painter.text(
        egui::pos2(rect.right() - STEP_PAD, middle - STEP_LINE),
        egui::Align2::RIGHT_BOTTOM,
        &note.ago,
        theme::font(theme::SMALL, false),
        tokens.mute,
    );
    painter.text(
        egui::pos2(left, middle + STEP_LINE),
        egui::Align2::LEFT_TOP,
        &note.detail,
        theme::font(theme::SMALL, false),
        tokens.mute,
    );
    response.clicked()
}
