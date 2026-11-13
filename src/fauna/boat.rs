//! Barcos de pescadores de rede: entidades estáticas (posição fixa) que balançam nas ondas.
use std::collections::{HashMap, HashSet};

use glam::{Mat3, Vec3};

use crate::fauna::{hash3, unit, wave_height};
use crate::renderer::VoxelVertex;
use crate::world::chunk::{ChunkPos, CHUNK_SIZE};
use crate::world::gen::types::SEA_LEVEL;
use crate::physics::BlockQuery;
use crate::world::{BlockType, ChunkManager, ColorPalette};

pub const BOAT_RADIUS_CHUNKS: i32 = 8;
/// Altura da superfície d'água (topo do bloco de água) sem onda.
pub const SURFACE_Y: f32 = SEA_LEVEL as f32 + 1.0 - 0.12;

#[derive(Clone, Debug)]
pub struct Boat {
    pub pos: Vec3, // x, y(base), z
    pub yaw: f32,
    pub phase: f32,
}

pub struct BoatSystem {
    pub boats: HashMap<ChunkPos, Boat>,
    tried: HashSet<ChunkPos>,
}

/// Caixa em espaço local: (min, max, cor).
type LocalBox = (Vec3, Vec3, [f32; 4]);

fn boat_boxes() -> Vec<LocalBox> {
    let v = Vec3::new;
    vec![
        // casco: fundo e laterais
        (v(-1.3, 0.0, -3.0), v(1.3, 0.4, 3.0), ColorPalette::BOAT_HULL),
        (v(-1.5, 0.4, -2.8), v(-1.1, 1.3, 2.8), ColorPalette::BOAT_HULL),
        (v(1.1, 0.4, -2.8), v(1.5, 1.3, 2.8), ColorPalette::BOAT_HULL),
        // proa e popa
        (v(-0.9, 0.4, 2.8), v(0.9, 1.3, 3.4), ColorPalette::BOAT_HULL),
        (v(-1.1, 0.4, -3.4), v(1.1, 1.3, -2.8), ColorPalette::BOAT_HULL),
        // convés
        (v(-1.1, 0.4, -2.8), v(1.1, 0.7, 2.8), ColorPalette::BOAT_DECK),
        // mastro e vela
        (v(-0.12, 0.7, 0.4), v(0.12, 4.6, 0.64), ColorPalette::BOAT_MAST),
        (v(-0.05, 1.6, 0.7), v(0.05, 4.3, 2.4), ColorPalette::BOAT_SAIL),
        // rede dobrada na popa
        (v(-0.9, 0.7, -2.6), v(0.9, 1.2, -1.2), ColorPalette::BOAT_NET),
    ]
}

/// Faces: (normal, 4 cantos como combinação de min/max) CCW visto de fora.
const FACES: [([f32; 3], [[usize; 3]; 4]); 6] = [
    ([1.0, 0.0, 0.0], [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]]),
    ([-1.0, 0.0, 0.0], [[0, 0, 1], [0, 1, 1], [0, 1, 0], [0, 0, 0]]),
    ([0.0, 1.0, 0.0], [[0, 1, 1], [1, 1, 1], [1, 1, 0], [0, 1, 0]]),
    ([0.0, -1.0, 0.0], [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]]),
    ([0.0, 0.0, 1.0], [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]]),
    ([0.0, 0.0, -1.0], [[1, 0, 0], [0, 0, 0], [0, 1, 0], [1, 1, 0]]),
];

/// Orientação do barco no instante `t`: guinada + balanço (roll) + arfagem (pitch).
pub fn boat_rotation(b: &Boat, t: f32) -> Mat3 {
    let roll = 0.07 * (t * 1.1 + b.phase).sin();
    let pitch = 0.05 * (t * 0.8 + b.phase * 1.3).sin();
    Mat3::from_rotation_y(b.yaw) * Mat3::from_rotation_z(roll) * Mat3::from_rotation_x(pitch)
}

pub fn boat_origin(b: &Boat, t: f32) -> Vec3 {
    Vec3::new(b.pos.x, SURFACE_Y - 0.25 + wave_height(b.pos.x, b.pos.z, t), b.pos.z)
}

/// Gera a malha (espaço de mundo) de todos os barcos em `t`.
pub fn build_mesh<'a>(boats: impl Iterator<Item = &'a Boat>, t: f32) -> (Vec<VoxelVertex>, Vec<u32>) {
    let boxes = boat_boxes();
    let mut vs = Vec::new();
    let mut is = Vec::new();
    for b in boats {
        let rot = boat_rotation(b, t);
        let origin = boat_origin(b, t);
        for (mn, mx, color) in &boxes {
            for (normal, corners) in FACES.iter() {
                let base = vs.len() as u32;
                let n = rot * Vec3::from_array(*normal);
                for c in corners {
                    let local = Vec3::new(
                        if c[0] == 1 { mx.x } else { mn.x },
                        if c[1] == 1 { mx.y } else { mn.y },
                        if c[2] == 1 { mx.z } else { mn.z },
                    );
                    let p = origin + rot * local;
                    vs.push(VoxelVertex { position: p.to_array(), color: *color, normal: n.to_array() });
                }
                is.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
    }
    (vs, is)
}

/// Posição determinística do barco do chunk, se o chunk for oceano profundo.
pub fn spawn_boat(seed: u32, pos: ChunkPos, cm: &ChunkManager) -> Option<Boat> {
    if unit(hash3(seed, pos, 201)) > 0.10 {
        return None;
    }
    let s = CHUNK_SIZE as i32;
    let lx = 6 + (unit(hash3(seed, pos, 202)) * (s - 12) as f32) as i32;
    let lz = 6 + (unit(hash3(seed, pos, 203)) * (s - 12) as f32) as i32;
    let (wx, wz) = (pos.x * s + lx, pos.z * s + lz);
    // Fundo profundo no centro e em volta (barco ocupa ~7x4)
    for (dx, dz) in [(0, 0), (4, 0), (-4, 0), (0, 4), (0, -4)] {
        if cm.block_at(wx + dx, SEA_LEVEL, wz + dz) != BlockType::Water {
            return None;
        }
        let h = cm.surface_height(wx + dx, wz + dz)?;
        if h > SEA_LEVEL - 4 {
            return None;
        }
    }
    Some(Boat {
        pos: Vec3::new(wx as f32 + 0.5, SURFACE_Y, wz as f32 + 0.5),
        yaw: unit(hash3(seed, pos, 204)) * std::f32::consts::TAU,
        phase: unit(hash3(seed, pos, 205)) * 6.28,
    })
}

impl BoatSystem {
    pub fn new() -> Self {
        Self { boats: HashMap::new(), tried: HashSet::new() }
    }

    pub fn update(&mut self, player: Vec3, cm: &ChunkManager) {
        let pc = ChunkPos::from_world_pos(player.x, player.z);
        let r = BOAT_RADIUS_CHUNKS;
        self.boats.retain(|p, _| cm.chunks.contains_key(p) && (p.x - pc.x).abs() <= r + 1 && (p.z - pc.z).abs() <= r + 1);
        self.tried.retain(|p| (p.x - pc.x).abs() <= r + 1 && (p.z - pc.z).abs() <= r + 1);
        let mut budget = 4;
        'outer: for cx in pc.x - r..=pc.x + r {
            for cz in pc.z - r..=pc.z + r {
                let p = ChunkPos::new(cx, cz);
                if self.tried.contains(&p) || !cm.chunks.contains_key(&p) {
                    continue;
                }
                self.tried.insert(p);
                if let Some(b) = spawn_boat(cm.seed, p, cm) {
                    self.boats.insert(p, b);
                }
                budget -= 1;
                if budget == 0 { break 'outer; }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boat() -> Boat {
        Boat { pos: Vec3::new(10.0, SURFACE_Y, -4.0), yaw: 0.7, phase: 1.0 }
    }

    #[test]
    fn mesh_is_well_formed() {
        let b = boat();
        let (vs, is) = build_mesh(std::iter::once(&b), 1.5);
        assert_eq!(vs.len(), boat_boxes().len() * 24);
        assert_eq!(is.len(), boat_boxes().len() * 36);
        assert!(is.iter().all(|&i| (i as usize) < vs.len()));
        assert!(vs.iter().all(|v| v.position.iter().all(|c| c.is_finite())));
    }

    #[test]
    fn faces_wind_outward() {
        // Sem rotação: o produto vetorial de cada triângulo deve apontar na direção da normal.
        let b = Boat { pos: Vec3::ZERO, yaw: 0.0, phase: 0.0 };
        let (vs, is) = build_mesh(std::iter::once(&b), 0.0);
        for tri in is.chunks(3) {
            let [a, b2, c] = [tri[0], tri[1], tri[2]].map(|i| Vec3::from_array(vs[i as usize].position));
            let n = Vec3::from_array(vs[tri[0] as usize].normal);
            let cr = (b2 - a).cross(c - a);
            assert!(cr.dot(n) > 0.0, "triângulo voltado para dentro");
        }
    }

    #[test]
    fn boat_rocks_over_time_and_follows_waves() {
        let b = boat();
        let r0 = boat_rotation(&b, 0.0);
        let r1 = boat_rotation(&b, 2.0);
        assert!((r0.to_cols_array()[0] - r1.to_cols_array()[0]).abs() > 1e-4 || r0 != r1);
        let y0 = boat_origin(&b, 0.0).y;
        let y1 = boat_origin(&b, 1.0).y;
        assert!(y0 != y1);
        assert!((y0 - SURFACE_Y).abs() < 0.5);
    }

    #[test]
    fn wave_bounded() {
        for i in 0..1000 {
            let w = wave_height(i as f32 * 0.37, i as f32 * 0.91, i as f32 * 0.1);
            assert!(w.abs() <= 0.12 + 1e-6);
        }
    }
}
