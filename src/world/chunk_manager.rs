use std::collections::HashMap;
use glam::Vec3;
use wgpu::util::DeviceExt;
use crate::world::block::BlockType;
use crate::world::chunk::{Chunk, ChunkPos, CHUNK_SIZE};
use crate::world::gen::gpu_gen::{GpuGen, cpu_columns};
use crate::world::gen::types::{Biome, ColumnData, ColumnSampler, SEA_LEVEL};
use crate::world::gen::{biome, features, tree};
use crate::world::palette::ColorPalette;
use crate::world::save::{apply_edits, WorldEdits};
use crate::renderer::lod::{lod_for_distance, Lod};
use crate::renderer::mesh::{column_top, ChunkMesh};
use crate::camera::Frustum;
use crate::ui::settings::LodLevel;

pub struct MeshBuffers {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl MeshBuffers {
    fn new(device: &wgpu::Device, vertices: &[u8], indices: &[u32]) -> Self {
        Self {
            vertex_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Chunk Vertex Buffer"),
                contents: vertices,
                usage: wgpu::BufferUsages::VERTEX,
            }),
            index_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Chunk Index Buffer"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            index_count: indices.len() as u32,
        }
    }

    fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

pub struct ChunkGpuData {
    pub opaque: Option<MeshBuffers>,
    pub water: Option<MeshBuffers>,
}

pub struct LoadedChunk {
    pub chunk: Chunk,
    pub gpu_data: Option<ChunkGpuData>,
    /// LOD desejado/atual da malha.
    pub lod: Lod,
    /// Maior Y ocupado da malha atual (caixa do frustum culling).
    pub max_y: f32,
}

/// Posições de chunk dentro do quadrado de raio `rd` ao redor de `center`.
pub fn needed_positions(center: ChunkPos, rd: i32) -> Vec<ChunkPos> {
    let mut v = Vec::with_capacity(((2 * rd + 1) * (2 * rd + 1)).max(0) as usize);
    for cx in (center.x - rd)..=(center.x + rd) {
        for cz in (center.z - rd)..=(center.z + rd) {
            v.push(ChunkPos::new(cx, cz));
        }
    }
    v
}

/// Distância de Chebyshev (em chunks).
pub fn chunk_distance(a: ChunkPos, b: ChunkPos) -> i32 {
    (a.x - b.x).abs().max((a.z - b.z).abs())
}

/// Gera os blocos de um chunk a partir das colunas (determinístico) e aplica o delta salvo.
pub fn generate_chunk(
    seed: u32,
    pos: ChunkPos,
    columns: &HashMap<ChunkPos, ColumnData>,
    edits: &WorldEdits,
) -> Chunk {
    let mut chunk = Chunk::new(pos);
    biome::populate_chunk(&mut chunk, &columns[&pos], seed);
    features::carve_and_ores(&mut chunk, &columns[&pos], seed);
    features::react_pyrite_water(&mut chunk);
    tree::decorate_chunk(&mut chunk, seed, &CacheSampler(columns));
    if let Some(e) = edits.get(&pos) {
        apply_edits(&mut chunk, e);
    }
    chunk.is_dirty = true;
    chunk
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
    /// Delta do jogador sobre o mundo procedural (persiste ao descarregar chunks).
    edits: WorldEdits,
    /// Houve alteração desde o último save.
    pub unsaved: bool,
    /// Nível de LOD configurado (decide o LOD de cada chunk pela distância).
    pub lod_level: LodLevel,
    /// Máximo de chunks (re)malhados por chamada de update_view.
    pub max_mesh_per_update: usize,
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
            edits: WorldEdits::new(),
            unsaved: false,
            lod_level: LodLevel::Medium,
            max_mesh_per_update: 8,
        }
    }

    pub fn edits(&self) -> &WorldEdits {
        &self.edits
    }

    pub fn set_edits(&mut self, edits: WorldEdits) {
        self.edits = edits;
        self.unsaved = false;
    }

    pub fn update_view(
        &mut self,
        player_pos: Vec3,
        _palette: &ColorPalette,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let player_chunk = ChunkPos::from_world_pos(player_pos.x, player_pos.z);
        let rd = self.render_distance;

        let needed = needed_positions(player_chunk, rd);

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
        let edits = &self.edits;
        let built: Vec<Chunk> = {
            use rayon::prelude::*;
            let work = || to_gen.par_iter()
                .map(|pos| generate_chunk(seed, *pos, columns, edits))
                .collect::<Vec<Chunk>>();
            match &self.pool {
                Some(pool) => pool.install(work),
                None => work(),
            }
        };
        for chunk in built {
            self.chunks.insert(chunk.position, LoadedChunk { chunk, gpu_data: None, lod: Lod::Full, max_y: 256.0 });
        }

        // LOD desejado por distância; mudança => remalhar (dentro do orçamento)
        let level = self.lod_level;
        for (pos, lc) in self.chunks.iter_mut() {
            let want = lod_for_distance(chunk_distance(*pos, player_chunk), level);
            if lc.lod != want {
                lc.lod = want;
                lc.chunk.is_dirty = true;
            }
        }

        // Malhas pendentes: mais próximas primeiro, limitadas por frame, construídas em paralelo.
        let mut dirty: Vec<ChunkPos> = self.chunks.iter()
            .filter(|(_, lc)| lc.chunk.is_dirty)
            .map(|(p, _)| *p)
            .collect();
        dirty.sort_by_key(|p| chunk_distance(*p, player_chunk));
        dirty.truncate(self.max_mesh_per_update);
        let meshes: Vec<(ChunkPos, ChunkMesh)> = {
            use rayon::prelude::*;
            let chunks = &self.chunks;
            let work = || dirty.par_iter().map(|pos| {
                let lc = &chunks[pos];
                let neighbor_top = |lx: i32, lz: i32| -> Option<i32> {
                    let s = CHUNK_SIZE as i32;
                    let (wx, wz) = (pos.x * s + lx, pos.z * s + lz);
                    let n = chunks.get(&ChunkPos::new(wx.div_euclid(s), wz.div_euclid(s)))?;
                    let (x, z) = (wx.rem_euclid(s) as usize, wz.rem_euclid(s) as usize);
                    Some(column_top(&n.chunk, x, z).map_or(-1, |(y, _)| y as i32))
                };
                (*pos, ChunkMesh::build_lod(&lc.chunk, lc.lod, &neighbor_top))
            }).collect::<Vec<_>>();
            match &self.pool {
                Some(pool) => pool.install(work),
                None => work(),
            }
        };
        for (pos, mesh) in meshes {
            if let Some(loaded) = self.chunks.get_mut(&pos) {
                loaded.chunk.is_dirty = false;
                loaded.max_y = mesh.max_y;
                let opaque = (!mesh.vertices.is_empty())
                    .then(|| MeshBuffers::new(device, bytemuck::cast_slice(&mesh.vertices), &mesh.indices));
                let water = (!mesh.water_vertices.is_empty())
                    .then(|| MeshBuffers::new(device, bytemuck::cast_slice(&mesh.water_vertices), &mesh.water_indices));
                loaded.gpu_data = if opaque.is_none() && water.is_none() {
                    None
                } else {
                    Some(ChunkGpuData { opaque, water })
                };
            }
        }

        // Unload far chunks
        let needed_set: std::collections::HashSet<ChunkPos> = needed.into_iter().collect();
        self.chunks.retain(|pos, _| needed_set.contains(pos));
        self.columns.retain(|p, _| (p.x - player_chunk.x).abs() <= rd + MARGIN + 2
            && (p.z - player_chunk.z).abs() <= rd + MARGIN + 2);
    }

    /// Terreno opaco (e folhas).
    pub fn render_opaque<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, frustum: &Frustum) {
        for (pos, loaded) in &self.chunks {
            if !Self::visible(pos, loaded, frustum) { continue; }
            if let Some(m) = loaded.gpu_data.as_ref().and_then(|g| g.opaque.as_ref()) {
                m.draw(render_pass);
            }
        }
    }

    /// Água translúcida, de trás para frente em relação à câmera.
    pub fn render_water<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, camera: Vec3, frustum: &Frustum) {
        let mut list: Vec<(f32, &MeshBuffers)> = self
            .chunks
            .iter()
            .filter_map(|(pos, lc)| {
                if !Self::visible(pos, lc, frustum) { return None; }
                let m = lc.gpu_data.as_ref()?.water.as_ref()?;
                let (ox, oz) = pos.world_origin();
                let c = Vec3::new(ox + 16.0, camera.y, oz + 16.0);
                Some((c.distance_squared(camera), m))
            })
            .collect();
        list.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, m) in list {
            m.draw(render_pass);
        }
    }

    fn visible(pos: &ChunkPos, lc: &LoadedChunk, frustum: &Frustum) -> bool {
        let (ox, oz) = pos.world_origin();
        let s = CHUNK_SIZE as f32;
        frustum.intersects_aabb(Vec3::new(ox, 0.0, oz), Vec3::new(ox + s, lc.max_y + 1.0, oz + s))
    }

    /// Quantos chunks seriam desenhados (visíveis no frustum e com malha).
    #[allow(dead_code)]
    pub fn visible_count(&self, frustum: &Frustum) -> usize {
        self.chunks.iter()
            .filter(|(p, lc)| lc.gpu_data.is_some() && Self::visible(p, lc, frustum))
            .count()
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
        self.edits
            .entry(ChunkPos::new(cx, cz))
            .or_default()
            .insert(crate::world::chunk::Chunk::block_index(lx as usize, wy as usize, lz as usize) as u32, block as u8);
        self.unsaved = true;
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

/// Utilidades de teste compartilhadas (geração 100% CPU, sem GPU).
#[cfg(test)]
pub mod test_support {
    use super::*;

    /// Colunas CPU para a área `pos ± margem` (a margem alimenta árvores/estruturas).
    pub fn cpu_column_cache(seed: u32, center: ChunkPos, radius: i32) -> HashMap<ChunkPos, ColumnData> {
        let mut m = HashMap::new();
        for p in needed_positions(center, radius) {
            m.insert(p, cpu_columns(seed, p));
        }
        m
    }

    pub fn generate_cpu(seed: u32, pos: ChunkPos) -> Chunk {
        let cols = cpu_column_cache(seed, pos, 3);
        generate_chunk(seed, pos, &cols, &WorldEdits::new())
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::camera::Camera;

    #[test]
    fn needed_positions_is_a_centered_square() {
        let c = ChunkPos::new(-3, 7);
        for rd in [0, 1, 4, 8] {
            let v = needed_positions(c, rd);
            assert_eq!(v.len() as i32, (2 * rd + 1).pow(2));
            assert!(v.contains(&c));
            assert!(v.iter().all(|p| chunk_distance(*p, c) <= rd));
            let set: std::collections::HashSet<_> = v.iter().collect();
            assert_eq!(set.len(), v.len(), "sem duplicatas");
        }
    }

    #[test]
    fn chunk_distance_is_chebyshev() {
        assert_eq!(chunk_distance(ChunkPos::new(0, 0), ChunkPos::new(3, -5)), 5);
        assert_eq!(chunk_distance(ChunkPos::new(2, 2), ChunkPos::new(2, 2)), 0);
    }

    #[test]
    fn generation_is_deterministic_and_applies_edits() {
        let a = generate_cpu(42, ChunkPos::new(1, 1));
        let b = generate_cpu(42, ChunkPos::new(1, 1));
        assert_eq!(a.blocks, b.blocks);
        let c = generate_cpu(43, ChunkPos::new(1, 1));
        assert_ne!(a.blocks, c.blocks);

        let pos = ChunkPos::new(1, 1);
        let cols = cpu_column_cache(42, pos, 3);
        let mut edits = WorldEdits::new();
        let idx = Chunk::block_index(5, 100, 5) as u32;
        edits.entry(pos).or_default().insert(idx, BlockType::Gold as u8);
        let d = generate_chunk(42, pos, &cols, &edits);
        assert_eq!(d.get(5, 100, 5), BlockType::Gold);
        assert!(d.is_dirty);
    }

    /// Superfície sólida mais alta de uma coluna de chunk.
    fn top(c: &Chunk, x: usize, z: usize) -> i32 {
        (0..crate::world::chunk::CHUNK_HEIGHT).rev()
            .find(|&y| c.get(x, y, z).is_solid()).map_or(-1, |y| y as i32)
    }

    #[test]
    fn terrain_has_no_gaps_across_chunk_borders() {
        // Teste de continuidade ponta a ponta (colunas -> blocos): degraus entre colunas vizinhas
        // na fronteira do chunk ficam dentro do limite, sem paredes/fendas artificiais.
        let seed = 42;
        let cols = cpu_column_cache(seed, ChunkPos::new(0, 0), 4);
        let edits = WorldEdits::new();
        let mut worst = 0;
        for cz in -1..=1 {
            for cx in -1..=1 {
                let a = generate_chunk(seed, ChunkPos::new(cx, cz), &cols, &edits);
                let bx = generate_chunk(seed, ChunkPos::new(cx + 1, cz), &cols, &edits);
                let bz = generate_chunk(seed, ChunkPos::new(cx, cz + 1), &cols, &edits);
                for k in 0..CHUNK_SIZE {
                    worst = worst.max((top(&a, CHUNK_SIZE - 1, k) - top(&bx, 0, k)).abs());
                    worst = worst.max((top(&a, k, CHUNK_SIZE - 1) - top(&bz, k, 0)).abs());
                }
            }
        }
        eprintln!("maior degrau na fronteira: {worst}");
        assert!(worst <= 22, "degrau {worst} excede o esperado (árvores/estruturas incluídas)");
    }

    // ---- Integração com GPU (pulada sem adaptador, como em gpu_gen) ----

    fn manager(rd: i32, level: LodLevel) -> Option<(ChunkManager, wgpu::Device, wgpu::Queue)> {
        let (device, queue) = GpuGen::new_headless()?;
        let mut cm = ChunkManager::new(rd, 42);
        cm.lod_level = level;
        cm.max_gen_per_update = usize::MAX;
        cm.max_mesh_per_update = usize::MAX;
        Some((cm, device, queue))
    }

    #[test]
    fn load_unload_follows_the_player() {
        let Some((mut cm, device, queue)) = manager(3, LodLevel::Medium) else {
            eprintln!("sem adaptador GPU, pulando");
            return;
        };
        let pal = ColorPalette;
        cm.update_view(Vec3::new(16.0, 80.0, 16.0), &pal, &device, &queue);
        assert_eq!(cm.chunks.len(), 49);
        assert!(cm.chunks.values().all(|c| !c.chunk.is_dirty), "tudo malhado");

        // Anda 5 chunks para +X: o que ficou fora do raio é descarregado, o resto permanece.
        let far = Vec3::new(16.0 + 5.0 * 32.0, 80.0, 16.0);
        cm.update_view(far, &pal, &device, &queue);
        assert_eq!(cm.chunks.len(), 49);
        let center = ChunkPos::from_world_pos(far.x, far.z);
        assert!(cm.chunks.keys().all(|p| chunk_distance(*p, center) <= 3));
        assert!(!cm.chunks.contains_key(&ChunkPos::new(0, 0)));
        assert!(cm.chunks.contains_key(&center));

        // Voltar regenera o mesmo conteúdo.
        let before = generate_chunk(42, ChunkPos::new(5, 0), &cm.columns, cm.edits());
        assert_eq!(cm.chunks[&ChunkPos::new(5, 0)].chunk.blocks, before.blocks);
    }

    #[test]
    fn edits_survive_unload_and_reload() {
        let Some((mut cm, device, queue)) = manager(2, LodLevel::Off) else { return };
        let pal = ColorPalette;
        let start = Vec3::new(16.0, 80.0, 16.0);
        cm.update_view(start, &pal, &device, &queue);
        let y = cm.surface_height(8, 8).unwrap();
        assert!(cm.set_block_world(8, y, 8, BlockType::Air));
        assert!(cm.unsaved);
        cm.update_view(Vec3::new(16.0 + 10.0 * 32.0, 80.0, 16.0), &pal, &device, &queue);
        assert!(cm.get_block_at_world(8.5, y as f32, 8.5).is_none(), "descarregado");
        cm.update_view(start, &pal, &device, &queue);
        assert_eq!(cm.get_block_at_world(8.5, y as f32, 8.5), Some(BlockType::Air));
    }

    #[test]
    fn lod_follows_distance_and_setting_changes_remesh() {
        let Some((mut cm, device, queue)) = manager(8, LodLevel::Low) else {
            eprintln!("sem adaptador GPU, pulando");
            return;
        };
        let pal = ColorPalette;
        let pos = Vec3::new(16.0, 80.0, 16.0);
        cm.update_view(pos, &pal, &device, &queue);
        let at = |cm: &ChunkManager, x, z| cm.chunks[&ChunkPos::new(x, z)].lod;
        assert_eq!(at(&cm, 0, 0), Lod::Full);
        assert_eq!(at(&cm, 5, 0), Lod::Merged);
        assert_eq!(at(&cm, 8, 8), Lod::Light);

        cm.lod_level = LodLevel::Off;
        cm.update_view(pos, &pal, &device, &queue);
        assert!(cm.chunks.values().all(|c| c.lod == Lod::Full && !c.chunk.is_dirty));
    }

    #[test]
    fn mesh_budget_is_respected_and_nearest_first() {
        let Some((mut cm, device, queue)) = manager(4, LodLevel::Off) else { return };
        cm.max_mesh_per_update = 5;
        let pal = ColorPalette;
        let pos = Vec3::new(16.0, 80.0, 16.0);
        cm.update_view(pos, &pal, &device, &queue);
        let dirty = cm.chunks.values().filter(|c| c.chunk.is_dirty).count();
        assert_eq!(dirty, 81 - 5);
        // Os 5 primeiros são os mais próximos (o do jogador incluso).
        assert!(!cm.chunks[&ChunkPos::new(0, 0)].chunk.is_dirty);
        for _ in 0..20 {
            cm.update_view(pos, &pal, &device, &queue);
        }
        assert!(cm.chunks.values().all(|c| !c.chunk.is_dirty));
    }

    #[test]
    fn frustum_culls_chunks_behind_the_camera() {
        let Some((mut cm, device, queue)) = manager(4, LodLevel::Off) else { return };
        let pos = Vec3::new(16.0, 80.0, 16.0);
        cm.update_view(pos, &ColorPalette, &device, &queue);
        let mut cam = Camera::new(pos, 16.0 / 9.0);
        cam.set_far_for_render_distance(4);
        let seen = cm.visible_count(&cam.frustum());
        let total = cm.chunks.values().filter(|c| c.gpu_data.is_some()).count();
        eprintln!("visíveis {seen}/{total}");
        assert!(seen > 0 && seen < total, "culling deve remover parte dos chunks");
    }

    // ---- Benchmarks (cargo test --release -- --ignored --nocapture bench_) ----

    fn time_ms<T>(iters: u32, mut f: impl FnMut() -> T) -> f64 {
        let t = std::time::Instant::now();
        for _ in 0..iters {
            std::hint::black_box(f());
        }
        t.elapsed().as_secs_f64() * 1000.0 / iters as f64
    }

    /// Meta do plano: < 16 ms por chunk. Só é imposta em builds otimizadas.
    fn check_budget(label: &str, ms: f64) {
        eprintln!("{label}: {ms:.2} ms/chunk");
        if !cfg!(debug_assertions) {
            assert!(ms < 16.0, "{label}: {ms:.2} ms excede 16 ms");
        }
    }

    #[test]
    #[ignore = "benchmark: cargo test --release -- --ignored --nocapture"]
    fn bench_chunk_generation() {
        let seed = 42;
        let pos = ChunkPos::new(2, 3); // inclui a vizinhança
        let cols = cpu_column_cache(seed, pos, 3);
        let edits = WorldEdits::new();
        // Latência de um chunk novo no pool que o jogo usa por padrão (metade dos núcleos).
        let game_threads = crate::ui::settings::GraphicsSettings::default().gen_threads;
        let pool = rayon::ThreadPoolBuilder::new().num_threads(game_threads).build().unwrap();
        let ms = pool.install(|| time_ms(20, || generate_chunk(seed, pos, &cols, &edits)));
        eprintln!("(pool de {game_threads} threads)");
        // Custo de CPU de um núcleo só (vazão por thread), apenas informativo.
        let one = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
        let single = one.install(|| time_ms(10, || generate_chunk(seed, pos, &cols, &edits)));
        eprintln!("geração de blocos (1 thread): {single:.2} ms/chunk");
        check_budget("geração de blocos", ms);
    }

    #[test]
    #[ignore = "benchmark: cargo test --release -- --ignored --nocapture"]
    fn bench_chunk_meshing() {
        for pos in [ChunkPos::new(2, 3), ChunkPos::new(-6, 5)] {
            let chunk = generate_cpu(42, pos);
            for lod in [Lod::Full, Lod::Merged, Lod::Light] {
                let ms = time_ms(20, || ChunkMesh::build_lod(&chunk, lod, &|_, _| None));
                check_budget(&format!("malha {lod:?} {pos:?}"), ms);
            }
        }
    }
}
