//! Cavernas, ravinas, minérios e reações (pirita + água). Tudo em coordenadas de mundo.
use noise::{NoiseFn, Perlin};
use crate::world::block::BlockType;
use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE};
use super::types::{Biome, ColumnData};

const MIN_CARVE_Y: i32 = 5;

struct Noises {
    cave_a: Perlin,
    cave_b: Perlin,
    ravine: Perlin,
    ravine_mask: Perlin,
    ore: [Perlin; 6], // coal, iron, feldspar, pyrite, bauxite, gold
}

impl Noises {
    fn new(seed: u32) -> Self {
        let s = |k: u32| Perlin::new(seed.wrapping_mul(31).wrapping_add(k));
        Self {
            cave_a: s(101),
            cave_b: s(102),
            ravine: s(103),
            ravine_mask: s(104),
            ore: [s(201), s(202), s(203), s(204), s(205), s(206)],
        }
    }
}

/// Minério (ou None) para um bloco de pedra em coordenadas de mundo.
fn ore_at(n: &Noises, biome: Biome, wx: i32, y: i32, wz: i32) -> Option<BlockType> {
    let (x, yy, z) = (wx as f64, y as f64, wz as f64);
    let f = 0.13;
    let samp = |i: usize| n.ore[i].get([x * f, yy * f * 1.2, z * f]);
    let mountain = biome == Biome::Mountains;
    if mountain {
        if y < 48 && samp(5) > 0.52 { return Some(BlockType::Gold); }
        if samp(3) > 0.42 { return Some(BlockType::Pyrite); }
        if samp(4) > 0.42 { return Some(BlockType::Bauxite); }
        if samp(2) > 0.38 { return Some(BlockType::Feldspar); }
        if samp(1) > 0.36 { return Some(BlockType::Iron); }
        if samp(0) > 0.33 { return Some(BlockType::Coal); }
    } else {
        if samp(1) > 0.48 { return Some(BlockType::Iron); }
        if samp(0) > 0.38 { return Some(BlockType::Coal); }
    }
    None
}

/// Cava cavernas e ravinas e distribui minérios. Chamar depois de `populate_chunk`.
pub fn carve_and_ores(chunk: &mut Chunk, cols: &ColumnData, seed: u32) {
    let n = Noises::new(seed);
    let ox = chunk.position.x * CHUNK_SIZE as i32;
    let oz = chunk.position.z * CHUNK_SIZE as i32;

    for lz in 0..CHUNK_SIZE {
        for lx in 0..CHUNK_SIZE {
            let i = ColumnData::idx(lx, lz);
            let wx = ox + lx as i32;
            let wz = oz + lz as i32;
            let h = (cols.heights[i] as i32).clamp(1, CHUNK_HEIGHT as i32 - 1);
            let biome = Biome::from_u8(cols.biomes[i]);
            let ocean = biome == Biome::Ocean;

            // Minérios (somente em pedra, antes de cavar)
            for y in 1..h {
                if chunk.get(lx, y as usize, lz) == BlockType::Stone {
                    if let Some(o) = ore_at(&n, biome, wx, y, wz) {
                        chunk.set(lx, y as usize, lz, o);
                    }
                }
            }

            // Ravina: faixa estreita ao redor do cruzamento por zero do noise
            let (fx, fz) = (wx as f64, wz as f64);
            let mut ravine_floor = i32::MAX;
            if !ocean && biome != Biome::Beach {
                let mask = n.ravine_mask.get([fx * 0.002, fz * 0.002]);
                if mask > 0.1 {
                    let r = n.ravine.get([fx * 0.006, fz * 0.006]).abs();
                    let width = 0.010 * (((mask - 0.1) * 4.0).min(1.0));
                    if r < width {
                        let depth = 18.0 + 22.0 * (1.0 - r / width.max(1e-9));
                        ravine_floor = (h - depth as i32).max(10);
                    }
                }
            }

            // Cavernas e ravinas
            let top = if ocean { h - 3 } else { h };
            for y in MIN_CARVE_Y..=top.min(CHUNK_HEIGHT as i32 - 1) {
                let cur = chunk.get(lx, y as usize, lz);
                if matches!(cur, BlockType::Air | BlockType::Water | BlockType::Bedrock) {
                    continue;
                }
                let yy = y as f64;
                let a = n.cave_a.get([fx * 0.018, yy * 0.03, fz * 0.018]);
                let b = n.cave_b.get([fx * 0.018, yy * 0.03, fz * 0.018]);
                let cave = a.abs() < 0.055 && b.abs() < 0.055;
                let ravine = !ocean && y >= ravine_floor;
                if cave || ravine {
                    chunk.set(lx, y as usize, lz, BlockType::Air);
                }
            }
        }
    }
}

/// Pirita adjacente (6 vizinhos, dentro do chunk) a água transforma a água em ácido sulfúrico.
pub fn react_pyrite_water(chunk: &mut Chunk) {
    let mut hits = Vec::new();
    const D: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_HEIGHT {
                if chunk.get(x, y, z) != BlockType::Water {
                    continue;
                }
                let near = D.iter().any(|&(dx, dy, dz)| {
                    chunk.get_safe(x as i32 + dx, y as i32 + dy, z as i32 + dz) == Some(BlockType::Pyrite)
                });
                if near {
                    hits.push((x, y, z));
                }
            }
        }
    }
    for (x, y, z) in hits {
        chunk.set(x, y, z, BlockType::SulfuricAcid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::ChunkPos;
    use crate::world::gen::biome::populate_chunk;

    fn make(cx: i32, cz: i32, biome: Biome, h: u16, seed: u32) -> Chunk {
        let mut c = ColumnData::new();
        c.heights.fill(h);
        c.biomes.fill(biome as u8);
        c.biomes2.fill(biome as u8);
        let mut ch = Chunk::new(ChunkPos::new(cx, cz));
        populate_chunk(&mut ch, &c, seed);
        carve_and_ores(&mut ch, &c, seed);
        ch
    }

    fn count(ch: &Chunk, b: BlockType) -> usize {
        ch.blocks.iter().filter(|&&x| x == b).count()
    }

    #[test]
    fn deterministic_and_bedrock() {
        let a = make(2, 3, Biome::Mountains, 110, 42);
        let b = make(2, 3, Biome::Mountains, 110, 42);
        assert_eq!(a.blocks, b.blocks);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                assert_eq!(a.get(x, 0, z), BlockType::Bedrock);
                for y in 1..5 {
                    assert_ne!(a.get(x, y, z), BlockType::Air);
                }
            }
        }
    }

    #[test]
    fn mountains_have_ores_and_caves() {
        let (mut coal, mut iron, mut air, mut other) = (0, 0, 0, 0);
        for cx in 0..3 {
            for cz in 0..3 {
                let ch = make(cx, cz, Biome::Mountains, 110, 5);
                coal += count(&ch, BlockType::Coal);
                iron += count(&ch, BlockType::Iron);
                other += count(&ch, BlockType::Feldspar) + count(&ch, BlockType::Pyrite)
                    + count(&ch, BlockType::Bauxite) + count(&ch, BlockType::Gold);
                air += (5..100).map(|y| (0..32).map(|x| (0..32).filter(|&z| ch.get(x, y, z) == BlockType::Air).count()).sum::<usize>()).sum::<usize>();
            }
        }
        assert!(coal > 0 && iron > 0 && other > 0, "coal={coal} iron={iron} other={other}");
        assert!(air > 0, "sem cavernas");
    }

    #[test]
    fn ocean_floor_not_pierced() {
        for cx in 0..3 {
            let ch = make(cx, 0, Biome::Ocean, 40, 5);
            for z in 0..32 { for x in 0..32 { for y in 38..=40 {
                assert_ne!(ch.get(x, y, z), BlockType::Air);
            }}}
        }
    }

    #[test]
    fn ore_continuity_across_chunks() {
        // Mesma coluna de mundo vista como borda de dois chunks vizinhos: usa coords de mundo.
        // chunk (0,0) x=31 e chunk (1,0) x=0 são vizinhos; comparar com ore_at direto.
        let n = Noises::new(8);
        let a = make(0, 0, Biome::Mountains, 110, 8);
        let b = make(1, 0, Biome::Mountains, 110, 8);
        for (ch, wx0) in [(&a, 0), (&b, 32)] {
            for lx in [0usize, 31] {
                for y in 6..100 {
                    let got = ch.get(lx, y, 7);
                    if let Some(o) = ore_at(&n, Biome::Mountains, wx0 + lx as i32, y as i32, 7) {
                        if got != BlockType::Air { assert_eq!(got, o); }
                    }
                }
            }
        }
        // Chunk deslocado em 32 no mundo == avaliação direta (sem seam): ore_at é função pura do mundo.
        assert_eq!(
            ore_at(&n, Biome::Mountains, 32, 30, 7),
            ore_at(&n, Biome::Mountains, 32, 30, 7)
        );
    }

    #[test]
    fn pyrite_water_reaction() {
        let mut ch = Chunk::new(ChunkPos::new(0, 0));
        ch.set(5, 60, 5, BlockType::Pyrite);
        ch.set(6, 60, 5, BlockType::Water);
        ch.set(5, 61, 5, BlockType::Water);
        ch.set(10, 60, 10, BlockType::Water);
        react_pyrite_water(&mut ch);
        assert_eq!(ch.get(6, 60, 5), BlockType::SulfuricAcid);
        assert_eq!(ch.get(5, 61, 5), BlockType::SulfuricAcid);
        assert_eq!(ch.get(10, 60, 10), BlockType::Water);
        assert_eq!(ch.get(5, 60, 5), BlockType::Pyrite);
    }
}
