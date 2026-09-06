//! dmap: map display for a tabletop RPG table with a TV.
//!
//! One process opens two windows. The DM window holds the editor UI.
//! The TV window fills one display and shows the players' view.

// Rust guideline compliant 2026-02-21

mod color;
mod gpu;
mod project;
mod tv;
mod ui;

use std::path::PathBuf;
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

use crate::gpu::{Gpu, Pane};
use crate::project::Project;
use crate::tv::{display_at, dm_move_target, placement_for, resolve_tv_display};
use crate::ui::{DmUi, Settings};

/// Size the DM window opens with. The design mockups use this frame.
const DM_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1440.0, 900.0);

/// Size of the TV window when no display is free for it.
const TV_FALLBACK_SIZE: LogicalSize<f64> = LogicalSize::new(960.0, 540.0);

/// Project file used when no path is given on the command line.
const DEFAULT_PROJECT: &str = "project.json";

fn main() -> Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from(DEFAULT_PROJECT), PathBuf::from);
    let project = load_project(&path)?;
    let event_loop = EventLoop::new()?;
    let mut app = App {
        path,
        project,
        running: None,
        error: None,
    };
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

/// Reads the project file, or starts a new project when there is none.
fn load_project(path: &std::path::Path) -> Result<Project> {
    match std::fs::read_to_string(path) {
        Ok(json) => {
            Project::from_json(&json).with_context(|| format!("{}: broken project", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Project::default()),
        Err(error) => Err(error).with_context(|| format!("{}: cannot read", path.display())),
    }
}

/// Application state across the event loop.
#[derive(Debug)]
struct App {
    path: PathBuf,
    project: Project,
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
    displays: Vec<MonitorHandle>,
    settings: Settings,
}

impl Running {
    fn new(event_loop: &ActiveEventLoop, project: &Project) -> Result<Self> {
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
            resolve_tv_display(&project.tv_display, &display_names(&displays), dm_display);
        let tv_window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("dmap TV")
                    .with_inner_size(TV_FALLBACK_SIZE)
                    .with_fullscreen(fullscreen_on(&displays, tv_display)),
            )?,
        );
        move_dm_off_tv(&dm_window, &displays, tv_display);
        let gpu = Gpu::new(&dm_window)?;
        let dm = gpu.pane(dm_window)?;
        let tv = gpu.pane(tv_window)?;
        let ui = DmUi::new(&gpu, &dm);
        Ok(Self {
            gpu,
            dm,
            tv,
            ui,
            displays,
            settings: Settings { tv_display },
        })
    }

    /// Handles a window event. Returns `true` when the DM changed a setting.
    fn window_event(&mut self, id: WindowId, event: &WindowEvent) -> Result<bool> {
        let is_dm = id == self.dm.window.id();
        if is_dm && self.ui.on_event(&self.dm, event) {
            return Ok(false);
        }
        let pane = if is_dm { &mut self.dm } else { &mut self.tv };
        match event {
            WindowEvent::Resized(size) => pane.resize(&self.gpu.device, size.width, size.height),
            // The window manager places a new window where it likes, so check
            // after every move that the DM window is not on the TV display.
            WindowEvent::Moved(_) if is_dm => {
                move_dm_off_tv(&self.dm.window, &self.displays, self.settings.tv_display);
            }
            WindowEvent::RedrawRequested if is_dm => return self.redraw_dm(),
            WindowEvent::RedrawRequested => {
                self.gpu.clear(pane, color::linear_color(color::CANVAS))?;
            }
            _ => {}
        }
        Ok(false)
    }

    /// Runs a DM frame. Returns `true` when the DM changed a setting.
    fn redraw_dm(&mut self) -> Result<bool> {
        let before = self.settings.clone();
        self.ui
            .frame(&self.gpu, &mut self.dm, &self.displays, &mut self.settings)?;
        let changed = self.settings != before;
        if changed {
            place_tv(&self.tv.window, &self.displays, self.settings.tv_display);
            move_dm_off_tv(&self.dm.window, &self.displays, self.settings.tv_display);
        }
        Ok(changed)
    }

    /// Copies the settings into the project.
    fn update_project(&self, project: &mut Project) {
        project.tv_display =
            placement_for(self.settings.tv_display, &display_names(&self.displays));
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

/// The display that holds the window's top-left corner.
///
/// winit's own lookup reports the wrong display on some X11 setups, so this
/// compares the window position with the display rectangles.
fn display_of(window: &Window, displays: &[MonitorHandle]) -> Option<usize> {
    let position = window.outer_position().ok()?;
    let rects: Vec<_> = displays
        .iter()
        .map(|display| {
            let position = display.position();
            let size = display.size();
            ((position.x, position.y), (size.width, size.height))
        })
        .collect();
    display_at((position.x, position.y), &rects)
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

/// Borderless full screen on the chosen display, or `None` for a normal window.
fn fullscreen_on(displays: &[MonitorHandle], index: Option<usize>) -> Option<Fullscreen> {
    index.map(|i| Fullscreen::Borderless(Some(displays[i].clone())))
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        match Running::new(event_loop, &self.project) {
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
                running.update_project(&mut self.project);
                std::fs::write(&self.path, self.project.to_json())
                    .with_context(|| format!("{}: cannot save", self.path.display()))?;
            }
            Ok(())
        });
        if let Err(error) = result {
            self.error = Some(error);
            event_loop.exit();
        }
    }
}
