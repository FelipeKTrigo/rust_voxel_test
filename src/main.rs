mod app;
mod camera;
mod culling;
mod debug;
mod drops;
mod fauna;
mod input;
mod inventory;
mod particles;
mod physics;
mod player;
mod ui;
mod renderer;
mod sky;
mod world;

use winit::event_loop::{ControlFlow, EventLoop};
use app::App;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut App::new())?;
    Ok(())
}