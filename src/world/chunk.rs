use super::block::BlockType;

pub const CHUNK_SIZE: usize = 32;
pub const CHUNK_HEIGHT: usize = 256;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub fn from_world_pos(wx: f32, wz: f32) -> Self {
        Self {
            x: (wx / CHUNK_SIZE as f32).floor() as i32,
            z: (wz / CHUNK_SIZE as f32).floor() as i32,
        }
    }

    pub fn world_origin(&self) -> (f32, f32) {
        (
            self.x as f32 * CHUNK_SIZE as f32,
            self.z as f32 * CHUNK_SIZE as f32,
        )
    }
}

pub struct Chunk {
    pub blocks: Box<[BlockType]>,
    pub position: ChunkPos,
    pub is_dirty: bool,
}

impl Chunk {
    pub fn new(position: ChunkPos) -> Self {
        Self {
            blocks: vec![BlockType::Air; CHUNK_VOLUME].into_boxed_slice(),
            position,
            is_dirty: true,
        }
    }

    /// Index formula: y + x * CHUNK_HEIGHT + z * CHUNK_HEIGHT * CHUNK_SIZE
    #[inline]
    pub fn block_index(x: usize, y: usize, z: usize) -> usize {
        y + x * CHUNK_HEIGHT + z * CHUNK_HEIGHT * CHUNK_SIZE
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> BlockType {
        self.blocks[Self::block_index(x, y, z)]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, block: BlockType) {
        let idx = Self::block_index(x, y, z);
        self.blocks[idx] = block;
        self.is_dirty = true;
    }

    pub fn get_safe(&self, x: i32, y: i32, z: i32) -> Option<BlockType> {
        if x < 0 || x >= CHUNK_SIZE as i32
            || y < 0 || y >= CHUNK_HEIGHT as i32
            || z < 0 || z >= CHUNK_SIZE as i32
        {
            return None;
        }
        Some(self.get(x as usize, y as usize, z as usize))
    }
}
