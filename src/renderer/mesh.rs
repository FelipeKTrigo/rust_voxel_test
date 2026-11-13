use crate::world::block::BlockType;
use crate::world::chunk::{Chunk, CHUNK_SIZE, CHUNK_HEIGHT};
use crate::world::palette::ColorPalette;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VoxelVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub normal: [f32; 3],
}

impl VoxelVertex {
    pub const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x4,
        2 => Float32x3,
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<VoxelVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

// 6 faces: +X, -X, +Y, -Y, +Z, -Z
// Each face: 4 vertices in CCW order (verified via cross-product)
// (B-A) x (C-A) must equal outward normal
const FACE_VERTICES: [[[i32; 3]; 4]; 6] = [
    [[1,0,0],[1,1,0],[1,1,1],[1,0,1]],  // +X  normal=(1,0,0)
    [[0,0,1],[0,1,1],[0,1,0],[0,0,0]],  // -X  normal=(-1,0,0)
    [[0,1,1],[1,1,1],[1,1,0],[0,1,0]],  // +Y  normal=(0,1,0)
    [[0,0,0],[1,0,0],[1,0,1],[0,0,1]],  // -Y  normal=(0,-1,0)
    [[0,0,1],[1,0,1],[1,1,1],[0,1,1]],  // +Z  normal=(0,0,1)
    [[1,0,0],[0,0,0],[0,1,0],[1,1,0]],  // -Z  normal=(0,0,-1)
];

const FACE_NORMALS: [[f32; 3]; 6] = [
    [1.0,  0.0,  0.0],
    [-1.0, 0.0,  0.0],
    [0.0,  1.0,  0.0],
    [0.0, -1.0,  0.0],
    [0.0,  0.0,  1.0],
    [0.0,  0.0, -1.0],
];

// Neighbor offsets for each face
const FACE_NEIGHBORS: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

/// Vértice da água: `wave` = 1.0 nos vértices da superfície (deslocados pelas ondas no shader).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WaterVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub normal: [f32; 3],
    pub wave: f32,
}

impl WaterVertex {
    pub const ATTRIBS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x4,
        2 => Float32x3,
        3 => Float32,
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WaterVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct ChunkMesh {
    pub vertices: Vec<VoxelVertex>,
    pub indices: Vec<u32>,
    pub water_vertices: Vec<WaterVertex>,
    pub water_indices: Vec<u32>,
}

fn push_quad_indices(indices: &mut Vec<u32>, base: u32) {
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

impl ChunkMesh {
    pub fn build(chunk: &Chunk, palette: &ColorPalette) -> Self {
        let _ = palette; // palette is stateless; methods are static
        let mut vertices: Vec<VoxelVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut water_vertices: Vec<WaterVertex> = Vec::new();
        let mut water_indices: Vec<u32> = Vec::new();

        let (ox, oz) = chunk.position.world_origin();

        for lx in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_HEIGHT {
                for lz in 0..CHUNK_SIZE {
                    let block = chunk.get(lx, ly, lz);
                    if block.is_air() {
                        continue;
                    }

                    let wx = ox + lx as f32;
                    let wy = ly as f32;
                    let wz = oz + lz as f32;

                    if block == BlockType::Water {
                        // Água: face só se o vizinho está DENTRO do chunk e é ar. Fora do chunk
                        // não emitimos (evita "paredes" translúcidas a cada 32 blocos no oceano).
                        let above_is_water = chunk.get_safe(lx as i32, ly as i32 + 1, lz as i32)
                            == Some(BlockType::Water);
                        for face_idx in 0..6 {
                            let n = FACE_NEIGHBORS[face_idx];
                            let neighbor = chunk.get_safe(lx as i32 + n[0], ly as i32 + n[1], lz as i32 + n[2]);
                            if neighbor != Some(BlockType::Air) {
                                continue;
                            }
                            let normal = FACE_NORMALS[face_idx];
                            let color = ColorPalette::get_color(block, normal);
                            let base_idx = water_vertices.len() as u32;
                            for vert in &FACE_VERTICES[face_idx] {
                                let wave = if vert[1] == 1 && face_idx != 3 && !above_is_water { 1.0 } else { 0.0 };
                                water_vertices.push(WaterVertex {
                                    position: [
                                        wx + vert[0] as f32,
                                        wy + vert[1] as f32,
                                        wz + vert[2] as f32,
                                    ],
                                    color,
                                    normal,
                                    wave,
                                });
                            }
                            push_quad_indices(&mut water_indices, base_idx);
                        }
                        continue;
                    }

                    for face_idx in 0..6 {
                        let n = FACE_NEIGHBORS[face_idx];
                        let nx = lx as i32 + n[0];
                        let ny = ly as i32 + n[1];
                        let nz = lz as i32 + n[2];

                        // Check if neighbor is transparent/air (emit face)
                        let neighbor_transparent = match chunk.get_safe(nx, ny, nz) {
                            Some(neighbor) => neighbor.is_transparent(),
                            None => true, // out of bounds = air
                        };

                        if !neighbor_transparent {
                            continue; // Face is occluded
                        }

                        // Don't render transparent block faces against same type
                        if block.is_transparent() {
                            if let Some(neighbor) = chunk.get_safe(nx, ny, nz) {
                                if neighbor == block {
                                    continue;
                                }
                            }
                        }

                        let normal = FACE_NORMALS[face_idx];
                        let color = ColorPalette::get_color(block, normal);

                        let base_idx = vertices.len() as u32;
                        for vert in &FACE_VERTICES[face_idx] {
                            vertices.push(VoxelVertex {
                                position: [
                                    wx + vert[0] as f32,
                                    wy + vert[1] as f32,
                                    wz + vert[2] as f32,
                                ],
                                color,
                                normal,
                            });
                        }
                        push_quad_indices(&mut indices, base_idx);
                    }
                }
            }
        }

        Self { vertices, indices, water_vertices, water_indices }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::ChunkPos;

    fn pool_chunk() -> Chunk {
        let mut c = Chunk::new(ChunkPos::new(0, 0));
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for y in 0..=40 {
                    c.set(x, y, z, if y <= 30 { BlockType::Stone } else { BlockType::Water });
                }
            }
        }
        c
    }

    #[test]
    fn water_goes_to_its_own_mesh() {
        let m = ChunkMesh::build(&pool_chunk(), &ColorPalette);
        assert!(m.vertices.iter().all(|v| v.color[3] >= 0.99), "terreno opaco sem água");
        // Apenas a superfície (y=40 -> topo em 41): 32*32 quads, sem paredes nas bordas do chunk
        assert_eq!(m.water_vertices.len(), CHUNK_SIZE * CHUNK_SIZE * 4);
        assert!(m.water_vertices.iter().all(|v| v.normal == [0.0, 1.0, 0.0] && v.wave == 1.0));
        assert_eq!(m.water_indices.len(), CHUNK_SIZE * CHUNK_SIZE * 6);
    }

    #[test]
    fn terrain_under_water_is_still_meshed() {
        let m = ChunkMesh::build(&pool_chunk(), &ColorPalette);
        // Topo da pedra coberto por água continua visível (vizinho transparente)
        assert!(m.vertices.iter().any(|v| v.normal == [0.0, 1.0, 0.0] && v.position[1] == 31.0));
    }

    #[test]
    fn exposed_water_side_gets_face_and_only_top_waves() {
        let mut c = Chunk::new(ChunkPos::new(0, 0));
        c.set(5, 10, 5, BlockType::Water);
        let m = ChunkMesh::build(&c, &ColorPalette);
        assert_eq!(m.water_vertices.len(), 6 * 4);
        for v in &m.water_vertices {
            let expect = v.position[1] == 11.0 && v.normal[1] > -0.5;
            assert_eq!(v.wave == 1.0, expect);
        }
    }
}
