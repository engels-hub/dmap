//! dmap: map display for a tabletop RPG table with a TV.
//!
//! One process opens two windows. The DM window holds the editor UI.
//! The TV window fills one display and shows the players' view.

// A release build opens no console beside its windows. Windows gives a
// console to every program that does not say otherwise, and that one sat
// behind the map window all session: a DM who closed it killed the run.
// A debug build keeps the console, so a developer still reads what the
// program writes. The attribute means nothing on Linux.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Rust guideline compliant 2026-02-21

mod camera;
mod color;
mod command;
mod config;
mod gpu;
mod grid;
mod icon;
mod icons;
mod images;
mod maps;
mod pointer;
mod scene;
mod theme;
mod transform;
mod tv;
mod tvbox;
mod ui;
mod widget;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use egui_winit::winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    monitor::MonitorHandle,
    window::{Fullscreen, Window, WindowId},
};

use crate::camera::{Camera, DEFAULT_PIXELS_PER_INCH};
use crate::command::{History, reshape};
use crate::config::Config;
use crate::gpu::{Gpu, Pane};
use crate::grid::GridLayer;
use crate::images::Loader;
use crate::maps::{MapLayer, relative_path};
use crate::pointer::PointerDisc;
use crate::scene::copy_into_scene;
use crate::scene::{Asset, Audience, Node, Scene, draw_order, push_into};
use crate::tv::{display_at, dm_move_target, placement_for, resolve_tv_display};
use crate::tvbox::clamp_snap_percent;
use crate::ui::{DmUi, Frame, SceneCommand, Settings};

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
    let scene_dir = scene_to_open(&mut config)?;
    std::fs::create_dir_all(&scene_dir)
        .with_context(|| format!("{}: cannot make the scene folder", scene_dir.display()))?;
    let scene = load_scene(&scene_dir)?;
    // The config remembers this scene for the next run. The scene file is
    // written only when it is missing, so a scene on a read-only stick
    // still opens, and a write that fails does not end the run.
    if let Err(error) = save_config(&config) {
        eprintln!("{error:#}");
    }
    if !scene_dir.join(SCENE_FILE).exists()
        && let Err(error) = save_scene(&scene, &scene_dir)
    {
        eprintln!("{error:#}");
    }
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
fn scene_to_open(config: &mut Config) -> Result<PathBuf> {
    if let Some(argument) = std::env::args_os().nth(1) {
        // A path from the shell is often relative to the folder the DM
        // stood in. The config outlives that folder, so it holds a whole
        // path or a name, and never a way back to somewhere else.
        let given = std::path::absolute(PathBuf::from(argument))
            .context("cannot work out the folder of that scene")?;
        let dir = if given.file_name().is_some_and(|name| name == SCENE_FILE) {
            given
                .parent()
                .context("a scene file needs a folder around it")?
                .to_path_buf()
        } else if given.is_file() {
            bail!("{}: a scene is a folder, not a file", given.display());
        } else {
            given
        };
        config.last_scene = Some(config.remember(&dir));
        return Ok(dir);
    }
    let scene = config.last_scene.clone().unwrap_or_else(|| {
        let first = PathBuf::from(FIRST_SCENE);
        config.last_scene = Some(first.clone());
        first
    });
    Ok(config.scene_dir(&scene))
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

/// What one event left for the program to do.
#[derive(Debug, Default)]
struct Outcome {
    /// Write the config and the scene.
    save: bool,
    /// What the DM asked the scenes dialog to do.
    scene: Option<SceneCommand>,
}

impl Outcome {
    /// An outcome that only writes the files.
    fn saving(save: bool) -> Self {
        Self { save, scene: None }
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
    /// The folder that holds every scene.
    scenes_dir: PathBuf,
    /// What went wrong with the last thing the scenes dialog asked for.
    scene_error: String,
    /// The group a new asset joins.
    active_group: scene::NodeId,
    /// The tree the DM works on.
    scene: Scene,
    /// Every change the DM made to that tree, for undo and redo.
    ///
    /// The stack belongs to the open scene, not to the files: a save
    /// leaves it alone, so the DM saves and undoes past the save. Another
    /// scene on the canvas clears it.
    history: History,
    map_layer: MapLayer,
    grid_layer: GridLayer,
    loader: Loader,
    camera: Camera,
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
                    .with_title(window_title(&scene_dir))
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
        let grid_layer = GridLayer::new(&gpu.device, dm.config.format);
        let wake_window = Arc::clone(&dm.window);
        let loader = Loader::spawn(gpu.device.limits().max_texture_dimension_2d, move || {
            wake_window.request_redraw();
        });
        for asset in scene::assets(scene) {
            loader.request(scene_dir.join(&asset.path));
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
                theme: config.theme,
                ui_scale: crate::theme::clamp_scale(config.ui_scale),
            },
            placed: false,
            tv_pointer: None,
            scene_dir,
            scenes_dir: config.scenes_dir.clone(),
            scene_error: String::new(),
            active_group: scene::ROOT_ID,
            scene: scene.clone(),
            history: History::default(),
            map_layer,
            grid_layer,
            loader,
            camera: DM_CAMERA,
        })
    }

    /// Handles a window event. Returns what the program must do next.
    fn window_event(&mut self, id: WindowId, event: &WindowEvent) -> Result<Outcome> {
        let is_dm = id == self.dm.window.id();
        if let (true, WindowEvent::DroppedFile(path)) = (is_dm, event) {
            return Ok(Outcome::saving(self.add_map(path)));
        }
        if is_dm && self.ui.on_event(&self.dm, event) {
            return Ok(Outcome::default());
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
                let canvas = self.settings.theme.tokens().canvas;
                let tv_camera = self.scene.tv_box.camera(viewport);
                // The TV draws what it shows, and every map at full strength.
                let shown: Vec<(&Asset, f32)> = draw_order(&self.scene, Audience::Tv)
                    .into_iter()
                    .map(|asset| (asset, maps::FULL_STRENGTH))
                    .collect();
                let map_layer = &mut self.map_layer;
                let grid_layer = &self.grid_layer;
                let line = self.settings.theme.tokens().grid_line();
                let width = pane.window.scale_factor() as f32;
                self.gpu.clear(pane, color::linear_token(canvas), |pass| {
                    map_layer.draw(device, queue, pass, &shown, &tv_camera, viewport);
                    // DESIGN.md 5.1: one grid covers the canvas and it
                    // lies over every map, on both screens.
                    grid_layer.draw(queue, pass, &tv_camera, viewport, line, width);
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
        Ok(Outcome::default())
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
        // A new asset joins the group the DM works in. That group may
        // have gone since the DM marked it, and the root is always there.
        if !scene::has_group(&self.scene, self.active_group) {
            self.active_group = scene::ROOT_ID;
        }
        let id = self.scene.next_id();
        let asset = Asset::new(id, stored, self.camera.center);
        let into = self.active_group;
        let Some(change) = reshape(&mut self.scene, "Add a map", |scene| {
            push_into(scene, into, Node::Asset(asset));
        }) else {
            return false;
        };
        self.history.kept(change);
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
            // The scene may have changed while the image decoded. A result
            // from another folder is not part of what the canvas shows.
            if file.parent() != Some(self.scene_dir.as_path()) {
                continue;
            }
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

    /// Runs a DM frame. Returns what the program must do next.
    fn redraw_dm(&mut self) -> Result<Outcome> {
        self.upload_loaded_images();
        let before = self.settings.clone();
        let map_layer = &self.map_layer;
        let output = self.ui.run(
            &self.dm,
            Frame {
                displays: &self.displays,
                settings: &mut self.settings,
                scene: &mut self.scene,
                history: &mut self.history,
                camera: &mut self.camera,
                scene_dir: &self.scene_dir,
                list_scenes: &|| config::scene_list(&self.scenes_dir),
                scene_error: &self.scene_error,
                scenes_dir: &self.scenes_dir,
                tv_viewport: (self.tv.config.width, self.tv.config.height),
                size_of: &|path| map_layer.size_of(path),
            },
        );
        let viewport = (self.dm.config.width, self.dm.config.height);
        let (device, queue) = (&self.gpu.device, &self.gpu.queue);
        // DESIGN.md 5.6: a map the TV does not show draws faint here, so
        // the DM sees at a glance what the players cannot.
        let shown: Vec<(&Asset, f32)> = crate::scene::dm_draw_order(&self.scene)
            .into_iter()
            .map(|(asset, on_tv)| {
                let strength = if on_tv {
                    maps::FULL_STRENGTH
                } else {
                    maps::HIDDEN_STRENGTH
                };
                (asset, strength)
            })
            .collect();
        let (map_layer, camera) = (&mut self.map_layer, &self.camera);
        let grid_layer = &self.grid_layer;
        let tokens = self.settings.theme.tokens();
        let line = tokens.grid_line();
        let width = self.dm.window.scale_factor() as f32;
        self.ui.render(
            &self.gpu,
            &mut self.dm,
            output.paint,
            tokens.canvas,
            |pass| {
                map_layer.draw(device, queue, pass, &shown, camera, viewport);
                grid_layer.draw(queue, pass, camera, viewport, line, width);
            },
        )?;
        if output.edited {
            self.tv.window.request_redraw();
        }
        self.active_group = output.active_group;
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
        Ok(Outcome {
            save: added || output.save || settings_changed,
            scene: output.scene,
        })
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

    /// Puts another scene on the canvas.
    ///
    /// The images of the old scene leave the GPU. A scene names its images
    /// by the file beside it, so two scenes can hold a `grid.png` and the
    /// new one must not draw the old one.
    fn open_scene(&mut self, dir: PathBuf, scene: &Scene) {
        self.scene_dir = dir;
        self.scene = scene.clone();
        self.scene.tv_box = scene.tv_box.clamped();
        // The changes belong to the scene that has left the canvas.
        self.history.clear();
        self.map_layer.clear();
        self.reload_images();
        self.dm.window.set_title(&window_title(&self.scene_dir));
        self.dm.window.request_redraw();
        self.tv.window.request_redraw();
    }

    /// Asks the loader for every image of the open scene again.
    ///
    /// A scene that moves takes its images with it, so what is in flight
    /// carries the old folder and never arrives.
    fn reload_images(&self) {
        for asset in scene::assets(&self.scene) {
            self.loader.request(self.scene_dir.join(&asset.path));
        }
    }

    /// Copies what the DM changed into the config and the scene.
    fn update(&self, config: &mut Config, scene: &mut Scene) {
        config.tv_display = placement_for(self.settings.tv_display, &display_names(&self.displays));
        config.swap_windows = self.settings.swap_windows;
        config.snap_percent = self.settings.snap_percent;
        config.theme = self.settings.theme;
        config.ui_scale = self.settings.ui_scale;
        scene.clone_from(&self.scene);
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

/// The title of the DM window: the program and the open scene.
fn window_title(scene_dir: &Path) -> String {
    format!("dmap: {}", scene_name(scene_dir))
}

/// The name of a scene, which is the name of its folder.
fn scene_name(scene_dir: &Path) -> String {
    scene_dir.file_name().map_or_else(
        || "scene".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Shows a folder in the file manager of the desktop.
///
/// # Errors
///
/// Returns an error when the file manager cannot be started.
fn reveal(dir: &Path) -> Result<()> {
    let program = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(dir)
        .spawn()
        .with_context(|| format!("{}: cannot show the folder", dir.display()))?;
    Ok(())
}

/// Writes the scene beside its images, and the config in its own folder.
///
/// # Errors
///
/// Returns an error when a folder cannot be made or a file cannot be written.
fn save(config: &Config, scene: &Scene, scene_dir: &Path) -> Result<()> {
    save_scene(scene, scene_dir)?;
    save_config(config)
}

/// Writes the scene file beside its images.
///
/// # Errors
///
/// Returns an error when the file cannot be written.
fn save_scene(scene: &Scene, scene_dir: &Path) -> Result<()> {
    let scene_file = scene_dir.join(SCENE_FILE);
    std::fs::write(&scene_file, scene.to_json())
        .with_context(|| format!("{}: cannot save the scene", scene_file.display()))
}

/// Writes the config in the folder the desktop keeps configs in.
///
/// # Errors
///
/// Returns an error when the folder or the file cannot be written.
fn save_config(config: &Config) -> Result<()> {
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

impl App {
    /// Handles one window event and does what it left behind.
    ///
    /// # Errors
    ///
    /// Returns an error when a file cannot be written or read.
    fn handle_event(
        &mut self,
        running: &mut Running,
        window_id: WindowId,
        event: &WindowEvent,
    ) -> Result<()> {
        let outcome = running.window_event(window_id, event)?;
        if outcome.save {
            running.update(&mut self.config, &mut self.scene);
            // A write that fails leaves the session alone. The DM keeps
            // working, and the message says what went wrong.
            if let Err(error) = save(&self.config, &self.scene, &self.scene_dir) {
                eprintln!("{error:#}");
                running.scene_error = format!("{error:#}");
            }
        }
        if let Some(command) = outcome.scene {
            // A dialog that asks for the impossible, such as a name another
            // scene holds, says so and the session carries on.
            running.scene_error = match self.run_scene_command(running, command) {
                Ok(()) => String::new(),
                Err(error) => format!("{error:#}"),
            };
            running.dm.window.request_redraw();
        }
        Ok(())
    }

    /// Does what the scenes dialog asked for.
    ///
    /// # Errors
    ///
    /// Returns an error when the folder work fails, such as a name another
    /// scene already holds.
    fn run_scene_command(&mut self, running: &mut Running, command: SceneCommand) -> Result<()> {
        let scenes_dir = self.config.scenes_dir.clone();
        match command {
            SceneCommand::Open(name) => self.open_scene(running, &name)?,
            SceneCommand::New => {
                let name = config::new_scene(&scenes_dir, FIRST_SCENE)?;
                self.open_scene(running, &name)?;
            }
            SceneCommand::Rename { from, to } => {
                let to = config::rename_scene(&scenes_dir, &from, &to)?;
                if self.scene_dir == scenes_dir.join(&from) {
                    self.scene_dir = scenes_dir.join(&to);
                    running.scene_dir.clone_from(&self.scene_dir);
                    self.config.last_scene = Some(self.config.remember(&self.scene_dir));
                    running.dm.window.set_title(&window_title(&self.scene_dir));
                    running.reload_images();
                }
            }
            SceneCommand::Delete(name) => {
                config::delete_scene(&scenes_dir, &name)?;
                if self.scene_dir == scenes_dir.join(&name) {
                    // The open scene went with the folder, so another one
                    // takes the canvas, and a new one when none is left.
                    let next = match config::scene_list(&scenes_dir).first() {
                        Some(name) => name.clone(),
                        None => config::new_scene(&scenes_dir, FIRST_SCENE)?,
                    };
                    // The folder of the open scene is gone, so there is
                    // nothing left to save it into.
                    self.load_scene(running, &next)?;
                }
            }
            SceneCommand::Reveal(name) => reveal(&scenes_dir.join(&name))?,
            SceneCommand::ScenesFolder => {
                let Some(picked) = rfd::FileDialog::new()
                    .set_directory(&scenes_dir)
                    .pick_folder()
                else {
                    return Ok(());
                };
                std::fs::create_dir_all(&picked)
                    .with_context(|| format!("{}: cannot use this folder", picked.display()))?;
                self.config.scenes_dir = picked;
                running.scenes_dir.clone_from(&self.config.scenes_dir);
                // The open scene may sit outside the new folder, and then
                // it is remembered by its whole path.
                self.config.last_scene = Some(self.config.remember(&self.scene_dir));
            }
        }
        save(&self.config, &self.scene, &self.scene_dir)
    }

    /// Saves the open scene, then puts another one on the canvas.
    ///
    /// # Errors
    ///
    /// Returns an error when a file cannot be written or read.
    fn open_scene(&mut self, running: &mut Running, name: &str) -> Result<()> {
        running.update(&mut self.config, &mut self.scene);
        save(&self.config, &self.scene, &self.scene_dir)?;
        self.load_scene(running, name)
    }

    /// Puts a scene on the canvas without a word about the last one.
    ///
    /// # Errors
    ///
    /// Returns an error when the folder cannot be made or the scene file
    /// cannot be read.
    fn load_scene(&mut self, running: &mut Running, name: &str) -> Result<()> {
        let dir = self.config.scene_dir(Path::new(name));
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("{}: cannot make the scene folder", dir.display()))?;
        self.scene = load_scene(&dir)?;
        self.scene_dir.clone_from(&dir);
        self.config.last_scene = Some(self.config.remember(&dir));
        running.open_scene(dir, &self.scene);
        Ok(())
    }
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
        // The running half comes out of its place for the call, so that
        // a scene command can borrow it and the program at the same time.
        let Some(mut running) = self.running.take() else {
            return;
        };
        let result = self.handle_event(&mut running, window_id, &event);
        self.running = Some(running);
        if let Err(error) = result {
            self.error = Some(error);
            event_loop.exit();
        }
    }
}
