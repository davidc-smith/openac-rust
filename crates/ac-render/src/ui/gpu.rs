//! The UI on the GPU: one pipeline (screen-pixel triangles, straight
//! alpha, no depth), a texture per render surface the UI names, sampled
//! linear or nearest and always repeating, and one pass drawn over the
//! world with what the frame's batch holds.
//!
//! OpenAC's `TextRenderer` and the UI half of `TextureCache`
//! (`GetOrUploadRenderSurface`, `UploadUiTexture`): a surface the dats do
//! not hold draws as the 1 x 1 magenta placeholder (named once); a runtime
//! texture nobody assigned draws nothing.

use std::collections::{HashMap, HashSet};

use ac_dat::texture::DecodedTexture;
use ac_ui::draw::{Filter, TextureRef};

use super::batch::{UiBatch, UiTexture, UiTextureKey};

/// Where the UI's texture pixels come from.
pub trait UiTextureSource {
    /// The pixels of `texture`, `None` when there are none.
    fn decode(&mut self, texture: TextureRef) -> Option<DecodedTexture>;
}

struct GpuTexture {
    /// The texture the UI uploaded (`None`: one the renderer draws into,
    /// bound by [`UiRenderer::bind_view`]).
    _texture: Option<wgpu::Texture>,
    linear: wgpu::BindGroup,
    nearest: wgpu::BindGroup,
}

impl GpuTexture {
    fn bind_group(&self, filter: Filter) -> &wgpu::BindGroup {
        match filter {
            Filter::Linear => &self.linear,
            Filter::Nearest => &self.nearest,
        }
    }
}

/// A run ready to draw.
#[derive(Debug, Clone, Copy)]
struct PreparedRun {
    key: UiTextureKey,
    first: u32,
    count: u32,
}

/// The UI's GPU objects. Built with the window, dropped with it.
pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    screen_buffer: wgpu::Buffer,
    screen_group: wgpu::BindGroup,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    vertices: wgpu::Buffer,
    vertex_capacity: u64,
    white: GpuTexture,
    textures: HashMap<TextureRef, GpuTexture>,
    /// Textures drawn from a view the renderer draws into, by the view's
    /// generation.
    views: HashMap<TextureRef, u64>,
    missing: HashSet<u32>,
    new_missing: Vec<u32>,
    prepared: Vec<PreparedRun>,
}

impl std::fmt::Debug for UiRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiRenderer")
            .field("textures", &self.textures.len())
            .field("missing", &self.missing.len())
            .field("runs", &self.prepared.len())
            .finish_non_exhaustive()
    }
}

/// The magenta placeholder of a surface that cannot be read.
fn magenta() -> DecodedTexture {
    DecodedTexture {
        width: 1,
        height: 1,
        rgba: vec![0xFF, 0x00, 0xFF, 0xFF],
    }
}

fn sampler(device: &wgpu::Device, label: &str, filter: wgpu::FilterMode) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(label),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

impl UiRenderer {
    /// The pipeline and samplers for a target of `format`.
    #[must_use]
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        use crate::renderer::pipelines::{sampler_entry, texture_entry, uniform_entry};
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/ui.wgsl").into()),
        });
        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui screen"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui texture"),
            entries: &[
                texture_entry(0, wgpu::TextureViewDimension::D2),
                sampler_entry(1),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&screen_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x2,
            1 => Float32x2,
            2 => Float32x4,
        ];
        // Straight alpha: colour = src * a + dst * (1 - a).
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<super::batch::UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui screen"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui screen"),
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_buffer.as_entire_binding(),
            }],
        });
        let linear = sampler(device, "ui linear repeat", wgpu::FilterMode::Linear);
        let nearest = sampler(device, "ui nearest repeat", wgpu::FilterMode::Nearest);
        let vertex_capacity = 64 * 1024;
        let vertices = crate::renderer::pipelines::vertex_buffer(
            device,
            "ui vertices",
            vertex_capacity as usize,
        );
        let white = upload(
            device,
            queue,
            &texture_layout,
            (&linear, &nearest),
            "ui white",
            &DecodedTexture {
                width: 1,
                height: 1,
                rgba: vec![0xFF; 4],
            },
        );
        Self {
            pipeline,
            texture_layout,
            screen_buffer,
            screen_group,
            linear,
            nearest,
            vertices,
            vertex_capacity,
            white,
            textures: HashMap::new(),
            views: HashMap::new(),
            missing: HashSet::new(),
            new_missing: Vec::new(),
            prepared: Vec::new(),
        }
    }

    /// Upload what `batch` draws for a target of `size` pixels; textures
    /// first seen are read from `source`.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: &UiBatch,
        size: (u32, u32),
        source: &mut dyn UiTextureSource,
    ) {
        self.prepared.clear();
        if batch.is_empty() || size.0 == 0 || size.1 == 0 {
            return;
        }
        let screen = [size.0 as f32, size.1 as f32, 0.0, 0.0];
        queue.write_buffer(&self.screen_buffer, 0, bytemuck::cast_slice(&screen));
        let bytes: &[u8] = bytemuck::cast_slice(&batch.vertices);
        let needed = bytes.len() as u64;
        if needed > self.vertex_capacity {
            self.vertex_capacity = needed.next_power_of_two();
            self.vertices = crate::renderer::pipelines::vertex_buffer(
                device,
                "ui vertices",
                self.vertex_capacity as usize,
            );
        }
        queue.write_buffer(&self.vertices, 0, bytes);
        for run in &batch.runs {
            let ready = match run.key.texture {
                UiTexture::White => true,
                UiTexture::Named(texture) => self.ensure(device, queue, texture, source),
            };
            if ready {
                self.prepared.push(PreparedRun {
                    key: run.key,
                    first: run.first,
                    count: run.count,
                });
            }
        }
    }

    /// Draw `texture` from `view`, a texture the renderer draws into (a 3-D
    /// viewport's); `generation` names the view, and the same generation
    /// again keeps the binding.
    pub fn bind_view(
        &mut self,
        device: &wgpu::Device,
        texture: TextureRef,
        view: &wgpu::TextureView,
        generation: u64,
    ) {
        if self.views.get(&texture) == Some(&generation) && self.textures.contains_key(&texture) {
            return;
        }
        let label = match texture {
            TextureRef::Surface(id) => format!("ui view {id:#010x}"),
            TextureRef::Runtime(h) => format!("ui view {h}"),
        };
        let gpu = bind(
            device,
            &self.texture_layout,
            (&self.linear, &self.nearest),
            &label,
            view,
        );
        self.textures.insert(texture, gpu);
        self.views.insert(texture, generation);
    }

    /// Render surfaces the dats could not give since the last call (each
    /// named once per window).
    pub fn take_missing(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.new_missing)
    }

    /// How many textures are on the GPU.
    #[must_use]
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Record the prepared UI over what `target` holds.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        if self.prepared.is_empty() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.screen_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        for run in &self.prepared {
            let texture = match run.key.texture {
                UiTexture::White => &self.white,
                UiTexture::Named(t) => match self.textures.get(&t) {
                    Some(texture) => texture,
                    None => continue,
                },
            };
            pass.set_bind_group(1, texture.bind_group(run.key.filter), &[]);
            pass.draw(run.first..run.first + run.count, 0..1);
        }
    }

    /// Make sure `texture` is on the GPU; false when it draws nothing.
    fn ensure(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: TextureRef,
        source: &mut dyn UiTextureSource,
    ) -> bool {
        if self.textures.contains_key(&texture) {
            return true;
        }
        let pixels = match (source.decode(texture), texture) {
            (Some(pixels), _) => pixels,
            (None, TextureRef::Surface(id)) => {
                if self.missing.insert(id) {
                    self.new_missing.push(id);
                }
                magenta()
            }
            // A runtime texture nobody has assigned yet: nothing to draw.
            (None, TextureRef::Runtime(_)) => return false,
        };
        let label = match texture {
            TextureRef::Surface(id) => format!("ui surface {id:#010x}"),
            TextureRef::Runtime(h) => format!("ui runtime {h}"),
        };
        let gpu = upload(
            device,
            queue,
            &self.texture_layout,
            (&self.linear, &self.nearest),
            &label,
            &pixels,
        );
        self.textures.insert(texture, gpu);
        true
    }
}

/// Put `pixels` on the GPU with a linear and a nearest binding. Pixels that
/// do not fill their size (a broken decode) upload as magenta.
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    (linear, nearest): (&wgpu::Sampler, &wgpu::Sampler),
    label: &str,
    pixels: &DecodedTexture,
) -> GpuTexture {
    let whole = pixels.width > 0
        && pixels.height > 0
        && pixels.rgba.len() as u64 == 4 * u64::from(pixels.width) * u64::from(pixels.height);
    let fallback;
    let pixels = if whole {
        pixels
    } else {
        fallback = magenta();
        &fallback
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: pixels.width,
            height: pixels.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    crate::renderer::pipelines::write_layer(
        queue,
        &texture,
        0,
        0,
        pixels.width,
        pixels.height,
        &pixels.rgba,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    GpuTexture {
        _texture: Some(texture),
        ..bind(device, layout, (linear, nearest), label, &view)
    }
}

/// A texture view bound with a linear and a nearest sampler.
fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    (linear, nearest): (&wgpu::Sampler, &wgpu::Sampler),
    label: &str,
    view: &wgpu::TextureView,
) -> GpuTexture {
    let group = |sampler: &wgpu::Sampler| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    };
    GpuTexture {
        linear: group(linear),
        nearest: group(nearest),
        _texture: None,
    }
}
