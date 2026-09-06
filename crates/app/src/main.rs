//! dmap: map display for a tabletop RPG table with a TV.
//!
//! One process opens two windows. The DM window holds the editor UI.
//! The TV window fills one display and shows the players' view.

// Rust guideline compliant 2026-02-21

mod color;
mod gpu;
mod tv;

use std::sync::Arc;

use anyhow::Result;
use egui_winit::winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Fullscreen, Window, WindowId},
};

use crate::gpu::{Gpu, Pane};
use crate::tv::pick_tv_display;

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
        let tv_display = pick_tv_display(&displays, dm_window.current_monitor().as_ref())
            .map(|i| Fullscreen::Borderless(Some(displays[i].clone())));
        let tv_window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("dmap TV")
                    .with_inner_size(TV_FALLBACK_SIZE)
                    .with_fullscreen(tv_display),
            )?,
        );
        let gpu = Gpu::new(&dm_window)?;
        let dm = gpu.pane(dm_window)?;
        let tv = gpu.pane(tv_window)?;
        Ok(Self { gpu, dm, tv })
    }

    fn window_event(&mut self, id: WindowId, event: &WindowEvent) -> Result<()> {
        let pane = if id == self.dm.window.id() {
            &mut self.dm
        } else {
            &mut self.tv
        };
        match event {
            WindowEvent::Resized(size) => pane.resize(&self.gpu.device, size.width, size.height),
            WindowEvent::RedrawRequested => {
                self.gpu.clear(pane, color::linear_color(color::CANVAS))?;
            }
            _ => {}
        }
        Ok(())
    }
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
