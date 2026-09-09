//! The color tokens and the type scale of DESIGN.md 2 and 3.

// Rust guideline compliant 2026-02-21

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};

/// Atkinson Hyperlegible in weight 400, with tabular figures.
///
/// `tools/fonts.py` builds the file. See DESIGN.md 3.
const REGULAR: &[u8] = include_bytes!("../assets/fonts/AtkinsonHyperlegible-Regular.ttf");

/// Atkinson Hyperlegible in weight 700, with tabular figures.
const BOLD: &[u8] = include_bytes!("../assets/fonts/AtkinsonHyperlegible-Bold.ttf");

/// The name the bold family takes in `egui`.
const BOLD_FAMILY: &str = "bold";

/// Where `install` leaves the theme, so any widget can read its tokens.
const MODE_KEY: &str = "dmap-theme";

/// The size of a toolbar label, a helper line and small print, in points.
///
/// The floor of DESIGN.md 1. No text in the window goes under it.
pub const SMALL: f32 = 13.0;

/// The size of control text, a row label and dialog navigation, in points.
pub const BODY: f32 = 14.0;

/// The size of a dialog title, in points. DESIGN.md 3.
pub const TITLE: f32 = 16.0;

/// The size of a panel title, in points. DESIGN.md 7.1.
pub const PANEL_TITLE: f32 = 14.0;

/// The smallest interface scale the DM can pick. DESIGN.md 3.1.
pub const MIN_SCALE: f64 = 0.75;

/// The largest interface scale the DM can pick. DESIGN.md 3.1.
pub const MAX_SCALE: f64 = 1.75;

/// The scale a new install opens at: the sizes DESIGN.md writes down.
pub const DEFAULT_SCALE: f64 = 1.0;

/// Holds an interface scale inside the range DESIGN.md 3.1 allows.
///
/// A scale that is not a number falls back to the default, so a
/// hand-edited config file cannot leave the window unreadable.
pub fn clamp_scale(scale: f64) -> f64 {
    if scale.is_finite() {
        scale.clamp(MIN_SCALE, MAX_SCALE)
    } else {
        DEFAULT_SCALE
    }
}

/// Which theme the window draws. DESIGN.md 2.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Cream surfaces, dark brown ink, vermilion accent.
    #[default]
    Light,
    /// Near-black surfaces, warm grey text, amber accent.
    Dark,
}

impl Mode {
    /// The tokens of this theme.
    pub fn tokens(self) -> Tokens {
        match self {
            Self::Light => LIGHT,
            Self::Dark => DARK,
        }
    }

    /// The name of the theme, for a segmented control.
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

/// One column of the token table of DESIGN.md 2.
///
/// Every name here is the name the table gives it. A screen that needs a
/// color takes it from this struct and never writes a hex value of its own.
#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    /// Toolbar, panel and dialog surface.
    pub surface: Color32,
    /// The canvas background the program falls back to.
    pub canvas: Color32,
    /// Active tool, selected segment and active navigation entry.
    pub raised: Color32,
    /// Input, select, button and checkbox background.
    pub field: Color32,
    /// Text, icons and the border of a dialog.
    pub ink: Color32,
    /// Secondary text, units and helper lines.
    pub mute: Color32,
    /// The color of a 1 px border or divider.
    pub rule: Color32,
    /// The canvas grid lines: an sRGB value and how solid a line is.
    ///
    /// The GPU draws the grid, not `egui`, so this pair stays straight
    /// and never comes premultiplied.
    pub grid: (u32, f32),
    /// The one accent of DESIGN.md 1.
    pub accent: Color32,
    /// The wash over the canvas outside the TV box.
    pub dim: Color32,
    /// The wash behind a dialog.
    pub scrim: Color32,
    /// The hard offset shadow, or `None` when the theme has none.
    pub shadow: Option<Color32>,
    /// Whether this theme is the dark one.
    pub dark: bool,
}

/// The light theme of DESIGN.md 2. The program opens with it.
pub const LIGHT: Tokens = Tokens {
    surface: Color32::from_rgb(0xec, 0xe4, 0xd2),
    canvas: Color32::from_rgb(0xe3, 0xd9, 0xc3),
    raised: Color32::from_rgb(0xe3, 0xd9, 0xc3),
    field: Color32::from_rgb(0xf5, 0xef, 0xe2),
    ink: Color32::from_rgb(0x2b, 0x24, 0x19),
    mute: Color32::from_rgb(0x7d, 0x74, 0x62),
    rule: Color32::from_rgb(0xb9, 0xad, 0x92),
    grid: (0x9f_b1_c4, 0.33),
    accent: Color32::from_rgb(0xb4, 0x45, 0x2c),
    // `rgba(43, 36, 25, 0.12)`, multiplied by 31/255.
    dim: Color32::from_rgba_premultiplied(5, 4, 3, 31),
    // `rgba(43, 36, 25, 0.35)`, multiplied by 89/255.
    scrim: Color32::from_rgba_premultiplied(15, 13, 9, 89),
    shadow: Some(Color32::from_rgb(0xb9, 0xad, 0x92)),
    dark: false,
};

/// The dark theme of DESIGN.md 2.
pub const DARK: Tokens = Tokens {
    surface: Color32::from_rgb(0x19, 0x1a, 0x1c),
    canvas: Color32::from_rgb(0x10, 0x11, 0x12),
    raised: Color32::from_rgb(0x2c, 0x2e, 0x31),
    field: Color32::from_rgb(0x1f, 0x21, 0x24),
    ink: Color32::from_rgb(0xdc, 0xd8, 0xcf),
    mute: Color32::from_rgb(0x8a, 0x87, 0x7f),
    rule: Color32::from_rgb(0x2c, 0x2e, 0x31),
    grid: (0x2c_2e_31, 1.0),
    accent: Color32::from_rgb(0xf0, 0xa8, 0x30),
    // `rgba(16, 17, 18, 0.45)`, multiplied by 115/255.
    dim: Color32::from_rgba_premultiplied(7, 8, 8, 115),
    // `rgba(0, 0, 0, 0.5)`, multiplied by 128/255.
    scrim: Color32::from_rgba_premultiplied(0, 0, 0, 128),
    shadow: None,
    dark: true,
};

impl Tokens {
    /// The grid line in linear light, with straight alpha, for the GPU.
    ///
    /// The surface is sRGB, so the shader must be given linear values for
    /// the line to draw the color the table names.
    pub fn grid_line(self) -> [f32; 4] {
        let (rgb, alpha) = self.grid;
        [
            crate::color::linear_from_srgb((rgb >> 16) as u8) as f32,
            crate::color::linear_from_srgb((rgb >> 8) as u8) as f32,
            crate::color::linear_from_srgb(rgb as u8) as f32,
            alpha,
        ]
    }

    /// A 1 px `rule` border.
    pub fn hairline(self) -> Stroke {
        Stroke::new(1.0, self.rule)
    }

    /// The offset of the hard shadow of DESIGN.md 1, in points.
    ///
    /// Returns `None` in a theme that has no shadow value.
    pub fn shadow_offset(self) -> Option<egui::Vec2> {
        self.shadow.map(|_| egui::vec2(2.0, 3.0))
    }
}

/// Loads the fonts and points every `egui` style at `mode`.
///
/// Call it once for each context, and again when the DM picks the other
/// theme. It replaces the whole style, so nothing of the `egui` default
/// survives.
pub fn install(ctx: &egui::Context, mode: Mode) {
    fonts(ctx);
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(MODE_KEY), mode));
    let tokens = mode.tokens();
    // The program owns both themes, so `egui` gets the same style whichever
    // one it thinks is current. Nothing then depends on the system setting.
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    style.text_styles = text_styles();
    style.spacing.item_spacing = egui::vec2(6.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 0.0);
    style.spacing.interact_size = egui::vec2(28.0, 28.0);
    style.spacing.icon_width = 18.0;
    style.spacing.icon_width_inner = 14.0;
    style.spacing.icon_spacing = 8.0;
    style.spacing.menu_margin = egui::Margin::symmetric(0, 4);
    style.spacing.window_margin = egui::Margin::same(0);
    // DESIGN.md 1 bans the rounded corner, so no radius survives anywhere.
    style.visuals = visuals(tokens);
    let style = std::sync::Arc::new(style);
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        ctx.set_style_of(theme, std::sync::Arc::clone(&style));
    }
}

/// The tokens the context carries, as `install` last left them.
pub fn of(ctx: &egui::Context) -> Tokens {
    ctx.data(|data| data.get_temp::<Mode>(egui::Id::new(MODE_KEY)))
        .unwrap_or_default()
        .tokens()
}

/// The four sizes of DESIGN.md 3, in the `egui` text styles.
fn text_styles() -> std::collections::BTreeMap<TextStyle, FontId> {
    let bold = FontFamily::Name(BOLD_FAMILY.into());
    [
        (
            TextStyle::Small,
            FontId::new(SMALL, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(BODY, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(BODY, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(SMALL, FontFamily::Proportional),
        ),
        (TextStyle::Heading, FontId::new(TITLE, bold)),
    ]
    .into_iter()
    .collect()
}

/// A font that carries a weight of DESIGN.md 3.
///
/// The bold weight is its own family, because `egui` picks a face by family
/// and never synthesizes a weight.
pub fn font(size: f32, bold: bool) -> FontId {
    if bold {
        FontId::new(size, FontFamily::Name(BOLD_FAMILY.into()))
    } else {
        FontId::new(size, FontFamily::Proportional)
    }
}

/// Puts Atkinson Hyperlegible in front of every family.
fn fonts(ctx: &egui::Context) {
    let mut definitions = FontDefinitions::default();
    definitions.font_data.insert(
        "atkinson".to_owned(),
        std::sync::Arc::new(FontData::from_static(REGULAR)),
    );
    definitions.font_data.insert(
        "atkinson-bold".to_owned(),
        std::sync::Arc::new(FontData::from_static(BOLD)),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        definitions
            .families
            .entry(family)
            .or_default()
            .insert(0, "atkinson".to_owned());
    }
    definitions.families.insert(
        FontFamily::Name(BOLD_FAMILY.into()),
        vec!["atkinson-bold".to_owned()],
    );
    ctx.set_fonts(definitions);
}

/// The `egui` visuals that carry `tokens`.
fn visuals(tokens: Tokens) -> egui::Visuals {
    let mut visuals = if tokens.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    let square = CornerRadius::ZERO;
    visuals.override_text_color = Some(tokens.ink);
    visuals.panel_fill = tokens.surface;
    visuals.window_fill = tokens.surface;
    visuals.window_stroke = Stroke::new(1.0, tokens.ink);
    visuals.window_corner_radius = square;
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    visuals.menu_corner_radius = square;
    visuals.popup_shadow = egui::epaint::Shadow::NONE;
    visuals.faint_bg_color = tokens.raised;
    visuals.extreme_bg_color = tokens.field;
    visuals.code_bg_color = tokens.field;
    visuals.hyperlink_color = tokens.accent;
    visuals.warn_fg_color = tokens.accent;
    visuals.error_fg_color = tokens.accent;
    visuals.selection.bg_fill = tokens.accent.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, tokens.ink);
    // The wash behind a dialog. DESIGN.md 9.
    visuals.window_highlight_topmost = false;
    for state in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        state.corner_radius = square;
        state.bg_fill = tokens.field;
        state.weak_bg_fill = tokens.field;
        state.bg_stroke = tokens.hairline();
        state.fg_stroke = Stroke::new(1.0, tokens.ink);
        state.expansion = 0.0;
    }
    visuals.widgets.noninteractive.bg_fill = tokens.surface;
    visuals.widgets.noninteractive.weak_bg_fill = tokens.surface;
    visuals.widgets.hovered.weak_bg_fill = tokens.raised;
    visuals.widgets.active.weak_bg_fill = tokens.raised;
    visuals.widgets.open.weak_bg_fill = tokens.raised;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, tokens.ink);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, tokens.accent);
    visuals
}

#[cfg(test)]
mod tests {
    use super::{DARK, LIGHT, Mode};

    #[test]
    fn a_scale_stays_inside_the_range() {
        assert!((super::clamp_scale(1.0) - 1.0).abs() < 1e-9);
        assert!((super::clamp_scale(0.1) - super::MIN_SCALE).abs() < 1e-9);
        assert!((super::clamp_scale(9.0) - super::MAX_SCALE).abs() < 1e-9);
    }

    #[test]
    fn a_scale_that_is_not_a_number_takes_the_default() {
        // A hand-edited config file must never leave the window unreadable.
        assert!((super::clamp_scale(f64::NAN) - super::DEFAULT_SCALE).abs() < 1e-9);
        assert!((super::clamp_scale(f64::INFINITY) - super::DEFAULT_SCALE).abs() < 1e-9);
    }

    #[test]
    fn the_light_theme_is_the_default() {
        assert_eq!(Mode::default(), Mode::Light);
        assert!(!Mode::Light.tokens().dark);
        assert!(Mode::Dark.tokens().dark);
    }

    #[test]
    fn each_mode_gives_its_own_tokens() {
        assert_eq!(Mode::Light.tokens().accent, LIGHT.accent);
        assert_eq!(Mode::Dark.tokens().accent, DARK.accent);
    }

    #[test]
    fn only_the_light_theme_has_a_shadow() {
        assert!(LIGHT.shadow.is_some());
        assert!(LIGHT.shadow_offset().is_some());
        assert!(DARK.shadow.is_none());
        assert!(DARK.shadow_offset().is_none());
    }
}
