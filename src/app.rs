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
use crate::renderer::{RenderContext, VoxelPipeline};
use crate::world::{BlockType, ChunkManager, ColorPalette};
use crate::physics::BlockQuery;
use crate::player::{Player, PlayerMode};
use crate::ui::{self, PlayerModeUi};
use crate::ui::main_menu::{main_menu_screen, settings_screen, MainMenuAction};
use crate::ui::pause_menu::{pause_menu_screen, PauseAction, PauseState};
use crate::ui::settings::GraphicsSettings;
use crate::ui::world_select::{list_worlds, world_select_screen, WorldEntry, WorldSelectAction, WorldSelectState};
use std::path::PathBuf;

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
        cm.max_gen_per_update = 4 + (data.settings.cpu_gpu_balance.clamp(0.0, 1.0) * 28.0) as usize;

        data.player.update(dt, &data.input, data.camera.yaw, &*cm);
        data.camera.position = data.player.eye_position();

        // Quebrar bloco (clique esquerdo)
        if std::mem::take(&mut data.break_requested) && data.player.mode.can_break_blocks() {
            if let Some(hit) = data.player.target_block(data.camera.forward(), &*cm) {
                if cm.block_at(hit.block.x, hit.block.y, hit.block.z) != BlockType::Bedrock {
                    cm.set_block_world(hit.block.x, hit.block.y, hit.block.z, BlockType::Air);
                }
            }
        }

        let player_pos = data.player.position;
        cm.update_view(player_pos, &data.palette, &data.ctx.device, &data.ctx.queue);
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
        let data = match &mut self.data {
            Some(d) => d,
            None => return,
        };

        data.pipeline.update_camera(&data.ctx.queue, &data.camera);

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

        // --- 3D Render Pass ---
        {
            let mut encoder = data.ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("3D Encoder"),
            });

            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("3D Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &surface_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.53, g: 0.81, b: 0.98, a: 1.0,
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
                        cm.render(&mut render_pass);
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
                        // Salvar em arquivo é a Fase 7; por ora ambos voltam ao menu.
                        Some(PauseAction::SaveAndQuit) | Some(PauseAction::QuitWithoutSaving) => cmd = Some(UiCmd::QuitToMenu),
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
                Self::start_world(data, name, seed);
                self.state = GameState::InGame;
                Self::set_cursor_captured(data.window.as_ref(), true);
            }
            UiCmd::Load(i) => {
                if let Some(w) = data.worlds.get(i).cloned() {
                    Self::start_world(data, w.name, w.seed);
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
            UiCmd::QuitToMenu => {
                let _ = data.settings.save(std::path::Path::new(SETTINGS_PATH));
                data.chunk_manager = None;
                data.current_world = None;
                self.state = GameState::MainMenu;
            }
        }
    }

    /// Cria um novo ChunkManager para o mundo e posiciona o jogador na superfície.
    fn start_world(data: &mut GameData, name: String, seed: u32) {
        let mut cm = ChunkManager::new(data.settings.render_distance, seed);
        cm.set_threads(data.settings.gen_threads);
        data.applied_threads = data.settings.gen_threads;
        cm.max_gen_per_update = usize::MAX;
        cm.update_view(Vec3::new(0.5, 80.0, 0.5), &data.palette, &data.ctx.device, &data.ctx.queue);
        cm.max_gen_per_update = 16;
        let ground = cm.surface_height(0, 0).unwrap_or(80);
        let mode = data.player.mode;
        data.player = Player::new(Vec3::new(0.5, (ground.max(crate::world::gen::types::SEA_LEVEL) + 2) as f32, 0.5));
        data.player.set_mode(mode);
        data.camera.position = data.player.eye_position();
        data.chunk_manager = Some(cm);
        data.current_world = Some(name);
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

        let palette = ColorPalette;
        let mut settings = GraphicsSettings::load_or_default(std::path::Path::new(SETTINGS_PATH));
        settings.clamp();
        self.data = Some(GameData {
            window,
            ctx,
            pipeline,
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
            egui_ctx,
            egui_state,
            egui_renderer,
            last_frame: Instant::now(),
        });
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
