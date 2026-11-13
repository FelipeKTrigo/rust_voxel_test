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

pub struct ChunkMesh {
    pub vertices: Vec<VoxelVertex>,
    pub indices: Vec<u32>,
}

impl ChunkMesh {
    pub fn build(chunk: &Chunk, palette: &ColorPalette) -> Self {
        let _ = palette; // palette is stateless; methods are static
        let mut vertices: Vec<VoxelVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

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

                        // Two triangles per quad (CCW)
                        indices.push(base_idx);
                        indices.push(base_idx + 1);
                        indices.push(base_idx + 2);
                        indices.push(base_idx);
                        indices.push(base_idx + 2);
                        indices.push(base_idx + 3);
                    }
                }
            }
        }

        Self { vertices, indices }
    }
}
