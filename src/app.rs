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
use crate::world::{ChunkManager, ColorPalette};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerMode {
    Survival,
    Creative,
    Spectator,
}

impl PlayerMode {
    pub fn speed(&self) -> f32 {
        match self {
            PlayerMode::Survival => 5.0,
            PlayerMode::Creative => 15.0,
            PlayerMode::Spectator => 30.0,
        }
    }

    pub fn horizontal_only(&self) -> bool {
        matches!(self, PlayerMode::Survival)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    MainMenu,
    InGame,
    Paused,
}

struct GameData {
    window: Arc<Window>,
    ctx: RenderContext,
    pipeline: VoxelPipeline,
    camera: Camera,
    input: InputState,
    chunk_manager: ChunkManager,
    palette: ColorPalette,
    player_mode: PlayerMode,
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

        let speed = data.player_mode.speed();
        let horizontal_only = data.player_mode.horizontal_only();
        data.camera.update_movement(&data.input, dt, speed, horizontal_only);

        let player_pos = data.camera.position;
        data.chunk_manager.update_view(player_pos, &data.palette, &data.ctx.device, &data.ctx.queue);
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
                    data.chunk_manager.render(&mut render_pass);
                }
            } // render_pass dropped

            data.ctx.queue.submit(std::iter::once(encoder.finish()));
        }

        // --- Egui Processing ---
        let is_main_menu = self.state == GameState::MainMenu;
        let is_paused = self.state == GameState::Paused;
        let is_in_game = self.state == GameState::InGame;

        let mut act_start_game = false;
        let mut act_resume = false;
        let mut act_exit = false;
        let mut act_mode_survival = false;
        let mut act_mode_creative = false;
        let mut act_mode_spectator = false;

        let raw_input = data.egui_state.take_egui_input(data.window.as_ref());
        let egui_ctx = data.egui_ctx.clone();

        let full_output = egui_ctx.run(raw_input, |ctx| {
            Self::build_ui(
                ctx,
                is_main_menu,
                is_paused,
                is_in_game,
                &mut act_start_game,
                &mut act_resume,
                &mut act_exit,
                &mut act_mode_survival,
                &mut act_mode_creative,
                &mut act_mode_spectator,
            );
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

        // Apply UI actions after GPU submit
        if act_start_game {
            self.state = GameState::InGame;
            if let Some(d) = &self.data {
                Self::set_cursor_captured(d.window.as_ref(), true);
            }
        }
        if act_resume {
            self.state = GameState::InGame;
            if let Some(d) = &self.data {
                Self::set_cursor_captured(d.window.as_ref(), true);
            }
        }
        if act_exit {
            event_loop.exit();
        }
        if let Some(data) = &mut self.data {
            if act_mode_survival  { data.player_mode = PlayerMode::Survival; }
            if act_mode_creative  { data.player_mode = PlayerMode::Creative; }
            if act_mode_spectator { data.player_mode = PlayerMode::Spectator; }
        }
    }

    fn build_ui(
        ctx: &egui::Context,
        is_main_menu: bool,
        is_paused: bool,
        _is_in_game: bool,
        act_start_game: &mut bool,
        act_resume: &mut bool,
        act_exit: &mut bool,
        act_mode_survival: &mut bool,
        act_mode_creative: &mut bool,
        act_mode_spectator: &mut bool,
    ) {
        if is_main_menu {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::from_rgba_premultiplied(0, 0, 0, 200)))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(120.0);
                        ui.heading(
                            egui::RichText::new("Dungeon n Evolve")
                                .size(48.0)
                                .color(egui::Color32::WHITE),
                        );
                        ui.add_space(40.0);

                        let btn = egui::Vec2::new(200.0, 45.0);
                        if ui.add_sized(btn, egui::Button::new("▶  Play")).clicked() {
                            *act_start_game = true;
                        }
                        ui.add_space(10.0);
                        if ui.add_sized(btn, egui::Button::new("✕  Exit")).clicked() {
                            *act_exit = true;
                        }
                    });
                });
        }

        if is_paused {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::from_rgba_premultiplied(0, 0, 0, 160)))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(100.0);
                        ui.heading(
                            egui::RichText::new("Paused")
                                .size(36.0)
                                .color(egui::Color32::WHITE),
                        );
                        ui.add_space(30.0);

                        let btn = egui::Vec2::new(200.0, 40.0);
                        if ui.add_sized(btn, egui::Button::new("Resume")).clicked() {
                            *act_resume = true;
                        }
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Mode").color(egui::Color32::LIGHT_GRAY));
                        ui.add_space(4.0);
                        if ui.add_sized(btn, egui::Button::new("Survival")).clicked() {
                            *act_mode_survival = true;
                        }
                        ui.add_space(4.0);
                        if ui.add_sized(btn, egui::Button::new("Creative")).clicked() {
                            *act_mode_creative = true;
                        }
                        ui.add_space(4.0);
                        if ui.add_sized(btn, egui::Button::new("Spectator")).clicked() {
                            *act_mode_spectator = true;
                        }
                        ui.add_space(16.0);
                        if ui.add_sized(btn, egui::Button::new("Quit to Menu")).clicked() {
                            *act_exit = true;
                        }
                    });
                });
        }
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

        let mut chunk_manager = ChunkManager::new(5, 42);
        let palette = ColorPalette;
        chunk_manager.max_gen_per_update = usize::MAX;
        chunk_manager.update_view(camera.position, &palette, &ctx.device, &ctx.queue);
        chunk_manager.max_gen_per_update = 16;

        self.data = Some(GameData {
            window,
            ctx,
            pipeline,
            camera,
            input: InputState::new(),
            chunk_manager,
            palette,
            player_mode: PlayerMode::Creative,
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
