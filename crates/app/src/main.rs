//! dmap: map display for a tabletop RPG table with a TV.
//!
//! One process opens two windows. The DM window holds the editor UI.
//! The TV window fills one monitor and shows the players' view.

// Rust guideline compliant 2026-02-21

use std::sync::Arc;

use anyhow::{Context as _, Result};
use egui_wgpu::wgpu;
use egui_winit::winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    monitor::MonitorHandle,
    window::{Fullscreen, Window, WindowId},
};
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

/// Default canvas color, `#e3d9c3` from DESIGN.md, given in linear light
/// because the surface format is sRGB and converts on write.
const CANVAS_COLOR: wgpu::Color = wgpu::Color {
    r: 0.77,
    g: 0.70,
    b: 0.55,
    a: 1.0,
};

/// Size the DM window opens with. The design mockups use this frame.
const DM_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1440.0, 900.0);

/// Size of the TV window when it cannot go full screen on its own monitor.
const TV_FALLBACK_SIZE: LogicalSize<f64> = LogicalSize::new(960.0, 540.0);

fn main() -> Result<()> {
    env_logger::init();
    let event_loop = EventLoop::new()?;
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

/// Application state across the event loop.
#[derive(Default)]
struct App {
    running: Option<Running>,
    error: Option<anyhow::Error>,
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
        let Some(running) = self.running.as_mut() else {
            return;
        };
        if let Err(error) = running.window_event(window_id, &event) {
            self.error = Some(error);
            event_loop.exit();
        }
        if running.quit {
            event_loop.exit();
        }
    }
}

/// One window and the wgpu surface that draws into it.
struct Pane {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl Pane {
    /// Acquires the next frame, or `None` when this frame should be skipped.
    fn acquire(&mut self, device: &wgpu::Device) -> Result<Option<wgpu::SurfaceTexture>> {
        use wgpu::CurrentSurfaceTexture as Current;
        Ok(match self.surface.get_current_texture() {
            Current::Success(frame) | Current::Suboptimal(frame) => Some(frame),
            Current::Timeout | Current::Occluded => None,
            Current::Outdated | Current::Lost => {
                self.surface.configure(device, &self.config);
                None
            }
            Current::Validation => anyhow::bail!("surface validation error"),
        })
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(device, &self.config);
    }
}

/// Everything that exists once the windows and the GPU are up.
struct Running {
    device: wgpu::Device,
    queue: wgpu::Queue,
    dm: Pane,
    tv: Pane,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    monitors: Vec<MonitorHandle>,
    tv_monitor: Option<usize>,
    quit: bool,
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
        let monitors: Vec<MonitorHandle> = event_loop.available_monitors().collect();
        let tv_monitor = pick_tv_monitor(&monitors, dm_window.current_monitor().as_ref());
        let tv_window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("dmap TV")
                    .with_inner_size(TV_FALLBACK_SIZE)
                    .with_decorations(false)
                    .with_fullscreen(
                        tv_monitor.map(|i| Fullscreen::Borderless(Some(monitors[i].clone()))),
                    ),
            )?,
        );

        let instance = wgpu::Instance::default();
        let dm_surface = instance.create_surface(Arc::clone(&dm_window))?;
        let tv_surface = instance.create_surface(Arc::clone(&tv_window))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&dm_surface),
            ..Default::default()
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;

        let dm = configure(&adapter, &device, dm_window, dm_surface)?;
        let tv = configure(&adapter, &device, tv_window, tv_surface)?;

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx,
            egui::ViewportId::ROOT,
            dm.window.as_ref(),
            Some(dm.window.scale_factor() as f32),
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            dm.config.format,
            egui_wgpu::RendererOptions::default(),
        );

        Ok(Self {
            device,
            queue,
            dm,
            tv,
            egui_state,
            egui_renderer,
            monitors,
            tv_monitor,
            quit: false,
        })
    }

    fn window_event(&mut self, id: WindowId, event: &WindowEvent) -> Result<()> {
        let is_dm = id == self.dm.window.id();
        if is_dm {
            let response = self.egui_state.on_window_event(&self.dm.window, event);
            if response.repaint {
                self.dm.window.request_redraw();
            }
            if response.consumed {
                return Ok(());
            }
        }
        match event {
            WindowEvent::CloseRequested => self.quit = true,
            WindowEvent::Resized(size) => {
                let target = if is_dm { &mut self.dm } else { &mut self.tv };
                target.resize(&self.device, size.width, size.height);
            }
            WindowEvent::RedrawRequested if is_dm => self.render_dm()?,
            WindowEvent::RedrawRequested => self.render_tv()?,
            _ => {}
        }
        Ok(())
    }

    fn render_tv(&mut self) -> Result<()> {
        let Some(frame) = self.tv.acquire(&self.device)? else {
            return Ok(());
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        drop(begin_clear_pass(&mut encoder, &view, CANVAS_COLOR));
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(())
    }

    fn render_dm(&mut self) -> Result<()> {
        let raw_input = self.egui_state.take_egui_input(&self.dm.window);
        let mut selected = self.tv_monitor;
        let output = self.egui_state.egui_ctx().clone().run_ui(raw_input, |ui| {
            settings_ui(ui, &self.monitors, &mut selected);
        });
        if selected != self.tv_monitor {
            self.tv_monitor = selected;
            self.tv.window.set_fullscreen(
                selected.map(|i| Fullscreen::Borderless(Some(self.monitors[i].clone()))),
            );
        }
        let egui::FullOutput {
            platform_output,
            mut textures_delta,
            shapes,
            viewport_output,
            ..
        } = output;
        self.egui_state
            .handle_platform_output(&self.dm.window, platform_output);

        let ctx = self.egui_state.egui_ctx();
        let pixels_per_point = ctx.pixels_per_point();
        let paint_jobs = ctx.tessellate(shapes, pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.dm.config.width, self.dm.config.height],
            pixels_per_point,
        };
        for (id, deltas) in &textures_delta.set {
            for delta in deltas {
                self.egui_renderer
                    .update_texture(&self.device, &self.queue, *id, delta);
            }
        }

        let Some(frame) = self.dm.acquire(&self.device)? else {
            textures_delta.clear();
            return Ok(());
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let buffers = self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen,
        );
        {
            let mut pass = begin_clear_pass(&mut encoder, &view, CANVAS_COLOR);
            self.egui_renderer.render(&mut pass, &paint_jobs, &screen);
        };
        for id in &textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
        textures_delta.clear();
        self.queue
            .submit(buffers.into_iter().chain([encoder.finish()]));
        self.queue.present(frame);

        if viewport_output
            .values()
            .any(|viewport| viewport.repaint_delay.is_zero())
        {
            self.dm.window.request_redraw();
        }
        Ok(())
    }
}

/// Configures a surface for its window's current size.
fn configure(
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
) -> Result<Pane> {
    let size = window.inner_size();
    let config = surface
        .get_default_config(adapter, size.width.max(1), size.height.max(1))
        .context("the GPU adapter cannot draw to this window")?;
    surface.configure(device, &config);
    Ok(Pane {
        window,
        surface,
        config,
    })
}

/// Begins a render pass that clears `view` to `color`.
///
/// The returned pass has no lifetime tie to `encoder`, which egui's renderer
/// needs. Drop the pass before you call `encoder.finish()`.
fn begin_clear_pass(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    color: wgpu::Color,
) -> wgpu::RenderPass<'static> {
    encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
        .forget_lifetime()
}

/// Picks the monitor for the TV: the first one that is not the DM's.
///
/// Returns `None` when there is no such monitor. Then the TV opens as a
/// normal window, so it does not cover the DM window.
fn pick_tv_monitor(
    monitors: &[MonitorHandle],
    dm_monitor: Option<&MonitorHandle>,
) -> Option<usize> {
    monitors
        .iter()
        .position(|monitor| Some(monitor) != dm_monitor)
}

/// The DM window's UI for M0: a rail and a settings panel with the TV monitor picker.
fn settings_ui(ui: &mut egui::Ui, monitors: &[MonitorHandle], tv_monitor: &mut Option<usize>) {
    egui::Panel::left("rail")
        .exact_size(72.0)
        .resizable(false)
        .show(ui, |ui| {
            ui.label("dmap");
        });
    egui::Panel::right("settings").show(ui, |ui| {
        ui.heading("Settings");
        ui.label("TV display");
        let selected_text =
            tv_monitor.map_or_else(|| "Window".to_owned(), |i| monitor_label(&monitors[i]));
        egui::ComboBox::from_id_salt("tv_monitor")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                ui.selectable_value(tv_monitor, None, "Window");
                for (i, monitor) in monitors.iter().enumerate() {
                    ui.selectable_value(tv_monitor, Some(i), monitor_label(monitor));
                }
            });
    });
}

/// Human-readable name for a monitor: its name and its resolution.
fn monitor_label(monitor: &MonitorHandle) -> String {
    let size = monitor.size();
    let name = monitor.name().unwrap_or_else(|| "Display".to_owned());
    format!("{name} · {} × {}", size.width, size.height)
}
