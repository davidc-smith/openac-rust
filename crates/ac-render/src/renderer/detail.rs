//! The retail detail textures: the region's building and environment
//! detail images, repeated over building shells and interior cell walls
//! and blended into the lit surface by the mesh shader. They draw only with
//! the "building detail textures" option on and a non-zero tiling.

use super::pipelines::write_layer;
use crate::assets::DetailTexture;

/// The tilings the shader reads: x building shells, y interior cells; 0
/// where the detail is off (the option, a missing texture or a zero
/// tiling).
#[must_use]
pub fn detail_tiling(enabled: bool, tilings: [Option<f32>; 2]) -> [f32; 4] {
    let on = |t: Option<f32>| t.filter(|t| enabled && *t != 0.0).unwrap_or(0.0);
    [on(tilings[0]), on(tilings[1]), 0.0, 0.0]
}

/// The detail textures on the GPU and their bind group.
pub(super) struct DetailGpu {
    white: wgpu::TextureView,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
    pub group: wgpu::BindGroup,
    /// Building (0) and environment (1): the image and its tiling.
    textures: [Option<(wgpu::TextureView, f32)>; 2],
    enabled: bool,
}

impl DetailGpu {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        enabled: bool,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("detail none"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        write_layer(queue, &texture, 0, 0, 1, 1, &[255; 4]);
        let white = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("detail"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("detail params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = Self::group(device, layout, &white, &white, &sampler, &params);
        let this = Self {
            white,
            sampler,
            params,
            group,
            textures: [None, None],
            enabled,
        };
        this.write_params(queue);
        this
    }

    fn group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        building: &wgpu::TextureView,
        environment: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        params: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("detail"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(building),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(environment),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params.as_entire_binding(),
                },
            ],
        })
    }

    fn write_params(&self, queue: &wgpu::Queue) {
        let tiling = detail_tiling(
            self.enabled,
            [
                self.textures[0].as_ref().map(|t| t.1),
                self.textures[1].as_ref().map(|t| t.1),
            ],
        );
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&tiling));
    }

    /// The region's detail textures arrived: upload them with their mips.
    pub fn set_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        details: &[Option<DetailTexture>; 2],
    ) {
        for (slot, detail) in self.textures.iter_mut().zip(details) {
            *slot = detail.as_ref().map(|d| {
                let image = &d.image;
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("detail"),
                    size: wgpu::Extent3d {
                        width: image.width,
                        height: image.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: image.levels.len() as u32,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let (mut w, mut h) = (image.width, image.height);
                for (level, pixels) in image.levels.iter().enumerate() {
                    write_layer(queue, &texture, level as u32, 0, w, h, pixels);
                    (w, h) = ((w / 2).max(1), (h / 2).max(1));
                }
                (
                    texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    d.tiling,
                )
            });
        }
        let view = |i: usize| self.textures[i].as_ref().map_or(&self.white, |t| &t.0);
        self.group = Self::group(
            device,
            layout,
            view(0),
            view(1),
            &self.sampler,
            &self.params,
        );
        self.write_params(queue);
    }

    /// The "building detail textures" option changed.
    pub fn set_enabled(&mut self, queue: &wgpu::Queue, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.write_params(queue);
        }
    }
}

#[cfg(test)]
mod tests {
    //! OpenAC `RetailDetailTextureContract.ShouldRender` (the option and an
    //! available texture) and the dispatchers' `Tiling != 0` gate.

    use super::*;

    #[test]
    fn detail_draws_only_with_the_option_a_texture_and_a_tiling() {
        assert_eq!(
            detail_tiling(true, [Some(10.0), Some(4.0)]),
            [10.0, 4.0, 0.0, 0.0]
        );
        assert_eq!(detail_tiling(false, [Some(10.0), Some(4.0)]), [0.0; 4]);
        assert_eq!(detail_tiling(true, [None, Some(0.0)]), [0.0; 4]);
    }
}
