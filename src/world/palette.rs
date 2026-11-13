use super::block::BlockType;

/// Returns [r, g, b, a] color for a block given its type and face normal
pub struct ColorPalette;

impl ColorPalette {
    pub fn get_color(block: BlockType, normal: [f32; 3]) -> [f32; 4] {
        let is_top = normal[1] > 0.5;
        let is_bottom = normal[1] < -0.5;

        match block {
            BlockType::Air => [0.0, 0.0, 0.0, 0.0],
            BlockType::Grass => {
                if is_top {
                    [0.33, 0.68, 0.22, 1.0] // bright green top
                } else if is_bottom {
                    [0.55, 0.37, 0.21, 1.0] // dirt bottom
                } else {
                    [0.42, 0.52, 0.28, 1.0] // side grass
                }
            }
            BlockType::Dirt => [0.55, 0.37, 0.21, 1.0],
            BlockType::Stone => [0.50, 0.50, 0.50, 1.0],
            BlockType::Sand => [0.90, 0.85, 0.60, 1.0],
            BlockType::Water => [0.20, 0.45, 0.80, 0.75],
            BlockType::Wood => {
                if is_top || is_bottom {
                    [0.60, 0.48, 0.28, 1.0] // ring pattern top
                } else {
                    [0.52, 0.38, 0.22, 1.0] // bark side
                }
            }
            BlockType::Leaves => [0.18, 0.55, 0.15, 0.85],
            BlockType::Snow => {
                if is_top {
                    [0.95, 0.97, 1.00, 1.0]
                } else {
                    [0.80, 0.85, 0.90, 1.0]
                }
            }
            BlockType::Gravel => [0.58, 0.55, 0.52, 1.0],
            BlockType::Cobblestone => [0.45, 0.45, 0.42, 1.0],
            BlockType::Bedrock => [0.15, 0.15, 0.15, 1.0],
            BlockType::Coal => {
                if is_top || is_bottom {
                    [0.48, 0.48, 0.48, 1.0]
                } else {
                    [0.35, 0.33, 0.31, 1.0] // coal veins
                }
            }
            BlockType::Iron => [0.75, 0.68, 0.60, 1.0],
            BlockType::Gold => [0.95, 0.85, 0.20, 1.0],
            BlockType::Cactus => {
                if is_top {
                    [0.22, 0.60, 0.18, 1.0]
                } else {
                    [0.20, 0.50, 0.15, 1.0]
                }
            }
            BlockType::Feldspar => [0.85, 0.65, 0.60, 1.0],
            BlockType::Pyrite => [0.80, 0.72, 0.30, 1.0],
            BlockType::Bauxite => [0.70, 0.40, 0.30, 1.0],
            BlockType::Moss => [0.25, 0.42, 0.20, 1.0],
            BlockType::PineLeaves => [0.10, 0.36, 0.20, 0.9],
            BlockType::PalmLeaves => [0.30, 0.65, 0.20, 0.9],
            BlockType::Coconut => [0.40, 0.28, 0.16, 1.0],
            BlockType::Planks => [0.70, 0.55, 0.32, 1.0],
            BlockType::FlowerRed => [0.90, 0.15, 0.20, 1.0],
            BlockType::FlowerYellow => [0.95, 0.85, 0.15, 1.0],
            BlockType::BerryBushRed => [0.75, 0.15, 0.20, 1.0],
            BlockType::BerryBushBlue => [0.20, 0.25, 0.80, 1.0],
            BlockType::SulfuricAcid => [0.70, 0.90, 0.20, 0.7],
            BlockType::DeadBush => [0.55, 0.45, 0.25, 1.0],
            BlockType::CactusFruit => [0.90, 0.30, 0.55, 1.0],
            BlockType::Roof => [0.60, 0.22, 0.18, 1.0],
        }
    }
}

/// Cores dos objetos que não são blocos (barcos, etc.).
impl ColorPalette {
    pub const BOAT_HULL: [f32; 4] = [0.45, 0.28, 0.14, 1.0];
    pub const BOAT_DECK: [f32; 4] = [0.62, 0.45, 0.25, 1.0];
    pub const BOAT_MAST: [f32; 4] = [0.35, 0.24, 0.13, 1.0];
    pub const BOAT_SAIL: [f32; 4] = [0.92, 0.9, 0.82, 1.0];
    pub const BOAT_NET: [f32; 4] = [0.70, 0.62, 0.40, 1.0];
}
