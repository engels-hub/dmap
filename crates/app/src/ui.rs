//! The DM window's UI: egui on top of the wgpu pane.

// Rust guideline compliant 2026-02-21

use std::path::Path;

use anyhow::Result;
use egui_wgpu::wgpu;
use egui_winit::winit::{event::WindowEvent, monitor::MonitorHandle};

use crate::camera::Camera;
use crate::color;
use crate::gpu::{Gpu, Pane, begin_clear_pass};
use crate::scene::MapObject;
use crate::transform::{
    MAX_GRID_PX, MIN_GRID_PX, corner_offset, edge_midpoint, grid_px_from_measure, hit_test,
    pick_handle, reorder, rotation_from_drag, rotation_handle, scale_from_drag, snap_corner,
    step_scale,
};
use crate::tv::display_label;
use crate::tvbox::{TV_PPI, TvBox, clamp_width, snap_to_true_size};

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

/// Size of the zoom label on the TV box, in points. DESIGN.md 5.3.
const ZOOM_LABEL_SIZE: f32 = 14.0;

/// Gap between the top edge of the box and its zoom label, in points.
const ZOOM_LABEL_GAP: f32 = 6.0;

/// The widest snap window the settings accept, in percent. Half of that
/// would already reach from 50 to 150 percent.
const MAX_SNAP_PERCENT: f64 = 50.0;

/// One grid cell on the canvas, in inches. The arrow keys move the TV box
/// by this much.
const CELL: f64 = 1.0;

/// egui state and renderer for one window.
pub struct DmUi {
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    tool: Tool,
    select: Select,
    table: Table,
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

/// Everything one UI frame reads and edits.
pub struct Frame<'a> {
    pub displays: &'a [MonitorHandle],
    pub settings: &'a mut Settings,
    pub maps: &'a mut Vec<MapObject>,
    pub camera: &'a Camera,
    /// The part of the canvas the TV shows.
    pub tv_box: &'a mut TvBox,
    /// Pixel size of the TV window. It gives the box its shape.
    pub tv_viewport: (u32, u32),
    /// Pixel size of a map's image, once loaded.
    pub size_of: &'a dyn Fn(&Path) -> Option<(u32, u32)>,
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

/// What one canvas frame reads from the pointer.
#[derive(Debug, Clone, Copy)]
struct Pointer {
    /// The primary button went down this frame.
    pressed: bool,
    /// The primary button is down now.
    down: bool,
    /// Where the pointer is, in points, or `None` when it is away.
    pos: Option<egui::Pos2>,
    /// The pointer is over the canvas.
    hovered: bool,
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
        Self {
            state,
            renderer,
            tool: Tool::default(),
            select: Select::default(),
            table: Table::default(),
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
        let mut add_map = false;
        let mut edited = false;
        let select = &mut self.select;
        let table = &mut self.table;
        let tool = &mut self.tool;
        let selected_before = select.selected;
        let output = ctx.run_ui(raw_input, |ui| {
            // A click that closes a popup must not reach the canvas.
            let popup_open = egui::Popup::is_any_open(ui.ctx());
            let (pressed, panel_edited) = rail_and_settings(ui, &mut frame, select, tool);
            add_map = pressed;
            edited = panel_edited;
            if !popup_open {
                edited |= match *tool {
                    Tool::Select => canvas(ui, select, &mut frame, viewport),
                    Tool::Table => table_tool(ui, table, &mut frame, viewport),
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
) -> (bool, bool) {
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
            ui.label(egui::RichText::new("either side of 100 %").color(egui::Color32::GRAY));
        });
        if *tool == Tool::Select {
            edited = map_properties(ui, select, frame.maps);
        } else {
            // Another tool does not run the measure, so the panel must not
            // leave a measure armed behind it.
            select.measure = None;
        }
    });
    (add_map, edited)
}

/// The properties of the selected map. Returns `true` when one changed.
///
/// The grid size decides the true size of the map: one grid cell is one
/// inch on the canvas. Only a Foundry or a Universal VTT file carries that
/// number, so for a plain PNG or JPEG the DM types it or measures it.
fn map_properties(ui: &mut egui::Ui, select: &mut Select, maps: &mut [MapObject]) -> bool {
    let Some(map) = select.selected.and_then(|i| maps.get_mut(i)) else {
        return false;
    };
    let mut edited = false;
    ui.separator();
    ui.heading("Map");
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
) -> bool {
    let (rect, view, pointer) = canvas_area(ui, *frame.camera, viewport);
    let snap = !ui.input(|i| i.modifiers.ctrl);

    // The measure tool takes the canvas for two clicks. Escape gives up.
    if select.measure.is_some() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            select.measure = None;
            return false;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        let mut edited = false;
        if let (true, true, Some(pos)) = (pointer.pressed, pointer.hovered, pointer.pos) {
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
    if let (true, true, Some(pos)) = (pointer.pressed, pointer.hovered, pointer.pos) {
        press(select, frame, &handles, pos, view.to_world(pos));
    }
    let mut edited = false;
    if let (true, Some(pos)) = (pointer.down, pointer.pos) {
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
) -> bool {
    let (rect, view, pointer) = canvas_area(ui, *frame.camera, viewport);
    let corners = frame.tv_box.corners(frame.tv_viewport);
    let handles: Vec<egui::Pos2> = corners.iter().map(|&c| view.to_screen(c)).collect();
    let handle_points: Vec<(f64, f64)> = handles
        .iter()
        .map(|h| (f64::from(h.x), f64::from(h.y)))
        .collect();

    if let (true, true, Some(pos)) = (pointer.pressed, pointer.hovered, pointer.pos) {
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
    match (pointer.down, pointer.pos, table.drag) {
        (true, Some(pos), Some(drag)) => {
            edited = drag_box(frame.tv_box, drag, view.to_world(pos));
        }
        (true, _, _) => {}
        // The drag is over. A size that came close to true size takes it
        // exactly, so that a miniature covers the cell it stands in. Alt
        // keeps the size the DM dragged. PLAN.md section 3.2.
        _ => {
            if let (Some(BoxDrag::Resize { .. }), false) = (table.drag.take(), alt) {
                let snapped = snap_to_true_size(
                    frame.tv_box.width,
                    frame.tv_viewport,
                    TV_PPI,
                    frame.settings.snap_percent,
                );
                edited |= (snapped - frame.tv_box.width).abs() > f64::EPSILON;
                frame.tv_box.width = snapped;
            }
        }
    }
    // The keys wait for the drag to end. A drag rewrites the box from its
    // start state every frame, so a key press in the middle of one is lost.
    if table.drag.is_none() {
        edited |= arrow_keys(ui, frame.tv_box);
    }

    let icon = table.drag.map(|_| egui::CursorIcon::ResizeNwSe);
    set_cursor(ui, table.drag.is_some(), icon, pointer, &handles);
    let zoom = frame.tv_box.zoom(frame.tv_viewport, TV_PPI);
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
    // the accent while the box is at true size.
    let at_true_size = (zoom - 1.0).abs() < 1e-9;
    painter.text(
        egui::pos2(handles[1].x, handles[1].y - ZOOM_LABEL_GAP),
        egui::Align2::RIGHT_BOTTOM,
        format!("{} %", (zoom * 100.0).round()),
        egui::FontId::proportional(ZOOM_LABEL_SIZE),
        if at_true_size { ACCENT } else { INK },
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
    camera: Camera,
    viewport: (u32, u32),
) -> (egui::Rect, View, Pointer) {
    let rect = ui.available_rect_before_wrap();
    let response = ui.interact(rect, ui.id().with("canvas"), egui::Sense::click_and_drag());
    let view = View::new(ui, camera, viewport);
    let (pressed, down, pos) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.interact_pos(),
        )
    });
    let pointer = Pointer {
        pressed,
        down,
        pos,
        hovered: response.hovered(),
    };
    (rect, view, pointer)
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
