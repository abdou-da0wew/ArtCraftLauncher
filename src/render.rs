//! GPU setup and the two-pass frame: the hero field, then the UI canvas.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use crate::gfx::canvas::Canvas;

fn make_ui_tex(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ui-canvas"),
        size: wgpu::Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub ui_tex: wgpu::Texture,
    pub hero: crate::hero::HeroScene,
}

impl Gpu {
    /// # Safety (caller's obligation)
    ///
    /// `window` must outlive the returned `Gpu`. The surface is built from
    /// *copies* of the raw display/window handles, so the type system cannot
    /// see the borrow; the `App` struct keeps `gpu` declared **before**
    /// `window` so the surface is dropped first, and the `Window` is only
    /// released when the whole `App` is torn down.
    pub fn new(
        window: &winit::window::Window,
        size: (u32, u32),
        shots: &[(Vec<u8>, u32, u32)],
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let display = window
            .display_handle()
            .map_err(|e| format!("display handle: {e}"))?
            .as_raw();
        let handle = window
            .window_handle()
            .map_err(|e| format!("window handle: {e}"))?
            .as_raw();
        // The handles are plain values, so the surface does not borrow the
        // Window — it is 'static as far as the type system is concerned.
        let surface: wgpu::Surface<'static> = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: display,
                raw_window_handle: handle,
            })
        }
        .map_err(|e| format!("surface: {e}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| {
            "no graphics adapter — the launcher's GUI needs a GPU. The CLI works without one: \
             `artcraft-launcher list`, `install`, `update`, `check`."
                .to_string()
        })?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("artcraft"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
            },
            None,
        ))
        .map_err(|e| format!("device: {e}"))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let ui_tex = make_ui_tex(&device, config.width, config.height);
        let mut hero = crate::hero::HeroScene::new(&device, &queue, format, &ui_tex, shots);
        hero.resize(config.width, config.height);

        Ok(Self {
            device,
            queue,
            surface,
            config,
            ui_tex,
            hero,
        })
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.ui_tex = make_ui_tex(&self.device, w, h);
        self.hero.rebind_ui(&self.device, &self.ui_tex);
        self.hero.resize(w, h);
    }

    pub fn render(&mut self, canvas: &Canvas) {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame.texture.create_view(&Default::default());

        let w = canvas.w as u32;
        let h = canvas.h as u32;
        if w > 0 && h > 0 && w == self.ui_tex.width() && h == self.ui_tex.height() {
            let bpr = (w * 4 + 255) / 256 * 256;
            if bpr == w * 4 {
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.ui_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &canvas.px,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(w * 4),
                        // A single 2D layer must not declare rows_per_image.
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                );
            } else {
                let mut pad = vec![0u8; (bpr * h) as usize];
                for y in 0..h as usize {
                    let s = y * canvas.w * 4;
                    let d = y * bpr as usize;
                    pad[d..d + canvas.w * 4].copy_from_slice(&canvas.px[s..s + canvas.w * 4]);
                }
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.ui_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &pad,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bpr),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        // Per-frame hero instances, then one render pass.
        self.hero.write_instances(&self.queue);
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.hero.draw(&mut pass);
            self.hero.draw_ui(&mut pass);
        }
        self.queue.submit(Some(enc.finish()));
        frame.present();
    }
}

/// True if wgpu can find an adapter on this machine. Cheap enough to call from
/// `doctor`; never creates a surface.
pub fn gpu_available() -> bool {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: true,
    }))
    .is_some()
}
