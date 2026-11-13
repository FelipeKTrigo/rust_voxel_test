//! Fase 8 — fauna do oceano: peixes (cardume) e barcos de pescadores.
pub mod boat;
pub mod fish;

use glam::Vec3;

use crate::world::chunk::ChunkPos;
use crate::world::ChunkManager;

/// Onda da superfície da água. DEVE espelhar `wave_height` em assets/shaders/common.wgsl.
pub fn wave_height(x: f32, z: f32, t: f32) -> f32 {
    0.05 * (x * 0.9 + t * 1.6).sin()
        + 0.04 * (z * 1.1 + t * 1.3).sin()
        + 0.03 * ((x + z) * 0.5 + t * 0.9).sin()
}

/// Hash determinístico (seed, chunk, sal) -> u32.
pub fn hash3(seed: u32, pos: ChunkPos, salt: u32) -> u32 {
    let mut h = seed ^ 0x9e37_79b9;
    for v in [pos.x as u32, pos.z as u32, salt] {
        h ^= v.wrapping_add(0x7f4a_7c15).wrapping_add(h << 6).wrapping_add(h >> 2);
        h = h.wrapping_mul(0x85eb_ca6b);
        h ^= h >> 13;
    }
    h ^ (h >> 16)
}

/// Float em [0,1) a partir de um hash.
pub fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

pub struct Fauna {
    pub fish: fish::FishSystem,
    pub boats: boat::BoatSystem,
}

impl Fauna {
    pub fn new() -> Self {
        Self { fish: fish::FishSystem::new(), boats: boat::BoatSystem::new() }
    }

    pub fn update(&mut self, dt: f32, time: f32, player: Vec3, cm: &ChunkManager, reduced: bool) {
        self.fish.update(dt, time, player, cm, reduced);
        self.boats.update(player, cm);
    }
}
