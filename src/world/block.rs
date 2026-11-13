#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum BlockType {
    #[default]
    Air = 0,
    Grass = 1,
    Dirt = 2,
    Stone = 3,
    Sand = 4,
    Water = 5,
    Wood = 6,
    Leaves = 7,
    Snow = 8,
    Gravel = 9,
    Cobblestone = 10,
    Bedrock = 11,
    Coal = 12,
    Iron = 13,
    Gold = 14,
    Cactus = 15,
    Feldspar = 16,
    Pyrite = 17,
    Bauxite = 18,
    Moss = 19,
    PineLeaves = 20,
    PalmLeaves = 21,
    Coconut = 22,
    Planks = 23,
    FlowerRed = 24,
    FlowerYellow = 25,
    BerryBushRed = 26,
    BerryBushBlue = 27,
    SulfuricAcid = 28,
    DeadBush = 29,
    CactusFruit = 30,
    Roof = 31,
}

impl BlockType {
    /// Converte o byte de save de volta para o bloco (valor desconhecido = Air).
    pub fn from_u8(v: u8) -> BlockType {
        use BlockType::*;
        const ALL: [BlockType; 32] = [
            Air, Grass, Dirt, Stone, Sand, Water, Wood, Leaves, Snow, Gravel, Cobblestone, Bedrock,
            Coal, Iron, Gold, Cactus, Feldspar, Pyrite, Bauxite, Moss, PineLeaves, PalmLeaves,
            Coconut, Planks, FlowerRed, FlowerYellow, BerryBushRed, BerryBushBlue, SulfuricAcid,
            DeadBush, CactusFruit, Roof,
        ];
        ALL.get(v as usize).copied().unwrap_or(Air)
    }

    pub fn is_solid(&self) -> bool {
        !matches!(self, BlockType::Air | BlockType::Water | BlockType::SulfuricAcid | BlockType::FlowerRed | BlockType::FlowerYellow | BlockType::DeadBush)
    }

    pub fn is_transparent(&self) -> bool {
        matches!(self, BlockType::Air | BlockType::Water | BlockType::Leaves | BlockType::PineLeaves | BlockType::PalmLeaves)
    }

    pub fn is_air(&self) -> bool {
        matches!(self, BlockType::Air)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u8_roundtrip_all_blocks() {
        for v in 0u8..32 {
            assert_eq!(BlockType::from_u8(v) as u8, v);
        }
        assert_eq!(BlockType::from_u8(200), BlockType::Air);
    }
}
