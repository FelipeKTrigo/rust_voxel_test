//! Vegetação e estruturas determinísticas, cross-chunk.
//!
//! Tudo é função pura de (seed, coordenadas de mundo, ColumnSampler):
//! - árvores: grade jitterada de células 8x8 por chunk (`tree_positions_in_chunk`);
//! - estruturas (casa, moinho, casa de pescador, oásis): célula âncora de 64x64;
//! - `decorate_chunk` aplica no chunk os blocos de tudo que o alcança (±2 chunks),
//!   então chunks vizinhos geram sempre a mesma união, sem lacunas.
use super::types::{Biome, ColumnSampler, SEA_LEVEL};
use crate::world::block::BlockType;
use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE};

const CS: i32 = CHUNK_SIZE as i32;
const CELL: i32 = 8;
const STRUCT_CELL: i32 = 64;
/// Maior raio horizontal de exclusão de uma estrutura (+ margem).
const MAX_STRUCT_REACH: i32 = 10;
const SNOW_LINE: i32 = 110;

const SALT_TREE_POS: u32 = 0x7001;
const SALT_TREE_ROLL: u32 = 0x7002;
const SALT_TREE_KIND: u32 = 0x7003;
const SALT_TREE_VAR: u32 = 0x7004;
const SALT_STRUCT_POS: u32 = 0x8001;
const SALT_STRUCT_ROLL: u32 = 0x8002;
const SALT_STRUCT_KIND: u32 = 0x8003;
const SALT_VEG: u32 = 0x9001;

// ---------------------------------------------------------------- hash

#[inline]
fn mix(mut h: u64) -> u64 {
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 31)
}

#[inline]
fn hash4(seed: u32, a: i32, b: i32, salt: u32) -> u32 {
    let mut h = mix((seed as u64) ^ ((salt as u64) << 32) ^ 0x9E37_79B9_7F4A_7C15);
    h = mix(h ^ (a as u32 as u64));
    h = mix(h ^ ((b as u32 as u64) << 32) ^ 0x1234_5678);
    (h >> 32) as u32
}

#[inline]
fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / 16_777_216.0
}

// ---------------------------------------------------------------- árvores

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    Pine,
    Oak,
    Palm,
    Coconut,
    Cactus,
    /// Arbusto de bagas (1 bloco).
    Bush,
}

/// `(x, z)` em mundo; `y` = altura da superfície (o primeiro bloco da planta fica em `y + 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreePlacement {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub kind: TreeKind,
    /// Variante determinística (altura, frutos...).
    pub variant: u32,
}

fn is_leaf(b: BlockType) -> bool {
    matches!(b, BlockType::Leaves | BlockType::PineLeaves | BlockType::PalmLeaves)
}

fn excluded(x: i32, z: i32, structs: &[Structure]) -> bool {
    structs.iter().any(|s| {
        let r = s.kind.radius() + 3;
        (x - s.x).abs() <= r && (z - s.z).abs() <= r
    })
}

/// Posições de plantas ancoradas neste chunk (determinístico por seed+chunk).
pub fn tree_positions_in_chunk(seed: u32, cx: i32, cz: i32, sampler: &dyn ColumnSampler) -> Vec<TreePlacement> {
    let structs = structures_in_area(seed, cx * CS, cz * CS, cx * CS + CS - 1, cz * CS + CS - 1, sampler);
    tree_positions_with(seed, cx, cz, sampler, &structs)
}

fn tree_positions_with(seed: u32, cx: i32, cz: i32, sampler: &dyn ColumnSampler, structs: &[Structure]) -> Vec<TreePlacement> {
    let mut out = Vec::new();
    for gz in 0..(CS / CELL) {
        for gx in 0..(CS / CELL) {
            let cell_x = cx * CS + gx * CELL;
            let cell_z = cz * CS + gz * CELL;
            let hp = hash4(seed, cell_x, cell_z, SALT_TREE_POS);
            let x = cell_x + (hp & 7) as i32;
            let z = cell_z + ((hp >> 3) & 7) as i32;
            let roll = unit(hash4(seed, cell_x, cell_z, SALT_TREE_ROLL));
            let kroll = unit(hash4(seed, cell_x, cell_z, SALT_TREE_KIND));
            let variant = hash4(seed, cell_x, cell_z, SALT_TREE_VAR);

            let h = sampler.height_at(x, z);
            if h <= SEA_LEVEL || h >= SNOW_LINE || h > CHUNK_HEIGHT as i32 - 30 {
                continue;
            }
            let kind = match sampler.biome_at(x, z) {
                Biome::Forest => {
                    if roll < 0.80 {
                        if kroll < 0.8 { TreeKind::Pine } else { TreeKind::Oak }
                    } else if roll < 0.92 {
                        TreeKind::Bush
                    } else {
                        continue;
                    }
                }
                Biome::Plains => {
                    if roll < 0.035 { TreeKind::Oak } else { continue }
                }
                Biome::Mountains => {
                    if h < 100 && roll < 0.18 { TreeKind::Pine } else { continue }
                }
                Biome::Beach => {
                    if roll < 0.12 {
                        if kroll < 0.55 { TreeKind::Palm } else { TreeKind::Coconut }
                    } else {
                        continue;
                    }
                }
                Biome::Desert => {
                    if roll < 0.05 { TreeKind::Cactus } else { continue }
                }
                Biome::Ocean => continue,
            };
            if excluded(x, z, structs) {
                continue;
            }
            out.push(TreePlacement { x, y: h, z, kind, variant });
        }
    }
    out
}

/// Todos os blocos (mundo) de uma planta. Troncos primeiro, copa depois não é garantido
/// aqui: `decorate_chunk` aplica em duas passadas (não-folhas, depois folhas).
pub fn tree_blocks(p: &TreePlacement, out: &mut Vec<(i32, i32, i32, BlockType)>) {
    let v = p.variant;
    let (x, y, z) = (p.x, p.y, p.z);
    match p.kind {
        TreeKind::Pine => {
            let hgt = 9 + (v % 6) as i32;
            for dy in 1..=hgt {
                out.push((x, y + dy, z, BlockType::Wood));
            }
            let top = y + hgt + 1;
            for ly in (y + 4)..=top {
                let d = top - ly;
                let mut r = if d == 0 { 0 } else { ((d + 1) / 2).min(3) };
                if d >= 3 && d % 2 == 1 {
                    r -= 1;
                }
                let lim = r * r + if r >= 1 { 1 } else { 0 };
                for dz in -r..=r {
                    for dx in -r..=r {
                        if dx * dx + dz * dz <= lim {
                            out.push((x + dx, ly, z + dz, BlockType::PineLeaves));
                        }
                    }
                }
            }
        }
        TreeKind::Oak => {
            let hgt = 4 + (v % 3) as i32;
            for dy in 1..=hgt {
                out.push((x, y + dy, z, BlockType::Wood));
            }
            let top = y + hgt;
            for (ly, lim) in [(top - 2, 5), (top - 1, 5), (top, 2), (top + 1, 1)] {
                for dz in -2..=2 {
                    for dx in -2..=2 {
                        if dx * dx + dz * dz <= lim {
                            out.push((x + dx, ly, z + dz, BlockType::Leaves));
                        }
                    }
                }
            }
        }
        TreeKind::Palm | TreeKind::Coconut => {
            let hgt = 5 + (v % 4) as i32;
            for dy in 1..=hgt {
                out.push((x, y + dy, z, BlockType::Wood));
            }
            let top = y + hgt;
            if p.kind == TreeKind::Coconut {
                out.push((x + 1, top, z, BlockType::Coconut));
                if v & 0x10 != 0 {
                    out.push((x, top, z - 1, BlockType::Coconut));
                }
            }
            out.push((x, top + 1, z, BlockType::PalmLeaves));
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                out.push((x + dx, top + 1, z + dz, BlockType::PalmLeaves));
                out.push((x + 2 * dx, top + 1, z + 2 * dz, BlockType::PalmLeaves));
                out.push((x + 3 * dx, top, z + 3 * dz, BlockType::PalmLeaves));
            }
            for (dx, dz) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
                out.push((x + dx, top + 1, z + dz, BlockType::PalmLeaves));
                out.push((x + 2 * dx, top, z + 2 * dz, BlockType::PalmLeaves));
            }
        }
        TreeKind::Cactus => {
            let hgt = 2 + (v % 3) as i32;
            for dy in 1..=hgt {
                out.push((x, y + dy, z, BlockType::Cactus));
            }
            if (v >> 8) % 4 == 0 {
                out.push((x, y + hgt + 1, z, BlockType::CactusFruit));
            }
        }
        TreeKind::Bush => {
            let b = if v & 1 == 0 { BlockType::BerryBushRed } else { BlockType::BerryBushBlue };
            out.push((x, y + 1, z, b));
        }
    }
}

// ---------------------------------------------------------------- estruturas

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureKind {
    House,
    Mill,
    FisherHut,
    Oasis,
}

impl StructureKind {
    /// Alcance horizontal máximo dos blocos a partir da âncora.
    pub fn radius(self) -> i32 {
        match self {
            StructureKind::House => 4,
            StructureKind::Mill => 5,
            StructureKind::FisherHut => 3,
            StructureKind::Oasis => 6,
        }
    }
    fn flat_radius(self) -> i32 {
        match self {
            StructureKind::Oasis => 6,
            _ => 3,
        }
    }
    fn biome(self) -> Biome {
        match self {
            StructureKind::House | StructureKind::Mill => Biome::Plains,
            StructureKind::FisherHut => Biome::Beach,
            StructureKind::Oasis => Biome::Desert,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Structure {
    pub kind: StructureKind,
    pub x: i32,
    pub z: i32,
    /// Altura do piso (superfície nivelada).
    pub base_y: i32,
    pub rot: u8,
}

/// Bloco de estrutura em coordenadas de mundo; `force` sobrescreve qualquer bloco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blk {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub b: BlockType,
    pub force: bool,
}

/// Estruturas cuja âncora cai a até `MAX_STRUCT_REACH` do retângulo mundo dado.
pub fn structures_in_area(seed: u32, min_x: i32, min_z: i32, max_x: i32, max_z: i32, sampler: &dyn ColumnSampler) -> Vec<Structure> {
    let (x0, z0) = (min_x - MAX_STRUCT_REACH, min_z - MAX_STRUCT_REACH);
    let (x1, z1) = (max_x + MAX_STRUCT_REACH, max_z + MAX_STRUCT_REACH);
    let mut out = Vec::new();
    for qz in z0.div_euclid(STRUCT_CELL)..=z1.div_euclid(STRUCT_CELL) {
        for qx in x0.div_euclid(STRUCT_CELL)..=x1.div_euclid(STRUCT_CELL) {
            let hp = hash4(seed, qx, qz, SALT_STRUCT_POS);
            let ax = qx * STRUCT_CELL + 8 + (hp % 48) as i32;
            let az = qz * STRUCT_CELL + 8 + ((hp >> 8) % 48) as i32;
            if ax < x0 || ax > x1 || az < z0 || az > z1 {
                continue;
            }
            let roll = unit(hash4(seed, qx, qz, SALT_STRUCT_ROLL));
            let kroll = unit(hash4(seed, qx, qz, SALT_STRUCT_KIND));
            let rot = ((hp >> 16) & 3) as u8;
            let kind = match sampler.biome_at(ax, az) {
                Biome::Plains if roll < 0.30 => {
                    if kroll < 0.55 { StructureKind::House } else { StructureKind::Mill }
                }
                Biome::Beach if roll < 0.18 => StructureKind::FisherHut,
                Biome::Desert if roll < 0.10 => StructureKind::Oasis,
                _ => continue,
            };
            if let Some(base_y) = validate_site(kind, ax, az, sampler) {
                out.push(Structure { kind, x: ax, z: az, base_y, rot });
            }
        }
    }
    out
}

fn validate_site(kind: StructureKind, ax: i32, az: i32, sampler: &dyn ColumnSampler) -> Option<i32> {
    let r = kind.flat_radius();
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for dz in -r..=r {
        for dx in -r..=r {
            let (x, z) = (ax + dx, az + dz);
            if sampler.biome_at(x, z) != kind.biome() {
                return None;
            }
            let h = sampler.height_at(x, z);
            if h <= SEA_LEVEL || h > CHUNK_HEIGHT as i32 - 30 {
                return None;
            }
            lo = lo.min(h);
            hi = hi.max(h);
        }
    }
    let tol = if kind == StructureKind::Oasis { 1 } else { 3 };
    if hi - lo > tol {
        return None;
    }
    Some(if kind == StructureKind::Oasis { sampler.height_at(ax, az) } else { hi })
}

#[inline]
fn rot(dx: i32, dz: i32, r: u8) -> (i32, i32) {
    match r & 3 {
        0 => (dx, dz),
        1 => (-dz, dx),
        2 => (-dx, -dz),
        _ => (dz, -dx),
    }
}

struct Em<'a> {
    s: &'a Structure,
    sampler: &'a dyn ColumnSampler,
    out: &'a mut Vec<Blk>,
}

impl Em<'_> {
    fn world(&self, dx: i32, dz: i32) -> (i32, i32) {
        let (rx, rz) = rot(dx, dz, self.s.rot);
        (self.s.x + rx, self.s.z + rz)
    }
    fn put(&mut self, dx: i32, y: i32, dz: i32, b: BlockType, force: bool) {
        let (x, z) = self.world(dx, dz);
        self.out.push(Blk { x, y, z, b, force });
    }
    /// Preenche do chão real até abaixo do piso.
    fn foundation(&mut self, dx: i32, dz: i32, b: BlockType) {
        let (x, z) = self.world(dx, dz);
        let h = self.sampler.height_at(x, z);
        for y in (h + 1)..self.s.base_y {
            self.out.push(Blk { x, y, z, b, force: true });
        }
    }
    fn roof(&mut self, y0: i32, r0: i32) {
        for k in 0..=r0 {
            let r = r0 - k;
            for dz in -r..=r {
                for dx in -r..=r {
                    self.put(dx, y0 + k, dz, BlockType::Roof, false);
                }
            }
        }
    }
}

pub fn structure_blocks(s: &Structure, sampler: &dyn ColumnSampler, out: &mut Vec<Blk>) {
    let by = s.base_y;
    let mut e = Em { s, sampler, out };
    match s.kind {
        StructureKind::House => {
            for dz in -3..=3i32 {
                for dx in -3..=3i32 {
                    e.foundation(dx, dz, BlockType::Cobblestone);
                    e.put(dx, by, dz, BlockType::Planks, true);
                    if dx.abs().max(dz.abs()) == 3 {
                        let corner = dx.abs() == 3 && dz.abs() == 3;
                        for k in 1..=3 {
                            let door = dx == 0 && dz == -3 && k <= 2;
                            let window = k == 2 && ((dx.abs() == 3 && dz == 0) || (dz == 3 && dx == 0));
                            if door || window {
                                continue;
                            }
                            e.put(dx, by + k, dz, if corner { BlockType::Wood } else { BlockType::Planks }, false);
                        }
                    }
                }
            }
            e.roof(by + 4, 4);
        }
        StructureKind::Mill => {
            for dz in -2..=2i32 {
                for dx in -2..=2i32 {
                    e.foundation(dx, dz, BlockType::Cobblestone);
                    e.put(dx, by, dz, BlockType::Cobblestone, true);
                    if dx.abs().max(dz.abs()) == 2 {
                        for k in 1..=6 {
                            let door = dx == 0 && dz == -2 && k <= 2;
                            let window = k == 4 && dz == 0 && dx.abs() == 2;
                            if door || window {
                                continue;
                            }
                            e.put(dx, by + k, dz, BlockType::Cobblestone, false);
                        }
                    }
                }
            }
            e.roof(by + 7, 3);
            // Pás no lado +z (oposto à porta).
            let hub = by + 6;
            e.put(0, hub, 3, BlockType::Wood, false);
            for k in 1..=4 {
                e.put(k, hub, 3, BlockType::Wood, false);
                e.put(-k, hub, 3, BlockType::Wood, false);
                e.put(0, hub + k, 3, BlockType::Wood, false);
                e.put(0, hub - k, 3, BlockType::Wood, false);
                if k >= 2 {
                    e.put(k, hub + 1, 3, BlockType::Planks, false);
                    e.put(-k, hub - 1, 3, BlockType::Planks, false);
                    e.put(1, hub + k, 3, BlockType::Planks, false);
                    e.put(-1, hub - k, 3, BlockType::Planks, false);
                }
            }
        }
        StructureKind::FisherHut => {
            for dz in -2..=2i32 {
                for dx in -2..=2i32 {
                    e.foundation(dx, dz, BlockType::Wood);
                    e.put(dx, by, dz, BlockType::Planks, true);
                    if dx.abs().max(dz.abs()) == 2 {
                        let corner = dx.abs() == 2 && dz.abs() == 2;
                        for k in 1..=3 {
                            let door = dx == 0 && dz == -2 && k <= 2;
                            let window = k == 2 && dz == 0 && dx.abs() == 2;
                            if door || window {
                                continue;
                            }
                            e.put(dx, by + k, dz, if corner { BlockType::Wood } else { BlockType::Planks }, false);
                        }
                    }
                }
            }
            e.roof(by + 4, 3);
        }
        StructureKind::Oasis => {
            for dz in -6..=6i32 {
                for dx in -6..=6i32 {
                    let d2 = dx * dx + dz * dz;
                    if d2 > 20 {
                        continue;
                    }
                    let (wx, wz) = e.world(dx, dz);
                    let h = sampler.height_at(wx, wz);
                    for y in (by + 1)..=h {
                        e.put(dx, y, dz, BlockType::Air, true);
                    }
                    for y in (h + 1)..by {
                        e.put(dx, y, dz, BlockType::Sand, true);
                    }
                    if d2 <= 9 {
                        e.put(dx, by, dz, BlockType::Water, true);
                        e.put(dx, by - 1, dz, if d2 <= 4 { BlockType::Water } else { BlockType::Sand }, true);
                    } else if d2 >= 10 {
                        e.put(dx, by, dz, BlockType::Grass, true);
                    }
                }
            }
            let spots = [(4, 1), (-4, -2), (1, -4), (-2, 4)];
            let hv = hash4(0xA51, s.x, s.z, 7);
            let n = 2 + (hv % 3) as usize;
            for i in 0..n {
                let (dx, dz) = spots[(i + (hv >> 4) as usize) % 4];
                let (wx, wz) = e.world(dx, dz);
                let kind = if (hv >> (8 + i)) & 1 == 0 { TreeKind::Palm } else { TreeKind::Coconut };
                let p = TreePlacement { x: wx, y: by, z: wz, kind, variant: hash4(hv, wx, wz, 3) };
                let mut tb = Vec::new();
                tree_blocks(&p, &mut tb);
                for (x, y, z, b) in tb {
                    e.out.push(Blk { x, y, z, b, force: false });
                }
            }
        }
    }
}

// ---------------------------------------------------------------- aplicação no chunk

#[inline]
fn put_world(chunk: &mut Chunk, ox: i32, oz: i32, x: i32, y: i32, z: i32, b: BlockType, force: bool) {
    let (lx, lz) = (x - ox, z - oz);
    if lx < 0 || lx >= CS || lz < 0 || lz >= CS || y < 1 || y >= CHUNK_HEIGHT as i32 {
        return;
    }
    let (lx, ly, lz) = (lx as usize, y as usize, lz as usize);
    if force || chunk.get(lx, ly, lz) == BlockType::Air {
        chunk.set(lx, ly, lz, b);
    }
}

/// Coloca no chunk: estruturas, plantas de todos os chunks a ±2 e vegetação de chão.
/// O terreno do chunk deve já estar gerado (só Air é sobrescrito, exceto pisos/oásis).
pub fn decorate_chunk(chunk: &mut Chunk, seed: u32, sampler: &dyn ColumnSampler) {
    let (cx, cz) = (chunk.position.x, chunk.position.z);
    let (ox, oz) = (cx * CS, cz * CS);

    let structs = structures_in_area(seed, ox - 2 * CS, oz - 2 * CS, ox + 3 * CS - 1, oz + 3 * CS - 1, sampler);

    // 1) estruturas
    let mut sb = Vec::new();
    for s in &structs {
        let r = s.kind.radius();
        if s.x + r < ox || s.x - r >= ox + CS || s.z + r < oz || s.z - r >= oz + CS {
            continue;
        }
        sb.clear();
        structure_blocks(s, sampler, &mut sb);
        for b in &sb {
            put_world(chunk, ox, oz, b.x, b.y, b.z, b.b, b.force);
        }
    }

    // 2) plantas dos chunks vizinhos (ordem fixa; passada 1 = troncos, passada 2 = folhas)
    let mut blocks = Vec::new();
    for dz in -2..=2 {
        for dx in -2..=2 {
            for p in tree_positions_with(seed, cx + dx, cz + dz, sampler, &structs) {
                if p.x < ox - 5 || p.x >= ox + CS + 5 || p.z < oz - 5 || p.z >= oz + CS + 5 {
                    continue;
                }
                tree_blocks(&p, &mut blocks);
            }
        }
    }
    for &(x, y, z, b) in blocks.iter().filter(|t| !is_leaf(t.3)) {
        put_world(chunk, ox, oz, x, y, z, b, false);
    }
    for &(x, y, z, b) in blocks.iter().filter(|t| is_leaf(t.3)) {
        put_world(chunk, ox, oz, x, y, z, b, false);
    }

    // 3) vegetação de chão
    for lz in 0..CHUNK_SIZE {
        for lx in 0..CHUNK_SIZE {
            let (wx, wz) = (ox + lx as i32, oz + lz as i32);
            let h = sampler.height_at(wx, wz);
            if h <= SEA_LEVEL || h + 1 >= CHUNK_HEIGHT as i32 {
                continue;
            }
            let hy = h as usize;
            if chunk.get(lx, hy + 1, lz) != BlockType::Air {
                continue;
            }
            let ground = chunk.get(lx, hy, lz);
            let hv = hash4(seed, wx, wz, SALT_VEG);
            let r = unit(hv);
            let flower = if hv & 1 == 0 { BlockType::FlowerRed } else { BlockType::FlowerYellow };
            let put = match (sampler.biome_at(wx, wz), ground) {
                (Biome::Plains, BlockType::Grass) if r < 0.05 => Some(flower),
                (Biome::Forest, BlockType::Grass) if r < 0.012 => Some(flower),
                (Biome::Desert, BlockType::Sand) if r < 0.008 => Some(BlockType::DeadBush),
                (Biome::Beach, BlockType::Sand) if r < 0.012 => {
                    Some(if r < 0.004 { BlockType::Cobblestone } else { BlockType::Gravel })
                }
                _ => None,
            };
            if let Some(b) = put {
                chunk.set(lx, hy + 1, lz, b);
            }
        }
    }
}

// ---------------------------------------------------------------- testes

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::ChunkPos;
    use std::collections::HashMap;

    struct Test {
        f: Box<dyn Fn(i32, i32) -> (i32, Biome)>,
    }
    impl ColumnSampler for Test {
        fn height_at(&self, x: i32, z: i32) -> i32 { (self.f)(x, z).0 }
        fn biome_at(&self, x: i32, z: i32) -> Biome { (self.f)(x, z).1 }
    }
    fn flat(h: i32, b: Biome) -> Test {
        Test { f: Box::new(move |_, _| (h, b)) }
    }

    fn make_chunk(cx: i32, cz: i32, seed: u32, s: &Test) -> Chunk {
        let mut c = Chunk::new(ChunkPos::new(cx, cz));
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let (wx, wz) = (cx * CS + lx as i32, cz * CS + lz as i32);
                let h = s.height_at(wx, wz);
                let top = match s.biome_at(wx, wz) {
                    Biome::Desert | Biome::Beach => BlockType::Sand,
                    _ => BlockType::Grass,
                };
                for y in 0..h as usize {
                    c.set(lx, y, lz, BlockType::Stone);
                }
                c.set(lx, h as usize, lz, top);
            }
        }
        decorate_chunk(&mut c, seed, s);
        c
    }

    fn world(map: &HashMap<(i32, i32), Chunk>, x: i32, y: i32, z: i32) -> BlockType {
        let c = map.get(&(x.div_euclid(CS), z.div_euclid(CS))).expect("chunk ausente");
        c.get(x.rem_euclid(CS) as usize, y as usize, z.rem_euclid(CS) as usize)
    }

    fn gen_area(x0: i32, x1: i32, z0: i32, z1: i32, seed: u32, s: &Test) -> HashMap<(i32, i32), Chunk> {
        let mut m = HashMap::new();
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                m.insert((cx, cz), make_chunk(cx, cz, seed, s));
            }
        }
        m
    }

    #[test]
    fn deterministic() {
        let s = flat(70, Biome::Forest);
        assert_eq!(tree_positions_in_chunk(42, 3, -2, &s), tree_positions_in_chunk(42, 3, -2, &s));
        let a = make_chunk(1, 1, 42, &s);
        let b = make_chunk(1, 1, 42, &s);
        assert!(a.blocks == b.blocks);
        assert!(!tree_positions_in_chunk(42, 3, -2, &s).is_empty());
        assert_ne!(tree_positions_in_chunk(42, 3, -2, &s), tree_positions_in_chunk(43, 3, -2, &s));
    }

    #[test]
    fn cross_chunk_tree_is_continuous() {
        let s = flat(70, Biome::Forest);
        let seed = 7;
        let mut found = None;
        'o: for cx in -4..4 {
            for p in tree_positions_in_chunk(seed, cx, 0, &s) {
                if p.kind == TreeKind::Pine && p.x.rem_euclid(CS) >= 30 {
                    found = Some((cx, p));
                    break 'o;
                }
            }
        }
        let (cx, p) = found.expect("pinheiro na borda");
        let map = gen_area(cx - 1, cx + 2, -1, 1, seed, &s);
        let mut tb = Vec::new();
        tree_blocks(&p, &mut tb);
        let (mut in_a, mut in_b) = (0, 0);
        for &(x, y, z, b) in &tb {
            let got = world(&map, x, y, z);
            if is_leaf(b) {
                assert!(got != BlockType::Air, "buraco de folha em {x},{y},{z}");
            } else {
                assert_eq!(got, b, "tronco em {x},{y},{z}");
            }
            match x.div_euclid(CS) {
                c if c == cx => in_a += 1,
                c if c == cx + 1 => in_b += 1,
                _ => {}
            }
        }
        assert!(in_a > 0 && in_b > 0, "árvore deve aparecer nos dois chunks ({in_a},{in_b})");
    }

    #[test]
    fn nothing_in_ocean_or_underwater() {
        for s in [flat(50, Biome::Ocean), flat(60, Biome::Plains), flat(70, Biome::Ocean)] {
            for cx in -3..3 {
                assert!(tree_positions_in_chunk(1, cx, 0, &s).is_empty());
            }
            let c = make_chunk(0, 0, 1, &s);
            let non_terrain = (0..CHUNK_SIZE).all(|x| (0..CHUNK_SIZE).all(|z| {
                (s.height_at(0, 0) as usize + 1..CHUNK_HEIGHT).all(|y| c.get(x, y, z) == BlockType::Air)
            }));
            assert!(non_terrain);
        }
    }

    #[test]
    fn no_trees_on_high_snow() {
        let s = flat(150, Biome::Mountains);
        for cx in -3..3 {
            assert!(tree_positions_in_chunk(1, cx, 0, &s).is_empty());
        }
    }

    #[test]
    fn cactus_only_in_desert() {
        let d = flat(70, Biome::Desert);
        let mut n = 0;
        for cx in -5..5 {
            for p in tree_positions_in_chunk(9, cx, 0, &d) {
                assert_eq!(p.kind, TreeKind::Cactus);
                n += 1;
            }
        }
        assert!(n > 0);
        for b in [Biome::Plains, Biome::Forest, Biome::Mountains, Biome::Beach] {
            let s = flat(70, b);
            for cx in -5..5 {
                assert!(tree_positions_in_chunk(9, cx, 0, &s).iter().all(|p| p.kind != TreeKind::Cactus));
            }
        }
    }

    #[test]
    fn beach_has_palms_and_coconuts() {
        let s = flat(65, Biome::Beach);
        let mut kinds = std::collections::HashSet::new();
        for cx in -8..8 {
            for p in tree_positions_in_chunk(3, cx, 0, &s) {
                assert!(matches!(p.kind, TreeKind::Palm | TreeKind::Coconut));
                kinds.insert(p.kind as u8);
            }
        }
        assert_eq!(kinds.len(), 2);
    }

    #[test]
    fn forest_denser_than_plains() {
        let f = flat(70, Biome::Forest);
        let p = flat(70, Biome::Plains);
        let nf: usize = (0..8).map(|c| tree_positions_in_chunk(5, c, 0, &f).len()).sum();
        let np: usize = (0..8).map(|c| tree_positions_in_chunk(5, c, 0, &p).len()).sum();
        assert!(nf > np * 5);
    }

    #[test]
    fn ground_vegetation() {
        let count = |b: Biome, want: &[BlockType]| -> usize {
            let s = flat(70, b);
            let c = make_chunk(0, 0, 11, &s);
            (0..CHUNK_SIZE).flat_map(|x| (0..CHUNK_SIZE).map(move |z| (x, z)))
                .filter(|&(x, z)| want.contains(&c.get(x, 71, z))).count()
        };
        assert!(count(Biome::Plains, &[BlockType::FlowerRed, BlockType::FlowerYellow]) > 0);
        assert!(count(Biome::Desert, &[BlockType::DeadBush]) > 0 || {
            // seed pode não ter nenhum num chunk: procura em outro
            let s = flat(70, Biome::Desert);
            (1..6).any(|k| {
                let c = make_chunk(k, 0, 11, &s);
                (0..CHUNK_SIZE).any(|x| (0..CHUNK_SIZE).any(|z| c.get(x, 71, z) == BlockType::DeadBush))
            })
        });
        let beach = flat(70, Biome::Beach);
        assert!((0..6).any(|k| {
            let c = make_chunk(k, 0, 11, &beach);
            (0..CHUNK_SIZE).any(|x| (0..CHUNK_SIZE).any(|z| matches!(c.get(x, 71, z), BlockType::Gravel | BlockType::Cobblestone)))
        }));
    }

    /// Verifica que todos os blocos de uma estrutura estão presentes na união dos chunks.
    fn check_structure(s: &Structure, sampler: &Test, seed: u32) {
        let (acx, acz) = (s.x.div_euclid(CS), s.z.div_euclid(CS));
        let map = gen_area(acx - 1, acx + 1, acz - 1, acz + 1, seed, sampler);
        let mut sb = Vec::new();
        structure_blocks(s, sampler, &mut sb);
        assert!(sb.len() > 20);
        for b in sb {
            let got = world(&map, b.x, b.y, b.z);
            if b.force && b.b != BlockType::Air {
                assert_eq!(got, b.b, "{:?} em {},{},{}", s.kind, b.x, b.y, b.z);
            } else if !b.force {
                assert!(got != BlockType::Air, "{:?} faltando {:?} em {},{},{}", s.kind, b.b, b.x, b.y, b.z);
            }
        }
    }

    fn find_structures(sampler: &Test, seed: u32) -> Vec<Structure> {
        structures_in_area(seed, -1024, -1024, 1024, 1024, sampler)
    }

    #[test]
    fn plains_structures_present_across_chunks() {
        let s = flat(70, Biome::Plains);
        let seed = 21;
        let all = find_structures(&s, seed);
        assert!(all.iter().any(|x| x.kind == StructureKind::House));
        assert!(all.iter().any(|x| x.kind == StructureKind::Mill));
        for kind in [StructureKind::House, StructureKind::Mill] {
            let mut checked = 0;
            for st in all.iter().filter(|x| x.kind == kind).take(6) {
                check_structure(st, &s, seed);
                checked += 1;
            }
            assert!(checked > 0);
        }
        // Pelo menos uma estrutura cruza a borda de chunk (alcance maior que a distância à borda).
        let crossing = all.iter().any(|st| {
            let lx = st.x.rem_euclid(CS);
            let lz = st.z.rem_euclid(CS);
            let r = st.kind.radius();
            lx < r || lx >= CS - r || lz < r || lz >= CS - r
        });
        assert!(crossing);
    }

    #[test]
    fn border_structure_is_seamless() {
        let s = flat(70, Biome::Plains);
        let mut n = 0;
        for seed in 0..40 {
            for st in find_structures(&s, seed) {
                let lx = st.x.rem_euclid(CS);
                let r = st.kind.radius();
                if lx < 2 || lx >= CS - 2 {
                    assert!(r >= 3);
                    check_structure(&st, &s, seed);
                    n += 1;
                }
            }
        }
        assert!(n > 0, "nenhuma estrutura na borda encontrada");
    }

    #[test]
    fn structures_respect_biome_and_terrain() {
        // Oceano e terreno íngreme: nenhuma estrutura.
        assert!(find_structures(&flat(70, Biome::Ocean), 3).is_empty());
        let steep = Test { f: Box::new(|x, _| (70 + (x.rem_euclid(2)) * 8, Biome::Plains)) };
        assert!(find_structures(&steep, 3).is_empty());
        // Casa de pescador só em praia, oásis só em deserto.
        assert!(find_structures(&flat(65, Biome::Beach), 3).iter().all(|x| x.kind == StructureKind::FisherHut));
        assert!(find_structures(&flat(70, Biome::Desert), 3).iter().all(|x| x.kind == StructureKind::Oasis));
        assert!(!find_structures(&flat(65, Biome::Beach), 3).is_empty());
        assert!(!find_structures(&flat(70, Biome::Desert), 3).is_empty());
    }

    #[test]
    fn fisher_hut_and_oasis_built() {
        let b = flat(65, Biome::Beach);
        let st = find_structures(&b, 5).into_iter().next().unwrap();
        check_structure(&st, &b, 5);
        let d = flat(70, Biome::Desert);
        let st = find_structures(&d, 5).into_iter().next().unwrap();
        check_structure(&st, &d, 5);
        let map = gen_area(st.x.div_euclid(CS) - 1, st.x.div_euclid(CS) + 1, st.z.div_euclid(CS) - 1, st.z.div_euclid(CS) + 1, 5, &d);
        assert_eq!(world(&map, st.x, st.base_y, st.z), BlockType::Water);
        let palms = (-6..=6).flat_map(|dx| (-6..=6).map(move |dz| (dx, dz)))
            .filter(|&(dx, dz)| (1..14).any(|dy| world(&map, st.x + dx, st.base_y + dy, st.z + dz) == BlockType::PalmLeaves))
            .count();
        assert!(palms > 0);
    }

    #[test]
    fn no_trees_inside_structures() {
        let s = flat(70, Biome::Plains);
        let seed = 21;
        for st in find_structures(&s, seed).into_iter().take(10) {
            let r = st.kind.radius() + 3;
            for cx in (st.x - 40).div_euclid(CS)..=(st.x + 40).div_euclid(CS) {
                for cz in (st.z - 40).div_euclid(CS)..=(st.z + 40).div_euclid(CS) {
                    for p in tree_positions_in_chunk(seed, cx, cz, &s) {
                        assert!(!((p.x - st.x).abs() <= r && (p.z - st.z).abs() <= r));
                    }
                }
            }
        }
    }
}
