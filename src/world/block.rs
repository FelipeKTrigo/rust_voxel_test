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
