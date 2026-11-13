//! Classificação de bioma (mesma regra do shader GPU) + preenchimento de colunas.
use crate::world::block::BlockType;
use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE};
use super::types::{Biome, ColumnData, SEA_LEVEL};

/// Limite superior (inclusive) da faixa de praia.
pub const BEACH_MAX_Y: i32 = 66;
/// Altura mínima para montanha.
pub const MOUNTAIN_MIN_Y: i32 = 92;
/// Altura a partir da qual há neve nos picos.
pub const SNOW_LINE: i32 = 112;

/// Classifica o bioma. `temp` e `humidity` em [-1, 1]; `height` = Y da superfície (mundo).
/// Regra (idêntica no shader):
/// - height < SEA_LEVEL -> Ocean
/// - height <= 66 -> Beach
/// - height >= 92 && temp < 0.4 && humidity < 0.4 -> Mountains
/// - temp > 0.3 && humidity < -0.1 -> Desert
/// - temp < 0.3 && humidity > 0.1 -> Forest
/// - senão Plains
pub fn classify(temp: f32, humidity: f32, height: f32) -> Biome {
    if height < SEA_LEVEL as f32 {
        Biome::Ocean
    } else if height <= BEACH_MAX_Y as f32 {
        Biome::Beach
    } else if height >= MOUNTAIN_MIN_Y as f32 && temp < 0.4 && humidity < 0.4 {
        Biome::Mountains
    } else if temp > 0.3 && humidity < -0.1 {
        Biome::Desert
    } else if temp < 0.3 && humidity > 0.1 {
        Biome::Forest
    } else {
        Biome::Plains
    }
}

#[inline]
fn hash2(wx: i32, wz: i32, seed: u32) -> u32 {
    let mut h = (wx as u32).wrapping_mul(0x85EB_CA6B)
        ^ (wz as u32).wrapping_mul(0xC2B2_AE35)
        ^ seed.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

/// (bloco de topo, bloco de preenchimento, espessura do preenchimento) para uma coluna.
fn layers(biome: Biome, h: i32, r: u32) -> (BlockType, BlockType, i32) {
    match biome {
        Biome::Plains => (BlockType::Grass, BlockType::Dirt, 3),
        Biome::Forest => {
            let top = if r % 100 < 30 { BlockType::Moss } else { BlockType::Grass };
            (top, BlockType::Dirt, 3)
        }
        Biome::Mountains => {
            if h >= SNOW_LINE {
                (BlockType::Snow, BlockType::Stone, 3)
            } else {
                (BlockType::Stone, BlockType::Stone, 3)
            }
        }
        Biome::Beach => (BlockType::Sand, BlockType::Sand, 3),
        Biome::Desert => (BlockType::Sand, BlockType::Sand, 5),
        Biome::Ocean => {
            if h >= SEA_LEVEL - 8 && r % 100 >= 25 {
                (BlockType::Sand, BlockType::Sand, 3)
            } else {
                (BlockType::Gravel, BlockType::Gravel, 2)
            }
        }
    }
}

/// Preenche os blocos do chunk coluna a coluna a partir de `cols`. Determinístico.
pub fn populate_chunk(chunk: &mut Chunk, cols: &ColumnData, seed: u32) {
    let ox = chunk.position.x * CHUNK_SIZE as i32;
    let oz = chunk.position.z * CHUNK_SIZE as i32;
    for lz in 0..CHUNK_SIZE {
        for lx in 0..CHUNK_SIZE {
            let i = ColumnData::idx(lx, lz);
            let wx = ox + lx as i32;
            let wz = oz + lz as i32;
            let h = (cols.heights[i] as i32).clamp(1, CHUNK_HEIGHT as i32 - 1);
            let r = hash2(wx, wz, seed);
            let mut biome = Biome::from_u8(cols.biomes[i]);
            let w = cols.blend[i] as u32;
            // blend: até 50% de chance de usar a camada do 2º bioma na fronteira
            if w > 0 && ((r >> 8) % 255) < w / 2 {
                biome = Biome::from_u8(cols.biomes2[i]);
            }
            let (top, fill, thick) = layers(biome, h, r);

            chunk.set(lx, 0, lz, BlockType::Bedrock);
            for y in 1..CHUNK_HEIGHT as i32 {
                let b = if y > h {
                    if y <= SEA_LEVEL { BlockType::Water } else { BlockType::Air }
                } else if y == h {
                    top
                } else if y >= h - thick {
                    fill
                } else {
                    BlockType::Stone
                };
                if b != BlockType::Air {
                    chunk.set(lx, y as usize, lz, b);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::ChunkPos;

    fn synth(biome: Biome, h: u16) -> ColumnData {
        let mut c = ColumnData::new();
        c.heights.fill(h);
        c.biomes.fill(biome as u8);
        c.biomes2.fill(biome as u8);
        c
    }

    #[test]
    fn classify_all_six() {
        assert_eq!(classify(0.0, 0.0, 50.0), Biome::Ocean);
        assert_eq!(classify(0.0, 0.0, 64.0), Biome::Beach);
        assert_eq!(classify(0.0, 0.0, 100.0), Biome::Mountains);
        assert_eq!(classify(0.8, -0.5, 75.0), Biome::Desert);
        assert_eq!(classify(0.0, 0.6, 75.0), Biome::Forest);
        assert_eq!(classify(0.0, 0.0, 75.0), Biome::Plains);
        // quente/úmido e alto não vira montanha
        assert_ne!(classify(0.8, 0.8, 100.0), Biome::Mountains);
    }

    #[test]
    fn surface_blocks() {
        let cases = [
            (Biome::Plains, 80u16, vec![BlockType::Grass]),
            (Biome::Forest, 80, vec![BlockType::Grass, BlockType::Moss]),
            (Biome::Mountains, 100, vec![BlockType::Stone]),
            (Biome::Mountains, 130, vec![BlockType::Snow]),
            (Biome::Beach, 64, vec![BlockType::Sand]),
            (Biome::Desert, 80, vec![BlockType::Sand]),
            (Biome::Ocean, 55, vec![BlockType::Sand, BlockType::Gravel]),
            (Biome::Ocean, 30, vec![BlockType::Gravel]),
        ];
        for (b, h, ok) in cases {
            let mut ch = Chunk::new(ChunkPos::new(3, -2));
            populate_chunk(&mut ch, &synth(b, h), 7);
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    assert_eq!(ch.get(x, 0, z), BlockType::Bedrock);
                    let s = ch.get(x, h as usize, z);
                    assert!(ok.contains(&s), "{:?} h={} got {:?}", b, h, s);
                    assert_eq!(ch.get(x, h as usize + 1 + 40, z), BlockType::Air);
                    assert_eq!(ch.get(x, 5, z), if h > 10 { BlockType::Stone } else { ch.get(x, 5, z) });
                }
            }
        }
    }

    #[test]
    fn ocean_has_water_to_sea_level() {
        let mut ch = Chunk::new(ChunkPos::new(0, 0));
        populate_chunk(&mut ch, &synth(Biome::Ocean, 50), 1);
        assert_eq!(ch.get(4, SEA_LEVEL as usize, 4), BlockType::Water);
        assert_eq!(ch.get(4, 51, 4), BlockType::Water);
        assert_eq!(ch.get(4, SEA_LEVEL as usize + 1, 4), BlockType::Air);
        // terra acima do nível do mar não tem água
        let mut ch = Chunk::new(ChunkPos::new(0, 0));
        populate_chunk(&mut ch, &synth(Biome::Plains, 80), 1);
        assert!(!ch.blocks.contains(&BlockType::Water));
    }

    #[test]
    fn blend_mixes_surface_deterministically() {
        let mut c = synth(Biome::Plains, 80);
        c.biomes2.fill(Biome::Desert as u8);
        c.blend.fill(255);
        let mut a = Chunk::new(ChunkPos::new(1, 1));
        let mut b = Chunk::new(ChunkPos::new(1, 1));
        populate_chunk(&mut a, &c, 9);
        populate_chunk(&mut b, &c, 9);
        assert_eq!(a.blocks, b.blocks);
        let sand = (0..32).flat_map(|x| (0..32).map(move |z| (x, z)))
            .filter(|&(x, z)| a.get(x, 80, z) == BlockType::Sand).count();
        assert!(sand > 100 && sand < 1024, "sand={}", sand);
    }
}
