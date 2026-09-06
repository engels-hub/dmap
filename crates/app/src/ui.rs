//! The DM window's UI: egui on top of the wgpu pane.

// Rust guideline compliant 2026-02-21

use anyhow::Result;
use egui_wgpu::wgpu;
use egui_winit::winit::{event::WindowEvent, monitor::MonitorHandle};

use crate::color;
use crate::gpu::{Gpu, Pane, begin_clear_pass};
use crate::tv::display_label;

/// Width of the tool rail on the left, from DESIGN.md.
const RAIL_WIDTH: f32 = 72.0;

/// egui state and renderer for one window.
pub struct DmUi {
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
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
        Self { state, renderer }
    }

    /// Feeds a window event to egui. Returns `true` when egui consumed it.
    pub fn on_event(&mut self, pane: &Pane, event: &WindowEvent) -> bool {
        let response = self.state.on_window_event(&pane.window, event);
        if response.repaint {
            pane.window.request_redraw();
        }
        response.consumed
    }

    /// Runs one UI frame and draws it into the pane.
    ///
    /// Returns `true` when the DM pressed Add map.
    pub fn frame(
        &mut self,
        gpu: &Gpu,
        pane: &mut Pane,
        displays: &[MonitorHandle],
        settings: &mut Settings,
        draw_canvas: impl FnOnce(&mut wgpu::RenderPass<'static>),
    ) -> Result<bool> {
        let raw_input = self.state.take_egui_input(&pane.window);
        let ctx = self.state.egui_ctx().clone();
        let mut add_map = false;
        let output = ctx.run_ui(raw_input, |ui| {
            add_map = settings_ui(ui, displays, settings);
        });
        let egui::FullOutput {
            platform_output,
            mut textures_delta,
            shapes,
            viewport_output,
            ..
        } = output;
        self.state
            .handle_platform_output(&pane.window, platform_output);

        let pixels_per_point = ctx.pixels_per_point();
        let paint_jobs = ctx.tessellate(shapes, pixels_per_point);
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
            let buffers = self.renderer.update_buffers(
                &gpu.device,
                &gpu.queue,
                &mut encoder,
                &paint_jobs,
                &screen,
            );
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

        if viewport_output
            .values()
            .any(|viewport| viewport.repaint_delay.is_zero())
        {
            pane.window.request_redraw();
        }
        Ok(add_map)
    }
}

/// The rail and the settings panel. Returns `true` when Add map was pressed.
fn settings_ui(ui: &mut egui::Ui, displays: &[MonitorHandle], settings: &mut Settings) -> bool {
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
