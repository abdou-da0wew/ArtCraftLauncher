//! The hero field: a radial galaxy of bowed image cards carrying the seven
//! apps' screenshots, comet ribbon arms, click ripples, and the reveal wave.
//!
//! Ported from the site's `hero-galaxy.tsx`. The important behaviours kept:
//!  * cards de-phase their motion by the golden-ratio conjugate (2.618)
//!  * a frame-time EMA drives an adaptive quality scale
//!  * the radial reveal wave uncovers cards and ribbons with a trailing band
//!  * the theme cross-fades between light/dark by luminance test
//!  * four stackable click ripples in `(center.xy, radiusPx, amplitudePx)`

use std::path::PathBuf;

use wgpu::util::DeviceExt;

use crate::theme::Rgba;

/// `2.618` — the golden-ratio conjugate the site uses to de-phase cards.
pub const GOLDEN_CONJUGATE: f64 = 2.618;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    cam_z: f32,
    focal: f32,
    res: [f32; 2],
    time: f32,
    wave_r: f32,
    wave_band: f32,
    dark: f32,
    quality: f32,
    pointer: [f32; 2],
    curve: f32,
    warp: f32,
    ripple_w: f32,
    frame_a: f32,
    pulse: f32,
    _pad: [f32; 4],
    bg: [f32; 4],
    ripples: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct CardInstance {
    m0: [f32; 4],
    m1: [f32; 4],
    size: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct RibbonInstance {
    od: [f32; 4],
    params: [f32; 4],
}

/// One click ripple: `(center.xy, radiusPx, amplitudePx)`, decaying over time.
#[derive(Clone, Copy)]
pub struct Ripple {
    pub center: [f32; 2],
    pub radius: f32,
    pub amp: f32,
    pub age: f32,
}

impl Ripple {
    pub fn alive(&self) -> bool {
        self.amp > 0.01
    }
}

struct Card {
    /// field-space position
    x: f32,
    y: f32,
    z: f32,
    yaw: f32,
    w: f32,
    h: f32,
    tex: u32,
    /// base alpha when revealed
    alpha: f32,
    blur: f32,
    aber: f32,
    /// phase offset, de-phased by the golden conjugate
    phase: f64,
    /// distance from field centre, for the wave ordering
    radius: f32,
    /// gentle ambient bob
    bob: f32,
}

pub struct HeroScene {
    cards: Vec<Card>,
    ribbons: Vec<RibbonInstance>,
    card_buf: wgpu::Buffer,
    ribbon_buf: wgpu::Buffer,
    cam_buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    ribbon_pipeline: wgpu::RenderPipeline,
    ui_pipeline: wgpu::RenderPipeline,
    ui_bind: wgpu::BindGroup,
    ui_bgl: wgpu::BindGroupLayout,

    cam: CameraUniform,

    pub time: f64,
    pub dark: f32,
    pub dark_target: f32,
    pub quality: f32,
    pub wave_r: f32,
    pub wave_band: f32,
    pub wave_p: f64,
    pub spin: f32,
    pub spin_vel: f32,
    pub pointer: [f32; 2],
    pub pointer_target: [f32; 2],
    ripples: [Ripple; 4],
    ripple_slot: usize,
    ui_sampler: wgpu::Sampler,
    pub reduced: bool,
    pub fps_ema: f64,
    /// Whether the field should be visible at all (the launcher screens draw
    /// their own backdrop, but the field is always present for atmosphere).
    pub intensity: f32,
    pub intensity_target: f32,
}

/// Mix a hex token into 0..1 rgb.
fn tok(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    ]
}

impl HeroScene {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        ui_tex: &wgpu::Texture,
        screenshots: &[(Vec<u8>, u32, u32)],
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hero.wgsl"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../assets/shaders/hero.wgsl").into(),
            ),
        });

        // --- texture array: one layer per app screenshot -------------------
        let (tw, th) = screenshots
            .first()
            .map(|(_, w, h)| (*w, *h))
            .unwrap_or((1600, 1000));
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hero-screens"),
            size: wgpu::Extent3d {
                width: tw,
                height: th,
                depth_or_array_layers: screenshots.len() as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bpr = (tw * 4 + 255) / 256 * 256;
        let mut padded = vec![0u8; (bpr * th) as usize];
        for (i, (bytes, w, h)) in screenshots.iter().enumerate() {
            let bytes = bytes.as_slice();
            debug_assert_eq!(*w, tw);
            debug_assert_eq!(*h, th);
            for y in 0..*h as usize {
                let src = (y * *w as usize) * 4;
                let dst = (y * bpr as usize) as usize;
                padded[dst..dst + (*w as usize * 4)]
                    .copy_from_slice(&bytes[src..src + *w as usize * 4]);
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &padded,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: tw,
                    height: th,
                    depth_or_array_layers: 1,
                },
            );
        }
        let view = tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hero-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hero-smp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let cam = CameraUniform {
            cam_z: 900.0,
            focal: 900.0,
            res: [1280.0, 800.0],
            time: 0.0,
            wave_r: 0.0,
            wave_band: 260.0,
            dark: 0.0,
            quality: 1.0,
            pointer: [0.0, 0.0],
            curve: 1.0,
            warp: 26.0,
            ripple_w: 34.0,
            frame_a: 0.85,
            pulse: 0.22,
            _pad: [0.0; 4],
            bg: [tok(0xf2f1ee)[0], tok(0xf2f1ee)[1], tok(0xf2f1ee)[2], 0.0],
            ripples: [[0.0; 4]; 4],
        };
        let cam_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hero-cam"),
            contents: bytemuck::bytes_of(&cam),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hero-bind"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cam_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hero-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let make = |vs_entry: &str, fs_entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("hero-pipe"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs_entry),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x4,
                            1 => Float32x4,
                            2 => Float32x4,
                            3 => Float32x4,
                        ],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs_entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let pipeline = make("vs_card", "fs_card");
        let ribbon_pipeline = {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("hero-ribbon"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_ribbon"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_ribbon"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };

        // --- UI blit pipeline ---------------------------------------------
        let ui_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let ui_view = ui_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let ui_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-bind"),
            layout: &ui_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&ui_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let ui_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-layout"),
            bind_group_layouts: &[&bgl, &ui_bgl],
            push_constant_ranges: &[],
        });
        let ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-pipe"),
            layout: Some(&ui_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_fullscreen"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_ui"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let (cards, ribbons) = build_field(screenshots.len() as u32);
        let card_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hero-cards"),
            size: (cards.len().max(1) * std::mem::size_of::<CardInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ribbon_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hero-ribbons"),
            size: (ribbons.len().max(1) * std::mem::size_of::<RibbonInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let ui_sampler = sampler.clone();
        Self {
            cards,
            ribbons,
            card_buf,
            ribbon_buf,
            cam_buf,
            bind,
            pipeline,
            ribbon_pipeline,
            ui_pipeline,
            ui_bind,
            ui_bgl,
            cam,
            time: 0.0,
            dark: 0.0,
            dark_target: 0.0,
            quality: 1.0,
            wave_r: 0.0,
            wave_band: 260.0,
            wave_p: 0.0,
            spin: 0.0,
            spin_vel: 0.0,
            pointer: [0.0, 0.0],
            pointer_target: [0.0, 0.0],
            ripples: [Ripple {
                center: [0.0, 0.0],
                radius: 0.0,
                amp: 0.0,
                age: 0.0,
            }; 4],
            ripple_slot: 0,
            ui_sampler,
            reduced: false,
            fps_ema: 60.0,
            intensity: 0.0,
            intensity_target: 1.0,
        }
    }

    /// Re-point the UI blit bind group at a new canvas texture (after a resize).
    pub fn rebind_ui(&mut self, device: &wgpu::Device, ui_tex: &wgpu::Texture) {
        let view = ui_tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.ui_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-bind"),
            layout: &self.ui_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.ui_sampler),
                },
            ],
        });
    }

    pub fn set_dark(&mut self, dark: bool) {
        self.dark_target = if dark { 1.0 } else { 0.0 };
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.cam.res = [w as f32, h as f32];
        // Mega-pixel budget, like the site: card count scales with pixels.
        let mpx = (w as f64 * h as f64) / 1e6;
        self.quality = (mpx / 1.2).clamp(0.35, 1.0) as f32;
    }

    pub fn set_pointer(&mut self, x: f32, y: f32) {
        // Convert to card-local units: -1..1 over half the field.
        let hx = self.cam.res[0] * 0.5;
        let hy = self.cam.res[1] * 0.5;
        self.pointer_target = [(x - hx) / hx, (y - hy) / hy];
    }

    pub fn click(&mut self, x: f32, y: f32, amp: f32) {
        let hx = self.cam.res[0] * 0.5;
        let hy = self.cam.res[1] * 0.5;
        let nx = (x - hx) / hx;
        let ny = (y - hy) / hy;
        // Find the card whose centre is nearest to the click, so the ripple
        // reads as being on a specific card.
        let mut best = 0usize;
        let mut best_d = f32::MAX;
        for (i, c) in self.cards.iter().enumerate() {
            let d = (c.x - nx * 400.0).hypot(c.y - ny * 400.0);
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        let c = &self.cards[best];
        self.ripples[self.ripple_slot] = Ripple {
            center: [nx, ny],
            radius: 12.0,
            amp,
            age: 0.0,
        };
        self.ripple_slot = (self.ripple_slot + 1) % 4;
        let _ = c;
    }

    /// Advance the scene. `dt` is seconds, `reveal` is 0..1 from the intro.
    pub fn update(&mut self, dt: f64, reveal: f64, scroll: f64) {
        self.time += if self.reduced { 0.0 } else { dt };
        // theme cross-fade
        self.dark += (self.dark_target - self.dark) * (dt * 4.0).clamp(0.0, 1.0) as f32;
        self.intensity += (self.intensity_target - self.intensity) * (dt * 3.0).clamp(0.0, 1.0) as f32;
        // pointer easing
        let k = (dt * 6.0).clamp(0.0, 1.0) as f32;
        self.pointer[0] += (self.pointer_target[0] - self.pointer[0]) * k;
        self.pointer[1] += (self.pointer_target[1] - self.pointer[1]) * k;
        // idle spin, modulated by scroll like the site's `spin` / `holdRest`
        self.spin_vel += (0.02 - self.spin_vel) * 0.02;
        self.spin += (self.spin_vel as f32 + scroll as f32 * 0.0006) * dt as f32;
        // reveal wave: sweeps outward, saturating past every card
        let max_r = 1400.0;
        self.wave_p = if self.reduced {
            1.0
        } else {
            (reveal * 2.6).clamp(0.0, 1.0)
        };
        self.wave_r = (self.wave_p * max_r * 1.15) as f32;
        self.wave_band = 420.0;
        // ripples decay
        for r in self.ripples.iter_mut() {
            if r.amp > 0.0 {
                r.age += dt as f32;
                r.radius += 320.0 * dt as f32;
                r.amp *= (1.0 - dt * 1.4).max(0.0) as f32;
            }
        }
        self.cam.dark = self.dark;
        self.cam.time = self.time as f32;
        self.cam.wave_r = self.wave_r;
        self.cam.wave_band = self.wave_band;
        self.cam.quality = self.quality;
        self.cam.pointer = self.pointer;
        self.cam.pulse = 0.22;
        for (i, r) in self.ripples.iter().enumerate() {
            self.cam.ripples[i] = [r.center[0], r.center[1], r.radius, r.amp];
        }
        let bg = if self.dark > 0.5 {
            tok(0x121316)
        } else {
            tok(0xf2f1ee)
        };
        self.cam.bg = [bg[0], bg[1], bg[2], 0.0];
    }

    /// Write per-frame instance data.
    pub fn write_instances(&mut self, queue: &wgpu::Queue) {
        let t = self.time;
        let mut ci: Vec<CardInstance> = Vec::with_capacity(self.cards.len());
        for c in &self.cards {
            // Ambient motion: a slow orbit plus a per-card bob, de-phased by
            // the golden conjugate so nothing moves in lockstep.
            let ph = c.phase * GOLDEN_CONJUGATE;
            let orbit = ((t * 0.02 + ph).sin() * 14.0) as f32;
            let bob = ((t * 0.5 + ph * 2.0).sin() * c.bob as f64) as f32;
            let x = c.x + orbit;
            let y = c.y + bob;
            let (s, co) = c.yaw.sin_cos();
            let reveal = wave_at(self.wave_r, self.wave_band, c.radius);
            let a = c.alpha * reveal * self.intensity;
            if a <= 0.003 {
                continue;
            }
            ci.push(CardInstance {
                m0: [co * c.w, -s * c.h, x, y],
                m1: [s * c.w, co * c.h, c.z, 0.0],
                size: [c.w, c.h, 0.0, 0.0],
                params: [a, c.blur * (1.0 - self.quality * 0.5), c.aber, c.tex as f32],
            });
        }
        if !ci.is_empty() {
            queue.write_buffer(&self.card_buf, 0, bytemuck::cast_slice(&ci));
        }
        queue.write_buffer(
            &self.ribbon_buf,
            0,
            bytemuck::cast_slice(&self.ribbons),
        );
        let cam_clone = unsafe {
            let p = &self.cam as *const CameraUniform as *const u8;
            std::slice::from_raw_parts(p, std::mem::size_of::<CameraUniform>())
        };
        queue.write_buffer(&self.cam_buf, 0, cam_clone);
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.ribbon_pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_vertex_buffer(0, self.ribbon_buf.slice(..));
        pass.draw(0..6, 0..self.ribbons.len() as u32);

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_vertex_buffer(0, self.card_buf.slice(..));
        let n = self.cards.len() as u32;
        pass.draw(0..6, 0..n);
    }

    pub fn draw_ui(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.ui_pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_bind_group(1, &self.ui_bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn wave_at(r: f32, band: f32, radius: f32) -> f32 {
    if r <= 0.0 {
        return 0.0;
    }
    ((r - radius) / band.max(1.0)).clamp(0.0, 1.0)
}

/// Deterministic hash in 0..1, matching the feel of the site's seeded field.
fn hash(n: u32) -> f32 {
    let x = (n.wrapping_mul(2654435761) ^ (n >> 13)) as f32;
    (x.sin() * 43758.5453).fract().abs()
}

fn build_field(tex_count: u32) -> (Vec<Card>, Vec<RibbonInstance>) {
    let mut cards = Vec::new();
    let arms: u32 = 5;
    let per_arm: u32 = 9;
    let mut id = 0u32;
    for a in 0..arms {
        for i in 0..per_arm {
            let h1 = hash(id + 1);
            let h2 = hash(id + 77);
            let h3 = hash(id + 149);
            id += 1;
            let arm_ang = (a as f32 / arms as f32) * std::f32::consts::TAU + h1 * 0.34;
            // radius expands outward and rotates a little per ring
            let rr = 190.0 + (i as f32 + 0.35 + h2 * 0.5) * 165.0;
            let ang = arm_ang + (i as f32) * 0.14 + h3 * 0.05;
            let x = ang.cos() * rr;
            let y = ang.sin() * rr * 0.72;
            let z = -60.0 - rr * 0.42;
            // Cards face outward from the field centre.
            let yaw = ang + std::f32::consts::FRAC_PI_2;
            let depth_t = (-z / 700.0).clamp(0.0, 1.0);
            let w = (250.0 + 140.0 * depth_t + h1 * 60.0).max(70.0);
            let h = w / 1.6;
            cards.push(Card {
                x,
                y,
                z,
                yaw,
                w,
                h,
                tex: (id % tex_count) as u32,
                alpha: (0.92 - depth_t * 0.45).max(0.18),
                blur: (depth_t * 0.012 + 0.0004).min(0.014),
                aber: 0.35 + depth_t * 0.5,
                phase: (id as f64 * 0.137) % 1.0,
                radius: (x * x + y * y).sqrt(),
                bob: 3.0 + h2 * 6.0,
            });
        }
    }

    // Comet arms: radial spokes with pulses travelling outward.
    let mut ribbons = Vec::new();
    for a in 0..arms {
        let ang = (a as f32 / arms as f32) * std::f32::consts::TAU;
        let dir = [ang.cos(), ang.sin()];
        let len = 1500.0;
        ribbons.push(RibbonInstance {
            od: [-dir[0] * 20.0, -dir[1] * 20.0, dir[0], dir[1]],
            params: [len, a as f32, hash(a + 3) * 4.0, 0.5],
        });
    }
    (cards, ribbons)
}

pub fn load_screenshot(path: &PathBuf) -> Option<(Vec<u8>, u32, u32)> {
    let f = std::fs::File::open(path).ok()?;
    let dec = png::Decoder::new(f);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0u8; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let bytes = &buf[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Rgba => Some((bytes.to_vec(), info.width, info.height)),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity((info.width as usize) * (info.height as usize) * 4);
            for p in bytes.chunks_exact(3) {
                out.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
            Some((out, info.width, info.height))
        }
        _ => None,
    }
}

pub fn rgba_bytes(c: Rgba) -> [u8; 4] {
    c.to_bytes()
}
