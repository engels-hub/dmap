//! The DM window's UI: egui on top of the wgpu pane.
//!
//! This module holds the state the whole window shares, the frame loop
//! and the sizes every part reads. One submodule stands for each part of
//! the window: `toolbar`, `panel`, `tree`, `canvas`, `draw` and `dialog`.

// Rust guideline compliant 2026-02-21

mod canvas;
mod dialog;
mod draw;
mod panel;
mod toolbar;
mod tree;

use std::path::Path;

use anyhow::Result;
use egui_wgpu::wgpu;
use egui_winit::winit::{event::WindowEvent, monitor::MonitorHandle};

use crate::camera::Camera;
use crate::color;
use crate::command::History;
use crate::config::Paper;
use crate::gpu::{Gpu, Pane, begin_clear_pass};
use crate::icons::Icon;
use crate::scene::{Asset, NodeId, Placed, Scene};
use crate::stroke::{Ink, Rule, Stroke};
use crate::text;
use crate::theme;
use crate::tvbox::TvBox;

pub use dialog::Tab;
pub use draw::measure_overlay;

use canvas::{canvas_area, frame_tv_box, select_tool, table_tool, undo_keys};
use dialog::{Dialog, history_dialog, scenes_dialog, settings_dialog};
use draw::{Draw, draw_tool};
use panel::{Views, objects_panel, properties_panel, say_lengths};
use toolbar::{Press, history_button, toolbar};
use tree::Tree;

/// The gap between the chrome and the window edge, in points. DESIGN.md 5.3.
const MARGIN: f32 = 12.0;

/// The height of a toolbar entry, in points. DESIGN.md 5.2.
const TOOL_HEIGHT: f32 = 44.0;

/// The width of a toolbar entry, in points. DESIGN.md 5.2.
///
/// An entry grows past this when its label needs the room. DESIGN.md 1
/// puts legibility first, so the label never runs into its neighbour.
const TOOL_WIDTH: f32 = 52.0;

/// The padding beside a toolbar label that outgrows its entry, in points.
const TOOL_PAD: f32 = 8.0;

/// The gap between the views and the rest of the toolbar, in points.
///
/// DESIGN.md 5.2. The two groups read differently: one of the views is
/// always on, and none of the others ever is. A rule between them said
/// that too quietly, so they stand in boxes of their own.
const TOOL_GROUP_GAP: f32 = 8.0;

/// The size of a toolbar glyph, in points. DESIGN.md 4.
const TOOL_ICON: f32 = 18.0;

/// The gap between a toolbar glyph and its label, in points. DESIGN.md 5.2.
const TOOL_GAP: f32 = 3.0;

/// The width a panel opens at, in points. DESIGN.md 8.4.
const PANEL_WIDTH: f32 = 240.0;

/// The widest a panel goes when the DM drags its edge. DESIGN.md 8.4.
const PANEL_MAX: f32 = 480.0;

/// The height of a panel header, in points. DESIGN.md 7.1.
const PANEL_HEADER: f32 = 32.0;

/// The padding inside the body of a panel, in points. DESIGN.md 7.1.
const PANEL_PAD: f32 = 10.0;

/// The height of one row of the objects list, in points. DESIGN.md 8.4.
const ROW_HEIGHT: f32 = 26.0;

/// How far a child row stands from its parent, in points. DESIGN.md 8.4.
const ROW_INDENT: f32 = 14.0;

/// The size of the glyph that opens a group, in points. DESIGN.md 4.
const TWIST: f32 = 14.0;

/// The square that holds a switch on a list row, in points. DESIGN.md 8.4.
const SWITCH: f32 = 22.0;

/// The height of the path line over the objects list. DESIGN.md 8.4.
const PATH_HEIGHT: f32 = 24.0;

/// How many parts a path shows before it drops its middle. DESIGN.md 8.4.
const PATH_PARTS: usize = 4;

/// The size of the glyph between two parts of a path. DESIGN.md 4.
const PATH_GLYPH: f32 = 13.0;

/// The size of a dialog, in points. DESIGN.md 9.
///
/// The low end of the range. A label too long for the control column
/// wraps, so no row needs the extra width.
const DIALOG: egui::Vec2 = egui::vec2(660.0, 440.0);

/// The height of a dialog header, in points. DESIGN.md 9.
const DIALOG_HEADER: f32 = 42.0;

/// The width of the navigation column of a dialog, in points. DESIGN.md 9.
const DIALOG_NAV: f32 = 168.0;

/// The width of the label column in a dialog body, in points. DESIGN.md 9.
const LABEL_COLUMN: f32 = 140.0;

/// The gap between two rows of a dialog body, in points. DESIGN.md 9.
const ROW_GAP: f32 = 16.0;

/// The height of the footer of a dialog, in points. DESIGN.md 9.
const FOOTER: f32 = 48.0;

/// The height of one step row in the history dialog, in points.
///
/// DESIGN.md 9.8 gives a step two lines: what the DM did, then what the
/// step wrote.
const STEP_ROW: f32 = 46.0;

/// Half the gap between the two lines of a step row, in points.
const STEP_LINE: f32 = 3.0;

/// The padding on each side of a step row, in points. DESIGN.md 9.8.
const STEP_PAD: f32 = 12.0;

/// The height of one scene row, in points. DESIGN.md 9.6.
const SCENE_ROW: f32 = 40.0;

/// The width of the scenes folder input, in points. DESIGN.md 9.6.
const PATH_WIDTH: f32 = 360.0;

/// How far the eraser reaches from the pointer, in inches.
///
/// A fifth of an inch is a fifth of a grid cell: wide enough to rub a
/// line out with one pass, narrow enough to take a bite out of one.
const ERASER_REACH: f64 = 0.2;

/// How many feet a grid cell stands for. PLAN.md 1.
const FEET_PER_CELL: f64 = 5.0;

/// The gap between a ruler label and the point it belongs to, in points.
const LABEL_GAP: f32 = 6.0;

/// The padding inside a ruler label, in points.
const LABEL_PAD: f32 = 4.0;

/// The side of one square in the row of nibs, in points. DESIGN.md 8.3.
const NIB_SQUARE: f32 = 28.0;

/// How wide the width slider draws in the Draw panel, in points.
const PANEL_SLIDER: f32 = 150.0;

/// The least a shape may reach before it counts as one, in inches.
///
/// A press and a release on one spot is a click, not a drag, and the
/// hand moves a little between the two. Issue #12.
const MIN_SHAPE: f64 = 0.2;

/// How far from a stroke a press still takes it, in inches.
///
/// A thin line is hard to hit exactly, so the press reaches a tenth of a
/// cell past the edge of it.
const PICK_REACH: f64 = 0.1;

/// The shortest and the longest an area of effect may reach, in cells.
const MIN_REACH: f64 = 0.5;
const MAX_REACH: f64 = 200.0;

/// The thinnest and the thickest a stroke may draw, in inches.
///
/// A fiftieth of an inch is a hair at true size, and half an inch covers
/// a tenth of a grid cell. Wider than that is a fill, not a mark.
const MIN_INK_WIDTH: f64 = 0.02;
const MAX_INK_WIDTH: f64 = 0.5;

/// How far the pointer moves before a pen keeps another point, in inches.
///
/// A hundredth of an inch is under a pixel at the zoom a table works at,
/// so the line reads as smooth and holds a tenth of the points.
const PEN_STEP: f64 = 0.01;

/// Size of a corner handle in points.
const HANDLE_SIZE: f32 = 8.0;

/// How far a corner handle stands outside the TV box, in points.
///
/// DESIGN.md 5.4.
const HANDLE_STANDOFF: f32 = 4.0;

/// How far a click may miss a handle and still grab it, in points.
const HANDLE_REACH: f64 = 10.0;

/// Distance of the rotation handle from the top edge, in points.
const ROTATION_HANDLE_OFFSET: f64 = 24.0;

/// How far the dashed box of a group stands from what it holds, in points.
const GROUP_MARGIN: f32 = 6.0;

/// How wide the edge of a group box is for a click, in points.
const GROUP_REACH: f32 = 5.0;

/// The dash and the gap of a group box, in points.
const DASH: f32 = 6.0;

/// The dash and the gap between two segments of the toolbar, in points.
///
/// Shorter than the dash of a group box: the line is 44 points tall, and a
/// 6 point dash would leave it four marks that read as a broken border.
const SEGMENT_DASH: f32 = 3.0;

/// The smallest size the properties accept, in percent.
///
/// The same floor the `-` key keeps, so a map can never vanish.
const MIN_PERCENT: f64 = 1.0;

/// The largest size the properties accept, in percent.
const MAX_PERCENT: f64 = 10_000.0;

/// Size of the zoom label on the TV box, in points. DESIGN.md 5.4.
const ZOOM_LABEL_SIZE: f32 = 14.0;

/// Gap between the top edge of the box and its zoom label, in points.
const ZOOM_LABEL_GAP: f32 = 6.0;

/// The share of the canvas the TV box takes when `T` frames it.
const FRAME_MARGIN: f64 = 0.9;

/// How much one press of the zoom keys changes the DM zoom.
const KEY_ZOOM_STEP: f64 = 1.1;

/// Zoom in. Figma, a browser and touchegg all send this for a pinch.
const ZOOM_IN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Plus);

/// Zoom in from the main row, where `+` needs Shift and `=` does not.
const ZOOM_IN_EQUALS: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Equals);

/// Zoom out.
const ZOOM_OUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Minus);

/// Takes the last change to the scene back.
const UNDO: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);

/// Writes the last change the DM took back again.
const REDO: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::Z,
);

/// The same, for a DM who learned redo in a Windows program.
const REDO_Y: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);

/// Back to the zoom a new project opens with.
const ZOOM_RESET: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num0);

/// The smallest and the largest zoom the box properties accept, in percent.
const MIN_ZOOM_PERCENT: f64 = 5.0;
const MAX_ZOOM_PERCENT: f64 = 500.0;

/// egui state and renderer for one window.
pub struct DmUi {
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    tool: Tool,
    select: Select,
    draw: Draw,
    table: Table,
    scenes: Scenes,
    tree: Tree,
    /// The settings dialog and the tab it shows.
    dialog: Dialog,
    /// Whether the history dialog stands. DESIGN.md 9.8.
    history_open: bool,
    /// The theme the context carries, so a change installs once.
    theme: theme::Mode,
    /// The language the catalog holds, for the same reason.
    language: String,
    /// The interface scale the context carries, for the same reason.
    ui_scale: f64,
    /// The DM asked to see the whole TV box. The next frame acts on it.
    frame_box: bool,
    /// Who takes the zoom gesture that is running: the TV box, or the
    /// camera. `None` while no gesture runs.
    zoom_goes_to: Option<bool>,
    /// A map or the TV box changed since the last save.
    dirty: bool,
}

/// The tool the DM works with. The rail picks it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    /// Pick a map and move, turn, scale or flip it.
    #[default]
    Select,
    /// Draw on the canvas with a pen, a shape, an eraser or a ruler.
    Draw,
    /// Drag the box that decides what the TV shows.
    Table,
}

/// What the Draw view does with a drag. DESIGN.md 8.3.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Nib {
    /// A free line that follows the pointer.
    #[default]
    Pen,
    /// A straight line from the press to the pointer.
    Line,
    /// A box between the press and the pointer.
    Rect,
    /// The ellipse that fills that box.
    Ellipse,
    /// Takes a bite out of every stroke it passes over.
    Eraser,
    /// Measures a distance, and keeps it when the DM holds Shift.
    Ruler,
    /// A round area of effect, dragged from its middle to its edge.
    Burst,
    /// A cone of effect, dragged from its point to where it ends.
    Cone,
    /// A straight run of effect, one cell wide.
    Beam,
}

impl Nib {
    /// The kind of stroke this nib leaves behind, if it leaves one.
    fn ink(self) -> Option<Ink> {
        match self {
            Self::Pen => Some(Ink::Pen),
            Self::Line => Some(Ink::Line),
            Self::Rect => Some(Ink::Rect),
            Self::Ellipse => Some(Ink::Ellipse),
            Self::Ruler => Some(Ink::Measure),
            Self::Burst => Some(Ink::Burst),
            Self::Cone => Some(Ink::Cone),
            Self::Beam => Some(Ink::Beam),
            Self::Eraser => None,
        }
    }

    /// What the panel and the history call this nib.
    fn name(self) -> &'static str {
        match self {
            Self::Eraser => text::stroke_eraser(),
            Self::Ruler => text::stroke_ruler(),
            _ => self.ink().map_or_else(text::stroke_pen, Ink::name),
        }
    }

    /// The glyph of its square in the Draw panel.
    fn glyph(self) -> Icon {
        match self {
            Self::Eraser => Icon::Eraser,
            Self::Ruler => Icon::Ruler,
            _ => self.ink().map_or(Icon::Pencil, Ink::glyph),
        }
    }
}

impl std::fmt::Debug for DmUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DmUi").finish_non_exhaustive()
    }
}

/// What the DM can change in the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Index into the display list, or `None` for a normal window.
    pub tv_display: Option<usize>,
    /// Swap window roles instead of moving the DM window. See `Project`.
    pub swap_windows: bool,
    /// How close to true size the TV box must come before it snaps, in
    /// percent.
    pub snap_percent: f64,
    /// The theme the window draws. DESIGN.md 2.
    pub theme: theme::Mode,
    /// The language the window speaks, by the name of its file.
    pub language: String,
    /// The color the Draw view paints with. Issue #12.
    pub ink_color: [u8; 4],
    /// How thick a stroke draws, in inches.
    pub ink_width: f64,
    /// Which of the six squares the Draw panel has on.
    pub ink_nib: Nib,
    /// How a ruler counts its length.
    pub ink_rule: Rule,
    /// Whether a shape starts on a crossing of the grid.
    pub ink_snap: bool,
    /// What every size of DESIGN.md is multiplied by. DESIGN.md 3.1.
    pub ui_scale: f64,
    /// The canvas and the grid of the light theme. Issue #66.
    pub paper_light: Paper,
    /// The canvas and the grid of the dark theme. Issue #66.
    pub paper_dark: Paper,
    /// What shape a cell is. Issue #15.
    ///
    /// The shape belongs to the table, not to the theme, so a DM who
    /// swaps light for dark keeps the hexes they chose.
    pub grid_kind: crate::grid::Kind,
    /// How wide a cell is, in inches. A hex measures flat to flat.
    pub grid_cell: Option<f32>,
}

impl Settings {
    /// The colors of the theme the window draws.
    /// What shape the cells are, and how wide. Issue #15.
    pub fn cells(&self) -> crate::grid::Cells {
        crate::grid::Cells {
            kind: self.grid_kind,
            cell: self.grid_cell.map_or(crate::grid::DEFAULT_CELL, f64::from),
        }
    }

    pub fn paper(&self) -> &Paper {
        match self.theme {
            theme::Mode::Light => &self.paper_light,
            theme::Mode::Dark => &self.paper_dark,
        }
    }

    /// The colors of the theme the window draws, to change.
    pub fn paper_mut(&mut self) -> &mut Paper {
        match self.theme {
            theme::Mode::Light => &mut self.paper_light,
            theme::Mode::Dark => &mut self.paper_dark,
        }
    }
    /// Whether the two settings would put the windows in different places.
    ///
    /// Only a change here may touch the windows. Full screen takes the
    /// keyboard away from the DM window, so a DM who types in the panel
    /// would lose the rest of the number.
    pub fn moves_windows(&self, other: &Self) -> bool {
        self.tv_display != other.tv_display || self.swap_windows != other.swap_windows
    }
}

/// Everything one UI frame reads and edits.
pub struct Frame<'a> {
    pub displays: &'a [MonitorHandle],
    pub settings: &'a mut Settings,
    /// The tree the DM works on.
    pub scene: &'a mut Scene,
    /// Every change the DM made to that tree. No tool writes the scene
    /// without it.
    pub history: &'a mut History,
    pub camera: &'a mut Camera,
    /// The part of the canvas the TV shows.
    /// Pixel size of the TV window. It gives the box its shape.
    pub tv_viewport: (u32, u32),
    /// Pixel size of a map's image, once loaded.
    pub size_of: &'a dyn Fn(&Path) -> Option<(u32, u32)>,
    /// The folder of the open scene.
    pub scene_dir: &'a Path,
    /// The scenes the DM can open. Read only while the dialog is open.
    pub list_scenes: &'a dyn Fn() -> Vec<String>,
    /// What went wrong with the last thing the dialog asked for.
    pub scene_error: &'a str,
    /// The folder that holds the scenes.
    pub scenes_dir: &'a Path,
}

impl std::fmt::Debug for Frame<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame").finish_non_exhaustive()
    }
}

/// The Select tool's state between frames.
#[derive(Debug, Default)]
struct Select {
    /// What the DM holds, in the order the tree draws it.
    chosen: Vec<NodeId>,
    /// The list that says what a drag over the canvas picked.
    popup: bool,
    /// What the program has to say about the last thing the DM asked.
    note: String,
    drag: Option<Drag>,
    measure: Option<Measure>,
    /// The stroke as it stood when the DM took hold of a field.
    ///
    /// A drag of a number runs over many frames, and every one of them
    /// goes back to this one state, so the whole drag is one step.
    opened_ink: Option<Stroke>,
    /// The map as it stood when the DM took hold of a field in the panel.
    ///
    /// A drag of a number runs over many frames, and every one of them
    /// goes back to this one state, so the whole drag is one step.
    opened: Option<Asset>,
}

/// What the DM asked the scenes dialog to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneCommand {
    /// Put this scene on the canvas.
    Open(String),
    /// Make a scene and open it.
    New,
    /// Give a scene another name.
    Rename { from: String, to: String },
    /// Delete a scene and everything in its folder.
    Delete(String),
    /// Ask for another folder to keep the scenes in.
    ScenesFolder,
    /// Show the scene's folder in the file manager.
    Reveal(String),
}

/// The scenes dialog between frames.
#[derive(Debug, Default)]
struct Scenes {
    open: bool,
    /// The scene the DM is renaming, and the name as typed so far.
    renaming: Option<(String, String)>,
    /// The scene the DM asked to delete, before they answered the question.
    deleting: Option<String>,
}

/// The Table tool's state between frames.
#[derive(Debug, Default)]
struct Table {
    drag: Option<BoxDrag>,
    /// The box as it stood when the DM took hold of it.
    opened: Option<TvBox>,
}

/// Maps between window points and world inches for one frame.
#[derive(Debug, Clone, Copy)]
struct View {
    camera: Camera,
    viewport: (u32, u32),
    /// Window pixels in one egui point.
    ppp: f64,
}

impl View {
    fn new(ui: &egui::Ui, camera: Camera, viewport: (u32, u32)) -> Self {
        Self {
            camera,
            viewport,
            ppp: f64::from(ui.ctx().pixels_per_point()),
        }
    }

    fn to_world(self, pos: egui::Pos2) -> (f64, f64) {
        let screen = (f64::from(pos.x) * self.ppp, f64::from(pos.y) * self.ppp);
        self.camera.screen_to_world(screen, self.viewport)
    }

    fn to_screen(self, world: (f64, f64)) -> egui::Pos2 {
        let (x, y) = self.camera.world_to_screen(world, self.viewport);
        egui::pos2((x / self.ppp) as f32, (y / self.ppp) as f32)
    }
}

/// What the primary button is doing over the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    /// Not down.
    Up,
    /// It went down this frame.
    Pressed,
    /// It is down, and it went down before this frame.
    Held,
}

/// What one canvas frame reads from the pointer.
#[derive(Debug, Clone, Copy)]
struct Pointer {
    /// What the primary button is doing.
    button: Button,
    /// Where the pointer is, in points, or `None` when it is away.
    pos: Option<egui::Pos2>,
    /// The pointer is over the canvas.
    hovered: bool,
    /// The DM is moving the camera, so the tools keep their hands off.
    panning: bool,
    /// The zoom gesture that belongs to the TV box, if this one does.
    ///
    /// One read of the keyboard decides who takes a zoom gesture, so the
    /// camera and the box can never both act on the same one.
    box_zoom: Option<f32>,
}

impl Pointer {
    /// The button went down this frame, so a tool may start something.
    fn pressed(self) -> bool {
        self.button == Button::Pressed
    }

    /// The button is down, so a drag in progress carries on.
    fn down(self) -> bool {
        self.button != Button::Up
    }
}

impl Select {
    /// The one node the DM holds, when they hold one and no more.
    ///
    /// The handles, the properties and the keys work on one node.
    fn only(&self) -> Option<NodeId> {
        (self.chosen.len() == 1).then(|| self.chosen[0])
    }

    /// Whether the DM holds this node.
    fn holds(&self, id: NodeId) -> bool {
        self.chosen.contains(&id)
    }

    /// Takes hold of one node, or adds it when `add` is true.
    fn take(&mut self, id: NodeId, add: bool) {
        if !add {
            self.chosen.clear();
            self.chosen.push(id);
            return;
        }
        if let Some(place) = self.chosen.iter().position(|held| *held == id) {
            self.chosen.remove(place);
        } else {
            self.chosen.push(id);
        }
    }
}

/// The measure tool, once the DM started it from the map properties.
#[derive(Debug, Clone, Copy)]
enum Measure {
    /// Waiting for the first cell corner.
    Start,
    /// The first cell corner, in world inches.
    From((f64, f64)),
}

/// A drag of the TV box, all positions in world inches.
#[derive(Debug, Clone, Copy)]
enum BoxDrag {
    Move {
        start_center: (f64, f64),
        start_cursor: (f64, f64),
    },
    Resize {
        start_width: f64,
        start_cursor: (f64, f64),
    },
}

/// A drag in progress, all positions in world inches.
///
/// A drag carries the maps and the drawings it holds as each one stood,
/// because every frame of it rewrites them from there. A drawing has no
/// center of its own, so what it stood as is the whole stroke. Issue #69.
#[derive(Debug, Clone)]
enum Drag {
    /// Every asset and drawing the DM holds, as each one stood.
    Move {
        was: Vec<Asset>,
        ink: Vec<Stroke>,
        start_cursor: (f64, f64),
    },
    /// A rectangle over the canvas that picks what it covers.
    Band { start_cursor: (f64, f64) },
    /// Every asset and drawing the DM holds, where it stood, and the
    /// point it turns or grows around.
    Scale {
        starts: Vec<Placed>,
        ink: Vec<Stroke>,
        pivot: (f64, f64),
        start_cursor: (f64, f64),
    },
    Rotate {
        starts: Vec<Placed>,
        ink: Vec<Stroke>,
        pivot: (f64, f64),
        start_cursor: (f64, f64),
    },
}

#[hotpath::measure_all]
impl DmUi {
    /// Builds the DM interface, on the tool and the tab of the last run.
    pub fn new(gpu: &Gpu, pane: &Pane, tool: Tool, tab: Tab) -> Self {
        let state = egui_winit::State::new(
            egui::Context::default(),
            egui::ViewportId::ROOT,
            pane.window.as_ref(),
            Some(pane.window.scale_factor() as f32),
            None,
            None,
        );
        let renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            pane.config.format,
            egui_wgpu::RendererOptions::default(),
        );
        // egui scales its own UI on Ctrl with plus, minus or zero. The
        // canvas takes those keys instead: touchegg sends them for a pinch
        // on a touchpad, and a pinch must zoom the map, not the panel.
        state
            .egui_ctx()
            .options_mut(|options| options.zoom_with_keyboard = false);
        theme::install(state.egui_ctx(), theme::Mode::default());
        Self {
            state,
            renderer,
            tool,
            select: Select::default(),
            draw: Draw::default(),
            table: Table::default(),
            scenes: Scenes::default(),
            tree: Tree::default(),
            dialog: Dialog::on_tab(tab),
            history_open: false,
            theme: theme::Mode::default(),
            language: text::DEFAULT.to_owned(),
            ui_scale: theme::DEFAULT_SCALE,
            frame_box: false,
            zoom_goes_to: None,
            dirty: false,
        }
    }

    /// Takes the theme, the language and the scale the DM asked for.
    ///
    /// Each one costs something to install, so it is installed once and
    /// not on every frame that reads it.
    fn install(&mut self, ctx: &egui::Context, settings: &Settings) {
        if self.theme != settings.theme {
            theme::install(ctx, settings.theme);
            self.theme = settings.theme;
        }
        // The window takes a language the moment the DM picks it. Every
        // word comes from the catalog while the frame draws, so no panel
        // has to be built again.
        if self.language != settings.language {
            text::use_language(&settings.language);
            self.language.clone_from(&settings.language);
        }
        // egui multiplies the scale into `pixels_per_point`, so the chrome
        // grows and the canvas math, which reads that value, stays true.
        let scale = theme::clamp_scale(settings.ui_scale);
        if (self.ui_scale - scale).abs() > f64::EPSILON {
            ctx.set_zoom_factor(scale as f32);
            self.ui_scale = scale;
        }
    }

    /// Feeds a window event to egui. Returns `true` when egui consumed it.
    pub fn on_event(&mut self, pane: &Pane, event: &WindowEvent) -> bool {
        let response = self.state.on_window_event(&pane.window, event);
        if response.repaint {
            pane.window.request_redraw();
        }
        response.consumed
    }

    /// Runs one UI frame: the rail, the settings and the Select tool.
    pub fn run(&mut self, pane: &Pane, mut frame: Frame<'_>) -> UiOutput {
        let raw_input = self.state.take_egui_input(&pane.window);
        let ctx = self.state.egui_ctx().clone();
        let viewport = (pane.config.width, pane.config.height);
        self.install(&ctx, frame.settings);
        let mut add_map = false;
        let mut edited = false;
        // Whether the DM changed a setting. It belongs to the config file
        // and never to the scene. See the settings dialog below.
        let mut settings_edited = false;
        let mut scene = None;
        let select = &mut self.select;
        let draw = &mut self.draw;
        let table = &mut self.table;
        let tool = &mut self.tool;
        let scenes = &mut self.scenes;
        let tree = &mut self.tree;
        let dialog = &mut self.dialog;
        let history_open = &mut self.history_open;
        let frame_box = &mut self.frame_box;
        let zoom_goes_to = &mut self.zoom_goes_to;
        let selected_before = select.chosen.clone();
        let output = ctx.run_ui(raw_input, |ui| {
            // A click that closes a popup must not reach the canvas.
            let popup_open = egui::Popup::is_any_open(ui.ctx());
            let tokens = theme::of(ui.ctx());
            // DESIGN.md 5: the canvas fills the window, and the chrome
            // floats over it. So the canvas takes the whole rect, and the
            // panels come after it and draw on top.
            let rect = ui.ctx().content_rect();
            let over = popup_open || scenes.open || dialog.open || *history_open;
            if !over {
                frame_tv_box(ui, &mut frame, rect, viewport, *frame_box);
                // The ask lives one frame, because the panel that raised it
                // draws after the camera reads it.
                *frame_box = false;
                // Undo works in every view, and before the tools, so a
                // tool never writes over what it put back this frame.
                let dragging = select.drag.is_some() || table.drag.is_some() || draw.busy();
                edited |= undo_keys(ui, &mut frame, dragging);
                edited |= match *tool {
                    Tool::Select => {
                        select_tool(ui, select, &mut frame, viewport, zoom_goes_to, tokens)
                    }
                    Tool::Draw => draw_tool(ui, draw, &mut frame, viewport, zoom_goes_to),
                    Tool::Table => {
                        table_tool(ui, table, &mut frame, viewport, zoom_goes_to, tokens)
                    }
                };
                say_lengths(ui, &frame, draw.live.as_ref(), viewport, tokens);
            }
            edited |= objects_panel(ui.ctx(), &mut frame, select, tree, tokens);
            let mut views = Views { select, table };
            edited |= properties_panel(ui.ctx(), &mut frame, &mut views, *tool, frame_box, tokens);
            if history_button(ui, tokens) {
                *history_open = !*history_open;
            }
            edited |= history_dialog(ui, history_open, &mut frame, tokens);
            match toolbar(ui, *tool, tokens) {
                Some(Press::View(view)) => *tool = view,
                Some(Press::Scenes) => scenes.open = !scenes.open,
                Some(Press::AddMap) => add_map = true,
                Some(Press::Settings) => dialog.open = !dialog.open,
                None => {}
            }
            // A dialog over the canvas takes the keyboard too. egui holds
            // the pointer back on its own, but `R` and the arrow keys would
            // still reach the map behind it.
            //
            // A setting is not a change to the scene, and it stays out of
            // `edited` for that reason. The scene file grows with the
            // scene, and a drag of a slider in Settings would write every
            // stroke of it again for each frame of the drag. Issue #66.
            settings_edited = settings_dialog(ui.ctx(), &mut frame, dialog, tokens);
            scene = scenes_dialog(ui, scenes, &frame, tokens);
        });
        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            viewport_output,
            ..
        } = output;
        self.state
            .handle_platform_output(&pane.window, platform_output);
        let pixels_per_point = ctx.pixels_per_point();
        let paint_jobs = ctx.tessellate(shapes, pixels_per_point);
        let repaint = viewport_output
            .values()
            .any(|viewport| viewport.repaint_delay.is_zero())
            // The settings panel draws before the canvas picks a map, so the
            // properties of a new selection need one more frame to show.
            || self.select.chosen != selected_before;

        self.settle_history(&ctx, frame.history);
        // Save once a drag is over, not on every frame of it.
        self.dirty |= edited;
        // A rub of the eraser writes the scene on every frame it cuts,
        // unless the save waits for the hand to come off it.
        let save = self.dirty
            && self.select.drag.is_none()
            && self.table.drag.is_none()
            && !self.draw.busy();
        if save {
            self.dirty = false;
        }
        UiOutput {
            live: self.draw.live.clone(),
            add_map,
            // Both screens draw the canvas, so a setting that changes how
            // it looks reaches the TV as a change to the scene does.
            edited: edited || settings_edited,
            save,
            scene,
            active_group: self.tree.active,
            paint: Paint {
                jobs: paint_jobs,
                textures_delta,
                pixels_per_point,
                repaint,
            },
        }
    }

    /// Closes a step of the history that nobody holds any more.
    ///
    /// A drag of the canvas closes its own step when the button goes up, so
    /// this catches the fields of a panel and the keys.
    fn settle_history(&mut self, ctx: &egui::Context, history: &mut History) {
        let dragging = self.select.drag.is_some() || self.table.drag.is_some() || self.draw.busy();
        if dragging || ctx.egui_is_using_pointer() || ctx.egui_wants_keyboard_input() {
            return;
        }
        history.settle();
        self.select.opened = None;
        self.select.opened_ink = None;
        self.table.opened = None;
    }

    /// The tool the rail marks, so a new run opens on it.
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// The tab the settings dialog marks, for the same reason.
    pub fn settings_tab(&self) -> Tab {
        self.dialog.tab
    }

    /// Draws the maps, then the canvas, then the UI on top of both.
    pub fn render(
        &mut self,
        gpu: &Gpu,
        pane: &mut Pane,
        paint: Paint,
        canvas: egui::Color32,
        draw_maps: impl FnOnce(&mut wgpu::RenderPass<'static>),
        draw_canvas: impl FnOnce(&mut wgpu::RenderPass<'static>),
    ) -> Result<()> {
        render_pane(
            gpu,
            pane,
            &mut self.renderer,
            paint,
            canvas,
            draw_maps,
            draw_canvas,
        )?;
        Ok(())
    }
}

/// Draws one window: the maps, then the canvas, then what `egui` painted.
///
/// The frame takes two passes. `draw_maps` fills the texture the grid
/// reads, because a grid line can take its color from the map below it
/// and a shader cannot read the surface it writes to. `draw_canvas` then
/// draws into the window itself, and it must start with the grid: the
/// grid pass carries the maps across. DESIGN.md 5.1.
///
/// Both windows go through here. The TV runs an `egui` of its own for the
/// overlays it shows, and it draws them the same way the DM window draws
/// its panels. See [`crate::overlay`].
///
/// Returns `false` when the surface gave no frame and nothing was shown.
///
/// # Errors
///
/// Returns an error when the surface fails validation.
pub fn render_pane(
    gpu: &Gpu,
    pane: &mut Pane,
    renderer: &mut egui_wgpu::Renderer,
    paint: Paint,
    canvas: egui::Color32,
    draw_maps: impl FnOnce(&mut wgpu::RenderPass<'static>),
    draw_canvas: impl FnOnce(&mut wgpu::RenderPass<'static>),
) -> Result<bool> {
    {
        let Paint {
            jobs,
            mut textures_delta,
            pixels_per_point,
            repaint,
        } = paint;
        let renderer = &mut *renderer;
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [pane.config.width, pane.config.height],
            pixels_per_point,
        };
        for (id, deltas) in &textures_delta.set {
            for delta in deltas {
                renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
            }
        }

        let frame = pane.acquire(&gpu.device)?;
        let shown = frame.is_some();
        if let Some(frame) = frame {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            let buffers =
                renderer.update_buffers(&gpu.device, &gpu.queue, &mut encoder, &jobs, &screen);
            let ground = color::linear_token(canvas);
            {
                let mut pass = begin_clear_pass(&mut encoder, pane.beneath.view(), ground);
                draw_maps(&mut pass);
            };
            {
                let mut pass = begin_clear_pass(&mut encoder, &view, ground);
                draw_canvas(&mut pass);
                renderer.render(&mut pass, &jobs, &screen);
            };
            gpu.queue
                .submit(buffers.into_iter().chain([encoder.finish()]));
            gpu.queue.present(frame);
        }

        for id in &textures_delta.free {
            renderer.free_texture(id);
        }
        textures_delta.clear();
        if repaint {
            pane.window.request_redraw();
        }
        Ok(shown)
    }
}

/// What one UI frame decided.
#[derive(Debug)]
pub struct UiOutput {
    /// The stroke under the DM's hand, which is in no scene yet.
    ///
    /// Both windows draw it, so the players watch a line as it is drawn.
    pub live: Option<Stroke>,
    /// The DM pressed Add map.
    pub add_map: bool,
    /// A map changed this frame; the TV should redraw.
    pub edited: bool,
    /// The maps changed and no drag is in progress: write the project.
    pub save: bool,
    /// What the DM asked the scenes dialog to do.
    pub scene: Option<SceneCommand>,
    /// The group a new asset joins.
    pub active_group: NodeId,
    /// What `DmUi::render` needs.
    pub paint: Paint,
}

/// The tessellated UI of one frame, ready to draw.
pub struct Paint {
    /// What `egui` tessellated for this frame.
    pub jobs: Vec<egui::ClippedPrimitive>,
    /// The textures `egui` wants written before the frame draws.
    pub textures_delta: egui::TexturesDelta,
    /// How many surface pixels one `egui` point takes.
    pub pixels_per_point: f32,
    /// Whether `egui` asked for another frame at once.
    pub repaint: bool,
}

impl std::fmt::Debug for Paint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Paint").finish_non_exhaustive()
    }
}

/// Draws a dashed line down the two points it is given.
///
/// DESIGN.md 5.2 puts one between two segments of the toolbar, so the
/// group of views reads apart from the group of buttons beside it.
fn painter_dashes(ui: &egui::Ui, line: &[egui::Pos2; 2], color: egui::Color32) {
    ui.painter().add(egui::Shape::dashed_line(
        line,
        egui::Stroke::new(1.0, color),
        SEGMENT_DASH,
        SEGMENT_DASH,
    ));
}

#[cfg(test)]
mod tests {
    use super::{Paper, Settings, theme};

    fn settings() -> Settings {
        Settings {
            theme: theme::Mode::default(),
            language: crate::text::DEFAULT.to_owned(),
            ink_color: [0, 0, 0, 255],
            ink_width: 0.1,
            ink_nib: super::Nib::default(),
            ink_rule: crate::stroke::Rule::default(),
            ink_snap: true,
            ui_scale: theme::DEFAULT_SCALE,
            tv_display: Some(1),
            swap_windows: false,
            snap_percent: 8.0,
            paper_light: Paper::default(),
            paper_dark: Paper::default(),
            grid_kind: crate::grid::Kind::default(),
            grid_cell: None,
        }
    }

    #[test]
    fn the_snap_window_does_not_move_a_window() {
        // A DM who types in the panel must keep the keyboard, so a change
        // here may not send the TV window back to full screen.
        let mut typed = settings();
        typed.snap_percent = 12.0;
        assert!(!typed.moves_windows(&settings()));
    }

    #[test]
    fn a_display_or_a_swap_moves_a_window() {
        let mut display = settings();
        display.tv_display = None;
        assert!(display.moves_windows(&settings()));
        let mut swap = settings();
        swap.swap_windows = true;
        assert!(swap.moves_windows(&settings()));
    }
}
