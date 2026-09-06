//! dmap: map display for a tabletop RPG table with a TV.
//!
//! One process opens two windows. The DM window holds the editor UI.
//! The TV window fills one display and shows the players' view.

// Rust guideline compliant 2026-02-21

mod tv;

use anyhow::Result;
use egui_winit::winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

/// Size the DM window opens with. The design mockups use this frame.
const DM_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1440.0, 900.0);

fn main() -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

/// Application state across the event loop.
#[derive(Default)]
struct App {
    dm_window: Option<Window>,
    error: Option<anyhow::Error>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.dm_window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("dmap")
            .with_inner_size(DM_WINDOW_SIZE);
        match event_loop.create_window(attributes) {
            Ok(window) => self.dm_window = Some(window),
            Err(error) => {
                self.error = Some(error.into());
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if event == WindowEvent::CloseRequested {
            event_loop.exit();
        }
    }
}
