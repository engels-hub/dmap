//! dmap: map display for a tabletop RPG table with a TV.
//!
//! One process opens two windows. The DM window holds the editor UI.
//! The TV window fills one display and shows the players' view.

// Rust guideline compliant 2026-02-21

mod camera;
mod color;
mod config;
mod gpu;
mod images;
mod maps;
mod pointer;
mod scene;
mod transform;
mod tv;
mod tvbox;
mod ui;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use egui_winit::winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    monitor::MonitorHandle,
    window::{Fullscreen, Window, WindowId},
};

use crate::camera::{Camera, DEFAULT_PIXELS_PER_INCH};
use crate::config::Config;
use crate::gpu::{Gpu, Pane};
use crate::images::Loader;
use crate::maps::{MapLayer, relative_path};
use crate::pointer::PointerDisc;
use crate::scene::MapObject;
use crate::scene::{Scene, copy_into_scene};
use crate::tv::{display_at, dm_move_target, placement_for, resolve_tv_display};
use crate::tvbox::{TvBox, clamp_snap_percent};
use crate::ui::{DmUi, Frame, Settings};

/// Size the DM window opens with. The design mockups use this frame.
const DM_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1440.0, 900.0);

/// Size of the TV window when no display is free for it.
const TV_FALLBACK_SIZE: LogicalSize<f64> = LogicalSize::new(960.0, 540.0);

/// The file that holds one scene, inside the scene's own folder.
const SCENE_FILE: &str = "scene.json";

/// The scene a first run opens.
const FIRST_SCENE: &str = "New scene";

/// The DM camera at start: the origin in the middle of the view.
const DM_CAMERA: Camera = Camera {
    center: (0.0, 0.0),
    pixels_per_inch: DEFAULT_PIXELS_PER_INCH,
};

fn main() -> Result<()> {
    let mut config = load_config()?;
    let scene_dir = scene_to_open(&mut config);
    std::fs::create_dir_all(&scene_dir)
        .with_context(|| format!("{}: cannot make the scene folder", scene_dir.display()))?;
    let scene = load_scene(&scene_dir)?;
    // Write both files at once. The scene folder then exists on disk, and
    // the next run without an argument finds its way back to it.
    save(&config, &scene, &scene_dir)?;
    let event_loop = EventLoop::new()?;
    let mut app = App {
        config,
        scene_dir,
        scene,
        running: None,
        error: None,
    };
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

/// Reads the config, or starts a new one when there is none.
fn load_config() -> Result<Config> {
    let path = config::config_path();
    match std::fs::read_to_string(&path) {
        Ok(json) => {
            Config::from_json(&json).with_context(|| format!("{}: broken config", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(error) => Err(error).with_context(|| format!("{}: cannot read", path.display())),
    }
}

/// The folder of the scene this run opens.
///
/// A folder on the command line wins. A path to a scene file names its own
/// folder. Without an argument the program opens the scene of the last run,
/// and a first run gets a new one. `config.last_scene` follows the choice.
fn scene_to_open(config: &mut Config) -> PathBuf {
    if let Some(argument) = std::env::args_os().nth(1) {
        let given = PathBuf::from(argument);
        let dir = if given.file_name().is_some_and(|name| name == SCENE_FILE) {
            given.parent().unwrap_or(Path::new(".")).to_path_buf()
        } else {
            given
        };
        config.last_scene = Some(config.remember(&dir));
        return dir;
    }
    let scene = config.last_scene.clone().unwrap_or_else(|| {
        let first = PathBuf::from(FIRST_SCENE);
        config.last_scene = Some(first.clone());
        first
    });
    config.scene_dir(&scene)
}

/// Reads the scene file, or starts an empty scene when there is none.
fn load_scene(dir: &Path) -> Result<Scene> {
    let path = dir.join(SCENE_FILE);
    match std::fs::read_to_string(&path) {
        Ok(json) => {
            Scene::from_json(&json).with_context(|| format!("{}: broken scene", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Scene::default()),
        Err(error) => Err(error).with_context(|| format!("{}: cannot read", path.display())),
    }
}

/// Application state across the event loop.
#[derive(Debug)]
struct App {
    config: Config,
    /// The folder of the open scene. Its images sit beside its scene file.
    scene_dir: PathBuf,
    scene: Scene,
    running: Option<Running>,
    error: Option<anyhow::Error>,
}

/// Everything that exists once the windows and the GPU are up.
#[derive(Debug)]
struct Running {
    gpu: Gpu,
    dm: Pane,
    tv: Pane,
    ui: DmUi,
    pointer: PointerDisc,
    displays: Vec<MonitorHandle>,
    settings: Settings,
    /// Whether the first DM frame has checked the window placement.
    placed: bool,
    /// Where the pointer is over the TV window, in pixels, or `None` when outside.
    tv_pointer: Option<(f32, f32)>,
    /// Folder of the open scene; every map path is a name inside it.
    scene_dir: PathBuf,
    maps: Vec<MapObject>,
    map_layer: MapLayer,
    loader: Loader,
    camera: Camera,
    /// The part of the canvas the TV shows.
    tv_box: TvBox,
}

impl Running {
    fn new(
        event_loop: &ActiveEventLoop,
        config: &Config,
        scene: &Scene,
        scene_dir: PathBuf,
    ) -> Result<Self> {
        let dm_window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("dmap")
                    .with_inner_size(DM_WINDOW_SIZE),
            )?,
        );
        let displays: Vec<_> = event_loop.available_monitors().collect();
        let dm_display = display_of(&dm_window, &displays);
        let tv_display =
            resolve_tv_display(&config.tv_display, &display_names(&displays), dm_display);
        let tv_window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("dmap TV")
                    .with_inner_size(TV_FALLBACK_SIZE)
                    .with_fullscreen(fullscreen_on(&displays, tv_display)),
            )?,
        );
        // The TV draws its own pointer, so the system pointer stays hidden.
        tv_window.set_cursor_visible(false);
        let gpu = Gpu::new(&dm_window)?;
        let dm = gpu.pane(dm_window)?;
        let tv = gpu.pane(tv_window)?;
        let ui = DmUi::new(&gpu, &dm);
        let pointer = PointerDisc::new(&gpu.device, tv.config.format);
        let map_layer = MapLayer::new(&gpu.device, dm.config.format);
        let wake_window = Arc::clone(&dm.window);
        let loader = Loader::spawn(gpu.device.limits().max_texture_dimension_2d, move || {
            wake_window.request_redraw();
        });
        for map in &scene.maps {
            loader.request(scene_dir.join(&map.path));
        }
        Ok(Self {
            gpu,
            dm,
            tv,
            ui,
            pointer,
            displays,
            settings: Settings {
                tv_display,
                swap_windows: config.swap_windows,
                snap_percent: clamp_snap_percent(config.snap_percent),
            },
            placed: false,
            tv_pointer: None,
            scene_dir,
            maps: scene.maps.clone(),
            map_layer,
            loader,
            camera: DM_CAMERA,
            tv_box: scene.tv_box.clamped(),
        })
    }

    /// Handles a window event. Returns `true` when the DM changed a setting.
    fn window_event(&mut self, id: WindowId, event: &WindowEvent) -> Result<bool> {
        let is_dm = id == self.dm.window.id();
        if let (true, WindowEvent::DroppedFile(path)) = (is_dm, event) {
            return Ok(self.add_map(path));
        }
        if is_dm && self.ui.on_event(&self.dm, event) {
            return Ok(false);
        }
        let pane = if is_dm { &mut self.dm } else { &mut self.tv };
        match event {
            WindowEvent::Resized(size) => {
                pane.resize(&self.gpu.device, size.width, size.height);
                // The TV box on the DM screen has the shape of the TV, so a
                // TV that changes size changes what the DM must draw.
                if !is_dm {
                    self.dm.window.request_redraw();
                }
            }
            // The window manager places a new window where it likes, so check
            // after every move that the DM window is not on the TV display.
            // The window manager places a new window where it likes, so check
            // after every move that the DM window is not on the TV display.
            // Swap mode checks only on demand: a swap makes the window manager
            // move the windows, and a check on every move would swap again.
            WindowEvent::Moved(_) if is_dm && !self.settings.swap_windows => {
                move_dm_off_tv(&self.dm.window, &self.displays, self.settings.tv_display);
            }
            WindowEvent::RedrawRequested if is_dm => return self.redraw_dm(),
            WindowEvent::RedrawRequested => {
                let viewport = (pane.config.width, pane.config.height);
                let (device, queue) = (&self.gpu.device, &self.gpu.queue);
                let (pointer, tv_pointer) = (&self.pointer, self.tv_pointer);
                let tv_camera = self.tv_box.camera(viewport);
                let (map_layer, maps) = (&mut self.map_layer, &self.maps);
                self.gpu
                    .clear(pane, color::linear_color(color::CANVAS), |pass| {
                        map_layer.draw(device, queue, pass, maps, &tv_camera, viewport);
                        if let Some(center) = tv_pointer {
                            pointer.draw(queue, pass, center, viewport);
                        }
                    })?;
            }
            WindowEvent::CursorMoved { position, .. } if !is_dm => {
                self.tv_pointer = Some((position.x as f32, position.y as f32));
                self.tv.window.request_redraw();
            }
            WindowEvent::CursorLeft { .. } if !is_dm => {
                self.tv_pointer = None;
                self.tv.window.request_redraw();
            }
            _ => {}
        }
        Ok(false)
    }

    /// Adds a map file at the middle of the DM view.
    ///
    /// Returns `true` when the scene changed.
    fn add_map(&mut self, file: &Path) -> bool {
        let stored = match copy_into_scene(&self.scene_dir, file) {
            Ok(name) => name,
            Err(error) => {
                eprintln!("{error:#}");
                return false;
            }
        };
        self.loader.request(self.scene_dir.join(&stored));
        self.maps.push(MapObject::new(stored, self.camera.center));
        true
    }

    /// Asks for an image file and adds it. Returns `true` when a file was added.
    fn pick_map_file(&mut self) -> bool {
        let picked = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .set_directory(&self.scene_dir)
            .pick_file();
        picked.is_some_and(|file| self.add_map(&file))
    }

    /// Moves finished image files to the GPU.
    fn upload_loaded_images(&mut self) {
        while let Some((file, result)) = self.loader.poll() {
            let stored = relative_path(&self.scene_dir, &file);
            match result {
                Ok(decoded) => {
                    self.map_layer
                        .upload(&self.gpu.device, &self.gpu.queue, stored, &decoded);
                    self.tv.window.request_redraw();
                }
                Err(message) => eprintln!("{}: {message}", file.display()),
            }
        }
    }

    /// Runs a DM frame. Returns `true` when the DM changed a setting.
    fn redraw_dm(&mut self) -> Result<bool> {
        self.upload_loaded_images();
        let before = self.settings.clone();
        let map_layer = &self.map_layer;
        let output = self.ui.run(
            &self.dm,
            Frame {
                displays: &self.displays,
                settings: &mut self.settings,
                maps: &mut self.maps,
                camera: &mut self.camera,
                tv_box: &mut self.tv_box,
                tv_viewport: (self.tv.config.width, self.tv.config.height),
                size_of: &|path| map_layer.size_of(path),
            },
        );
        let viewport = (self.dm.config.width, self.dm.config.height);
        let (device, queue) = (&self.gpu.device, &self.gpu.queue);
        let (map_layer, maps, camera) = (&mut self.map_layer, &self.maps, &self.camera);
        self.ui
            .render(&self.gpu, &mut self.dm, output.paint, |pass| {
                map_layer.draw(device, queue, pass, maps, camera, viewport);
            })?;
        if output.edited {
            self.tv.window.request_redraw();
        }
        let added = output.add_map && self.pick_map_file();
        let settings_changed = self.settings != before;
        // Only a display or a swap moves a window. Every other setting, such
        // as the snap window, must leave the TV alone.
        if self.settings.moves_windows(&before) || !self.placed {
            self.placed = true;
            // Swap first: the swap decides which window is the TV.
            let swapped = self.keep_dm_off_tv();
            // Only a placement change touches the TV window: re-entering full
            // screen takes the keyboard focus away from the DM window.
            if self.settings.moves_windows(&before) || swapped {
                place_tv(&self.tv.window, &self.displays, self.settings.tv_display);
            }
        }
        Ok(added || output.save || settings_changed)
    }

    /// Keeps the DM window off the TV display, by a move or by a role swap.
    ///
    /// Returns `true` when the windows swapped roles.
    fn keep_dm_off_tv(&mut self) -> bool {
        let dm_display = display_of(&self.dm.window, &self.displays);
        let target = dm_move_target(self.settings.tv_display, dm_display, self.displays.len());
        match (target, self.settings.swap_windows) {
            (None, _) => false,
            (Some(_), false) => {
                move_dm_off_tv(&self.dm.window, &self.displays, self.settings.tv_display);
                false
            }
            (Some(target), true) => {
                self.swap_roles(target);
                true
            }
        }
    }

    /// Makes the TV window the DM window and the other way round.
    ///
    /// Nothing has to move, so this works where a program cannot position
    /// its windows, such as Wayland. The new DM window is asked to sit on
    /// display `target`, which only matters where positioning works. The
    /// new TV window still needs `place_tv`.
    fn swap_roles(&mut self, target: usize) {
        std::mem::swap(&mut self.dm, &mut self.tv);
        self.dm.window.set_title("dmap");
        self.dm.window.set_fullscreen(None);
        self.dm.window.set_decorations(true);
        self.dm
            .window
            .set_outer_position(self.displays[target].position());
        self.dm.window.set_maximized(true);
        self.tv.window.set_title("dmap TV");
        self.tv.window.set_decorations(false);
        self.ui = DmUi::new(&self.gpu, &self.dm);
    }

    /// Copies what the DM changed into the config and the scene.
    fn update(&self, config: &mut Config, scene: &mut Scene) {
        config.tv_display = placement_for(self.settings.tv_display, &display_names(&self.displays));
        config.swap_windows = self.settings.swap_windows;
        config.snap_percent = self.settings.snap_percent;
        scene.maps.clone_from(&self.maps);
        scene.tv_box = self.tv_box;
    }
}

/// Puts the TV window full screen on the chosen display, or back into a window.
///
/// A window that is already full screen ignores a change of display on X11,
/// so it leaves full screen and moves there first.
fn place_tv(tv_window: &Window, displays: &[MonitorHandle], tv_display: Option<usize>) {
    tv_window.set_fullscreen(None);
    if let Some(i) = tv_display {
        tv_window.set_outer_position(displays[i].position());
    }
    tv_window.set_fullscreen(fullscreen_on(displays, tv_display));
}

/// The display that holds the window's center.
///
/// winit's own lookup reports the wrong display on some X11 setups, so this
/// compares the window position with the display rectangles. It uses the
/// center rather than the top-left corner: on Windows a maximized window's
/// outer rectangle starts a few pixels off its own monitor (an invisible
/// resize border), which misclassified a maximized DM window as sitting on
/// the neighboring display and sent it into a move-resize loop. Where the
/// position is unknown, as on Wayland, winit's lookup is used instead.
fn display_of(window: &Window, displays: &[MonitorHandle]) -> Option<usize> {
    let Ok(position) = window.outer_position() else {
        let current = window.current_monitor()?;
        return displays.iter().position(|display| *display == current);
    };
    let size = window.inner_size();
    let center = (
        position.x + (size.width / 2).cast_signed(),
        position.y + (size.height / 2).cast_signed(),
    );
    let rects: Vec<_> = displays
        .iter()
        .map(|display| {
            let position = display.position();
            let size = display.size();
            ((position.x, position.y), (size.width, size.height))
        })
        .collect();
    display_at(center, &rects)
}

/// Moves the DM window to another display when the TV took its display.
fn move_dm_off_tv(dm_window: &Window, displays: &[MonitorHandle], tv_display: Option<usize>) {
    let dm_display = display_of(dm_window, displays);
    if let Some(target) = dm_move_target(tv_display, dm_display, displays.len()) {
        // A maximized window ignores a move, so release it first. Maximize
        // again after the move, so the window fits a display with another
        // scale factor.
        dm_window.set_maximized(false);
        dm_window.set_outer_position(displays[target].position());
        dm_window.set_maximized(true);
    }
}

/// The names of the displays, in order.
fn display_names(displays: &[MonitorHandle]) -> Vec<Option<String>> {
    displays.iter().map(MonitorHandle::name).collect()
}

/// Writes the scene beside its images, and the config in its own folder.
///
/// # Errors
///
/// Returns an error when a folder cannot be made or a file cannot be written.
fn save(config: &Config, scene: &Scene, scene_dir: &Path) -> Result<()> {
    let scene_file = scene_dir.join(SCENE_FILE);
    std::fs::write(&scene_file, scene.to_json())
        .with_context(|| format!("{}: cannot save the scene", scene_file.display()))?;
    let config_file = config::config_path();
    if let Some(folder) = config_file.parent() {
        std::fs::create_dir_all(folder)
            .with_context(|| format!("{}: cannot make the config folder", folder.display()))?;
    }
    std::fs::write(&config_file, config.to_json())
        .with_context(|| format!("{}: cannot save the config", config_file.display()))
}

/// Borderless full screen on the chosen display, or `None` for a normal window.
fn fullscreen_on(displays: &[MonitorHandle], index: Option<usize>) -> Option<Fullscreen> {
    index.map(|i| Fullscreen::Borderless(Some(displays[i].clone())))
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        match Running::new(
            event_loop,
            &self.config,
            &self.scene,
            self.scene_dir.clone(),
        ) {
            Ok(running) => self.running = Some(running),
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if event == WindowEvent::CloseRequested {
            event_loop.exit();
            return;
        }
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let result = running.window_event(window_id, &event).and_then(|changed| {
            if changed {
                running.update(&mut self.config, &mut self.scene);
                save(&self.config, &self.scene, &self.scene_dir)?;
            }
            Ok(())
        });
        if let Err(error) = result {
            self.error = Some(error);
            event_loop.exit();
        }
    }
}
