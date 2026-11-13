use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, WindowEvent, ElementState};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use glam::Vec3;

use crate::camera::Camera;
use crate::input::InputState;
use crate::renderer::{DynMesh, FishPipeline, RenderContext, VoxelPipeline};
use crate::fauna::Fauna;
use crate::particles::ParticleSystem;
use crate::world::{BlockType, ChunkManager, ColorPalette};
use crate::physics::BlockQuery;
use crate::player::{Player, PlayerMode};
use crate::ui::{self, PlayerModeUi};
use crate::ui::main_menu::{main_menu_screen, settings_screen, MainMenuAction};
use crate::ui::pause_menu::{pause_menu_screen, PauseAction, PauseState};
use crate::ui::settings::GraphicsSettings;
use crate::ui::world_select::{list_worlds, world_select_screen, WorldEntry, WorldSelectAction, WorldSelectState};
use std::path::PathBuf;
use crate::world::save::{self, SaveHeader, WorldSave};

const SAVES_DIR: &str = "saves";
const SETTINGS_PATH: &str = "settings.bin";

fn mode_to_ui(m: PlayerMode) -> PlayerModeUi {
    match m {
        PlayerMode::Survival => PlayerModeUi::Survival,
        PlayerMode::Creative => PlayerModeUi::Creative,
        PlayerMode::Spectator => PlayerModeUi::Spectator,
    }
}

fn mode_from_ui(m: PlayerModeUi) -> PlayerMode {
    match m {
        PlayerModeUi::Survival => PlayerMode::Survival,
        PlayerModeUi::Creative => PlayerMode::Creative,
        PlayerModeUi::Spectator => PlayerMode::Spectator,
    }
}

/// Comandos produzidos pela UI e aplicados após o frame.
enum UiCmd {
    ToWorldSelect,
    ToSettings,
    ToMain,
    Quit,
    Create { name: String, seed: u32 },
    Load(usize),
    Delete(usize),
    Resume,
    SetMode(PlayerMode),
    QuitToMenu,
    SaveAndQuit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    MainMenu,
    WorldSelect,
    Settings,
    InGame,
    Paused,
}

struct GameData {
    window: Arc<Window>,
    ctx: RenderContext,
    pipeline: VoxelPipeline,
    fish_pipeline: FishPipeline,
    boat_mesh: DynMesh,
    particles: ParticleSystem,
    particle_mesh: DynMesh,
    fauna: Fauna,
    /// Tempo de jogo (só avança em InGame; congela na pausa).
    time: f32,
    underwater: bool,
    camera: Camera,
    input: InputState,
    chunk_manager: Option<ChunkManager>,
    palette: ColorPalette,
    player: Player,
    settings: GraphicsSettings,
    applied_threads: usize,
    worlds: Vec<WorldEntry>,
    ws_state: WorldSelectState,
    pause_state: PauseState,
    break_requested: bool,
    current_world: Option<String>,
    current_seed: u32,
    autosave_timer: f32,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    last_frame: Instant,
}

pub struct App {
    data: Option<GameData>,
    state: GameState,
}

impl App {
    pub fn new() -> Self {
        Self {
            data: None,
            state: GameState::MainMenu,
        }
    }

    fn set_cursor_captured(window: &Window, captured: bool) {
        if captured {
            let _ = window.set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            window.set_cursor_visible(false);
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
        }
    }

    fn update(&mut self, dt: f32) {
        let data = match &mut self.data {
            Some(d) => d,
            None => return,
        };

        if self.state != GameState::InGame {
            return;
        }

        let (dx, dy) = data.input.take_mouse_delta();
        data.camera.rotate(dx, dy);

        let Some(cm) = data.chunk_manager.as_mut() else { return };

        // Configurações em tempo real
        cm.render_distance = data.settings.render_distance;
        if data.applied_threads != data.settings.gen_threads {
            cm.set_threads(data.settings.gen_threads);
            data.applied_threads = data.settings.gen_threads;
        }
        cm.lod_level = data.settings.lod_level;
        // Balanço CPU/GPU: fatia do orçamento por frame dedicada ao trabalho de CPU
        // (geração de blocos e construção de malhas, ambos paralelos no pool).
        let bal = data.settings.cpu_gpu_balance.clamp(0.0, 1.0);
        cm.max_gen_per_update = 4 + (bal * 28.0) as usize;
        cm.max_mesh_per_update = 4 + (bal * 20.0) as usize;
        data.camera.set_far_for_render_distance(data.settings.render_distance);

        data.player.update(dt, &data.input, data.camera.yaw, &*cm);
        data.camera.position = data.player.eye_position();

        // Quebrar bloco (clique esquerdo)
        if std::mem::take(&mut data.break_requested) && data.player.mode.can_break_blocks() {
            if let Some(hit) = data.player.target_block(data.camera.forward(), &*cm) {
                let broken = cm.block_at(hit.block.x, hit.block.y, hit.block.z);
                if broken != BlockType::Bedrock {
                    cm.set_block_world(hit.block.x, hit.block.y, hit.block.z, BlockType::Air);
                    data.particles.spawn_break(hit.block, broken, data.settings.reduce_particles);
                }
            }
        }

        // Água, peixes e barcos (congelam na pausa, pois update só roda em InGame)
        data.time += dt;
        {
            let eye = data.camera.position;
            data.underwater = cm.block_at(eye.x.floor() as i32, eye.y.floor() as i32, eye.z.floor() as i32)
                == BlockType::Water;
            let reduced = data.settings.reduce_particles;
            data.fauna.update(dt, data.time, data.player.position, &*cm, reduced);
            let (bv, bi) = crate::fauna::boat::build_mesh(data.fauna.boats.boats.values(), data.time);
            data.boat_mesh.update(&data.ctx.device, &bv, &bi);
            data.particles.update(dt, &*cm, reduced);
            let (pv, pi) = data.particles.build_mesh(&data.camera);
            data.particle_mesh.update(&data.ctx.device, &pv, &pi);
            let inst = data.fauna.fish.instances(data.time, reduced);
            data.fish_pipeline.update(&data.ctx.queue, &inst);
        }

        if std::env::var_os("DNE_SMOKE").is_some() && (data.time % 2.0) < dt {
            let water = cm.chunks.values().filter(|c| c.gpu_data.as_ref().map_or(false, |g| g.water.is_some())).count();
            log::warn!("SMOKE t={:.1} fish={} boats={} water_chunks={} pos={:?} underwater={}",
                data.time, data.fauna.fish.count(), data.fauna.boats.boats.len(), water, data.player.position, data.underwater);
        }

        let minutes = data.settings.auto_save_minutes;
        data.autosave_timer += dt;
        let due = minutes > 0 && data.autosave_timer >= minutes as f32 * 60.0;
        let player_pos = data.player.position;
        cm.update_view(player_pos, &data.palette, &data.ctx.device, &data.ctx.queue);

        if due {
            data.autosave_timer = 0.0;
            // Salva sempre (posição do jogador também é estado), o arquivo é pequeno.
            if let Err(e) = Self::save_world(data) {
                log::warn!("Auto-save falhou: {e}");
            }
        }
    }

    /// Grava o mundo atual em `saves/<nome>.dne`.
    fn save_world(data: &mut GameData) -> std::io::Result<()> {
        let (Some(name), Some(cm)) = (data.current_world.clone(), data.chunk_manager.as_mut()) else {
            return Ok(());
        };
        let mode = match data.player.mode {
            PlayerMode::Survival => 0,
            PlayerMode::Creative => 1,
            PlayerMode::Spectator => 2,
        };
        let ws = WorldSave {
            header: SaveHeader {
                version: save::VERSION,
                name: name.clone(),
                seed: data.current_seed,
                player_pos: data.player.position.to_array(),
                yaw: data.camera.yaw,
                pitch: data.camera.pitch,
                mode,
            },
            edits: cm.edits().clone(),
        };
        save::write_world(&save::world_path(std::path::Path::new(SAVES_DIR), &name), &ws)?;
        cm.unsaved = false;
        Ok(())
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
        let data = match &mut self.data {
            Some(d) => d,
            None => return,
        };

        data.pipeline.update_camera(&data.ctx.queue, &data.camera, data.time, data.underwater);

        let surface_texture = match data.ctx.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let size = data.window.inner_size();
                data.ctx.resize(size);
                return;
            }
            Err(wgpu::SurfaceError::Timeout) => return,
            Err(e) => {
                log::error!("Surface error: {:?}", e);
                event_loop.exit();
                return;
            }
        };

        let surface_view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let frustum = data.camera.frustum();

        // --- 3D Render Pass ---
        {
            let mut encoder = data.ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("3D Encoder"),
            });

            let clear = if data.underwater { (0.01, 0.05, 0.12) } else { (0.53, 0.81, 0.98) };
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("3D Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &surface_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: clear.0, g: clear.1, b: clear.2, a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &data.ctx.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });

                if self.state == GameState::InGame || self.state == GameState::Paused {
                    render_pass.set_pipeline(&data.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &data.pipeline.camera_bind_group, &[]);
                    if let Some(cm) = &data.chunk_manager {
                        cm.render_opaque(&mut render_pass, &frustum);
                    }
                    // Ordem: terreno opaco -> barcos -> peixes (alpha-test) -> água translúcida
                    data.boat_mesh.draw(&mut render_pass);
                    data.particle_mesh.draw(&mut render_pass);
                    data.fish_pipeline.draw(&mut render_pass, &data.pipeline.camera_bind_group);
                    if let Some(cm) = &data.chunk_manager {
                        render_pass.set_pipeline(&data.pipeline.water_pipeline);
                        render_pass.set_bind_group(0, &data.pipeline.camera_bind_group, &[]);
                        cm.render_water(&mut render_pass, data.camera.position, &frustum);
                    }
                }
            } // render_pass dropped

            data.ctx.queue.submit(std::iter::once(encoder.finish()));
        }

        // --- Egui Processing ---
        let state = self.state;
        let mut cmd: Option<UiCmd> = None;
        let mut settings_dirty = false;

        let raw_input = data.egui_state.take_egui_input(data.window.as_ref());
        let egui_ctx = data.egui_ctx.clone();
        let cur_mode = mode_to_ui(data.player.mode);

        let full_output = egui_ctx.run(raw_input, |ctx| {
            match state {
                GameState::MainMenu => match main_menu_screen(ctx) {
                    Some(MainMenuAction::Play) => cmd = Some(UiCmd::ToWorldSelect),
                    Some(MainMenuAction::Settings) => cmd = Some(UiCmd::ToSettings),
                    Some(MainMenuAction::Quit) => cmd = Some(UiCmd::Quit),
                    None => {}
                },
                GameState::Settings => {
                    if settings_screen(ctx, &mut data.settings) {
                        settings_dirty = true;
                        cmd = Some(UiCmd::ToMain);
                    }
                }
                GameState::WorldSelect => match world_select_screen(ctx, &mut data.ws_state, &data.worlds) {
                    Some(WorldSelectAction::Create { name, seed }) => cmd = Some(UiCmd::Create { name, seed }),
                    Some(WorldSelectAction::Load(i)) => cmd = Some(UiCmd::Load(i)),
                    Some(WorldSelectAction::Delete(i)) => cmd = Some(UiCmd::Delete(i)),
                    Some(WorldSelectAction::Back) => cmd = Some(UiCmd::ToMain),
                    None => {}
                },
                GameState::Paused => {
                    match pause_menu_screen(ctx, cur_mode, &mut data.settings, &mut data.pause_state) {
                        Some(PauseAction::Resume) => cmd = Some(UiCmd::Resume),
                        Some(PauseAction::SetMode(m)) => cmd = Some(UiCmd::SetMode(mode_from_ui(m))),
                        Some(PauseAction::SaveAndQuit) => cmd = Some(UiCmd::SaveAndQuit),
                        Some(PauseAction::QuitWithoutSaving) => cmd = Some(UiCmd::QuitToMenu),
                        None => {}
                    }
                }
                GameState::InGame => {
                    // mira simples
                    let c = ctx.screen_rect().center();
                    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("xhair")));
                    let st = egui::Stroke::new(1.5, egui::Color32::WHITE);
                    p.line_segment([c - egui::vec2(8.0, 0.0), c + egui::vec2(8.0, 0.0)], st);
                    p.line_segment([c - egui::vec2(0.0, 8.0), c + egui::vec2(0.0, 8.0)], st);
                }
            }
        });

        data.egui_state.handle_platform_output(data.window.as_ref(), full_output.platform_output);

        let prims = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen_desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [data.ctx.config.width, data.ctx.config.height],
            pixels_per_point: data.window.scale_factor() as f32,
        };

        for (id, delta) in &full_output.textures_delta.set {
            data.egui_renderer.update_texture(&data.ctx.device, &data.ctx.queue, *id, delta);
        }

        // --- Egui Render Pass ---
        // egui_wgpu Renderer::render<'rp>(&'rp self, pass: &mut RenderPass<'rp>) ties the pass
        // lifetime to the renderer. We use forget_lifetime() to detach the render pass from the
        // encoder's borrow so encoder.finish() can be called after dropping the pass.
        {
            let egui_renderer = &mut data.egui_renderer;
            let device = &data.ctx.device;
            let queue = &data.ctx.queue;

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Egui Encoder"),
            });

            egui_renderer.update_buffers(device, queue, &mut encoder, &prims, &screen_desc);

            {
                let egui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Egui Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &surface_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    ..Default::default()
                });
                // Detach the pass from encoder's borrow so the renderer's 'rp lifetime
                // doesn't require encoder to outlive the renderer.
                let mut static_pass = egui_pass.forget_lifetime();
                egui_renderer.render(&mut static_pass, &prims, &screen_desc);
                drop(static_pass); // must drop before encoder.finish()
            }

            queue.submit(std::iter::once(encoder.finish()));
        }

        for id in &full_output.textures_delta.free {
            data.egui_renderer.free_texture(id);
        }

        surface_texture.present();

        // Aplicar comandos da UI
        if settings_dirty {
            if let Some(d) = &mut self.data {
                d.settings.clamp();
                let _ = d.settings.save(std::path::Path::new(SETTINGS_PATH));
            }
        }
        let Some(cmd) = cmd else { return };
        let Some(data) = &mut self.data else { return };
        match cmd {
            UiCmd::Quit => event_loop.exit(),
            UiCmd::ToMain => self.state = GameState::MainMenu,
            UiCmd::ToSettings => self.state = GameState::Settings,
            UiCmd::ToWorldSelect => {
                data.worlds = list_worlds(std::path::Path::new(SAVES_DIR));
                self.state = GameState::WorldSelect;
            }
            UiCmd::Delete(i) => {
                if let Some(w) = data.worlds.get(i) {
                    let _ = if w.path.is_dir() { std::fs::remove_dir_all(&w.path) } else { std::fs::remove_file(&w.path) };
                }
                data.worlds = list_worlds(std::path::Path::new(SAVES_DIR));
            }
            UiCmd::Create { name, seed } => {
                let _ = std::fs::create_dir_all(PathBuf::from(SAVES_DIR));
                Self::start_world(data, name, seed, None);
                if let Err(e) = Self::save_world(data) {
                    log::warn!("Falha ao criar arquivo do mundo: {e}");
                }
                self.state = GameState::InGame;
                Self::set_cursor_captured(data.window.as_ref(), true);
            }
            UiCmd::Load(i) => {
                if let Some(w) = data.worlds.get(i).cloned() {
                    let loaded = match save::read_world(&w.path) {
                        Ok(ws) => Some(ws),
                        Err(e) => {
                            log::warn!("Falha ao ler {:?}: {e}", w.path);
                            None
                        }
                    };
                    let seed = loaded.as_ref().map_or(w.seed, |l| l.header.seed);
                    Self::start_world(data, w.name, seed, loaded);
                    self.state = GameState::InGame;
                    Self::set_cursor_captured(data.window.as_ref(), true);
                }
            }
            UiCmd::Resume => {
                let _ = data.settings.save(std::path::Path::new(SETTINGS_PATH));
                self.state = GameState::InGame;
                Self::set_cursor_captured(data.window.as_ref(), true);
            }
            UiCmd::SetMode(m) => data.player.set_mode(m),
            UiCmd::SaveAndQuit => {
                if let Err(e) = Self::save_world(data) {
                    log::error!("Falha ao salvar: {e}");
                }
                let _ = data.settings.save(std::path::Path::new(SETTINGS_PATH));
                data.chunk_manager = None;
                data.fauna = Fauna::new();
                data.underwater = false;
                data.boat_mesh.update(&data.ctx.device, &[], &[]);
                data.particles.clear();
                data.particle_mesh.update(&data.ctx.device, &[], &[]);
                data.fish_pipeline.update(&data.ctx.queue, &[]);
                data.current_world = None;
                self.state = GameState::MainMenu;
            }
            UiCmd::QuitToMenu => {
                let _ = data.settings.save(std::path::Path::new(SETTINGS_PATH));
                data.chunk_manager = None;
                data.fauna = Fauna::new();
                data.underwater = false;
                data.boat_mesh.update(&data.ctx.device, &[], &[]);
                data.particles.clear();
                data.particle_mesh.update(&data.ctx.device, &[], &[]);
                data.fish_pipeline.update(&data.ctx.queue, &[]);
                data.current_world = None;
                self.state = GameState::MainMenu;
            }
        }
    }

    /// Cria o ChunkManager do mundo, aplica o save (se houver) e posiciona o jogador.
    fn start_world(data: &mut GameData, name: String, seed: u32, saved: Option<WorldSave>) {
        let mut cm = ChunkManager::new(data.settings.render_distance, seed);
        cm.set_threads(data.settings.gen_threads);
        data.applied_threads = data.settings.gen_threads;

        let mode = data.player.mode;
        let (spawn_hint, saved_state) = match saved {
            Some(ws) => {
                cm.set_edits(ws.edits);
                let h = ws.header;
                (Vec3::from_array(h.player_pos), Some((h.yaw, h.pitch, h.mode)))
            }
            None => (Vec3::new(0.5, 80.0, 0.5), None),
        };

        // Gera ao redor do ponto salvo antes de posicionar o jogador (chunk ausente = sólido).
        cm.max_gen_per_update = usize::MAX;
        cm.max_mesh_per_update = usize::MAX;
        cm.lod_level = data.settings.lod_level;
        cm.update_view(spawn_hint, &data.palette, &data.ctx.device, &data.ctx.queue);
        cm.max_gen_per_update = 16;
        cm.max_mesh_per_update = 8;

        let pos = if saved_state.is_some() {
            spawn_hint
        } else {
            let ground = cm.surface_height(0, 0).unwrap_or(80);
            Vec3::new(0.5, (ground.max(crate::world::gen::types::SEA_LEVEL) + 2) as f32, 0.5)
        };
        data.player = Player::new(pos);
        data.player.set_mode(mode);
        if let Some((yaw, pitch, m)) = saved_state {
            data.camera.yaw = yaw;
            data.camera.pitch = pitch;
            data.player.set_mode(match m {
                0 => PlayerMode::Survival,
                2 => PlayerMode::Spectator,
                _ => PlayerMode::Creative,
            });
        }
        data.camera.position = data.player.eye_position();
        data.chunk_manager = Some(cm);
        data.current_world = Some(name);
        data.fauna = Fauna::new();
        data.time = 0.0;
        data.underwater = false;
        data.current_seed = seed;
        data.autosave_timer = 0.0;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.data.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_title("Dungeon n Evolve")
            .with_inner_size(PhysicalSize::new(1280u32, 720u32));

        let window = Arc::new(
            event_loop.create_window(window_attrs).expect("Failed to create window"),
        );

        let ctx = pollster::block_on(RenderContext::new(window.clone()))
            .expect("Failed to create render context");

        let pipeline = VoxelPipeline::new(&ctx);
        let fish_pipeline = FishPipeline::new(&ctx, &pipeline);

        let aspect = ctx.config.width as f32 / ctx.config.height as f32;
        let camera = Camera::new(Vec3::new(0.0, 80.0, 0.0), aspect);

        let egui_ctx = egui::Context::default();
        let max_tex = ctx.device.limits().max_texture_dimension_2d;

        // egui-winit 0.29: (ctx, viewport_id, window, native_ppp, theme, max_texture_side)
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,                    // theme: Option<egui::Theme>
            Some(max_tex as usize),  // max_texture_side
        );
        // egui-wgpu 0.29: (device, format, depth_format, msaa, dithering)
        let egui_renderer = egui_wgpu::Renderer::new(
            &ctx.device,
            ctx.surface_format,
            None,
            1,
            false,
        );

        let smoke = std::env::var_os("DNE_SMOKE").is_some();
        let palette = ColorPalette;
        let mut settings = GraphicsSettings::load_or_default(std::path::Path::new(SETTINGS_PATH));
        settings.clamp();
        self.data = Some(GameData {
            window,
            ctx,
            pipeline,
            fish_pipeline,
            boat_mesh: DynMesh::default(),
            particles: ParticleSystem::new(),
            particle_mesh: DynMesh::default(),
            fauna: Fauna::new(),
            time: 0.0,
            underwater: false,
            camera,
            input: InputState::new(),
            chunk_manager: None,
            palette,
            player: Player::new(Vec3::new(0.5, 80.0, 0.5)),
            applied_threads: 0,
            settings,
            worlds: Vec::new(),
            ws_state: WorldSelectState::default(),
            pause_state: PauseState::default(),
            break_requested: false,
            current_world: None,
            current_seed: 0,
            autosave_timer: 0.0,
            egui_ctx,
            egui_state,
            egui_renderer,
            last_frame: Instant::now(),
        });
        // Diagnóstico sem UI: DNE_SMOKE=<seed> entra direto num mundo temporário (não salva).
        if smoke {
            let seed = std::env::var("DNE_SMOKE").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
            if let Some(d) = &mut self.data {
                Self::start_world(d, "__smoke".into(), seed, None);
                d.current_world = None; // impede qualquer save
                let sea = crate::world::gen::types::SEA_LEVEL as f32;
                d.player = Player::new(Vec3::new(d.player.position.x, sea + 3.0, d.player.position.z));
                d.player.set_mode(PlayerMode::Spectator);
            }
            self.state = GameState::InGame;
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let consumed = if let Some(data) = &mut self.data {
            let response = data.egui_state.on_window_event(data.window.as_ref(), &event);
            response.consumed
        } else {
            false
        };

        match &event {
            WindowEvent::CloseRequested => {
                // Fechar a janela com um mundo aberto salva antes de sair.
                if let Some(data) = &mut self.data {
                    if let Err(e) = Self::save_world(data) {
                        log::error!("Falha ao salvar ao fechar: {e}");
                    }
                }
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                if let Some(data) = &mut self.data {
                    data.ctx.resize(*size);
                    let aspect = size.width as f32 / size.height as f32;
                    data.camera.set_aspect(aspect);
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if !consumed {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        let pressed = event.state == ElementState::Pressed;
                        if let Some(data) = &mut self.data {
                            data.input.update_key(code, pressed);
                        }

                        if code == KeyCode::Escape && pressed {
                            match self.state {
                                GameState::InGame => {
                                    self.state = GameState::Paused;
                                    if let Some(d) = &self.data {
                                        Self::set_cursor_captured(d.window.as_ref(), false);
                                    }
                                }
                                GameState::Paused => {
                                    self.state = GameState::InGame;
                                    if let Some(d) = &self.data {
                                        Self::set_cursor_captured(d.window.as_ref(), true);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            WindowEvent::MouseInput { state: ElementState::Pressed, button: winit::event::MouseButton::Left, .. } => {
                if self.state == GameState::InGame && !consumed {
                    if let Some(data) = &mut self.data {
                        data.break_requested = true;
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                let dt = {
                    if let Some(data) = &mut self.data {
                        let now = Instant::now();
                        let dt = now.duration_since(data.last_frame).as_secs_f32().min(0.1);
                        data.last_frame = now;
                        dt
                    } else {
                        return;
                    }
                };

                self.update(dt);
                self.render(event_loop);

                if let Some(data) = &self.data {
                    data.window.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.state == GameState::InGame {
                if let Some(data) = &mut self.data {
                    data.input.add_mouse_delta(dx, dy);
                }
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(data) = &self.data {
            data.window.request_redraw();
        }
    }
}
