use std::collections::HashMap;
use glam::Vec3;
use wgpu::util::DeviceExt;
use crate::world::block::BlockType;
use crate::world::chunk::{Chunk, ChunkPos, CHUNK_SIZE};
use crate::world::gen::gpu_gen::{GpuGen, cpu_columns};
use crate::world::gen::types::{Biome, ColumnData, ColumnSampler, SEA_LEVEL};
use crate::world::gen::{biome, features, tree};
use crate::world::palette::ColorPalette;
use crate::renderer::mesh::ChunkMesh;

pub struct ChunkGpuData {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

pub struct LoadedChunk {
    pub chunk: Chunk,
    pub gpu_data: Option<ChunkGpuData>,
}

pub struct ChunkManager {
    pub chunks: HashMap<ChunkPos, LoadedChunk>,
    pub render_distance: i32,
    pub seed: u32,
    gpu: Option<GpuGen>,
    columns: HashMap<ChunkPos, ColumnData>,
    /// Máximo de chunks gerados por chamada de update_view (evita travadas).
    pub max_gen_per_update: usize,
    pool: Option<rayon::ThreadPool>,
}

/// Amostrador de colunas em coordenadas de mundo sobre o cache.
struct CacheSampler<'a>(&'a HashMap<ChunkPos, ColumnData>);

impl CacheSampler<'_> {
    fn col(&self, wx: i32, wz: i32) -> Option<(&ColumnData, usize, usize)> {
        let s = CHUNK_SIZE as i32;
        let pos = ChunkPos::new(wx.div_euclid(s), wz.div_euclid(s));
        self.0.get(&pos).map(|c| (c, wx.rem_euclid(s) as usize, wz.rem_euclid(s) as usize))
    }
}

impl ColumnSampler for CacheSampler<'_> {
    fn height_at(&self, wx: i32, wz: i32) -> i32 {
        self.col(wx, wz).map(|(c, x, z)| c.height(x, z)).unwrap_or(SEA_LEVEL - 10)
    }
    fn biome_at(&self, wx: i32, wz: i32) -> Biome {
        self.col(wx, wz).map(|(c, x, z)| c.biome(x, z)).unwrap_or(Biome::Ocean)
    }
}

impl ChunkManager {
    pub fn new(render_distance: i32, seed: u32) -> Self {
        Self {
            chunks: HashMap::new(),
            render_distance,
            seed,
            gpu: None,
            columns: HashMap::new(),
            max_gen_per_update: 16,
            pool: None,
        }
    }

    pub fn update_view(
        &mut self,
        player_pos: Vec3,
        palette: &ColorPalette,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let player_chunk = ChunkPos::from_world_pos(player_pos.x, player_pos.z);
        let rd = self.render_distance;

        // Collect positions to load
        let mut needed: Vec<ChunkPos> = Vec::new();
        for cx in (player_chunk.x - rd)..=(player_chunk.x + rd) {
            for cz in (player_chunk.z - rd)..=(player_chunk.z + rd) {
                let pos = ChunkPos::new(cx, cz);
                needed.push(pos);
            }
        }

        // Colunas (GPU) para a área + margem de decoração (árvores/estruturas)
        const MARGIN: i32 = 3;
        let gpu = self.gpu.get_or_insert_with(|| GpuGen::new(device, queue, self.seed));
        let mut missing_cols: Vec<ChunkPos> = Vec::new();
        for cx in (player_chunk.x - rd - MARGIN)..=(player_chunk.x + rd + MARGIN) {
            for cz in (player_chunk.z - rd - MARGIN)..=(player_chunk.z + rd + MARGIN) {
                let p = ChunkPos::new(cx, cz);
                if !self.columns.contains_key(&p) {
                    missing_cols.push(p);
                }
            }
        }
        if !missing_cols.is_empty() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                gpu.generate_batch(device, queue, &missing_cols)
            }));
            let data = result.unwrap_or_else(|_| {
                log::warn!("GPU gen falhou; usando fallback CPU");
                missing_cols.iter().map(|p| cpu_columns(self.seed, *p)).collect()
            });
            for (p, d) in missing_cols.into_iter().zip(data) {
                self.columns.insert(p, d);
            }
        }

        // Gerar chunks faltantes (mais próximos primeiro, com orçamento)
        let mut to_gen: Vec<ChunkPos> = needed.iter().copied()
            .filter(|p| !self.chunks.contains_key(p)).collect();
        to_gen.sort_by_key(|p| (p.x - player_chunk.x).pow(2) + (p.z - player_chunk.z).pow(2));
        to_gen.truncate(self.max_gen_per_update);
        let seed = self.seed;
        let columns = &self.columns;
        let built: Vec<Chunk> = {
            use rayon::prelude::*;
            let work = || to_gen.par_iter().map(|pos| {
                let cols = &columns[pos];
                let mut chunk = Chunk::new(*pos);
                biome::populate_chunk(&mut chunk, cols, seed);
                features::carve_and_ores(&mut chunk, cols, seed);
                features::react_pyrite_water(&mut chunk);
                tree::decorate_chunk(&mut chunk, seed, &CacheSampler(columns));
                chunk.is_dirty = true;
                chunk
            }).collect::<Vec<Chunk>>();
            match &self.pool {
                Some(pool) => pool.install(work),
                None => work(),
            }
        };
        for chunk in built {
            self.chunks.insert(chunk.position, LoadedChunk { chunk, gpu_data: None });
        }

        // Build GPU data for dirty chunks
        let positions_to_mesh: Vec<ChunkPos> = self
            .chunks
            .iter()
            .filter(|(_, lc)| lc.chunk.is_dirty)
            .map(|(pos, _)| *pos)
            .collect();

        for pos in positions_to_mesh {
            if let Some(loaded) = self.chunks.get_mut(&pos) {
                let mesh = ChunkMesh::build(&loaded.chunk, palette);
                loaded.chunk.is_dirty = false;

                if mesh.vertices.is_empty() {
                    loaded.gpu_data = None;
                    continue;
                }

                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Chunk Vertex Buffer"),
                    contents: bytemuck::cast_slice(&mesh.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Chunk Index Buffer"),
                    contents: bytemuck::cast_slice(&mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                loaded.gpu_data = Some(ChunkGpuData {
                    vertex_buffer,
                    index_buffer,
                    index_count: mesh.indices.len() as u32,
                });
            }
        }

        // Unload far chunks
        let needed_set: std::collections::HashSet<ChunkPos> = needed.into_iter().collect();
        self.chunks.retain(|pos, _| needed_set.contains(pos));
        self.columns.retain(|p, _| (p.x - player_chunk.x).abs() <= rd + MARGIN + 2
            && (p.z - player_chunk.z).abs() <= rd + MARGIN + 2);
    }

    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        for loaded in self.chunks.values() {
            if let Some(gpu) = &loaded.gpu_data {
                render_pass.set_vertex_buffer(0, gpu.vertex_buffer.slice(..));
                render_pass.set_index_buffer(gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..gpu.index_count, 0, 0..1);
            }
        }
    }

    pub fn get_block_at_world(&self, x: f32, y: f32, z: f32) -> Option<BlockType> {
        let chunk_pos = ChunkPos::from_world_pos(x, z);
        let loaded = self.chunks.get(&chunk_pos)?;
        let (ox, oz) = chunk_pos.world_origin();
        let lx = (x - ox) as i32;
        let ly = y as i32;
        let lz = (z - oz) as i32;
        loaded.chunk.get_safe(lx, ly, lz)
    }
}

impl ChunkManager {
    /// Define o número de threads de CPU usadas na geração de chunks.
    pub fn set_threads(&mut self, n: usize) {
        self.pool = rayon::ThreadPoolBuilder::new().num_threads(n.max(1)).build().ok();
    }

    /// Altura (Y do topo sólido) da coluna em coordenadas de mundo, se o chunk estiver carregado.
    pub fn surface_height(&self, wx: i32, wz: i32) -> Option<i32> {
        let s = CHUNK_SIZE as i32;
        let lc = self.chunks.get(&ChunkPos::new(wx.div_euclid(s), wz.div_euclid(s)))?;
        let (lx, lz) = (wx.rem_euclid(s) as usize, wz.rem_euclid(s) as usize);
        (0..crate::world::chunk::CHUNK_HEIGHT).rev()
            .find(|&y| lc.chunk.get(lx, y, lz).is_solid())
            .map(|y| y as i32)
    }

    /// Altera um bloco em coordenadas de mundo; marca chunk (e vizinho de borda) para remesh.
    pub fn set_block_world(&mut self, wx: i32, wy: i32, wz: i32, block: BlockType) -> bool {
        if wy < 0 || wy >= crate::world::chunk::CHUNK_HEIGHT as i32 { return false; }
        let s = CHUNK_SIZE as i32;
        let (cx, cz) = (wx.div_euclid(s), wz.div_euclid(s));
        let (lx, lz) = (wx.rem_euclid(s), wz.rem_euclid(s));
        match self.chunks.get_mut(&ChunkPos::new(cx, cz)) {
            Some(lc) => lc.chunk.set(lx as usize, wy as usize, lz as usize, block),
            None => return false,
        }
        let mut touch = |dx: i32, dz: i32| {
            if let Some(n) = self.chunks.get_mut(&ChunkPos::new(cx + dx, cz + dz)) {
                n.chunk.is_dirty = true;
            }
        };
        if lx == 0 { touch(-1, 0); }
        if lx == s - 1 { touch(1, 0); }
        if lz == 0 { touch(0, -1); }
        if lz == s - 1 { touch(0, 1); }
        true
    }
}

impl crate::physics::BlockQuery for ChunkManager {
    fn block_at(&self, x: i32, y: i32, z: i32) -> BlockType {
        if y < 0 { return BlockType::Bedrock; }
        if y >= crate::world::chunk::CHUNK_HEIGHT as i32 { return BlockType::Air; }
        let s = CHUNK_SIZE as i32;
        match self.chunks.get(&ChunkPos::new(x.div_euclid(s), z.div_euclid(s))) {
            Some(lc) => lc.chunk.get(x.rem_euclid(s) as usize, y as usize, z.rem_euclid(s) as usize),
            None => BlockType::Bedrock, // chunk não carregado = sólido
        }
    }
}
