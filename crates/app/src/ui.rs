//! The DM window's UI: egui on top of the wgpu pane.

// Rust guideline compliant 2026-02-21

use std::path::Path;

use anyhow::Result;
use egui_wgpu::wgpu;
use egui_winit::winit::{event::WindowEvent, monitor::MonitorHandle};

use crate::camera::{Area, Camera, DEFAULT_PIXELS_PER_INCH, fit};
use crate::color;
use crate::gpu::{Gpu, Pane, begin_clear_pass};
use crate::icon;
use crate::icons::Icon;
use crate::scene::{Group, Node, NodeId, Placed, ROOT_ID, Scene};
use crate::theme::{self, Tokens};
use crate::transform::{
    MAX_GRID_PX, MIN_GRID_PX, corner_offset, edge_midpoint, grid_px_from_measure, hit_test,
    pick_handle, rotation_from_drag, rotation_handle, scale_from_drag, snap_corner, step_scale,
};
use crate::tv::display_label;
use crate::tvbox::{
    MAX_SNAP_PERCENT, TV_WIDTH_INCHES, TvBox, at_true_size, clamp_width, snap_to_true_size,
};
use crate::widget::{self, Height};

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

/// The height of one scene row, in points. DESIGN.md 9.6.
const SCENE_ROW: f32 = 40.0;

/// The width of the scenes folder input, in points. DESIGN.md 9.6.
const PATH_WIDTH: f32 = 360.0;

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

/// One grid cell on the canvas, in inches. The arrow keys move the TV box
/// by this much.
const CELL: f64 = 1.0;

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
    table: Table,
    scenes: Scenes,
    tree: Tree,
    /// The settings dialog and the tab it shows.
    dialog: Dialog,
    /// The theme the context carries, so a change installs once.
    theme: theme::Mode,
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Tool {
    /// Pick a map and move, turn, scale or flip it.
    #[default]
    Select,
    /// Drag the box that decides what the TV shows.
    Table,
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
    /// What every size of DESIGN.md is multiplied by. DESIGN.md 3.1.
    pub ui_scale: f64,
}

impl Settings {
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
#[derive(Debug, Clone)]
enum Drag {
    /// Every asset the DM holds, and where each one stood.
    Move {
        starts: Vec<(NodeId, (f64, f64))>,
        start_cursor: (f64, f64),
    },
    /// A rectangle over the canvas that picks what it covers.
    Band { start_cursor: (f64, f64) },
    /// Every asset the DM holds, where it stood, and the point it turns
    /// or grows around.
    Scale {
        starts: Vec<Placed>,
        pivot: (f64, f64),
        start_cursor: (f64, f64),
    },
    Rotate {
        starts: Vec<Placed>,
        pivot: (f64, f64),
        start_cursor: (f64, f64),
    },
}

#[hotpath::measure_all]
impl DmUi {
    pub fn new(gpu: &Gpu, pane: &Pane) -> Self {
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
            tool: Tool::default(),
            select: Select::default(),
            table: Table::default(),
            scenes: Scenes::default(),
            tree: Tree::default(),
            dialog: Dialog::default(),
            theme: theme::Mode::default(),
            ui_scale: theme::DEFAULT_SCALE,
            frame_box: false,
            zoom_goes_to: None,
            dirty: false,
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
        if self.theme != frame.settings.theme {
            theme::install(&ctx, frame.settings.theme);
            self.theme = frame.settings.theme;
        }
        // egui multiplies the scale into `pixels_per_point`, so the chrome
        // grows and the canvas math, which reads that value, stays true.
        let scale = theme::clamp_scale(frame.settings.ui_scale);
        if (self.ui_scale - scale).abs() > f64::EPSILON {
            ctx.set_zoom_factor(scale as f32);
            self.ui_scale = scale;
        }
        let mut add_map = false;
        let mut edited = false;
        let mut scene = None;
        let select = &mut self.select;
        let table = &mut self.table;
        let tool = &mut self.tool;
        let scenes = &mut self.scenes;
        let tree = &mut self.tree;
        let dialog = &mut self.dialog;
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
            let over = popup_open || scenes.open || dialog.open;
            if !over {
                frame_tv_box(ui, &mut frame, rect, viewport, *frame_box);
                // The ask lives one frame, because the panel that raised it
                // draws after the camera reads it.
                *frame_box = false;
                edited |= match *tool {
                    Tool::Select => canvas(ui, select, &mut frame, viewport, zoom_goes_to, tokens),
                    Tool::Table => {
                        table_tool(ui, table, &mut frame, viewport, zoom_goes_to, tokens)
                    }
                };
            }
            edited |= objects_panel(ui.ctx(), frame.scene, select, tree, tokens);
            edited |= properties_panel(ui.ctx(), &mut frame, select, *tool, frame_box, tokens);
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
            edited |= settings_dialog(ui.ctx(), &mut frame, dialog, tokens);
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

        // Save once a drag is over, not on every frame of it.
        self.dirty |= edited;
        let save = self.dirty && self.select.drag.is_none() && self.table.drag.is_none();
        if save {
            self.dirty = false;
        }
        UiOutput {
            add_map,
            edited,
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

    /// Draws the canvas through `draw_canvas`, then the UI on top of it.
    pub fn render(
        &mut self,
        gpu: &Gpu,
        pane: &mut Pane,
        paint: Paint,
        canvas: egui::Color32,
        draw_canvas: impl FnOnce(&mut wgpu::RenderPass<'static>),
    ) -> Result<()> {
        let Paint {
            jobs,
            mut textures_delta,
            pixels_per_point,
            repaint,
        } = paint;
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [pane.config.width, pane.config.height],
            pixels_per_point,
        };
        for (id, deltas) in &textures_delta.set {
            for delta in deltas {
                self.renderer
                    .update_texture(&gpu.device, &gpu.queue, *id, delta);
            }
        }

        if let Some(frame) = pane.acquire(&gpu.device)? {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            let buffers =
                self.renderer
                    .update_buffers(&gpu.device, &gpu.queue, &mut encoder, &jobs, &screen);
            {
                let mut pass = begin_clear_pass(&mut encoder, &view, color::linear_token(canvas));
                draw_canvas(&mut pass);
                self.renderer.render(&mut pass, &jobs, &screen);
            };
            gpu.queue
                .submit(buffers.into_iter().chain([encoder.finish()]));
            gpu.queue.present(frame);
        }

        for id in &textures_delta.free {
            self.renderer.free_texture(id);
        }
        textures_delta.clear();
        if repaint {
            pane.window.request_redraw();
        }
        Ok(())
    }
}

/// What one UI frame decided.
#[derive(Debug)]
pub struct UiOutput {
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
    jobs: Vec<egui::ClippedPrimitive>,
    textures_delta: egui::TexturesDelta,
    pixels_per_point: f32,
    repaint: bool,
}

impl std::fmt::Debug for Paint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Paint").finish_non_exhaustive()
    }
}

/// What the toolbar asked for this frame. DESIGN.md 5.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Press {
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
fn toolbar(ui: &egui::Ui, tool: Tool, tokens: Tokens) -> Option<Press> {
    let views = [
        (Press::View(Tool::Select), "Select", Icon::MousePointer),
        (Press::View(Tool::Table), "Table", Icon::Monitor),
    ];
    let actions = [
        (Press::Scenes, "Scenes", Icon::Layers),
        (Press::AddMap, "Add map", Icon::Plus),
        (Press::Settings, "Settings", Icon::SlidersHorizontal),
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
fn objects_panel(
    ctx: &egui::Context,
    scene: &mut Scene,
    select: &mut Select,
    tree: &mut Tree,
    tokens: Tokens,
) -> bool {
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
    let mut moved = None;
    panel(
        ctx,
        "objects",
        "Objects",
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
            let footer = 2.0 * Height::Panel.points() + 3.0 * PANEL_PAD;
            let list = ui.available_height() - footer;
            egui::ScrollArea::vertical()
                .max_height(list.max(ROW_HEIGHT))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    edited |= scoped_rows(ui, scene, select, tree, &mut moved, tokens);
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
            ui.horizontal(|ui| {
                // The DM can only take apart the one group they hold.
                let group = select.only().filter(|id| *id != ROOT_ID).filter(|id| {
                    crate::scene::find(scene, *id)
                        .and_then(Node::group)
                        .is_some()
                });
                if widget::button(ui, "New group", Some(Icon::Plus), Height::Panel).clicked() {
                    let id = scene.next_id();
                    let new = Group::new(id, format!("Group {id}"));
                    crate::scene::push_into(scene, tree.active, Node::Group(new));
                    tree.open.insert(id);
                    tree.active = id;
                    edited = true;
                }
                let ungroup = widget::button(ui, "Ungroup", None, Height::Panel);
                if group.is_some() && ungroup.clicked() {
                    // What was in it stands where it stood.
                    let id = group.unwrap_or(ROOT_ID);
                    let freed = crate::scene::assets_of(scene, id);
                    edited |= crate::scene::ungroup(scene, id);
                    select.chosen = freed;
                }
            });
            if select.note.is_empty() {
                widget::helper(ui, "A new asset joins the marked group.");
            } else {
                ui.label(
                    egui::RichText::new(&select.note)
                        .font(theme::font(theme::SMALL, false))
                        .color(tokens.accent),
                );
            }
        },
    );
    if let Some((node, target, into)) = moved {
        edited |= if into {
            crate::scene::move_into(scene, node, target)
        } else {
            crate::scene::move_above(scene, node, target)
        };
    }
    edited |= drag_panel_edge(ctx, rect, &mut tree.width, tokens);
    edited
}

/// The group the list shows, and every row under it. DESIGN.md 8.4.
///
/// The group stands at the top, so the DM sees where the list is before
/// they read a single child.
fn scoped_rows(
    ui: &mut egui::Ui,
    scene: &mut Scene,
    select: &mut Select,
    tree: &mut Tree,
    moved: &mut Option<(NodeId, NodeId, bool)>,
    tokens: Tokens,
) -> bool {
    ui.spacing_mut().item_spacing.y = 0.0;
    let scope = tree.scope;
    let open = tree.open.contains(&scope);
    let picked = scope != ROOT_ID && select.holds(scope);
    let Some(group) = crate::scene::group_mut(scene, scope) else {
        return false;
    };
    let mut edited = false;
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
    if tree.renaming == Some(scope) && scope != ROOT_ID {
        let (changed, done) = rename_field(ui, row.name, &mut group.name, tokens);
        edited |= changed;
        if done {
            tree.renaming = None;
        }
    } else {
        row_name(ui, row.name, &group.name, picked, tokens);
    }
    if row.body.double_clicked() && scope != ROOT_ID {
        tree.renaming = Some(scope);
    }
    // DESIGN.md 8.4: the root is always visible and carries no switches. A
    // group the DM went into carries its own.
    if scope != ROOT_ID {
        edited |= switches(ui, row.switches, &mut group.shown, tokens);
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
        edited |= tree_rows(ui, &mut group.children, select, tree, 1, moved, tokens);
    }
    edited
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
fn properties_panel(
    ctx: &egui::Context,
    frame: &mut Frame<'_>,
    select: &mut Select,
    tool: Tool,
    frame_box: &mut bool,
    tokens: Tokens,
) -> bool {
    let screen = ctx.content_rect();
    let held = match tool {
        Tool::Table => Held::TvBox,
        Tool::Select => match select
            .only()
            .map(|id| (id, crate::scene::find(frame.scene, id)))
        {
            Some((id, Some(Node::Asset(asset)))) => {
                Held::Map(id, asset.path.to_string_lossy().into_owned())
            }
            // A group has no panel. Its name and its two switches sit on
            // its row, and it carries no size and no turn of its own, so a
            // panel over it would hold nothing the list does not say.
            _ => return false,
        },
    };
    let left_top = egui::pos2(screen.right() - MARGIN - PANEL_WIDTH, MARGIN);
    let mut edited = false;
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
                edited = box_properties(ui, &mut frame.scene.tv_box, frame_box, tokens);
            }
            Held::Map(id, _) => edited = map_properties(ui, *id, select, frame.scene, tokens),
        },
    );
    edited
}

/// What the properties panel is about.
#[derive(Debug, Clone)]
enum Held {
    /// The box that decides what the TV shows. DESIGN.md 8.2.
    TvBox,
    /// One map, by id, with the name of its file. DESIGN.md 8.1.
    Map(NodeId, String),
}

impl Held {
    /// The title the panel takes.
    fn title(&self) -> &str {
        match self {
            Self::TvBox => "TV box",
            Self::Map(_, name) => name,
        }
    }
}

/// A tab of the settings dialog. DESIGN.md 9.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Tab {
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
            Self::Table => ("Table", Icon::Monitor),
            Self::Grid => ("Grid", Icon::Grid3x3),
            Self::Light => ("Light", Icon::Sun),
            Self::Shortcuts => ("Shortcuts", Icon::Keyboard),
            Self::About => ("About", Icon::Info),
        }
    }
}

/// What the settings dialog holds between frames.
#[derive(Debug, Default)]
struct Dialog {
    /// Whether the dialog stands over the canvas.
    open: bool,
    /// The tab the navigation column marks.
    tab: Tab,
    /// The scale the DM is dragging toward, until the button goes up.
    scale_drag: Option<f64>,
    /// The license the About tab shows.
    license: License,
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
fn settings_dialog(
    ctx: &egui::Context,
    frame: &mut Frame<'_>,
    dialog: &mut Dialog,
    tokens: Tokens,
) -> bool {
    if !dialog.open {
        return false;
    }
    let mut edited = false;
    let close = dialog_frame(ctx, "settings", "Settings", DIALOG, tokens, |ui, rest| {
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
                        not_built(body_ui, "The grid settings arrive with the hex grid (#15).");
                    }
                    Tab::Light => {
                        not_built(
                            body_ui,
                            "The light settings arrive with the darkness slider (#20).",
                        );
                    }
                    Tab::About => about_tab(body_ui, &mut dialog.license, tokens),
                    Tab::Shortcuts => {
                        not_built(
                            body_ui,
                            "The key list arrives with the shortcuts story (#36).",
                        );
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
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(LABEL_COLUMN, widget::CONTROL),
            egui::Sense::hover(),
        );
        ui.painter().text(
            // DESIGN.md 9: the label sits 6 points below the top of the row.
            egui::pos2(rect.left(), rect.top() + 6.0),
            egui::Align2::LEFT_TOP,
            label,
            theme::font(theme::BODY, false),
            tokens.ink,
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
    dialog_row(ui, "Display", |ui| {
        let shown = frame
            .settings
            .tv_display
            .map_or_else(|| "Window".to_owned(), &label);
        let field = widget::select_field(ui, &shown, widget::SELECT_WIDTH);
        let popup = egui::Popup::menu(&field)
            .gap(-1.0)
            .width(widget::SELECT_WIDTH);
        popup.show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let picked = frame.settings.tv_display;
            if widget::select_row(ui, "Window", picked.is_none()).clicked() {
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
    dialog_row(ui, "Size", |ui| {
        widget::helper(
            ui,
            "The TV diagonal, its pixels per inch and its width arrive with story #6.",
        );
    });
    dialog_row(ui, "Snap to true size", |ui| {
        let mut percent = frame.settings.snap_percent;
        if widget::input(ui, &mut percent, "%", 90.0, 0.0..=MAX_SNAP_PERCENT, 0.1).changed() {
            frame.settings.snap_percent = percent;
            edited = true;
        }
        widget::helper(ui, "either side of 100 %");
    });
    dialog_row(ui, "Windows", |ui| {
        let mut swap = frame.settings.swap_windows;
        if widget::checkbox(
            ui,
            &mut swap,
            "Swap the two windows instead of moving the DM window",
        )
        .clicked()
        {
            frame.settings.swap_windows = swap;
            edited = true;
        }
    });
    dialog_row(ui, "Theme", |ui| {
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
    dialog_row(ui, "Interface scale", |ui| {
        let mut scale = scale_drag.unwrap_or(frame.settings.ui_scale);
        let percent = format!("{} %", (scale * 100.0).round());
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
        widget::helper(
            ui,
            "How big the toolbar, the panels and the dialogs draw. The maps keep their size.",
        );
    });
    edited
}

/// What dmap carries, and the terms it comes under. DESIGN.md 9.5.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum License {
    /// The program itself.
    #[default]
    Program,
    /// Atkinson Hyperlegible, which DESIGN.md 3 gives the window.
    Font,
    /// The Lucide glyphs of DESIGN.md 4.
    Icons,
}

/// The text of the GPL, which the program comes under.
const GPL: &str = include_str!("../../../LICENSE");

/// The text of the SIL Open Font License, which the font comes under.
const OFL: &str = include_str!("../assets/fonts/LICENSE-OFL.txt");

/// The text of the ISC license, which the glyphs come under.
const ISC: &str = include_str!("../assets/icons/LICENSE-ISC.txt");

/// The people who built dmap, one to a line.
///
/// A line that starts with a hash is a note in the file, not a name.
const CONTRIBUTORS: &str = include_str!("../../../CONTRIBUTORS");

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
    widget::row_label(ui, &format!("dmap {}", env!("CARGO_PKG_VERSION")));
    widget::helper(ui, "Map display for a tabletop RPG table with a TV.");
    for one in [License::Program, License::Font, License::Icons] {
        let (name, terms, url) = one.about();
        ui.horizontal(|ui| {
            widget::row_label(ui, &format!("{name}, under the {terms}."));
            ui.hyperlink_to(
                egui::RichText::new("Source")
                    .font(theme::font(theme::SMALL, false))
                    .color(tokens.accent),
                url,
            );
        });
    }
    ui.add_space(4.0);
    widget::row_label(ui, "Built by");
    let names: Vec<&str> = CONTRIBUTORS
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    widget::helper(ui, &names.join(", "));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        widget::helper(ui, "dmap is free software, and it is not finished.");
        ui.hyperlink_to(
            egui::RichText::new("Bring a bug, a story or a patch")
                .font(theme::font(theme::SMALL, false))
                .color(tokens.accent),
            ISSUES,
        );
    });
    ui.add_space(4.0);
    let choices = [
        (License::Program, "dmap"),
        (License::Font, "Font"),
        (License::Icons, "Glyphs"),
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
fn not_built(ui: &mut egui::Ui, text: &str) {
    widget::helper(ui, text);
}

/// The scenes dialog of DESIGN.md 9.6.
///
/// Returns what the DM asked for. The program does the work, so an error
/// on the disk has one place to go.
fn scenes_dialog(
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
        "Scenes",
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
                widget::row_label(ui, "Scenes folder");
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
                if widget::button(ui, "Change", None, Height::Full).clicked() {
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
            if widget::button(&mut foot, "New scene", Some(Icon::Plus), Height::Full).clicked() {
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
            egui::RichText::new(format!("Delete {name} and its maps?"))
                .font(theme::font(theme::BODY, false))
                .color(tokens.accent),
        );
        if widget::button(&mut row, "Delete", Some(Icon::Trash), Height::Row).clicked() {
            *command = Some(SceneCommand::Delete(name.to_owned()));
        }
        if widget::button(&mut row, "Keep", None, Height::Row).clicked() {
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
        if done || widget::button(&mut row, "Save", None, Height::Row).clicked() {
            *command = Some(SceneCommand::Rename {
                from: from.clone(),
                to: typed.clone(),
            });
        }
        if widget::button(&mut row, "Cancel", None, Height::Row).clicked() {
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
        if widget::button(row, "Delete", None, Height::Row).clicked() {
            scenes.deleting = Some(name.to_owned());
        }
        if widget::button(row, "Folder", None, Height::Row).clicked() {
            *command = Some(SceneCommand::Reveal(name.to_owned()));
        }
        if widget::button(row, "Rename", None, Height::Row).clicked() {
            scenes.renaming = Some((name.to_owned(), name.to_owned()));
        }
        if open {
            row.label(
                egui::RichText::new("Open now")
                    .font(theme::font(theme::BODY, false))
                    .color(tokens.mute),
            );
        } else if widget::button(row, "Open", None, Height::Row).clicked() {
            *command = Some(SceneCommand::Open(name.to_owned()));
        }
    });
}

/// What the tree list holds between frames.
#[derive(Debug)]
struct Tree {
    /// The group a new asset joins.
    active: NodeId,
    /// The groups whose children the list shows.
    open: std::collections::HashSet<NodeId>,
    /// The node the list opened its groups for.
    shown: Option<NodeId>,
    /// How wide the panel stands, in points. DESIGN.md 8.4.
    width: f32,
    /// The group whose name the DM is writing, from a double click.
    renaming: Option<NodeId>,
    /// The group the list shows. Its children are the rows under it.
    ///
    /// DESIGN.md 8.4: the list never shows the whole tree from the root,
    /// so the indent of a deep scene has no room to run away.
    scope: NodeId,
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
/// The rows under one group. Returns `true` when the DM changed one.
///
/// DESIGN.md 8.4 gives every row the same shape: a twist, the glyph that
/// says what the row is, the name, then the two switches.
fn tree_rows(
    ui: &mut egui::Ui,
    nodes: &mut [Node],
    select: &mut Select,
    tree: &mut Tree,
    depth: usize,
    moved: &mut Option<(NodeId, NodeId, bool)>,
    tokens: Tokens,
) -> bool {
    let mut edited = false;
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
                edited |= switches(ui, row.switches, &mut group.shown, tokens);
                dropped_on(ui, &row.whole, id, true, moved, tokens);
                if open {
                    edited |= tree_rows(
                        ui,
                        &mut group.children,
                        select,
                        tree,
                        depth + 1,
                        moved,
                        tokens,
                    );
                }
            }
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
                edited |= switches(ui, row.switches, &mut asset.shown, tokens);
                dropped_on(ui, &row.whole, id, false, moved, tokens);
            }
        }
    }
    edited
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
fn row_name(ui: &egui::Ui, rect: egui::Rect, name: &str, picked: bool, tokens: Tokens) {
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
fn switches(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    shown: &mut crate::scene::Shown,
    tokens: Tokens,
) -> bool {
    let mut edited = false;
    for (index, (on, glyph, off_glyph)) in [
        (&mut shown.dm, Icon::Eye, Icon::EyeOff),
        (&mut shown.tv, Icon::Monitor, Icon::MonitorOff),
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
            *on = !*on;
            edited = true;
        }
        let (glyph, color) = if *on {
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
    edited
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

/// The properties of the TV box. DESIGN.md 8.2.
///
/// The DM drags a corner handle to reach a zoom by eye. This field reaches
/// an exact one, such as 50 percent for a map twice the size of the table.
///
/// `frame_box` comes back `true` when the DM asked to see the whole box.
fn box_properties(
    ui: &mut egui::Ui,
    tv_box: &mut TvBox,
    frame_box: &mut bool,
    tokens: Tokens,
) -> bool {
    let _ = tokens;
    let mut percent = tv_box.zoom(TV_WIDTH_INCHES) * 100.0;
    let mut edited = false;
    widget::row_label(ui, "Zoom");
    if widget::input(
        ui,
        &mut percent,
        "%",
        100.0,
        MIN_ZOOM_PERCENT..=MAX_ZOOM_PERCENT,
        0.1,
    )
    .changed()
    {
        tv_box.width = clamp_width(TV_WIDTH_INCHES / (percent / 100.0));
        edited = true;
    }
    widget::helper(ui, "100 % is true size on the TV");
    widget::row_label(ui, "Move");
    widget::helper(ui, "The arrow keys move the box one cell");
    widget::row_label(ui, "Frame");
    *frame_box = widget::button(ui, "Show the whole box", None, Height::Panel).clicked();
    edited
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
    scene: &mut Scene,
    tokens: Tokens,
) -> bool {
    let names = crate::scene::group_names(scene);
    let mut edited = false;
    // DESIGN.md 8.1 gives the file name a row of its own. The title of the
    // panel carries it now, so the row would say it twice.
    widget::row_label(ui, "Group");
    let holder = crate::scene::parent_of(scene, id).map(|(group, _)| group);
    let open = holder
        .and_then(|group| names.iter().find(|(other, ..)| *other == group))
        .map_or("", |(_, group_name, _)| group_name.as_str());
    let field = widget::select_field(ui, open, ui.available_width());
    egui::Popup::menu(&field)
        .gap(-1.0)
        .width(ui.available_width())
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (group, group_name, depth) in &names {
                let label = format!("{}{group_name}", "  ".repeat(*depth));
                if widget::select_row(ui, &label, holder == Some(*group)).clicked()
                    && crate::scene::move_into(scene, id, *group)
                {
                    edited = true;
                }
            }
        });
    let Some(map) = crate::scene::asset_mut(scene, id) else {
        return edited;
    };
    widget::row_label(ui, "Pixels per cell");
    let mut grid_px = map.grid_px;
    if widget::input(
        ui,
        &mut grid_px,
        "px",
        100.0,
        MIN_GRID_PX..=MAX_GRID_PX,
        1.0,
    )
    .changed()
    {
        map.grid_px = grid_px;
        edited = true;
    }
    // The size sits next to the grid size because the two multiply: a map
    // with the right grid size draws at true size only at 100 percent.
    let mut percent = map.scale * 100.0;
    widget::row_label(ui, "Size");
    if widget::input(ui, &mut percent, "%", 100.0, MIN_PERCENT..=MAX_PERCENT, 0.5).changed() {
        map.scale = percent / 100.0;
        edited = true;
    }
    let mut degrees = map.rotation.to_degrees();
    widget::row_label(ui, "Turn");
    if widget::input(ui, &mut degrees, "\u{b0}", 100.0, -360.0..=360.0, 0.5).changed() {
        map.rotation = degrees.to_radians();
        edited = true;
    }
    if widget::button(ui, "Measure a cell", Some(Icon::Ruler), Height::Panel).clicked() {
        select.measure = Some(Measure::Start);
    }
    if select.measure.is_some() {
        widget::helper(ui, "Click two corners of one cell. Escape gives it up.");
    }
    // DESIGN.md 8.1 puts the turn and the flip in the footer of the panel.
    ui.horizontal(|ui| {
        if widget::button(ui, "Turn", Some(Icon::RotateCw), Height::Row).clicked() {
            map.rotation += std::f64::consts::FRAC_PI_2;
            edited = true;
        }
        if widget::button(ui, "Flip", Some(Icon::FlipHorizontal2), Height::Row).clicked() {
            map.flip_x = !map.flip_x;
            edited = true;
        }
    });
    widget::helper(ui, "Shift with F flips the map the other way.");
    let _ = tokens;
    edited
}

/// The Select tool on the canvas: pick, move, scale, turn and flip maps.
///
/// Returns `true` when a map changed.
fn canvas(
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
    } else if let Some(Drag::Band { start_cursor }) = select.drag.take() {
        // The band is over: what it covered is what the DM now holds.
        if let Some(pos) = pointer.pos {
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

    edited |= keys(ui, select, frame.scene);
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

/// The list of what a band drag picked, and the button that groups it.
///
/// It floats at the top of the canvas, centered. The canvas fills the
/// window now, so its top-left corner sits under the objects list, and a
/// popup there would hide behind the panel.
fn selection_popup(
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
        &format!("{} picked", held.len()),
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
                    None => continue,
                };
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), ROW_HEIGHT),
                    egui::Sense::hover(),
                );
                row_name(ui, rect, &name, false, tokens);
            }
            ui.horizontal(|ui| {
                if widget::button(ui, "Group", Some(Icon::Plus), Height::Panel).clicked() {
                    group_them = true;
                }
                if widget::button(ui, "Close", None, Height::Panel).clicked() {
                    select.popup = false;
                }
            });
        },
    );
    if group_them {
        let name = format!("Group {}", frame.scene.next_id());
        if let Some(id) = crate::scene::group_selection(frame.scene, &select.chosen, name) {
            select.chosen = vec![id];
            select.popup = false;
            edited = true;
        }
    }
    edited
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
fn table_tool(
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
    match (pointer.down(), pointer.pos, table.drag) {
        (true, Some(pos), Some(drag)) => {
            edited = drag_box(&mut frame.scene.tv_box, drag, view.to_world(pos));
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
                let resized = (frame.scene.tv_box.width - start_width).abs() > f64::EPSILON;
                let snapped = snap_to_true_size(
                    frame.scene.tv_box.width,
                    TV_WIDTH_INCHES,
                    frame.settings.snap_percent,
                );
                if resized && (snapped - frame.scene.tv_box.width).abs() > f64::EPSILON {
                    frame.scene.tv_box.width = snapped;
                    edited = true;
                }
            }
        }
    }
    // The keys wait for the drag to end. A drag rewrites the box from its
    // start state every frame, so a key press in the middle of one is lost.
    if table.drag.is_none() {
        edited |= arrow_keys(ui, &mut frame.scene.tv_box);
    }

    // Ctrl and Alt with the wheel reach a zoom without a drag on a handle.
    // Figma has no gesture that resizes an object with the wheel, so Alt
    // marks this one as ours. The canvas already gave the gesture up.
    if let (Some(pinch), true) = (pointer.box_zoom, pointer.hovered) {
        let zoom = frame.scene.tv_box.zoom(TV_WIDTH_INCHES) * f64::from(pinch);
        frame.scene.tv_box.width = clamp_width(TV_WIDTH_INCHES / zoom);
        edited = true;
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
    // The box of a group takes the press first. Inside the box, the press
    // goes on to the asset under the pointer.
    let aimed = group_at(frame, view, pos).or_else(|| {
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
        starts: standing(frame.scene, &select.chosen),
        start_cursor: cursor,
    });
}

/// Where every asset the DM holds stands now.
///
/// A group in the selection hands over every asset under it, so a drag on
/// a group moves all of it and nothing loses its place inside.
fn standing(scene: &Scene, chosen: &[NodeId]) -> Vec<(NodeId, (f64, f64))> {
    crate::scene::normalize(scene, chosen)
        .iter()
        .flat_map(|id| crate::scene::assets_of(scene, *id))
        .filter_map(|id| {
            let center = crate::scene::find(scene, id)
                .and_then(Node::asset)
                .map(|asset| asset.center)?;
            Some((id, center))
        })
        .collect()
}

/// What the keyboard does to the selected map. Returns `true` when it
/// changed one.
fn keys(ui: &egui::Ui, select: &mut Select, scene: &mut Scene) -> bool {
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
    let held = crate::scene::normalize(scene, &select.chosen);
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
                if crate::scene::share_parent(scene, &held).is_none() {
                    "Order works inside one group. Pick nodes that sit together."
                        .clone_into(&mut select.note);
                    continue;
                }
                select.note.clear();
                edited |= crate::scene::reorder_all(scene, &held, *key == egui::Key::PageUp);
                continue;
            }
            let Some(asset) = one.and_then(|id| crate::scene::asset_mut(scene, id)) else {
                continue;
            };
            match key {
                egui::Key::R => asset.rotation += std::f64::consts::FRAC_PI_2,
                egui::Key::F if modifiers.shift => asset.flip_y = !asset.flip_y,
                egui::Key::F => asset.flip_x = !asset.flip_x,
                // Plus is the numpad key; Equals is the shared "=/+" main
                // row key, which egui reports without needing Shift.
                egui::Key::Plus | egui::Key::Equals => {
                    asset.scale = step_scale(asset.scale, true);
                }
                egui::Key::Minus => asset.scale = step_scale(asset.scale, false),
                _ => continue,
            }
            edited = true;
        }
    });
    edited
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
    let Some(map) = select
        .only()
        .and_then(|id| crate::scene::asset_mut(frame.scene, id))
    else {
        return false;
    };
    let Some(grid_px) = grid_px_from_measure(first, cursor, map.grid_px, map.scale) else {
        return false;
    };
    map.grid_px = grid_px;
    // A measured map draws straight from its grid size: one cell, one inch.
    map.scale = 1.0;
    true
}

/// The canvas area, and how this frame reads the pointer over it.
fn canvas_area(
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
fn frame_tv_box(
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
    let Some(drag) = select.drag.clone() else {
        return false;
    };
    match drag {
        // The band picks nothing until the DM lets go of it.
        Drag::Band { .. } => false,
        Drag::Move {
            starts,
            start_cursor,
        } => {
            // A press without motion picks and nothing more: a snap would
            // shift a map that the DM placed off the grid.
            if cursor == start_cursor || starts.is_empty() {
                return false;
            }
            let step = (cursor.0 - start_cursor.0, cursor.1 - start_cursor.1);
            // One asset snaps to its own grid. A selection moves as one
            // piece, so the first asset snaps and the rest follow it.
            let lead = snap.then(|| snap_step(frame, &starts[0], step)).flatten();
            let step = lead.unwrap_or(step);
            for (id, start) in &starts {
                let Some(asset) = crate::scene::asset_mut(frame.scene, *id) else {
                    continue;
                };
                asset.center = (start.0 + step.0, start.1 + step.1);
                if !snap && let Some(size) = (frame.size_of)(&asset.path.clone()) {
                    let corners = asset.corners(size);
                    asset.snap_offset = corner_offset(&corners);
                }
            }
            true
        }
        Drag::Scale {
            starts,
            pivot,
            start_cursor,
        } => {
            let factor = scale_from_drag(pivot, start_cursor, cursor);
            crate::scene::scale_about(frame.scene, &starts, pivot, factor);
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
            let turned = rotation_from_drag(base, pivot, start_cursor, cursor, snap);
            crate::scene::rotate_about(frame.scene, &starts, pivot, turned - base);
            true
        }
    }
}

/// The step a move takes once the leading asset snaps to its own grid.
fn snap_step(
    frame: &Frame<'_>,
    lead: &(NodeId, (f64, f64)),
    step: (f64, f64),
) -> Option<(f64, f64)> {
    let (id, start) = lead;
    let asset = crate::scene::find(frame.scene, *id).and_then(Node::asset)?;
    let size = (frame.size_of)(&asset.path)?;
    let moved = (start.0 + step.0, start.1 + step.1);
    let mut settled = asset.clone();
    settled.center = moved;
    let corners = settled.corners(size);
    let snapped = snap_corner(moved, &corners, asset.snap_offset);
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

#[cfg(test)]
mod tests {
    use super::{ROW_HEIGHT, Settings, row_parts, theme};

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

    fn settings() -> Settings {
        Settings {
            theme: theme::Mode::default(),
            ui_scale: theme::DEFAULT_SCALE,
            tv_display: Some(1),
            swap_windows: false,
            snap_percent: 8.0,
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
