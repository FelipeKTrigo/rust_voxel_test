use wgpu::util::DeviceExt;
use crate::camera::Camera;
use crate::renderer::context::RenderContext;
use crate::renderer::mesh::{VoxelVertex, WaterVertex};

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
    pub time: f32,
    /// x = câmera submersa (0/1), y = nível do mar, zw reservado.
    pub params: [f32; 4],
}

pub const COMMON_WGSL: &str = include_str!("../../assets/shaders/common.wgsl");

/// Fontes WGSL completas (prelúdio + shader). Usadas também pelos testes de validação.
pub fn voxel_source() -> String {
    format!("{}\n{}", COMMON_WGSL, include_str!("../../assets/shaders/voxel.wgsl"))
}
pub fn water_source() -> String {
    format!("{}\n{}", COMMON_WGSL, include_str!("../../assets/shaders/water.wgsl"))
}
pub fn fish_source() -> String {
    format!("{}\n{}", COMMON_WGSL, include_str!("../../assets/shaders/fish.wgsl"))
}

impl CameraUniform {
    pub fn new(camera: &Camera, time: f32, underwater: bool) -> Self {
        let vp = camera.view_proj();
        Self {
            view_proj: vp.to_cols_array_2d(),
            camera_pos: camera.position.to_array(),
            time,
            params: [if underwater { 1.0 } else { 0.0 }, crate::world::gen::types::SEA_LEVEL as f32, 0.0, 0.0],
        }
    }
}

pub struct VoxelPipeline {
    pub render_pipeline: wgpu::RenderPipeline,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub water_pipeline: wgpu::RenderPipeline,
}

impl VoxelPipeline {
    pub fn new(ctx: &RenderContext) -> Self {
        let device = &ctx.device;

        // Load shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Voxel Shader"),
            source: wgpu::ShaderSource::Wgsl(voxel_source().into()),
        });
        let water_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Water Shader"),
            source: wgpu::ShaderSource::Wgsl(water_source().into()),
        });

        // Camera uniform buffer
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Uniform Buffer"),
            contents: bytemuck::cast_slice(&[CameraUniform {
                view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
                camera_pos: [0.0, 0.0, 0.0],
                time: 0.0,
                params: [0.0; 4],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Voxel Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Voxel Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[VoxelVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: ctx.surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Água: translúcida, não escreve profundidade, visível dos dois lados.
        let water_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Water Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &water_shader,
                entry_point: "vs_main",
                buffers: &[WaterVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &water_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: ctx.surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            render_pipeline,
            camera_buffer,
            camera_bind_group,
            camera_bind_group_layout,
            water_pipeline,
        }
    }

    pub fn update_camera(&self, queue: &wgpu::Queue, camera: &Camera, time: f32, underwater: bool) {
        let uniform = CameraUniform::new(camera, time, underwater);
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Valida os shaders sem GPU (naga é o mesmo front-end WGSL usado pelo wgpu 22).
    fn validate(name: &str, src: &str) {
        let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }

    #[test]
    fn shaders_are_valid() {
        validate("voxel", &voxel_source());
        validate("water", &water_source());
        validate("fish", &fish_source());
    }

    #[test]
    fn camera_uniform_layout_matches_wgsl() {
        // mat4 (64) + vec3+f32 (16) + vec4 (16)
        assert_eq!(std::mem::size_of::<CameraUniform>(), 96);
    }

    #[test]
    fn vertex_strides() {
        assert_eq!(std::mem::size_of::<WaterVertex>(), 44);
        assert_eq!(std::mem::size_of::<VoxelVertex>(), 40);
    }
}
