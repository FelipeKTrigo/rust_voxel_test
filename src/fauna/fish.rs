//! Cardumes: separação + alinhamento + coesão, sempre dentro d'água.
use std::collections::{HashMap, HashSet};

use glam::Vec3;

use crate::fauna::{hash3, unit};
use crate::physics::BlockQuery;
use crate::world::chunk::{ChunkPos, CHUNK_SIZE};
use crate::world::gen::types::SEA_LEVEL;
use crate::world::{BlockType, ChunkManager};

pub const SPAWN_RADIUS_CHUNKS: i32 = 5;
pub const MAX_FISH: usize = 2048;
const MIN_SPEED: f32 = 1.0;
const MAX_SPEED: f32 = 2.4;

#[derive(Clone, Debug)]
pub struct Fish {
    pub pos: Vec3,
    pub vel: Vec3,
    pub phase: f32,
    pub tint: f32,
}

fn is_water(world: &dyn BlockQuery, p: Vec3) -> bool {
    world.block_at(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32) == BlockType::Water
}

/// Um passo de simulação do cardume. Peixes nunca saem da água: se o próximo ponto
/// não é água, o peixe vira (e não se move neste passo).
pub fn step_school(fish: &mut [Fish], dt: f32, time: f32, world: &dyn BlockQuery) {
    let snapshot: Vec<(Vec3, Vec3)> = fish.iter().map(|f| (f.pos, f.vel)).collect();
    for (i, f) in fish.iter_mut().enumerate() {
        let (mut sep, mut ali, mut coh) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
        let mut n = 0.0;
        for (j, (p, v)) in snapshot.iter().enumerate() {
            if i == j { continue; }
            let d = f.pos - *p;
            let dist = d.length();
            if dist < 7.0 {
                n += 1.0;
                ali += *v;
                coh += *p;
                if dist < 1.3 && dist > 1e-4 {
                    sep += d / (dist * dist);
                }
            }
        }
        let mut steer = Vec3::ZERO;
        if n > 0.0 {
            steer += (ali / n - f.vel) * 0.8;
            steer += (coh / n - f.pos) * 0.35;
        }
        steer += sep * 2.5;
        // Passeio suave para o cardume não ficar parado
        let w = time * 0.5 + f.phase;
        steer += Vec3::new(w.sin(), 0.15 * (w * 1.7).sin(), (w * 1.3).cos()) * 0.6;

        f.vel += steer * dt;
        let sp = f.vel.length();
        if sp > MAX_SPEED {
            f.vel *= MAX_SPEED / sp;
        } else if sp < MIN_SPEED {
            f.vel = if sp > 1e-4 { f.vel / sp * MIN_SPEED } else { Vec3::new(MIN_SPEED, 0.0, 0.0) };
        }
        f.vel.y = f.vel.y.clamp(-0.6, 0.6);

        // Antecipa 1.2 blocos à frente para virar antes de bater na margem.
        let step = f.vel * dt;
        let ahead = f.pos + f.vel.normalize_or_zero() * 1.2;
        if is_water(world, f.pos + step) && is_water(world, ahead) {
            f.pos += step;
        } else {
            // Gira: inverte o horizontal e tenta mergulhar/subir para o meio da coluna d'água.
            f.vel = Vec3::new(-f.vel.x, -f.vel.y * 0.5, -f.vel.z);
            let p = f.pos + f.vel * dt;
            if is_water(world, p) {
                f.pos = p;
            }
        }
        // Nunca fica na superfície (evita atravessar a onda)
        let top = SEA_LEVEL as f32 + 1.0 - 0.9;
        if f.pos.y > top {
            f.pos.y = top;
            f.vel.y = -f.vel.y.abs();
        }
        // Velocidade sempre dentro dos limites (a curva/superfície acima pode ter reduzido).
        let sp = f.vel.length();
        if sp > MAX_SPEED {
            f.vel *= MAX_SPEED / sp;
        } else if sp < MIN_SPEED {
            f.vel = if sp > 1e-4 { f.vel / sp * MIN_SPEED } else { Vec3::new(MIN_SPEED, 0.0, 0.0) };
        }
    }
}

pub struct FishSystem {
    pub schools: HashMap<ChunkPos, Vec<Fish>>,
    tried: HashSet<ChunkPos>,
}

/// Cria um cardume para o chunk (determinístico por seed+chunk). `None` se o local não serve.
pub fn spawn_school(seed: u32, pos: ChunkPos, cm: &ChunkManager) -> Option<Vec<Fish>> {
    if unit(hash3(seed, pos, 1)) > 0.55 {
        return None;
    }
    let s = CHUNK_SIZE as i32;
    let lx = 6 + (unit(hash3(seed, pos, 2)) * (s - 12) as f32) as i32;
    let lz = 6 + (unit(hash3(seed, pos, 3)) * (s - 12) as f32) as i32;
    let (wx, wz) = (pos.x * s + lx, pos.z * s + lz);
    if cm.block_at(wx, SEA_LEVEL, wz) != BlockType::Water {
        return None;
    }
    let floor = cm.surface_height(wx, wz)?;
    let depth = SEA_LEVEL - floor;
    if depth < 5 {
        return None;
    }
    let y_min = floor as f32 + 2.5;
    let y_max = SEA_LEVEL as f32 - 1.5;
    let y = y_min + unit(hash3(seed, pos, 4)) * (y_max - y_min).max(0.0);
    let count = 5 + (unit(hash3(seed, pos, 5)) * 5.0) as usize;
    let center = Vec3::new(wx as f32 + 0.5, y, wz as f32 + 0.5);
    let fish: Vec<Fish> = (0..count as u32)
        .map(|i| {
            let a = unit(hash3(seed, pos, 10 + i)) * std::f32::consts::TAU;
            let r = unit(hash3(seed, pos, 40 + i)) * 2.0;
            let dy = (unit(hash3(seed, pos, 70 + i)) - 0.5) * 1.5;
            Fish {
                pos: center + Vec3::new(a.cos() * r, dy, a.sin() * r),
                vel: Vec3::new(a.sin(), 0.0, a.cos()) * 1.4,
                phase: unit(hash3(seed, pos, 100 + i)) * 6.28,
                tint: unit(hash3(seed, pos, 130 + i)),
            }
        })
        .collect();
    let fish: Vec<Fish> = fish.into_iter().filter(|f| is_water(cm, f.pos)).collect();
    (fish.len() >= 3).then_some(fish)
}

impl FishSystem {
    pub fn new() -> Self {
        Self { schools: HashMap::new(), tried: HashSet::new() }
    }

    pub fn count(&self) -> usize {
        self.schools.values().map(|s| s.len()).sum()
    }

    pub fn update(&mut self, dt: f32, time: f32, player: Vec3, cm: &ChunkManager, reduced: bool) {
        let pc = ChunkPos::from_world_pos(player.x, player.z);
        let r = SPAWN_RADIUS_CHUNKS;
        // Despawn: chunk descarregado ou longe demais
        self.schools.retain(|p, _| cm.chunks.contains_key(p) && (p.x - pc.x).abs() <= r + 1 && (p.z - pc.z).abs() <= r + 1);
        self.tried.retain(|p| (p.x - pc.x).abs() <= r + 1 && (p.z - pc.z).abs() <= r + 1);
        // Spawn: no máximo alguns chunks por quadro
        let mut budget = 4;
        'outer: for cx in pc.x - r..=pc.x + r {
            for cz in pc.z - r..=pc.z + r {
                let p = ChunkPos::new(cx, cz);
                if self.tried.contains(&p) || !cm.chunks.contains_key(&p) {
                    continue;
                }
                self.tried.insert(p);
                if let Some(school) = spawn_school(cm.seed, p, cm) {
                    self.schools.insert(p, school);
                }
                budget -= 1;
                if budget == 0 { break 'outer; }
            }
        }
        for school in self.schools.values_mut() {
            step_school(school, dt, time, cm);
        }
        let _ = reduced;
    }

    /// Instâncias para desenho. Com `reduced` (partículas reduzidas) mostra metade dos peixes.
    pub fn instances(&self, time: f32, reduced: bool) -> Vec<FishInstance> {
        let mut out = Vec::new();
        for school in self.schools.values() {
            for (i, f) in school.iter().enumerate() {
                if reduced && i % 2 == 1 { continue; }
                if out.len() >= MAX_FISH { return out; }
                let d = Vec3::new(f.vel.x, 0.0, f.vel.z).normalize_or_zero();
                out.push(FishInstance {
                    pos_size: [f.pos.x, f.pos.y, f.pos.z, 0.9],
                    dir: [d.x, d.z],
                    frame: ((time * 7.0 + f.phase * 3.0) as i32 & 1) as f32,
                    tint: f.tint,
                });
            }
        }
        out
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FishInstance {
    pub pos_size: [f32; 4],
    pub dir: [f32; 2],
    pub frame: f32,
    pub tint: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Caixa d'água 20x8x20 (x,z em 0..20; y em 0..8), resto é pedra.
    struct Box;
    impl BlockQuery for Box {
        fn block_at(&self, x: i32, y: i32, z: i32) -> BlockType {
            if (0..20).contains(&x) && (0..8).contains(&y) && (0..20).contains(&z) {
                BlockType::Water
            } else {
                BlockType::Stone
            }
        }
    }

    fn school() -> Vec<Fish> {
        (0..8)
            .map(|i| Fish {
                pos: Vec3::new(10.0 + (i as f32) * 0.3, 4.0, 10.0 - (i as f32) * 0.2),
                vel: Vec3::new(1.0 + i as f32 * 0.1, 0.0, -0.5),
                phase: i as f32,
                tint: 0.5,
            })
            .collect()
    }

    #[test]
    fn fish_stay_in_water() {
        let mut s = school();
        for step in 0..6000 {
            step_school(&mut s, 1.0 / 30.0, step as f32 / 30.0, &Box);
            for f in &s {
                assert!(is_water(&Box, f.pos), "peixe saiu da água: {:?} no passo {step}", f.pos);
                assert!(f.vel.is_finite() && f.pos.is_finite());
            }
        }
    }

    #[test]
    fn school_keeps_cohesion_and_speed_limits() {
        let mut s = school();
        for step in 0..1500 {
            step_school(&mut s, 1.0 / 30.0, step as f32 / 30.0, &Box);
        }
        for f in &s {
            let sp = f.vel.length();
            assert!(sp <= MAX_SPEED + 1e-3 && sp >= MIN_SPEED - 1e-3, "velocidade {sp}");
        }
        let c = s.iter().map(|f| f.pos).sum::<Vec3>() / s.len() as f32;
        assert!(s.iter().all(|f| f.pos.distance(c) < 18.0));
    }

    #[test]
    fn separation_pushes_apart() {
        let mut s = vec![
            Fish { pos: Vec3::new(10.0, 4.0, 10.0), vel: Vec3::X * 1.0, phase: 0.0, tint: 0.0 },
            Fish { pos: Vec3::new(10.2, 4.0, 10.0), vel: Vec3::X * 1.0, phase: 0.0, tint: 0.0 },
        ];
        let d0 = s[0].pos.distance(s[1].pos);
        for k in 0..30 {
            step_school(&mut s, 1.0 / 30.0, k as f32 / 30.0, &Box);
        }
        assert!(s[0].pos.distance(s[1].pos) > d0);
    }

    #[test]
    fn instances_respect_reduction() {
        let mut fs = FishSystem::new();
        fs.schools.insert(ChunkPos::new(0, 0), school());
        assert_eq!(fs.instances(0.0, false).len(), 8);
        assert_eq!(fs.instances(0.0, true).len(), 4);
    }
}
