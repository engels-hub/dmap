//! dmap: map display for a tabletop RPG table with a TV.
//!
//! One process opens two windows. The DM window holds the editor UI.
//! The TV window fills one display and shows the players' view.

// Rust guideline compliant 2026-02-21

mod color;
mod gpu;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "wired into the app in the next commit")
)]
mod project;
mod tv;
mod ui;

use std::sync::Arc;

use anyhow::Result;
use egui_winit::winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    monitor::MonitorHandle,
    window::{Fullscreen, Window, WindowId},
};

use crate::gpu::{Gpu, Pane};
use crate::tv::pick_tv_display;
use crate::ui::{DmUi, Settings};

/// Size the DM window opens with. The design mockups use this frame.
const DM_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1440.0, 900.0);

/// Size of the TV window when no display is free for it.
const TV_FALLBACK_SIZE: LogicalSize<f64> = LogicalSize::new(960.0, 540.0);

fn main() -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

/// Application state across the event loop.
#[derive(Default, Debug)]
struct App {
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
    fn new(event_loop: &ActiveEventLoop) -> Result<Self> {
        let dm_window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("dmap")
                    .with_inner_size(DM_WINDOW_SIZE),
            )?,
        );
        let displays: Vec<_> = event_loop.available_monitors().collect();
        let tv_display = pick_tv_display(&displays, dm_window.current_monitor().as_ref());
        let tv_window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("dmap TV")
                    .with_inner_size(TV_FALLBACK_SIZE)
                    .with_fullscreen(fullscreen_on(&displays, tv_display)),
            )?,
        );
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

    fn window_event(&mut self, id: WindowId, event: &WindowEvent) -> Result<()> {
        let is_dm = id == self.dm.window.id();
        if is_dm && self.ui.on_event(&self.dm, event) {
            return Ok(());
        }
        let pane = if is_dm { &mut self.dm } else { &mut self.tv };
        match event {
            WindowEvent::Resized(size) => pane.resize(&self.gpu.device, size.width, size.height),
            WindowEvent::RedrawRequested if is_dm => self.redraw_dm()?,
            WindowEvent::RedrawRequested => {
                self.gpu.clear(pane, color::linear_color(color::CANVAS))?;
            }
            _ => {}
        }
        Ok(())
    }

    fn redraw_dm(&mut self) -> Result<()> {
        let before = self.settings.clone();
        self.ui
            .frame(&self.gpu, &mut self.dm, &self.displays, &mut self.settings)?;
        if self.settings != before {
            self.tv
                .window
                .set_fullscreen(fullscreen_on(&self.displays, self.settings.tv_display));
        }
        Ok(())
    }
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
        match Running::new(event_loop) {
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
        if let Err(error) = running.window_event(window_id, &event) {
            self.error = Some(error);
            event_loop.exit();
        }
    }
}
