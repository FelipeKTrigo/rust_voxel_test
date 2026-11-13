use wgpu::util::DeviceExt;
use crate::camera::Camera;
use crate::renderer::context::RenderContext;
use crate::renderer::mesh::{VoxelVertex, WaterVertex};
use crate::sky::SkyState;

/// Uniform da câmera + céu. DEVE espelhar `CameraUniform` em assets/shaders/common.wgsl.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
    pub time: f32,
    /// x = câmera submersa (0/1), y = nível do mar, z = início do fog, w = fim do fog.
    pub params: [f32; 4],
    /// xyz = direção do sol, w = luz do dia.
    pub sun: [f32; 4],
    /// xyz = direção da lua, w = fase.
    pub moon: [f32; 4],
    /// x = hora do dia, y = dia.
    pub sky: [f32; 4],
    /// xyz = origem do cometa, w = progresso (-1 = sem cometa).
    pub comet_a: [f32; 4],
    pub comet_b: [f32; 4],
}

pub const COMMON_WGSL: &str = include_str!("../../assets/shaders/common.wgsl");

/// Fontes WGSL completas (prelúdio + shader). Usadas também pelos testes de validação.
pub fn voxel_source() -> String {
    format!("{}\n{}", COMMON_WGSL, include_str!("../../assets/shaders/voxel.wgsl"))
}
pub fn water_source() -> String {
    format!("{}\n{}", COMMON_WGSL, include_str!("../../assets/shaders/water.wgsl"))
}
pub fn sky_source() -> String {
    format!("{}\n{}", COMMON_WGSL, include_str!("../../assets/shaders/sky.wgsl"))
}
pub fn fish_source() -> String {
    format!("{}\n{}", COMMON_WGSL, include_str!("../../assets/shaders/fish.wgsl"))
}

impl CameraUniform {
    pub fn new(camera: &Camera, time: f32, underwater: bool, sky: &SkyState, fog: (f32, f32)) -> Self {
        let vp = camera.view_proj();
        let (comet_a, comet_b) = match &sky.comet {
            Some(c) => ([c.from.x, c.from.y, c.from.z, c.progress], [c.to.x, c.to.y, c.to.z, 0.0]),
            None => ([0.0, 1.0, 0.0, -1.0], [0.0, 1.0, 0.0, 0.0]),
        };
        Self {
            view_proj: vp.to_cols_array_2d(),
            inv_view_proj: vp.inverse().to_cols_array_2d(),
            camera_pos: camera.position.to_array(),
            time,
            params: [
                if underwater { 1.0 } else { 0.0 },
                crate::world::gen::types::SEA_LEVEL as f32,
                fog.0,
                fog.1,
            ],
            sun: [sky.sun_dir.x, sky.sun_dir.y, sky.sun_dir.z, sky.daylight],
            moon: [sky.moon_dir.x, sky.moon_dir.y, sky.moon_dir.z, sky.moon_phase],
            sky: [sky.time_of_day, sky.day as f32, 0.0, 0.0],
            comet_a,
            comet_b,
        }
    }

    /// Distâncias de fog para uma distância de renderização em chunks: começa em ~55% do alcance
    /// e fecha exatamente na borda, escondendo o surgimento de chunks.
    pub fn fog_for_render_distance(render_distance: i32) -> (f32, f32) {
        let end = ((render_distance.max(2) as f32) * crate::world::chunk::CHUNK_SIZE as f32).max(96.0);
        (end * 0.55, end)
    }
}

/// Array de texturas dos blocos (uma camada por bloco × face) já na GPU.
pub struct BlockTextures {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl BlockTextures {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        use crate::world::textures as tx;
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("Block texture array"),
                size: wgpu::Extent3d {
                    width: tx::TILE as u32,
                    height: tx::TILE as u32,
                    depth_or_array_layers: tx::NUM_LAYERS as u32,
                },
                mip_level_count: tx::MIP_LEVELS as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &tx::gpu_bytes(),
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        // Ampliação "nearest" (pixels nítidos de perto); mipmaps lineares (sem cintilar de longe).
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Block sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Block texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Block texture bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        Self { bind_group_layout, bind_group }
    }
}

pub struct VoxelPipeline {
    pub render_pipeline: wgpu::RenderPipeline,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub water_pipeline: wgpu::RenderPipeline,
    /// Mesma geometria do terreno, mas com mistura alfa e sem escrever profundidade (partículas).
    pub fx_pipeline: wgpu::RenderPipeline,
    pub sky_pipeline: wgpu::RenderPipeline,
    pub textures: BlockTextures,
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

        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sky Shader"),
            source: wgpu::ShaderSource::Wgsl(sky_source().into()),
        });
        let textures = BlockTextures::new(device, &ctx.queue);

        // Camera uniform buffer
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Uniform Buffer"),
            contents: bytemuck::cast_slice(&[CameraUniform::new(
                &Camera::new(glam::Vec3::ZERO, 1.0),
                0.0,
                false,
                &SkyState::noon(),
                CameraUniform::fog_for_render_distance(8),
            )]),
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

        let camera_only_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Camera-only Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Voxel Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &textures.bind_group_layout],
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
                    // Terreno 100% opaco: sem mistura, nada deixa ver através dos chunks.
                    blend: None,
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
            layout: Some(&camera_only_layout),
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

        // Partículas: mesma geometria/shader do terreno, com mistura alfa e sem profundidade.
        let fx_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("FX Render Pipeline"),
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
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
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

        // Céu: triângulo de tela cheia no fundo (sem teste nem escrita de profundidade).
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky Render Pipeline"),
            layout: Some(&camera_only_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
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
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            render_pipeline,
            fx_pipeline,
            sky_pipeline,
            textures,
            camera_buffer,
            camera_bind_group,
            camera_bind_group_layout,
            water_pipeline,
        }
    }

    pub fn update_camera(
        &self,
        queue: &wgpu::Queue,
        camera: &Camera,
        time: f32,
        underwater: bool,
        sky: &SkyState,
        render_distance: i32,
    ) {
        let uniform = CameraUniform::new(camera, time, underwater, sky, CameraUniform::fog_for_render_distance(render_distance));
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
        validate("sky", &sky_source());
    }

    #[test]
    fn camera_uniform_layout_matches_wgsl() {
        // 2 × mat4 (128) + vec3+f32 (16) + 6 × vec4 (96)
        assert_eq!(std::mem::size_of::<CameraUniform>(), 240);
    }

    #[test]
    fn vertex_strides() {
        assert_eq!(std::mem::size_of::<WaterVertex>(), 44);
        assert_eq!(std::mem::size_of::<VoxelVertex>(), 52);
    }

    #[test]
    fn wgsl_constants_match_rust() {
        use crate::world::textures as tx;
        let src = include_str!("../../assets/shaders/voxel.wgsl");
        assert!(src.contains(&format!("const NO_TEXTURE: u32 = {}u;", tx::NO_TEXTURE)));
        assert!(src.contains(&format!("const TEX_LAYERS: u32 = {}u;", tx::NUM_LAYERS)));
        assert!(src.contains(&format!("const NO_VARIATION: u32 = {}u;", tx::NO_VARIATION_BIT)));
        assert!(src.contains("& 16383u"), "máscara que remove o bit de variação");
        assert_eq!(0x3FFF, tx::NO_VARIATION_BIT - 1, "a máscara cobre todas as camadas");
    }

    #[test]
    fn uniform_carries_the_sky_state() {
        use crate::sky::WorldClock;
        let cam = Camera::new(glam::Vec3::new(1.0, 80.0, 2.0), 16.0 / 9.0);
        let mut clock = WorldClock::new();
        clock.set_time_of_day(0.5);
        let u = CameraUniform::new(&cam, 3.5, true, &clock.sky_state(1), (100.0, 200.0));
        assert_eq!(u.camera_pos, [1.0, 80.0, 2.0]);
        assert_eq!(u.time, 3.5);
        assert_eq!(u.params, [1.0, crate::world::gen::types::SEA_LEVEL as f32, 100.0, 200.0]);
        assert!(u.sun[1] > 0.9 && u.sun[3] > 0.99, "meio-dia: sol alto e dia pleno");
        assert!((u.sky[0] - 0.5).abs() < 1e-3);
        assert_eq!(u.comet_a[3], -1.0, "sem cometa de dia");
        // inv_view_proj desfaz view_proj
        let vp = glam::Mat4::from_cols_array_2d(&u.view_proj);
        let inv = glam::Mat4::from_cols_array_2d(&u.inv_view_proj);
        let id = vp * inv;
        for (i, c) in id.to_cols_array().iter().enumerate() {
            let want = if i % 5 == 0 { 1.0 } else { 0.0 };
            assert!((c - want).abs() < 1e-3, "elemento {i} = {c}");
        }
    }

    #[test]
    fn fog_closes_at_the_edge_of_the_loaded_world() {
        let mut prev = 0.0;
        for rd in 4..=24 {
            let (start, end) = CameraUniform::fog_for_render_distance(rd);
            assert!(start < end && end > prev);
            assert_eq!(end, rd as f32 * 32.0, "opaco exatamente na borda dos chunks carregados");
            prev = end;
            let mut cam = Camera::new(glam::Vec3::ZERO, 1.0);
            cam.set_far_for_render_distance(rd);
            assert!(cam.far > end, "o plano distante fica além do fog");
        }
    }

    #[test]
    fn forced_comet_reaches_the_uniform() {
        use crate::sky::{Comet, WorldClock};
        let mut sky = WorldClock::new().sky_state(0);
        sky.comet = Some(Comet { from: glam::Vec3::X, to: glam::Vec3::Y, progress: 0.4 });
        let u = CameraUniform::new(&Camera::new(glam::Vec3::ZERO, 1.0), 0.0, false, &sky, (1.0, 2.0));
        assert_eq!(u.comet_a, [1.0, 0.0, 0.0, 0.4]);
        assert_eq!(u.comet_b[..3], [0.0, 1.0, 0.0]);
    }
}
