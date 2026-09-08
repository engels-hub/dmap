//! The DM window's UI: egui on top of the wgpu pane.

// Rust guideline compliant 2026-02-21

use std::path::Path;

use anyhow::Result;
use egui_wgpu::wgpu;
use egui_winit::winit::{event::WindowEvent, monitor::MonitorHandle};

use crate::camera::{Area, Camera, DEFAULT_PIXELS_PER_INCH, fit};
use crate::color;
use crate::gpu::{Gpu, Pane, begin_clear_pass};
use crate::scene::{
    Audience, Layer, MapObject, delete_layer, index_after_move, maps_on, move_layer,
};
use crate::transform::{
    MAX_GRID_PX, MIN_GRID_PX, corner_offset, edge_midpoint, grid_px_from_measure, hit_test,
    pick_handle, reorder, rotation_from_drag, rotation_handle, scale_from_drag, snap_corner,
    step_scale,
};
use crate::tv::display_label;
use crate::tvbox::{
    MAX_SNAP_PERCENT, TV_WIDTH_INCHES, TvBox, at_true_size, clamp_width, snap_to_true_size,
};

/// Width of the tool rail on the left, from DESIGN.md.
const RAIL_WIDTH: f32 = 72.0;

/// The accent color of the light theme, from DESIGN.md.
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0xb4, 0x45, 0x2c);

/// Size of a corner handle in points.
const HANDLE_SIZE: f32 = 8.0;

/// How far a click may miss a handle and still grab it, in points.
const HANDLE_REACH: f64 = 10.0;

/// Distance of the rotation handle from the top edge, in points.
const ROTATION_HANDLE_OFFSET: f64 = 24.0;

/// The smallest size the properties accept, in percent.
///
/// The same floor the `-` key keeps, so a map can never vanish.
const MIN_PERCENT: f64 = 1.0;

/// The largest size the properties accept, in percent.
const MAX_PERCENT: f64 = 10_000.0;

/// The wash over the canvas outside the TV box, from DESIGN.md.
///
/// DESIGN.md gives this as `rgba(43, 36, 25, 0.12)`. egui wants the color
/// already multiplied by the alpha, and only that form is a `const`, so the
/// channels below are 43, 36 and 25 times 31/255.
const DIM: egui::Color32 = egui::Color32::from_rgba_premultiplied(5, 4, 3, 31);

/// Text on the accent, from the `field` token in DESIGN.md.
const ON_ACCENT: egui::Color32 = egui::Color32::from_rgb(0xf5, 0xef, 0xe2);

/// The `ink` token of the light theme, from DESIGN.md.
const INK: egui::Color32 = egui::Color32::from_rgb(0x2b, 0x24, 0x19);

/// What the DM takes hold of to drag a layer through the pile.
const GRIP: &str = "::";

/// The `mute` token of the light theme, from DESIGN.md. Helper text.
const MUTE: egui::Color32 = egui::Color32::from_rgb(0x7d, 0x74, 0x62);

/// Size of the zoom label on the TV box, in points. DESIGN.md 5.3.
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
    /// The layer a new map joins, as a place in the scene's layers.
    active_layer: usize,
    /// The layer the DM asked to delete, before they answered the question.
    deleting_layer: Option<usize>,
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
    pub maps: &'a mut Vec<MapObject>,
    /// The layers of the scene, bottom one first.
    pub layers: &'a mut Vec<Layer>,
    pub camera: &'a mut Camera,
    /// The part of the canvas the TV shows.
    pub tv_box: &'a mut TvBox,
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
    selected: Option<usize>,
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
#[derive(Debug, Clone, Copy)]
enum Drag {
    Move {
        start_center: (f64, f64),
        start_cursor: (f64, f64),
    },
    Scale {
        start_scale: f64,
        start_cursor: (f64, f64),
    },
    Rotate {
        start_rotation: f64,
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
        Self {
            state,
            renderer,
            tool: Tool::default(),
            select: Select::default(),
            table: Table::default(),
            scenes: Scenes::default(),
            active_layer: 0,
            deleting_layer: None,
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
        let frame_layers = frame.layers.len();
        let raw_input = self.state.take_egui_input(&pane.window);
        let ctx = self.state.egui_ctx().clone();
        let viewport = (pane.config.width, pane.config.height);
        let mut add_map = false;
        let mut edited = false;
        let mut scene = None;
        let select = &mut self.select;
        let table = &mut self.table;
        let tool = &mut self.tool;
        let scenes = &mut self.scenes;
        let active_layer = &mut self.active_layer;
        let deleting_layer = &mut self.deleting_layer;
        let zoom_goes_to = &mut self.zoom_goes_to;
        let selected_before = select.selected;
        let output = ctx.run_ui(raw_input, |ui| {
            // A click that closes a popup must not reach the canvas.
            let popup_open = egui::Popup::is_any_open(ui.ctx());
            let rail = rail_and_settings(
                ui,
                &mut frame,
                select,
                tool,
                scenes,
                active_layer,
                deleting_layer,
            );
            add_map = rail.add_map;
            edited = rail.edited;
            scene = scenes_dialog(ui, scenes, &frame);
            // A dialog over the canvas takes the keyboard too. egui holds
            // the pointer back on its own, but `R` and the arrow keys would
            // still reach the map behind it.
            if !popup_open && !scenes.open {
                let rect = ui.available_rect_before_wrap();
                frame_tv_box(ui, &mut frame, rect, viewport);
                edited |= match *tool {
                    Tool::Select => canvas(ui, select, &mut frame, viewport, zoom_goes_to),
                    Tool::Table => table_tool(ui, table, &mut frame, viewport, zoom_goes_to),
                };
            }
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
            || self.select.selected != selected_before;

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
            active_layer: self.active_layer.min(frame_layers.saturating_sub(1)),
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
                let mut pass =
                    begin_clear_pass(&mut encoder, &view, color::linear_color(color::CANVAS));
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
    /// The layer a new map joins.
    pub active_layer: usize,
    /// What the DM asked the scenes dialog to do.
    pub scene: Option<SceneCommand>,
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

/// The rail and the settings panel.
///
/// Returns whether Add map was pressed, and whether a map property changed.
fn rail_and_settings(
    ui: &mut egui::Ui,
    frame: &mut Frame<'_>,
    select: &mut Select,
    tool: &mut Tool,
    scenes: &mut Scenes,
    active_layer: &mut usize,
    deleting_layer: &mut Option<usize>,
) -> Rail {
    let displays = frame.displays;
    let mut add_map = false;
    let mut edited = false;
    egui::Panel::left("rail")
        .exact_size(RAIL_WIDTH)
        .resizable(false)
        .show(ui, |ui| {
            ui.label("dmap");
            // DESIGN.md: the accent marks the active tool, and nothing else
            // in the rail takes a color.
            for (choice, label) in [(Tool::Select, "Select"), (Tool::Table, "Table")] {
                let active = *tool == choice;
                let color = if active {
                    ON_ACCENT
                } else {
                    ui.visuals().text_color()
                };
                let fill = if active {
                    ACCENT
                } else {
                    egui::Color32::TRANSPARENT
                };
                let button = egui::Button::new(egui::RichText::new(label).color(color)).fill(fill);
                if ui.add(button).clicked() {
                    *tool = choice;
                }
            }
            ui.separator();
            add_map = ui.button("Add map").clicked();
            if ui.button("Scenes").clicked() {
                scenes.open = !scenes.open;
            }
        });
    egui::Panel::right("settings").show(ui, |ui| {
        ui.heading("Settings");
        ui.label("TV display");
        let label = |i: usize| {
            let display = &displays[i];
            let size = display.size();
            display_label(display.name().as_deref(), size.width, size.height)
        };
        let selected = frame
            .settings
            .tv_display
            .map_or_else(|| "Window".to_owned(), label);
        egui::ComboBox::from_id_salt("tv_display")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut frame.settings.tv_display, None, "Window");
                for i in 0..displays.len() {
                    ui.selectable_value(&mut frame.settings.tv_display, Some(i), label(i));
                }
            });
        ui.checkbox(
            &mut frame.settings.swap_windows,
            "Swap mode (Wayland compat)",
        );
        ui.label("Snap to true size");
        ui.horizontal(|ui| {
            let field = egui::DragValue::new(&mut frame.settings.snap_percent)
                .suffix(" %")
                .range(0.0..=MAX_SNAP_PERCENT);
            ui.add(field);
            ui.label(egui::RichText::new("either side of 100 %").color(MUTE));
        });
        edited |= layer_panel(ui, frame, active_layer, deleting_layer);
        if *tool == Tool::Select {
            edited |= map_properties(ui, select, frame.maps, frame.layers);
        } else {
            // Another tool does not run the measure, so the panel must not
            // leave a measure armed behind it.
            select.measure = None;
            edited |= box_properties(ui, frame.tv_box);
        }
    });
    Rail { add_map, edited }
}

/// What the rail and the settings panel decided this frame.
struct Rail {
    /// The DM pressed Add map.
    add_map: bool,
    /// A value in the panel changed.
    edited: bool,
}

/// The scenes dialog: open, make, rename, delete, and show the folder.
///
/// Returns what the DM asked for. The program does the work, so an error
/// on the disk has one place to go.
fn scenes_dialog(ui: &egui::Ui, scenes: &mut Scenes, frame: &Frame<'_>) -> Option<SceneCommand> {
    if !scenes.open {
        return None;
    }
    let mut command = None;
    let modal = egui::Modal::new(egui::Id::new("scenes")).show(ui.ctx(), |ui| {
        ui.set_width(420.0);
        ui.heading("Scenes");
        ui.label(
            egui::RichText::new("Every scene is a folder of its own, with its maps beside it.")
                .color(MUTE),
        );
        ui.separator();
        for name in (frame.list_scenes)() {
            // Two scenes of one name can live in two folders, so the row
            // that stands out is the one whose folder is open.
            let open = frame.scenes_dir.join(&name) == *frame.scene_dir;
            scene_row(ui, scenes, open, &name, &mut command);
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Scenes folder").color(MUTE));
            if ui.button("Change").clicked() {
                command = Some(SceneCommand::ScenesFolder);
            }
        });
        let path = frame.scenes_dir.display().to_string();
        ui.add(egui::Label::new(egui::RichText::new(&path).color(MUTE)).truncate())
            .on_hover_text(&path);
        if !frame.scene_error.is_empty() {
            ui.label(egui::RichText::new(frame.scene_error).color(ACCENT));
        }
        ui.horizontal(|ui| {
            if ui.button("New scene").clicked() {
                command = Some(SceneCommand::New);
            }
            if ui.button("Close").clicked() {
                scenes.open = false;
            }
        });
    });
    if modal.should_close() {
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
fn scene_row(
    ui: &mut egui::Ui,
    scenes: &mut Scenes,
    open: bool,
    name: &str,
    command: &mut Option<SceneCommand>,
) {
    // A question stands in for the row until the DM answers it. Deleting a
    // scene takes its maps with it, so it never happens on one click.
    if scenes.deleting.as_deref() == Some(name) {
        ui.horizontal(|ui| {
            ui.label(format!("Delete {name} and its maps?"));
            if ui.button("Delete").clicked() {
                *command = Some(SceneCommand::Delete(name.to_owned()));
            }
            if ui.button("Keep").clicked() {
                scenes.deleting = None;
            }
        });
        return;
    }
    ui.horizontal(|ui| {
        if let Some((from, typed)) = scenes.renaming.as_mut().filter(|(from, _)| from == name) {
            let field = ui.add(egui::TextEdit::singleline(typed).desired_width(180.0));
            let done = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            field.request_focus();
            if done || ui.button("Save").clicked() {
                *command = Some(SceneCommand::Rename {
                    from: from.clone(),
                    to: typed.clone(),
                });
            }
            if ui.button("Cancel").clicked() {
                scenes.renaming = None;
            }
            return;
        }
        let label = if open {
            egui::RichText::new(name).color(ACCENT)
        } else {
            egui::RichText::new(name)
        };
        ui.allocate_ui_with_layout(
            egui::vec2(200.0, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| ui.add(egui::Label::new(label).truncate()),
        );
        if ui.add_enabled(!open, egui::Button::new("Open")).clicked() {
            *command = Some(SceneCommand::Open(name.to_owned()));
        }
        if ui.button("Rename").clicked() {
            scenes.renaming = Some((name.to_owned(), name.to_owned()));
        }
        if ui.button("Folder").clicked() {
            *command = Some(SceneCommand::Reveal(name.to_owned()));
        }
        if ui.button("Delete").clicked() {
            scenes.deleting = Some(name.to_owned());
        }
    });
}

/// Whether the DM screen draws this map, which is what makes it pickable.
fn shown_to_dm(map: &MapObject, layers: &[Layer]) -> bool {
    layers
        .get(map.layer)
        .is_some_and(|layer| layer.shows(Audience::Dm))
}

/// The layers of the scene. Returns `true` when one changed.
///
/// Each layer carries two switches, one for each screen. A layer with the
/// DM switch on and the TV switch off holds what the players must not see.
fn layer_panel(
    ui: &mut egui::Ui,
    frame: &mut Frame<'_>,
    active: &mut usize,
    deleting: &mut Option<usize>,
) -> bool {
    let mut edited = false;
    ui.separator();
    ui.heading("Layers");
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Layer").color(MUTE));
        ui.label(egui::RichText::new("Me").color(MUTE));
        ui.label(egui::RichText::new("TV").color(MUTE));
    });
    // The list reads from the top down, and the top layer draws over the
    // rest, so the last layer comes first.
    let mut moved = None;
    let mut gone = None;
    for index in (0..frame.layers.len()).rev() {
        // A question stands in for the row until the DM answers it. A layer
        // that goes takes its maps with it.
        if *deleting == Some(index) {
            let maps = maps_on(frame.maps, index);
            let name = frame.layers[index].name.clone();
            let carry = if maps == 1 {
                "1 map goes with it.".to_owned()
            } else {
                format!("{maps} maps go with it.")
            };
            // The panel is narrow, so the question stands on its own line.
            ui.label(format!("Delete {name}?"));
            ui.label(egui::RichText::new(carry).color(MUTE));
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    gone = Some(index);
                }
                if ui.button("Keep").clicked() {
                    *deleting = None;
                }
            });
            continue;
        }
        let row = ui.horizontal(|ui| {
            // Only the grip drags. A drag source over the whole row would
            // swallow every click meant for the switches.
            ui.dnd_drag_source(egui::Id::new(("layer", index)), index, |ui| {
                ui.label(egui::RichText::new(GRIP).color(MUTE));
            });
            edited |= layer_row(ui, frame.layers, index, active, deleting);
        });
        // A row under a layer on its way shows where that layer lands.
        if row.response.dnd_hover_payload::<usize>().is_some() {
            let rect = row.response.rect;
            ui.painter()
                .hline(rect.x_range(), rect.top(), egui::Stroke::new(2.0, ACCENT));
        }
        if let Some(from) = row.response.dnd_release_payload::<usize>() {
            moved = Some((*from, index));
        }
    }
    if let Some((from, to)) = moved {
        move_layer(frame.layers, frame.maps, from, to);
        *active = index_after_move(*active, from, to);
        edited = true;
    }
    if let Some(index) = gone {
        delete_layer(frame.layers, frame.maps, index);
        *deleting = None;
        *active = (*active).min(frame.layers.len() - 1);
        edited = true;
    }
    if ui.button("New layer").clicked() {
        let name = format!("Layer {}", frame.layers.len() + 1);
        frame.layers.push(Layer::new(name));
        *active = frame.layers.len() - 1;
        edited = true;
    }
    ui.label(egui::RichText::new("A new map joins the marked layer.").color(MUTE));
    edited
}

/// One layer in the panel. Returns `true` when the DM changed it.
///
/// The mark on the left says where a new map goes, and a drag on the row
/// moves the layer up or down the pile.
fn layer_row(
    ui: &mut egui::Ui,
    layers: &mut [Layer],
    index: usize,
    active: &mut usize,
    deleting: &mut Option<usize>,
) -> bool {
    let last = layers.len() == 1;
    let mut edited = false;
    let Some(layer) = layers.get_mut(index) else {
        return false;
    };
    {
        let ui = &mut *ui;
        let marked = *active == index;
        let mark = if marked { "*" } else { "\u{00b7}" };
        let color = if marked { ACCENT } else { MUTE };
        let button = egui::Button::new(egui::RichText::new(mark).color(color)).frame(false);
        if ui.add(button).clicked() {
            *active = index;
        }
        // The row must fit the panel. A wider name field pushes the last
        // button past the edge, where egui paints it but no click reaches
        // it.
        let name = egui::TextEdit::singleline(&mut layer.name).desired_width(78.0);
        let field = ui.add(name);
        if field.gained_focus() {
            *active = index;
        }
        edited |= field.changed();
        edited |= ui.checkbox(&mut layer.show_dm, "").changed();
        edited |= ui.checkbox(&mut layer.show_tv, "").changed();
        // A scene with no layer has nowhere to put a map, so the last one
        // has no way out.
        let button = egui::Button::new(egui::RichText::new("x").color(MUTE)).frame(false);
        if ui.add_enabled(!last, button).clicked() {
            *deleting = Some(index);
        }
    }
    edited
}

/// The properties of the TV box. Returns `true` when the zoom changed.
///
/// The DM drags a corner handle to reach a zoom by eye. This field reaches
/// an exact one, such as 50 percent for a map twice the size of the table.
fn box_properties(ui: &mut egui::Ui, tv_box: &mut TvBox) -> bool {
    let mut percent = tv_box.zoom(TV_WIDTH_INCHES) * 100.0;
    let mut edited = false;
    ui.separator();
    ui.heading("TV box");
    ui.horizontal(|ui| {
        ui.label("Zoom");
        let field = egui::DragValue::new(&mut percent)
            .suffix(" %")
            .range(MIN_ZOOM_PERCENT..=MAX_ZOOM_PERCENT);
        if ui.add(field).changed() {
            tv_box.width = clamp_width(TV_WIDTH_INCHES / (percent / 100.0));
            edited = true;
        }
    });
    ui.label(egui::RichText::new("100 % is true size on the TV").color(MUTE));
    edited
}

/// The properties of the selected map. Returns `true` when one changed.
///
/// The grid size decides the true size of the map: one grid cell is one
/// inch on the canvas. Only a Foundry or a Universal VTT file carries that
/// number, so for a plain PNG or JPEG the DM types it or measures it.
fn map_properties(
    ui: &mut egui::Ui,
    select: &mut Select,
    maps: &mut [MapObject],
    layers: &[Layer],
) -> bool {
    let Some(map) = select.selected.and_then(|i| maps.get_mut(i)) else {
        return false;
    };
    let mut edited = false;
    ui.separator();
    ui.heading("Map");
    ui.horizontal(|ui| {
        ui.label("Layer");
        let open = layers
            .get(map.layer)
            .map_or("", |layer| layer.name.as_str());
        egui::ComboBox::from_id_salt("map_layer")
            .selected_text(open)
            .show_ui(ui, |ui| {
                for (index, layer) in layers.iter().enumerate() {
                    edited |= ui
                        .selectable_value(&mut map.layer, index, &layer.name)
                        .changed();
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Pixels per cell");
        edited |= ui
            .add(egui::DragValue::new(&mut map.grid_px).range(MIN_GRID_PX..=MAX_GRID_PX))
            .changed();
    });
    // The size sits next to the grid size because the two multiply: a map
    // with the right grid size draws at true size only at 100 percent.
    let mut percent = map.scale * 100.0;
    ui.horizontal(|ui| {
        ui.label("Size");
        let field = egui::DragValue::new(&mut percent)
            .suffix(" %")
            .range(MIN_PERCENT..=MAX_PERCENT);
        if ui.add(field).changed() {
            map.scale = percent / 100.0;
            edited = true;
        }
    });
    if ui.button("Measure a cell").clicked() {
        select.measure = Some(Measure::Start);
    }
    if select.measure.is_some() {
        ui.label("Click two corners of one grid cell.");
    }
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
            ui.painter_at(rect)
                .line_segment([view.to_screen(first), pos], egui::Stroke::new(2.0, ACCENT));
        }
        return edited;
    }

    // Handles of the selected map, in points: four corners, then rotation.
    let handles: Vec<egui::Pos2> = select
        .selected
        .and_then(|i| frame.maps.get(i))
        .filter(|map| shown_to_dm(map, frame.layers))
        .and_then(|map| (frame.size_of)(&map.path).map(|size| map.corners(size)))
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
        press(select, frame, &handles, pos, view.to_world(pos));
    }
    let mut edited = false;
    if let (true, Some(pos)) = (pointer.down(), pointer.pos) {
        edited |= apply_drag(select, frame, view.to_world(pos), snap);
    } else {
        select.drag = None;
    }
    let icon = select.drag.and_then(|drag| match drag {
        Drag::Scale { .. } => Some(egui::CursorIcon::ResizeNwSe),
        Drag::Rotate { .. } => Some(egui::CursorIcon::Grabbing),
        Drag::Move { .. } => None,
    });
    set_cursor(ui, select.drag.is_some(), icon, pointer, &handles);

    edited |= keys(ui, select, frame.maps);

    if handles.len() == 5 {
        draw_selection(&ui.painter_at(rect), &handles);
    }
    edited
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
) -> bool {
    let (rect, view, pointer) = canvas_area(ui, frame.camera, viewport, true, zoom_goes_to);
    let corners = frame.tv_box.corners(frame.tv_viewport);
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
                start_width: frame.tv_box.width,
                start_cursor: cursor,
            })
        } else if hit_test(cursor, &corners) {
            Some(BoxDrag::Move {
                start_center: frame.tv_box.center,
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
            edited = drag_box(frame.tv_box, drag, view.to_world(pos));
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
                let resized = (frame.tv_box.width - start_width).abs() > f64::EPSILON;
                let snapped = snap_to_true_size(
                    frame.tv_box.width,
                    TV_WIDTH_INCHES,
                    frame.settings.snap_percent,
                );
                if resized && (snapped - frame.tv_box.width).abs() > f64::EPSILON {
                    frame.tv_box.width = snapped;
                    edited = true;
                }
            }
        }
    }
    // The keys wait for the drag to end. A drag rewrites the box from its
    // start state every frame, so a key press in the middle of one is lost.
    if table.drag.is_none() {
        edited |= arrow_keys(ui, frame.tv_box);
    }

    // Ctrl and Alt with the wheel reach a zoom without a drag on a handle.
    // Figma has no gesture that resizes an object with the wheel, so Alt
    // marks this one as ours. The canvas already gave the gesture up.
    if let (Some(pinch), true) = (pointer.box_zoom, pointer.hovered) {
        let zoom = frame.tv_box.zoom(TV_WIDTH_INCHES) * f64::from(pinch);
        frame.tv_box.width = clamp_width(TV_WIDTH_INCHES / zoom);
        edited = true;
    }

    let icon = table.drag.map(|_| egui::CursorIcon::ResizeNwSe);
    set_cursor(ui, table.drag.is_some(), icon, pointer, &handles);
    let zoom = frame.tv_box.zoom(TV_WIDTH_INCHES);
    draw_tv_box(&ui.painter_at(rect), rect, &handles, zoom);
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
fn draw_tv_box(painter: &egui::Painter, canvas: egui::Rect, handles: &[egui::Pos2], zoom: f64) {
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
        painter.rect_filled(wash.intersect(canvas), 0.0, DIM);
    }
    let stroke = egui::Stroke::new(2.0, ACCENT);
    painter.add(egui::Shape::closed_line(handles.to_vec(), stroke));
    for handle in handles {
        painter.rect_filled(
            egui::Rect::from_center_size(*handle, egui::vec2(HANDLE_SIZE, HANDLE_SIZE)),
            0.0,
            ACCENT,
        );
    }
    // DESIGN.md 5.3: the zoom stands above the top-right corner, and takes
    // the accent while the box is at true size. A box wider than the canvas
    // keeps its label on screen, since the corner it belongs to is not.
    let corner = egui::pos2(handles[1].x, handles[1].y - ZOOM_LABEL_GAP);
    painter.text(
        canvas.shrink(ZOOM_LABEL_GAP).clamp(corner),
        egui::Align2::RIGHT_BOTTOM,
        format!("{} %", (zoom * 100.0).round()),
        egui::FontId::proportional(ZOOM_LABEL_SIZE),
        if at_true_size(zoom) { ACCENT } else { INK },
    );
}

/// Starts the drag for a press at `pos` (points) and `cursor` (world).
fn press(
    select: &mut Select,
    frame: &mut Frame<'_>,
    handles: &[egui::Pos2],
    pos: egui::Pos2,
    cursor: (f64, f64),
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
    select.drag = match (hit_handle, select.selected.and_then(|i| frame.maps.get(i))) {
        (Some(4), Some(map)) => Some(Drag::Rotate {
            start_rotation: map.rotation,
            start_cursor: cursor,
        }),
        (Some(_), Some(map)) => Some(Drag::Scale {
            start_scale: map.scale,
            start_cursor: cursor,
        }),
        _ => {
            select.selected = frame.maps.iter().enumerate().rev().find_map(|(i, map)| {
                // A layer the DM cannot see takes no clicks from the DM.
                if !shown_to_dm(map, frame.layers) {
                    return None;
                }
                let size = (frame.size_of)(&map.path)?;
                hit_test(cursor, &map.corners(size)).then_some(i)
            });
            select.selected.map(|i| Drag::Move {
                start_center: frame.maps[i].center,
                start_cursor: cursor,
            })
        }
    };
}

/// What the keyboard does to the selected map. Returns `true` when it
/// changed one.
fn keys(ui: &egui::Ui, select: &mut Select, maps: &mut [MapObject]) -> bool {
    // A number in the panel takes the keyboard first, or `R` and `F` would
    // turn and flip the map while the DM types.
    if ui.ctx().egui_wants_keyboard_input() {
        return false;
    }
    let mut edited = false;
    // Keys act on the selection when no drag is in progress, since a drag
    // rewrites the map from its start state every frame. Held keys do not
    // repeat: one press is one turn, one flip, or one step in the stack.
    if let (None, Some(i)) = (select.drag, select.selected) {
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
                match key {
                    egui::Key::R => maps[i].rotation += std::f64::consts::FRAC_PI_2,
                    egui::Key::F if modifiers.shift => {
                        maps[i].flip_y = !maps[i].flip_y;
                    }
                    egui::Key::F => maps[i].flip_x = !maps[i].flip_x,
                    // Plus is the numpad key; Equals is the shared "=/+" main
                    // row key, which egui reports without needing Shift.
                    egui::Key::Plus | egui::Key::Equals => {
                        maps[i].scale = step_scale(maps[i].scale, true);
                    }
                    egui::Key::Minus => {
                        maps[i].scale = step_scale(maps[i].scale, false);
                    }
                    egui::Key::PageUp => {
                        let Some(j) = reorder(i, maps.len(), true) else {
                            continue;
                        };
                        maps.swap(i, j);
                        select.selected = Some(j);
                    }
                    egui::Key::PageDown => {
                        let Some(j) = reorder(i, maps.len(), false) else {
                            continue;
                        };
                        maps.swap(i, j);
                        select.selected = Some(j);
                    }
                    _ => continue,
                }
                edited = true;
            }
        });
    }
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
    let Some(map) = select.selected.and_then(|i| frame.maps.get_mut(i)) else {
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
fn frame_tv_box(ui: &egui::Ui, frame: &mut Frame<'_>, rect: egui::Rect, viewport: (u32, u32)) {
    let asked = ui.input(|i| i.key_pressed(egui::Key::T)) && !ui.ctx().egui_wants_keyboard_input();
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
    let size = (frame.tv_box.width, frame.tv_box.height(frame.tv_viewport));
    *frame.camera = fit(frame.tv_box.center, size, area, viewport, FRAME_MARGIN);
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
    let (Some(drag), Some(i)) = (select.drag, select.selected) else {
        return false;
    };
    let size = (frame.size_of)(&frame.maps[i].path);
    let map = &mut frame.maps[i];
    match drag {
        Drag::Move {
            start_center,
            start_cursor,
        } => {
            // A press without motion selects and nothing more: snapping a
            // map that was placed off the grid would shift it.
            if cursor == start_cursor {
                return false;
            }
            let moved = (
                start_center.0 + cursor.0 - start_cursor.0,
                start_center.1 + cursor.1 - start_cursor.1,
            );
            map.center = moved;
            // A snapped move steps along the map's own grid. A free move
            // decides where that grid starts, so letting Ctrl go never
            // pulls the map back off the spot the DM chose.
            if let Some(size) = size {
                let corners = map.corners(size);
                if snap {
                    map.center = snap_corner(moved, &corners, map.snap_offset);
                } else {
                    map.snap_offset = corner_offset(&corners);
                }
            }
        }
        Drag::Scale {
            start_scale,
            start_cursor,
        } => map.scale = start_scale * scale_from_drag(map.center, start_cursor, cursor),
        Drag::Rotate {
            start_rotation,
            start_cursor,
        } => {
            map.rotation =
                rotation_from_drag(start_rotation, map.center, start_cursor, cursor, snap);
        }
    }
    true
}

/// The outline, the corner handles and the rotation handle of the selection.
fn draw_selection(painter: &egui::Painter, handles: &[egui::Pos2]) {
    let stroke = egui::Stroke::new(2.0, ACCENT);
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
            ACCENT,
        );
    }
    painter.circle_filled(handles[4], HANDLE_SIZE / 2.0, ACCENT);
}

#[cfg(test)]
mod tests {
    use super::Settings;

    fn settings() -> Settings {
        Settings {
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
