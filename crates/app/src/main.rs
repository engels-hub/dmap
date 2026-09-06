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
    window::{Fullscreen, Window, WindowId},
};

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
#[derive(Default)]
struct App {
    dm_window: Option<Window>,
    tv_window: Option<Window>,
    error: Option<anyhow::Error>,
}

impl App {
    fn open_windows(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let dm_window = event_loop.create_window(
            Window::default_attributes()
                .with_title("dmap")
                .with_inner_size(DM_WINDOW_SIZE),
        )?;
        let displays: Vec<_> = event_loop.available_monitors().collect();
        let tv_display = pick_tv_display(&displays, dm_window.current_monitor().as_ref())
            .map(|i| Fullscreen::Borderless(Some(displays[i].clone())));
        let tv_window = event_loop.create_window(
            Window::default_attributes()
                .with_title("dmap TV")
                .with_inner_size(TV_FALLBACK_SIZE)
                .with_fullscreen(tv_display),
        )?;
        self.dm_window = Some(dm_window);
        self.tv_window = Some(tv_window);
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.dm_window.is_some() {
            return;
        }
        if let Err(error) = self.open_windows(event_loop) {
            self.error = Some(error);
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if event == WindowEvent::CloseRequested {
            event_loop.exit();
        }
    }
}
