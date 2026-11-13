//! Geração de colunas (heightmap + biomas) na GPU via compute shader, com
//! fallback CPU (`cpu_columns`) que espelha o mesmo algoritmo em f32.
//! Coordenadas de MUNDO globais => continuidade entre chunks.
use std::collections::HashMap;

use glam::Vec2;
use wgpu::util::DeviceExt;

use super::types::{Biome, ColumnData, COLUMNS, SEA_LEVEL};
use crate::world::chunk::{ChunkPos, CHUNK_SIZE};

const NOISE_WGSL: &str = include_str!("../../../assets/shaders/compute/noise.wgsl");
const BIOME_WGSL: &str = include_str!("../../../assets/shaders/compute/biome.wgsl");
const HEIGHT_WGSL: &str = include_str!("../../../assets/shaders/compute/heightmap.wgsl");

/// Máximo de chunks por dispatch/readback.
const MAX_BATCH: usize = 512;

pub struct GpuGen {
    pipeline: wgpu::ComputePipeline,
    seed: u32,
}

impl GpuGen {
    /// Cria device/queue sem janela (testes). `None` se não houver adaptador.
    pub fn new_headless() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).ok()
    }

    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, seed: u32) -> Self {
        let src = format!("{NOISE_WGSL}\n{BIOME_WGSL}\n{HEIGHT_WGSL}");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("worldgen_compute"),
            source: wgpu::ShaderSource::Wgsl(src.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("worldgen_pipeline"),
            layout: None,
            module: &module,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });
        Self { pipeline, seed }
    }

    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// Gera colunas para cada posição (mesma ordem de `positions`).
    pub fn generate_batch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        positions: &[ChunkPos],
    ) -> Vec<ColumnData> {
        let mut result = Vec::with_capacity(positions.len());
        for group in positions.chunks(MAX_BATCH) {
            self.run_group(device, queue, group, &mut result);
        }
        result
    }

    fn run_group(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        group: &[ChunkPos],
        result: &mut Vec<ColumnData>,
    ) {
        let n = group.len();
        let params: [u32; 4] = [self.seed, n as u32, 0, 0];
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gen_params"),
            contents: bytemuck::cast_slice(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let coords: Vec<i32> = group.iter().flat_map(|p| [p.x, p.z]).collect();
        let chunks_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gen_chunks"),
            contents: bytemuck::cast_slice(&coords),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let out_size = (n * COLUMNS * 2 * 4) as u64;
        let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gen_out"),
            size: out_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gen_staging"),
            size: out_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = self.pipeline.get_bind_group_layout(0);
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gen_bind"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: chunks_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: out_buf.as_entire_binding() },
            ],
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("gen_enc") });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("gen_pass"), timestamp_writes: None });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((CHUNK_SIZE / 8) as u32, (CHUNK_SIZE / 8) as u32, n as u32);
        }
        enc.copy_buffer_to_buffer(&out_buf, 0, &staging, 0, out_size);
        queue.submit(Some(enc.finish()));

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::Maintain::Wait);
        rx.recv().expect("map_async canal").expect("map_async falhou");
        {
            let data = slice.get_mapped_range();
            let words: &[u32] = bytemuck::cast_slice(&data);
            for c in 0..n {
                let mut cd = ColumnData::new();
                for i in 0..COLUMNS {
                    let a = words[(c * COLUMNS + i) * 2];
                    let b = words[(c * COLUMNS + i) * 2 + 1];
                    cd.heights[i] = (a & 0xffff) as u16;
                    cd.biomes[i] = ((a >> 16) & 0xff) as u8;
                    cd.biomes2[i] = ((a >> 24) & 0xff) as u8;
                    cd.blend[i] = (b & 0xff) as u8;
                }
                result.push(cd);
            }
        }
        staging.unmap();
    }
}

// ───────────────────────── Referência CPU (espelho do WGSL) ─────────────────────────

const CELL: f32 = 160.0;
const BLEND_SCALE: f32 = 20.0;

fn pcg(v: u32) -> u32 {
    let s = v.wrapping_mul(747796405).wrapping_add(2891336453);
    let w = ((s >> ((s >> 28) + 4)) ^ s).wrapping_mul(277803737);
    (w >> 22) ^ w
}

struct Noise {
    seed: u32,
}

impl Noise {
    fn hash2(&self, ix: i32, iz: i32, salt: u32) -> u32 {
        let mut h = self.seed ^ salt.wrapping_mul(2654435769);
        h = pcg(h.wrapping_add(ix as u32));
        pcg(h.wrapping_add(iz as u32))
    }
    fn grad(h: u32) -> Vec2 {
        let a = (h & 1023) as f32 * (6.2831853 / 1024.0);
        Vec2::new(a.cos(), a.sin())
    }
    fn snoise(&self, p: Vec2, salt: u32) -> f32 {
        const K1: f32 = 0.3660254;
        const K2: f32 = 0.21132487;
        let s = (p.x + p.y) * K1;
        let i = (p.x + s).floor();
        let j = (p.y + s).floor();
        let t = (i + j) * K2;
        let x0 = p.x - (i - t);
        let y0 = p.y - (j - t);
        let (i1x, i1y) = if x0 > y0 { (1, 0) } else { (0, 1) };
        let x1 = x0 - i1x as f32 + K2;
        let y1 = y0 - i1y as f32 + K2;
        let x2 = x0 - 1.0 + 2.0 * K2;
        let y2 = y0 - 1.0 + 2.0 * K2;
        let ii = i as i32;
        let jj = j as i32;
        let mut n = 0.0;
        let mut tt = 0.5 - x0 * x0 - y0 * y0;
        if tt > 0.0 {
            tt *= tt;
            n += tt * tt * Self::grad(self.hash2(ii, jj, salt)).dot(Vec2::new(x0, y0));
        }
        tt = 0.5 - x1 * x1 - y1 * y1;
        if tt > 0.0 {
            tt *= tt;
            n += tt * tt * Self::grad(self.hash2(ii + i1x, jj + i1y, salt)).dot(Vec2::new(x1, y1));
        }
        tt = 0.5 - x2 * x2 - y2 * y2;
        if tt > 0.0 {
            tt *= tt;
            n += tt * tt * Self::grad(self.hash2(ii + 1, jj + 1, salt)).dot(Vec2::new(x2, y2));
        }
        (n * 70.0).clamp(-1.0, 1.0)
    }
    fn fbm(&self, p: Vec2, salt: u32, oct: u32) -> f32 {
        let (mut sum, mut amp, mut f, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for o in 0..oct {
            sum += amp * self.snoise(p * f, salt + o);
            norm += amp;
            amp *= 0.5;
            f *= 2.0;
        }
        sum / norm
    }
    fn ridgef(&self, p: Vec2, salt: u32, oct: u32) -> f32 {
        let (mut sum, mut amp, mut f, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for o in 0..oct {
            let mut n = 1.0 - self.snoise(p * f, salt + o).abs();
            n *= n;
            sum += amp * n;
            norm += amp;
            amp *= 0.5;
            f *= 2.0;
        }
        sum / norm
    }
    fn cell_point(&self, cx: i32, cz: i32) -> Vec2 {
        let h = self.hash2(cx, cz, 100);
        let fx = (h & 0xffff) as f32 / 65535.0;
        let fz = (h >> 16) as f32 / 65535.0;
        Vec2::new(cx as f32 + 0.2 + 0.6 * fx, cz as f32 + 0.2 + 0.6 * fz) * CELL
    }
    fn classify(&self, p: Vec2) -> u8 {
        let temp = (self.fbm(p * 0.0012, 1, 3) * 1.8).clamp(-1.0, 1.0);
        let humid = (self.fbm(p * 0.0012, 2, 3) * 1.8).clamp(-1.0, 1.0);
        let cont = (self.fbm(p * 0.0008, 3, 3) * 1.8).clamp(-1.0, 1.0);
        let mnt = (self.fbm(p * 0.0014, 4, 3) * 1.8).clamp(-1.0, 1.0);
        let b = if cont < -0.2 {
            Biome::Ocean
        } else if mnt > 0.25 || (temp < -0.4 && humid < 0.0) {
            Biome::Mountains
        } else if temp > 0.2 && humid < 0.1 {
            Biome::Desert
        } else if humid > 0.0 {
            Biome::Forest
        } else {
            Biome::Plains
        };
        b as u8
    }
}

fn biome_height(b: u8, rolling: f32, detail: f32, dunes: f32, ridge: f32) -> f32 {
    match b {
        0 => (70.0 + 4.0 * rolling + detail).clamp(65.0, 75.0),
        1 => (80.0 + 8.0 * rolling + 2.0 * detail).clamp(70.0, 90.0),
        2 => (100.0 + 100.0 * ridge + 6.0 * rolling).clamp(100.0, 200.0),
        3 => 65.5 + 1.5 * detail,
        4 => (80.0 + 12.0 * dunes + 3.0 * rolling).clamp(65.0, 95.0),
        _ => (50.0 + 6.0 * rolling + 2.0 * detail).clamp(30.0, 61.0),
    }
}

/// (altura, bioma, bioma2, blend) para uma coluna em coordenadas de mundo.
fn column(nz: &Noise, p: Vec2, cache: &mut HashMap<(i32, i32), (Vec2, u8)>) -> (u16, u8, u8, u8) {
    let cx0 = (p.x / CELL).floor() as i32;
    let cz0 = (p.y / CELL).floor() as i32;
    let mut ds = [0f32; 9];
    let mut bs = [0u8; 9];
    for k in 0..9 {
        let cx = cx0 + (k % 3) as i32 - 1;
        let cz = cz0 + (k / 3) as i32 - 1;
        let (cp, b) = *cache
            .entry((cx, cz))
            .or_insert_with(|| {
                let cp = nz.cell_point(cx, cz);
                (cp, nz.classify(cp))
            });
        ds[k] = p.distance(cp);
        bs[k] = b;
    }
    let mut i1 = 0usize;
    let mut i2: i32 = -1;
    for k in 1..9 {
        if ds[k] < ds[i1] {
            i2 = i1 as i32;
            i1 = k;
        } else if i2 < 0 || ds[k] < ds[i2 as usize] {
            i2 = k as i32;
        }
    }
    let i2 = i2 as usize;
    let (d1, d2) = (ds[i1], ds[i2]);

    let rolling = (nz.fbm(p * 0.004, 5, 4) * 1.5).clamp(-1.0, 1.0);
    let detail = nz.snoise(p * 0.03, 6);
    let dunes = nz.snoise(Vec2::new(p.x * 0.012, p.y * 0.03), 7);
    let ridge = nz.ridgef(p * 0.0035, 8, 4);

    let (mut wsum, mut hsum, mut wocean, mut wmount) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for k in 0..9 {
        let w = (-(ds[k] - d1) / BLEND_SCALE).exp();
        wsum += w;
        hsum += w * biome_height(bs[k], rolling, detail, dunes, ridge);
        if bs[k] == 5 {
            wocean += w;
        }
        if bs[k] == 2 {
            wmount += w;
        }
    }
    let mut h = hsum / wsum;
    wocean /= wsum;
    wmount /= wsum;

    let rv = nz.snoise(p * 0.0045, 9).abs();
    const RW: f32 = 0.09;
    if rv < RW {
        let kk = 1.0 - rv / RW;
        let carve = kk * kk * (8.0 + 22.0 * wmount);
        h -= carve * ((h - 66.0) / 10.0).clamp(0.0, 1.0);
    }
    h = h.clamp(1.0, 255.0);
    let hi = h.floor() as i32;

    let (b1, b2) = (bs[i1], bs[i2]);
    let (mut bio, mut bio2) = (b1, b2);
    if hi < SEA_LEVEL {
        if b1 != 5 {
            bio2 = b1;
        }
        bio = 5;
    } else if b1 == 5 && hi > 68 {
        bio = if b2 == 5 { 0 } else { b2 };
        bio2 = b1;
    } else if hi <= 68 && b1 != 2 && (b1 == 5 || wocean > 0.02) {
        bio = 3;
        if b1 != 5 {
            bio2 = b1;
        }
    }
    let bl = (127.5 * (-(d2 - d1) / BLEND_SCALE).exp()).clamp(0.0, 255.0) as u8;
    (hi as u16, bio, bio2, bl)
}

/// Fallback CPU determinístico (mesmo algoritmo do compute shader).
pub fn cpu_columns(seed: u32, pos: ChunkPos) -> ColumnData {
    let nz = Noise { seed };
    let mut cache = HashMap::new();
    let mut cd = ColumnData::new();
    let (ox, oz) = (pos.x * CHUNK_SIZE as i32, pos.z * CHUNK_SIZE as i32);
    for lz in 0..CHUNK_SIZE {
        for lx in 0..CHUNK_SIZE {
            let p = Vec2::new((ox + lx as i32) as f32, (oz + lz as i32) as f32);
            let (h, b, b2, bl) = column(&nz, p, &mut cache);
            let i = ColumnData::idx(lx, lz);
            cd.heights[i] = h;
            cd.biomes[i] = b;
            cd.biomes2[i] = b2;
            cd.blend[i] = bl;
        }
    }
    cd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_edges(a: &ColumnData, b: &ColumnData, along_x: bool, maxd: i32) {
        let mut worst = 0;
        for k in 0..CHUNK_SIZE {
            let (ha, hb) = if along_x {
                (a.height(CHUNK_SIZE - 1, k), b.height(0, k))
            } else {
                (a.height(k, CHUNK_SIZE - 1), b.height(k, 0))
            };
            worst = worst.max((ha - hb).abs());
        }
        eprintln!("worst {worst}"); assert!(worst <= maxd, "salto de altura {worst} na fronteira");
    }

    #[test]
    fn cpu_continuity_and_determinism() {
        for cz in -6..6 {
            for cx in -6..6 {
                let a = cpu_columns(42, ChunkPos::new(cx, cz));
                let b = cpu_columns(42, ChunkPos::new(cx + 1, cz));
                let c = cpu_columns(42, ChunkPos::new(cx, cz + 1));
                check_edges(&a, &b, true, 12);
                check_edges(&a, &c, false, 12);
            }
        }
        let a = cpu_columns(7, ChunkPos::new(3, -2));
        let b = cpu_columns(7, ChunkPos::new(3, -2));
        assert_eq!(a.heights, b.heights);
        assert_eq!(a.biomes, b.biomes);
        let c = cpu_columns(8, ChunkPos::new(3, -2));
        assert_ne!(a.heights, c.heights);
    }

    #[test]
    fn cpu_all_biomes_appear() {
        let mut seen = [false; 6];
        let mut counts = [0usize; 6];
        for cz in (-40..40).step_by(2) {
            for cx in (-40..40).step_by(2) {
                let d = cpu_columns(1234, ChunkPos::new(cx, cz));
                for (i, &b) in d.biomes.iter().enumerate() {
                    seen[b as usize] = true;
                    counts[b as usize] += 1;
                    let h = d.heights[i] as i32;
                    if b == Biome::Ocean as u8 { assert!(h < SEA_LEVEL); }
                    if b == Biome::Mountains as u8 { assert!((100..=200).contains(&h) || h >= 69, "h={h}"); }
                }
            }
        }
        eprintln!("biome counts: {counts:?}");
        assert!(seen.iter().all(|&s| s), "biomas ausentes: {seen:?}");
    }

    #[test]
    fn gpu_matches_and_is_continuous() {
        let Some((device, queue)) = GpuGen::new_headless() else {
            eprintln!("sem adaptador GPU, pulando");
            return;
        };
        let g = GpuGen::new(&device, &queue, 42);
        let mut pos = Vec::new();
        for cz in -4..4 {
            for cx in -4..4 {
                pos.push(ChunkPos::new(cx, cz));
            }
        }
        let out = g.generate_batch(&device, &queue, &pos);
        let out2 = g.generate_batch(&device, &queue, &pos);
        assert_eq!(out.len(), pos.len());
        for (a, b) in out.iter().zip(&out2) {
            assert_eq!(a.heights, b.heights);
            assert_eq!(a.biomes, b.biomes);
        }
        // continuidade entre chunks adjacentes em x
        for i in 0..pos.len() {
            if pos[i].x < 3 {
                check_edges(&out[i], &out[i + 1], true, 12);
            }
        }
        // concordância com a referência CPU (ruído float pode divergir um pouco)
        let mut bad = 0;
        let mut total = 0;
        for (p, d) in pos.iter().zip(&out) {
            let c = cpu_columns(42, *p);
            for i in 0..COLUMNS {
                total += 1;
                if (c.heights[i] as i32 - d.heights[i] as i32).abs() > 3 { bad += 1; }
            }
        }
        assert!(bad * 20 < total, "GPU/CPU divergem: {bad}/{total}");
    }
}
