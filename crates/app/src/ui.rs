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
    hit_test, pick_handle, rotation_from_drag, rotation_handle, scale_from_drag, snap_corner,
};
use crate::tv::display_label;

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

/// egui state and renderer for one window.
pub struct DmUi {
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    select: Select,
    /// A map changed since the last save.
    dirty: bool,
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
}

/// Everything one UI frame reads and edits.
pub struct Frame<'a> {
    pub displays: &'a [MonitorHandle],
    pub settings: &'a mut Settings,
    pub maps: &'a mut Vec<MapObject>,
    pub camera: &'a Camera,
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
            select: Select::default(),
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
        let output = ctx.run_ui(raw_input, |ui| {
            // A click that closes a popup must not reach the canvas.
            let popup_open = egui::Popup::is_any_open(ui.ctx());
            add_map = rail_and_settings(ui, frame.displays, frame.settings);
            if !popup_open {
                edited = canvas(ui, select, &mut frame, viewport);
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
            .any(|viewport| viewport.repaint_delay.is_zero());

        // Save once a drag is over, not on every frame of it.
        self.dirty |= edited;
        let save = self.dirty && self.select.drag.is_none();
        if save {
            self.dirty = false;
        }
        UiOutput {
            add_map,
            edited,
            save,
            paint: Paint {
                jobs,
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
                self.renderer.render(&mut pass, &paint_jobs, &screen);
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

/// The rail and the settings panel. Returns `true` when Add map was pressed.
fn rail_and_settings(
    ui: &mut egui::Ui,
    displays: &[MonitorHandle],
    settings: &mut Settings,
) -> bool {
    let mut add_map = false;
    egui::Panel::left("rail")
        .exact_size(RAIL_WIDTH)
        .resizable(false)
        .show(ui, |ui| {
            ui.label("dmap");
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
        let selected = settings
            .tv_display
            .map_or_else(|| "Window".to_owned(), label);
        egui::ComboBox::from_id_salt("tv_display")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut settings.tv_display, None, "Window");
                for i in 0..displays.len() {
                    ui.selectable_value(&mut settings.tv_display, Some(i), label(i));
                }
            });
        ui.checkbox(&mut settings.swap_windows, "Swap mode (Wayland compat)");
    });
    add_map
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
    let rect = ui.available_rect_before_wrap();
    let response = ui.interact(rect, ui.id().with("canvas"), egui::Sense::click_and_drag());
    let ppp = f64::from(ui.ctx().pixels_per_point());
    let to_world = |pos: egui::Pos2| {
        frame
            .camera
            .screen_to_world((f64::from(pos.x) * ppp, f64::from(pos.y) * ppp), viewport)
    };
    let to_screen = |world: (f64, f64)| {
        let (x, y) = frame.camera.world_to_screen(world, viewport);
        egui::pos2((x / ppp) as f32, (y / ppp) as f32)
    };
    let snap = !ui.input(|i| i.modifiers.ctrl);

    // Handles of the selected map, in points: four corners, then rotation.
    let handles: Vec<egui::Pos2> = select
        .selected
        .and_then(|i| frame.maps.get(i))
        .and_then(|map| (frame.size_of)(&map.path).map(|size| map.corners(size)))
        .map(|corners| {
            let mut handles: Vec<egui::Pos2> = corners.iter().map(|&c| to_screen(c)).collect();
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
    let (pressed, down, pointer_pos) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.interact_pos(),
        )
    });
    if let (true, true, Some(pos)) = (pressed, response.hovered(), pointer_pos) {
        press(select, frame, &handles, pos, to_world(pos));
    }
    let mut edited = false;
    if let (true, Some(pos)) = (down, pointer_pos) {
        edited |= apply_drag(select, frame, to_world(pos), snap);
    } else {
        select.drag = None;
    }

    // Keys act on the selection when no drag is in progress, since a drag
    // rewrites the map from its start state every frame. Held keys do not
    // repeat: one press is one turn or one flip.
    if let (None, Some(map)) = (
        select.drag,
        select.selected.and_then(|i| frame.maps.get_mut(i)),
    ) {
        ui.input(|i| {
            for event in &i.events {
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
                    egui::Key::R => map.rotation += std::f64::consts::FRAC_PI_2,
                    egui::Key::F if modifiers.shift => map.flip_y = !map.flip_y,
                    egui::Key::F => map.flip_x = !map.flip_x,
                    _ => continue,
                }
                edited = true;
            }
        });
    }

    if handles.len() == 5 {
        draw_selection(&ui.painter_at(rect), &handles);
    }
    edited
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
            if let (true, Some(size)) = (snap, size) {
                map.center = snap_corner(moved, &map.corners(size));
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
    let top_mid = egui::pos2(
        f32::midpoint(handles[0].x, handles[1].x),
        f32::midpoint(handles[0].y, handles[1].y),
    );
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
