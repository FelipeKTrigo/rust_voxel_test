use crate::world::block::BlockType;
use crate::world::chunk::{Chunk, CHUNK_SIZE, CHUNK_HEIGHT};
use crate::world::palette::ColorPalette;
use crate::renderer::lod::Lod;

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
    /// Maior Y ocupado (para o frustum culling); 0 se o chunk está vazio.
    pub max_y: f32,
}

/// Altura (Y) do topo do terreno numa coluna vizinha, em coordenadas LOCAIS ao chunk
/// (x ou z fora de `0..32`). `None` = desconhecido.
pub type NeighborTop<'a> = &'a (dyn Fn(i32, i32) -> Option<i32> + Sync);

/// Profundidade da "saia" na borda do chunk quando o vizinho é desconhecido.
const UNKNOWN_SKIRT: i32 = 6;

fn push_quad_indices(indices: &mut Vec<u32>, base: u32) {
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Retângulo alinhado aos eixos de uma face: `origin` = canto mínimo da célula, `ext` = tamanho
/// (1 no eixo da normal). Preserva o enrolamento de `FACE_VERTICES`.
fn push_quad(vs: &mut Vec<VoxelVertex>, is: &mut Vec<u32>, face: usize, origin: [f32; 3], ext: [f32; 3], color: [f32; 4]) {
    let base = vs.len() as u32;
    for v in &FACE_VERTICES[face] {
        vs.push(VoxelVertex {
            position: [
                origin[0] + v[0] as f32 * ext[0],
                origin[1] + v[1] as f32 * ext[1],
                origin[2] + v[2] as f32 * ext[2],
            ],
            color,
            normal: FACE_NORMALS[face],
        });
    }
    push_quad_indices(is, base);
}

/// Eixos (u, v) do plano perpendicular ao eixo `s` da normal.
fn plane_axes(s: usize) -> (usize, usize) {
    match s {
        0 => (2, 1),
        1 => (0, 2),
        _ => (0, 1),
    }
}

/// Greedy meshing 2D: agrupa células adjacentes de mesma chave em retângulos.
/// `emit(u, v, largura, altura, chave)`. Consome (zera) a máscara.
fn greedy_rects<K: Copy + PartialEq>(
    mask: &mut [Option<K>],
    w: usize,
    h: usize,
    mut emit: impl FnMut(usize, usize, usize, usize, K),
) {
    for v in 0..h {
        let mut u = 0;
        while u < w {
            let Some(k) = mask[u + v * w] else {
                u += 1;
                continue;
            };
            let mut rw = 1;
            while u + rw < w && mask[u + rw + v * w] == Some(k) {
                rw += 1;
            }
            let mut rh = 1;
            'grow: while v + rh < h {
                for i in 0..rw {
                    if mask[u + i + (v + rh) * w] != Some(k) {
                        break 'grow;
                    }
                }
                rh += 1;
            }
            for dv in 0..rh {
                for du in 0..rw {
                    mask[u + du + (v + dv) * w] = None;
                }
            }
            emit(u, v, rw, rh, k);
            u += rw;
        }
    }
}

/// Bloco como visto pelos LODs distantes: minérios viram pedra e detalhes
/// pequenos desaparecem (invisíveis a distância, e permitem fundir mais quads).
fn simplify(b: BlockType) -> BlockType {
    use BlockType::*;
    match b {
        Coal | Iron | Gold | Feldspar | Pyrite | Bauxite => Stone,
        FlowerRed | FlowerYellow | DeadBush | BerryBushRed | BerryBushBlue | CactusFruit | Coconut => Air,
        other => other,
    }
}

/// Maior Y não vazio do chunk.
pub fn chunk_max_y(chunk: &Chunk) -> Option<usize> {
    chunk
        .blocks
        .chunks_exact(CHUNK_HEIGHT)
        .filter_map(|col| col.iter().rposition(|b| !b.is_air()))
        .max()
}

/// Topo do terreno da coluna (ignora ar e água; aplica `simplify`).
pub fn column_top(chunk: &Chunk, x: usize, z: usize) -> Option<(usize, BlockType)> {
    (0..CHUNK_HEIGHT).rev().find_map(|y| {
        let b = simplify(chunk.get(x, y, z));
        (!b.is_air() && b != BlockType::Water).then_some((y, b))
    })
}

impl ChunkMesh {
    /// Malha de resolução total (LOD 0).
    #[allow(dead_code)]
    pub fn build(chunk: &Chunk, _palette: &ColorPalette) -> Self {
        Self::build_lod(chunk, Lod::Full, &|_, _| None)
    }

    pub fn build_lod(chunk: &Chunk, lod: Lod, neighbor_top: NeighborTop) -> Self {
        let mut m = Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            water_vertices: Vec::new(),
            water_indices: Vec::new(),
            max_y: 0.0,
        };
        let Some(top) = chunk_max_y(chunk) else { return m };
        m.max_y = (top + 1) as f32;
        match lod {
            Lod::Full => {
                m.greedy_opaque(chunk, top, false);
                m.naive_water(chunk);
            }
            Lod::Merged => {
                m.greedy_opaque(chunk, top, true);
                m.naive_water(chunk);
            }
            Lod::Light => m.light(chunk, neighbor_top),
        }
        m
    }

    /// Faces opacas (e folhas) com greedy meshing. `distant` ativa `simplify` e descarta faces inferiores.
    fn greedy_opaque(&mut self, chunk: &Chunk, top: usize, distant: bool) {
        let get = |x: i32, y: i32, z: i32| -> Option<BlockType> {
            chunk.get_safe(x, y, z).map(|b| if distant { simplify(b) } else { b })
        };
        let (ox, oz) = chunk.position.world_origin();
        let lim = [CHUNK_SIZE, top + 1, CHUNK_SIZE];
        let mut mask: Vec<Option<BlockType>> = Vec::new();
        for face in 0..6 {
            if distant && face == 3 {
                continue;
            }
            let s = face / 2;
            let (ua, va) = plane_axes(s);
            let (wu, hv) = (lim[ua], lim[va]);
            let n = FACE_NEIGHBORS[face];
            for slice in 0..lim[s] {
                mask.clear();
                mask.resize(wu * hv, None);
                for v in 0..hv {
                    for u in 0..wu {
                        let mut c = [0i32; 3];
                        c[s] = slice as i32;
                        c[ua] = u as i32;
                        c[va] = v as i32;
                        let Some(b) = get(c[0], c[1], c[2]) else { continue };
                        if b.is_air() || b == BlockType::Water {
                            continue;
                        }
                        let nb = get(c[0] + n[0], c[1] + n[1], c[2] + n[2]);
                        if !nb.map_or(true, |x| x.is_transparent()) {
                            continue; // face oculta
                        }
                        if b.is_transparent() && nb == Some(b) {
                            continue; // folha contra folha
                        }
                        mask[u + v * wu] = Some(b);
                    }
                }
                greedy_rects(&mut mask, wu, hv, |u, v, rw, rh, b| {
                    let mut o = [ox, 0.0, oz];
                    let mut e = [1.0f32; 3];
                    o[s] += slice as f32;
                    o[ua] += u as f32;
                    o[va] += v as f32;
                    e[ua] = rw as f32;
                    e[va] = rh as f32;
                    push_quad(&mut self.vertices, &mut self.indices, face, o, e,
                        ColorPalette::get_color(b, FACE_NORMALS[face]));
                });
            }
        }
    }

    /// Água: face só se o vizinho está DENTRO do chunk e é ar. Fora do chunk não emitimos
    /// (evita "paredes" translúcidas a cada 32 blocos no oceano). Sem merge: as ondas
    /// deslocam vértices e precisam da grade fina.
    fn naive_water(&mut self, chunk: &Chunk) {
        let (ox, oz) = chunk.position.world_origin();
        for lx in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_HEIGHT {
                for lz in 0..CHUNK_SIZE {
                    if chunk.get(lx, ly, lz) != BlockType::Water {
                        continue;
                    }
                    let (wx, wy, wz) = (ox + lx as f32, ly as f32, oz + lz as f32);
                    let above_is_water =
                        chunk.get_safe(lx as i32, ly as i32 + 1, lz as i32) == Some(BlockType::Water);
                    for face_idx in 0..6 {
                        let n = FACE_NEIGHBORS[face_idx];
                        let neighbor = chunk.get_safe(lx as i32 + n[0], ly as i32 + n[1], lz as i32 + n[2]);
                        if neighbor != Some(BlockType::Air) {
                            continue;
                        }
                        let normal = FACE_NORMALS[face_idx];
                        let color = ColorPalette::get_color(BlockType::Water, normal);
                        let base_idx = self.water_vertices.len() as u32;
                        for vert in &FACE_VERTICES[face_idx] {
                            let wave = if vert[1] == 1 && face_idx != 3 && !above_is_water { 1.0 } else { 0.0 };
                            self.water_vertices.push(WaterVertex {
                                position: [wx + vert[0] as f32, wy + vert[1] as f32, wz + vert[2] as f32],
                                color,
                                normal,
                                wave,
                            });
                        }
                        push_quad_indices(&mut self.water_indices, base_idx);
                    }
                }
            }
        }
    }

    /// LOD distante: só o topo de cada coluna (retângulos fundidos) + "saias" laterais até a
    /// altura do vizinho (sem buracos em penhascos) + superfície da água.
    fn light(&mut self, chunk: &Chunk, neighbor_top: NeighborTop) {
        let (ox, oz) = chunk.position.world_origin();
        let s = CHUNK_SIZE;
        let tops: Vec<Option<(usize, BlockType)>> =
            (0..s * s).map(|i| column_top(chunk, i % s, i / s)).collect();
        let top_y = |x: i32, z: i32| -> Option<i32> {
            if (0..s as i32).contains(&x) && (0..s as i32).contains(&z) {
                Some(tops[z as usize * s + x as usize].map_or(-1, |(y, _)| y as i32))
            } else {
                neighbor_top(x, z)
            }
        };

        // Topos
        let mut mask: Vec<Option<(BlockType, u16)>> = tops
            .iter()
            .map(|t| t.map(|(y, b)| (b, y as u16)))
            .collect();
        greedy_rects(&mut mask, s, s, |u, v, rw, rh, (b, y)| {
            push_quad(&mut self.vertices, &mut self.indices, 2,
                [ox + u as f32, y as f32, oz + v as f32], [rw as f32, 1.0, rh as f32],
                ColorPalette::get_color(b, FACE_NORMALS[2]));
        });

        // Saias laterais
        for face in [0usize, 1, 4, 5] {
            let n = FACE_NEIGHBORS[face];
            let along_x = face / 2 == 2; // faces ±Z: a linha corre ao longo de X
            for slice in 0..s {
                let mut row: Vec<Option<(BlockType, u16, u16)>> = vec![None; s];
                for a in 0..s {
                    let (x, z) = if along_x { (a, slice) } else { (slice, a) };
                    let Some((t, tb)) = tops[z * s + x] else { continue };
                    let nt = top_y(x as i32 + n[0], z as i32 + n[2])
                        .unwrap_or(t as i32 - UNKNOWN_SKIRT)
                        .max(-1);
                    if nt >= t as i32 {
                        continue;
                    }
                    let side = if t > 0 {
                        let b = simplify(chunk.get(x, t - 1, z));
                        if b.is_air() || b == BlockType::Water { tb } else { b }
                    } else {
                        tb
                    };
                    row[a] = Some((side, (nt + 1) as u16, (t + 1) as u16));
                }
                greedy_rects(&mut row, s, 1, |u, _, rw, _, (b, y0, y1)| {
                    let h = (y1 - y0) as f32;
                    let (origin, ext) = if along_x {
                        ([ox + u as f32, y0 as f32, oz + slice as f32], [rw as f32, h, 1.0])
                    } else {
                        ([ox + slice as f32, y0 as f32, oz + u as f32], [1.0, h, rw as f32])
                    };
                    push_quad(&mut self.vertices, &mut self.indices, face, origin, ext,
                        ColorPalette::get_color(b, FACE_NORMALS[face]));
                });
            }
        }

        // Superfície da água (apenas onde está acima do terreno)
        let mut wmask: Vec<Option<u16>> = (0..s * s)
            .map(|i| {
                let (x, z) = (i % s, i / s);
                let wy = (0..CHUNK_HEIGHT).rev().find(|&y| chunk.get(x, y, z) == BlockType::Water)?;
                let ground = tops[i].map_or(-1, |(y, _)| y as i32);
                (wy as i32 > ground).then_some(wy as u16)
            })
            .collect();
        let color = ColorPalette::get_color(BlockType::Water, FACE_NORMALS[2]);
        greedy_rects(&mut wmask, s, s, |u, v, rw, rh, y| {
            let base = self.water_vertices.len() as u32;
            for vert in &FACE_VERTICES[2] {
                self.water_vertices.push(WaterVertex {
                    position: [
                        ox + u as f32 + vert[0] as f32 * rw as f32,
                        y as f32 + 1.0,
                        oz + v as f32 + vert[2] as f32 * rh as f32,
                    ],
                    color,
                    normal: FACE_NORMALS[2],
                    wave: 1.0,
                });
            }
            push_quad_indices(&mut self.water_indices, base);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
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

    // ---------- Fase 10: greedy meshing & LOD ----------

    use std::collections::HashMap;

    fn no_neighbors() -> impl Fn(i32, i32) -> Option<i32> + Sync {
        |_, _| None
    }

    fn slab(top: usize, block: BlockType) -> Chunk {
        let mut c = Chunk::new(ChunkPos::new(0, 0));
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for y in 0..=top {
                    c.set(x, y, z, block);
                }
            }
        }
        c
    }

    fn quad_area(q: &[VoxelVertex]) -> f32 {
        let (a, b, d) = (Vec3::from(q[0].position), Vec3::from(q[1].position), Vec3::from(q[3].position));
        (b - a).length() * (d - a).length()
    }

    /// Soma das áreas das faces opacas visíveis, contando bloco a bloco (referência ingênua).
    fn naive_visible_area(chunk: &Chunk) -> f32 {
        let mut area = 0.0;
        for x in 0..CHUNK_SIZE as i32 {
            for y in 0..CHUNK_HEIGHT as i32 {
                for z in 0..CHUNK_SIZE as i32 {
                    let b = chunk.get(x as usize, y as usize, z as usize);
                    if b.is_air() || b == BlockType::Water {
                        continue;
                    }
                    for n in FACE_NEIGHBORS {
                        let nb = chunk.get_safe(x + n[0], y + n[1], z + n[2]);
                        if !nb.map_or(true, |o| o.is_transparent()) { continue; }
                        if b.is_transparent() && nb == Some(b) { continue; }
                        area += 1.0;
                    }
                }
            }
        }
        area
    }

    fn mesh_area(m: &ChunkMesh) -> f32 {
        m.vertices.chunks(4).map(quad_area).sum()
    }

    fn real_chunk(cx: i32, cz: i32) -> Chunk {
        crate::world::chunk_manager::test_support::generate_cpu(42, ChunkPos::new(cx, cz))
    }

    #[test]
    fn greedy_flat_slab_is_six_quads() {
        let m = ChunkMesh::build_lod(&slab(10, BlockType::Stone), Lod::Full, &no_neighbors());
        assert_eq!(m.vertices.len(), 6 * 4, "topo, base e 4 laterais");
        assert_eq!(m.indices.len(), 6 * 6);
        assert_eq!(m.max_y, 11.0);
    }

    #[test]
    fn greedy_does_not_merge_different_blocks() {
        let mut c = slab(10, BlockType::Stone);
        c.set(5, 10, 5, BlockType::Sand);
        let m = ChunkMesh::build_lod(&c, Lod::Full, &no_neighbors());
        let top: Vec<_> = m.vertices.chunks(4)
            .filter(|q| q[0].normal == [0.0, 1.0, 0.0] && q[0].position[1] == 11.0).collect();
        assert!(top.len() >= 5, "areia isola o retângulo de pedra do topo: {}", top.len());
        let sand = ColorPalette::get_color(BlockType::Sand, [0.0, 1.0, 0.0]);
        let sand_area: f32 = top.iter().filter(|q| q[0].color == sand).map(|q| quad_area(q)).sum();
        assert_eq!(sand_area, 1.0);
        assert_eq!(mesh_area(&m), naive_visible_area(&c));
    }

    #[test]
    fn greedy_area_equals_naive_on_real_terrain() {
        for (cx, cz) in [(0, 0), (3, -2), (-5, 4)] {
            let c = real_chunk(cx, cz);
            let m = ChunkMesh::build_lod(&c, Lod::Full, &no_neighbors());
            assert_eq!(mesh_area(&m), naive_visible_area(&c), "chunk ({cx},{cz})");
            assert!(m.vertices.len() / 4 < naive_visible_area(&c) as usize, "greedy deve reduzir os quads");
        }
    }

    #[test]
    fn greedy_winding_matches_normals() {
        let m = ChunkMesh::build_lod(&real_chunk(0, 0), Lod::Full, &no_neighbors());
        for q in m.vertices.chunks(4) {
            let (a, b, c) = (Vec3::from(q[0].position), Vec3::from(q[1].position), Vec3::from(q[2].position));
            let n = (b - a).cross(c - a).normalize();
            assert!(n.dot(Vec3::from(q[0].normal)) > 0.99);
        }
        assert!(m.indices.iter().all(|&i| (i as usize) < m.vertices.len()));
    }

    #[test]
    fn leaves_skip_internal_faces() {
        let mut c = Chunk::new(ChunkPos::new(0, 0));
        for x in 4..8 { for y in 4..8 { for z in 4..8 { c.set(x, y, z, BlockType::Leaves); } } }
        let m = ChunkMesh::build_lod(&c, Lod::Full, &no_neighbors());
        assert_eq!(m.vertices.len(), 6 * 4, "cubo de folhas: só a casca, fundida");
    }

    #[test]
    fn merged_lod_simplifies_ores_and_drops_details() {
        let mut c = slab(10, BlockType::Stone);
        c.set(3, 11, 3, BlockType::Iron);
        c.set(8, 11, 8, BlockType::FlowerRed);
        let full = ChunkMesh::build_lod(&c, Lod::Full, &no_neighbors());
        let merged = ChunkMesh::build_lod(&c, Lod::Merged, &no_neighbors());
        let iron = ColorPalette::get_color(BlockType::Iron, [1.0, 0.0, 0.0]);
        let flower = ColorPalette::get_color(BlockType::FlowerRed, [0.0, 1.0, 0.0]);
        assert!(full.vertices.iter().any(|v| v.color == iron));
        assert!(full.vertices.iter().any(|v| v.color == flower));
        assert!(!merged.vertices.iter().any(|v| v.color == iron));
        assert!(!merged.vertices.iter().any(|v| v.color == flower));
        assert!(merged.vertices.iter().all(|v| v.normal[1] > -0.5), "sem faces inferiores");
        assert!(merged.vertices.len() < full.vertices.len());
    }

    #[test]
    fn light_lod_closes_cliffs_with_skirts() {
        // Degrau: x<16 topo em y=20, x>=16 topo em y=10.
        let mut c = Chunk::new(ChunkPos::new(0, 0));
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let top = if x < 16 { 20 } else { 10 };
                for y in 0..=top { c.set(x, y, z, BlockType::Stone); }
            }
        }
        let m = ChunkMesh::build_lod(&c, Lod::Light, &no_neighbors());
        // Saia em x=16 voltada para +X cobrindo y 11..21
        let wall: Vec<_> = m.vertices.chunks(4)
            .filter(|q| q[0].normal == [1.0, 0.0, 0.0] && q[0].position[0] == 16.0).collect();
        assert_eq!(wall.len(), 1, "saia fundida em um único quad");
        let ys: Vec<f32> = wall[0].iter().map(|v| v.position[1]).collect();
        assert_eq!(ys.iter().cloned().fold(f32::MAX, f32::min), 11.0);
        assert_eq!(ys.iter().cloned().fold(f32::MIN, f32::max), 21.0);
        // No sentido oposto (-X) não há saia: o lado alto não fica exposto.
        assert!(!m.vertices.iter().any(|v| v.normal == [-1.0, 0.0, 0.0] && v.position[0] == 16.0));
    }

    #[test]
    fn light_lod_uses_neighbor_heights_at_chunk_border() {
        let c = slab(30, BlockType::Stone);
        let low = |x: i32, _z: i32| if x < 0 { Some(5) } else { Some(30) };
        let m = ChunkMesh::build_lod(&c, Lod::Light, &low);
        let wall: Vec<_> = m.vertices.chunks(4)
            .filter(|q| q[0].normal == [-1.0, 0.0, 0.0] && q[0].position[0] == 0.0).collect();
        assert_eq!(wall.len(), 1);
        let miny = wall[0].iter().map(|v| v.position[1]).fold(f32::MAX, f32::min);
        assert_eq!(miny, 6.0, "desce até o topo do vizinho + 1");
        // vizinho de mesma altura => sem saia
        assert!(!m.vertices.iter().any(|v| v.normal == [1.0, 0.0, 0.0]));
    }

    /// Altura da face +Y mais alta de cada coluna (x, z) de mundo.
    fn top_surface(m: &ChunkMesh) -> HashMap<(i32, i32), f32> {
        let mut out: HashMap<(i32, i32), f32> = HashMap::new();
        for q in m.vertices.chunks(4).filter(|q| q[0].normal == [0.0, 1.0, 0.0]) {
            let xs = q.iter().map(|v| v.position[0]);
            let zs = q.iter().map(|v| v.position[2]);
            let (x0, x1) = (xs.clone().fold(f32::MAX, f32::min) as i32, xs.fold(f32::MIN, f32::max) as i32);
            let (z0, z1) = (zs.clone().fold(f32::MAX, f32::min) as i32, zs.fold(f32::MIN, f32::max) as i32);
            let y = q[0].position[1];
            for x in x0..x1 {
                for z in z0..z1 {
                    let e = out.entry((x, z)).or_insert(y);
                    *e = e.max(y);
                }
            }
        }
        out
    }

    #[test]
    fn lod_seams_are_continuous() {
        // Mesma superfície visível nos LODs Merged e Light, inclusive nas bordas do chunk,
        // então chunks vizinhos com LODs diferentes não deixam buracos.
        for (cx, cz) in [(0, 0), (1, 0), (-4, 3), (6, 6)] {
            let c = real_chunk(cx, cz);
            let merged = top_surface(&ChunkMesh::build_lod(&c, Lod::Merged, &no_neighbors()));
            let light = top_surface(&ChunkMesh::build_lod(&c, Lod::Light, &no_neighbors()));
            assert_eq!(merged.len(), light.len(), "colunas cobertas ({cx},{cz})");
            for (k, h) in &merged {
                assert_eq!(light.get(k), Some(h), "coluna {k:?} do chunk ({cx},{cz})");
            }
        }
    }

    #[test]
    fn lod_reduces_geometry() {
        let c = real_chunk(2, 2);
        let n = |l| ChunkMesh::build_lod(&c, l, &no_neighbors()).vertices.len();
        let (full, merged, light) = (n(Lod::Full), n(Lod::Merged), n(Lod::Light));
        eprintln!("vértices: full={full} merged={merged} light={light}");
        assert!(merged <= full);
        assert!(light < merged, "light deve ser bem menor");
    }

    #[test]
    fn light_lod_water_is_a_merged_surface() {
        let m = ChunkMesh::build_lod(&pool_chunk(), Lod::Light, &no_neighbors());
        assert_eq!(m.water_vertices.len(), 4, "oceano uniforme = um quad");
        assert!(m.water_vertices.iter().all(|v| v.position[1] == 41.0 && v.wave == 1.0));
    }

    #[test]
    fn empty_chunk_has_empty_mesh_for_all_lods() {
        let c = Chunk::new(ChunkPos::new(0, 0));
        for lod in [Lod::Full, Lod::Merged, Lod::Light] {
            let m = ChunkMesh::build_lod(&c, lod, &no_neighbors());
            assert!(m.vertices.is_empty() && m.water_vertices.is_empty());
        }
    }
}
