//! GPU device and the surfaces that draw into the windows.

// Rust guideline compliant 2026-02-21

use std::sync::Arc;

use anyhow::{Context as _, Result};
use egui_wgpu::wgpu;
use egui_winit::winit::window::Window;

/// The GPU device shared by all windows.
pub struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu").finish_non_exhaustive()
    }
}

impl Gpu {
    /// Opens the GPU that can draw to `window`.
    pub fn new(window: &Arc<Window>) -> Result<Self> {
        let instance = wgpu::Instance::default();
        let probe = instance.create_surface(Arc::clone(window))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&probe),
            ..Default::default()
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }

    /// Creates a drawing surface for `window` at its current size.
    pub fn pane(&self, window: Arc<Window>) -> Result<Pane> {
        let surface = self.instance.create_surface(Arc::clone(&window))?;
        let size = window.inner_size();
        let config = surface
            .get_default_config(&self.adapter, size.width.max(1), size.height.max(1))
            .context("the GPU adapter cannot draw to this window")?;
        surface.configure(&self.device, &config);
        Ok(Pane {
            window,
            surface,
            config,
        })
    }

    /// Clears the whole pane to `color`, then lets `draw` add to the pass.
    ///
    /// Returns `false` when the surface gave no frame and nothing was shown.
    ///
    /// # Errors
    ///
    /// Returns an error when the surface fails validation.
    pub fn clear(
        &self,
        pane: &mut Pane,
        color: wgpu::Color,
        draw: impl FnOnce(&mut wgpu::RenderPass<'static>),
    ) -> Result<bool> {
        let Some(frame) = pane.acquire(&self.device)? else {
            return Ok(false);
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = begin_clear_pass(&mut encoder, &view, color);
            draw(&mut pass);
        };
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(true)
    }
}

/// One window and the wgpu surface that draws into it.
pub struct Pane {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
}

impl std::fmt::Debug for Pane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pane")
            .field("window", &self.window.id())
            .finish_non_exhaustive()
    }
}

impl Pane {
    /// Acquires the next frame, or `None` when this frame should be skipped.
    ///
    /// A surface that went out of date is configured again, and then the
    /// frame is asked for once more. The caller sees `None` only when the
    /// window has nothing to show, such as a window behind another one.
    ///
    /// # Errors
    ///
    /// Returns an error when the surface fails validation.
    pub fn acquire(&mut self, device: &wgpu::Device) -> Result<Option<wgpu::SurfaceTexture>> {
        use wgpu::CurrentSurfaceTexture as Current;
        // Two turns: the first can find the surface out of date, and the
        // second draws on the surface the configuration made.
        for _ in 0..2 {
            match self.surface.get_current_texture() {
                Current::Success(frame) | Current::Suboptimal(frame) => return Ok(Some(frame)),
                Current::Timeout | Current::Occluded => return Ok(None),
                Current::Outdated | Current::Lost => self.surface.configure(device, &self.config),
                Current::Validation => anyhow::bail!("surface validation error"),
            }
        }
        Ok(None)
    }

    /// Reconfigures the surface after the window changed size.
    ///
    /// Returns `true` when the surface took a new size. A window that is
    /// minimized reports 0x0, and a window that only moved reports the size
    /// it already has. Both keep the surface as it is.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 || (width, height) == (self.config.width, self.config.height) {
            return false;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(device, &self.config);
        true
    }
}

/// Begins a render pass that clears `view` to `color`.
///
/// The returned pass has no lifetime tie to `encoder`, which egui's renderer
/// needs later. Drop the pass before you call `encoder.finish()`.
pub fn begin_clear_pass(
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
