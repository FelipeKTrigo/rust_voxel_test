use wgpu::util::DeviceExt;
use crate::fauna::fish::{FishInstance, MAX_FISH};
use crate::renderer::context::RenderContext;
use crate::renderer::pipeline::{fish_source, VoxelPipeline};

/// Pixel art 16x8, 2 frames (cauda para cima / para baixo). Peixe olhando para a direita.
/// '.' = transparente, 'b' = corpo, 'l' = barriga clara, 'd' = escuro, 'k' = olho.
pub const FRAME_W: usize = 16;
pub const FRAME_H: usize = 8;
const FRAME_A: [&str; FRAME_H] = [
    "................",
    "..dd.....dd.....",
    "..dbb.bbbbbd....",
    "...bbbbbbbbbbk..",
    "...bbllllllbbb..",
    "..dbbblllbbb....",
    "..dd..bbbd......",
    "................",
];
const FRAME_B: [&str; FRAME_H] = [
    "................",
    "................",
    "..dd.bbbbbd.....",
    "..dbbbbbbbbbbk..",
    "...bbllllllbbb..",
    "...bbblllbbb....",
    "..dbb.bbbd......",
    "..dd............",
];

fn texel(c: u8) -> [u8; 4] {
    match c {
        b'b' => [205, 205, 205, 255],
        b'l' => [250, 250, 250, 255],
        b'd' => [120, 120, 120, 255],
        b'k' => [15, 15, 25, 255],
        _ => [0, 0, 0, 0],
    }
}

/// RGBA do spritesheet (2 frames lado a lado): 32x8.
pub fn sprite_rgba() -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_W * 2 * FRAME_H * 4);
    for y in 0..FRAME_H {
        for frame in [&FRAME_A, &FRAME_B] {
            let row = frame[y].as_bytes();
            assert_eq!(row.len(), FRAME_W, "linha de sprite com largura errada");
            for &c in row {
                out.extend_from_slice(&texel(c));
            }
        }
    }
    out
}

pub struct FishPipeline {
    pipeline: wgpu::RenderPipeline,
    tex_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    count: u32,
}

impl FishPipeline {
    pub fn new(ctx: &RenderContext, voxel: &VoxelPipeline) -> Self {
        let device = &ctx.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fish Shader"),
            source: wgpu::ShaderSource::Wgsl(fish_source().into()),
        });

        let texture = device.create_texture_with_data(
            &ctx.queue,
            &wgpu::TextureDescriptor {
                label: Some("Fish Sprite"),
                size: wgpu::Extent3d { width: (FRAME_W * 2) as u32, height: FRAME_H as u32, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &sprite_rgba(),
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Fish Tex Layout"),
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
        let tex_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Fish Tex Bind Group"),
            layout: &tex_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Fish Pipeline Layout"),
            bind_group_layouts: &[&voxel.camera_bind_group_layout, &tex_layout],
            push_constant_ranges: &[],
        });

        let attribs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2, 2 => Float32, 3 => Float32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Fish Pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<FishInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attribs,
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: ctx.surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Fish Instances"),
            contents: &vec![0u8; MAX_FISH * std::mem::size_of::<FishInstance>()],
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self { pipeline, tex_bind_group, instance_buffer, count: 0 }
    }

    pub fn update(&mut self, queue: &wgpu::Queue, instances: &[FishInstance]) {
        let n = instances.len().min(MAX_FISH);
        if n > 0 {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances[..n]));
        }
        self.count = n as u32;
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, camera_bg: &'a wgpu::BindGroup) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bg, &[]);
        pass.set_bind_group(1, &self.tex_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, 0..self.count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_has_expected_size_and_content() {
        let px = sprite_rgba();
        assert_eq!(px.len(), FRAME_W * 2 * FRAME_H * 4);
        let opaque = px.chunks(4).filter(|p| p[3] == 255).count();
        assert!(opaque > 40 && opaque < FRAME_W * 2 * FRAME_H);
    }

    #[test]
    fn frames_differ() {
        assert_ne!(FRAME_A, FRAME_B);
    }
}
