pub mod types;
pub mod gpu_gen;
pub mod biome;
pub mod features;
pub mod tree;

use noise::{NoiseFn, Perlin};
use crate::world::block::BlockType;
use crate::world::chunk::{Chunk, ChunkPos, CHUNK_SIZE, CHUNK_HEIGHT};

pub struct WorldGen {
    perlin: Perlin,
    temp_noise: Perlin,
}

impl WorldGen {
    pub fn new(seed: u32) -> Self {
        Self {
            perlin: Perlin::new(seed),
            temp_noise: Perlin::new(seed.wrapping_add(1337)),
        }
    }

    fn fbm_height(&self, wx: f64, wz: f64) -> f64 {
        let h1 = self.perlin.get([wx * 0.003, wz * 0.003]) * 60.0;
        let h2 = self.perlin.get([wx * 0.012, wz * 0.012]) * 15.0;
        let h3 = self.perlin.get([wx * 0.05, wz * 0.05]) * 4.0;
        h1 + h2 + h3
    }

    fn temperature(&self, wx: f64, wz: f64) -> f64 {
        self.temp_noise.get([wx * 0.002, wz * 0.002])
    }

    pub fn generate_chunk(&self, pos: ChunkPos) -> Chunk {
        let mut chunk = Chunk::new(pos);
        let (ox, oz) = pos.world_origin();

        for lx in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                let wx = ox as f64 + lx as f64;
                let wz = oz as f64 + lz as f64;

                let height_offset = self.fbm_height(wx, wz);
                let surface_y = (64.0 + height_offset).clamp(1.0, (CHUNK_HEIGHT - 1) as f64) as usize;

                let temp = self.temperature(wx, wz);
                let is_desert = temp > 0.3;
                let is_snowy = surface_y > 100;

                // Bedrock at y=0
                chunk.set(lx, 0, lz, BlockType::Bedrock);

                for y in 1..CHUNK_HEIGHT {
                    let block = if y > surface_y {
                        // Above surface: water fill below sea level
                        if y < 62 {
                            BlockType::Water
                        } else {
                            BlockType::Air
                        }
                    } else if y == surface_y {
                        // Surface block
                        if is_desert {
                            BlockType::Sand
                        } else if is_snowy {
                            BlockType::Snow
                        } else {
                            BlockType::Grass
                        }
                    } else if y >= surface_y.saturating_sub(3) {
                        // Sub-surface: 3 layers
                        if is_desert {
                            BlockType::Sand
                        } else if is_snowy {
                            BlockType::Stone
                        } else {
                            BlockType::Dirt
                        }
                    } else if y < 20 {
                        // Deep underground: bedrock-ish zone with coal/iron/gold
                        let ore_noise = self.perlin.get([wx * 0.1, y as f64 * 0.1, wz * 0.1]);
                        if ore_noise > 0.75 && y < 5 {
                            BlockType::Bedrock
                        } else if ore_noise > 0.65 {
                            BlockType::Gold
                        } else if ore_noise > 0.55 {
                            BlockType::Iron
                        } else if ore_noise > 0.45 {
                            BlockType::Coal
                        } else {
                            BlockType::Stone
                        }
                    } else {
                        // Stone layer with veins
                        let ore_noise = self.perlin.get([wx * 0.08, y as f64 * 0.08, wz * 0.08]);
                        if ore_noise > 0.65 {
                            BlockType::Iron
                        } else if ore_noise > 0.55 {
                            BlockType::Coal
                        } else {
                            BlockType::Stone
                        }
                    };

                    chunk.set(lx, y, lz, block);
                }
            }
        }

        chunk.is_dirty = true;
        chunk
    }
}
