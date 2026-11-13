//! Contrato compartilhado da geração de mundo (Fases 3 e 4).
//! NÃO altere assinaturas sem combinar: vários módulos dependem disto.
use crate::world::chunk::CHUNK_SIZE;

pub const COLUMNS: usize = CHUNK_SIZE * CHUNK_SIZE;
pub const SEA_LEVEL: i32 = 63;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Biome {
    #[default]
    Plains = 0,
    Forest = 1,
    Mountains = 2,
    Beach = 3,
    Desert = 4,
    Ocean = 5,
}

impl Biome {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Biome::Forest,
            2 => Biome::Mountains,
            3 => Biome::Beach,
            4 => Biome::Desert,
            5 => Biome::Ocean,
            _ => Biome::Plains,
        }
    }
}

/// Saída da GPU (ou fallback CPU) por chunk. Índice: `lz * 32 + lx`.
#[derive(Clone)]
pub struct ColumnData {
    /// Altura da superfície (Y do bloco superior sólido), coordenadas de mundo.
    pub heights: Vec<u16>,
    /// Bioma dominante (Biome as u8).
    pub biomes: Vec<u8>,
    /// Peso 0..=255 de blend com o 2º bioma mais próximo (0 = puro).
    pub blend: Vec<u8>,
    /// 2º bioma mais próximo (Biome as u8), usado em blending de blocos.
    pub biomes2: Vec<u8>,
}

impl ColumnData {
    pub fn new() -> Self {
        Self {
            heights: vec![0; COLUMNS],
            biomes: vec![0; COLUMNS],
            blend: vec![0; COLUMNS],
            biomes2: vec![0; COLUMNS],
        }
    }
    #[inline]
    pub fn idx(lx: usize, lz: usize) -> usize { lz * CHUNK_SIZE + lx }
    pub fn height(&self, lx: usize, lz: usize) -> i32 { self.heights[Self::idx(lx, lz)] as i32 }
    pub fn biome(&self, lx: usize, lz: usize) -> Biome { Biome::from_u8(self.biomes[Self::idx(lx, lz)]) }
}

/// Consulta de coluna em coordenadas de MUNDO (qualquer chunk).
/// Usada por árvores/estruturas cross-chunk. Implementada pelo ColumnCache.
pub trait ColumnSampler {
    fn height_at(&self, wx: i32, wz: i32) -> i32;
    fn biome_at(&self, wx: i32, wz: i32) -> Biome;
}
