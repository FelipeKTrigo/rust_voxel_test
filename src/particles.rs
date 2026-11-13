//! Fase 9 — sistema de partículas simples (detritos ao quebrar blocos e fumaça de enxofre).
//! Desenhadas como quads voltados para a câmera pelo pipeline de voxels (via `DynMesh`).
//! A quantidade e o teto global são reduzidos pela opção "Reduzir partículas".
use glam::{IVec3, Vec3};

use crate::camera::Camera;
use crate::physics::BlockQuery;
use crate::renderer::mesh::VoxelVertex;
use crate::world::BlockType;

pub const MAX_PARTICLES: usize = 1500;
pub const MAX_PARTICLES_REDUCED: usize = 300;
const GRAVITY: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Debris,
    /// Dióxido de enxofre: sobe devagar, cresce e some.
    Smoke,
}

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: f32,
    pub life: f32,
    pub color: [f32; 4],
    pub size: f32,
    pub kind: Kind,
}

pub struct ParticleSystem {
    pub list: Vec<Particle>,
    rng: u32,
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleSystem {
    pub fn new() -> Self {
        Self { list: Vec::new(), rng: 0x9e37_79b9 }
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    pub fn cap(reduced: bool) -> usize {
        if reduced { MAX_PARTICLES_REDUCED } else { MAX_PARTICLES }
    }

    /// xorshift32 -> [0, 1)
    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn push(&mut self, p: Particle, reduced: bool) {
        let cap = Self::cap(reduced);
        if self.list.len() >= cap {
            // Descarta a mais antiga (a primeira) para abrir espaço.
            self.list.remove(0);
        }
        self.list.push(p);
    }

    /// Emite detritos (e fumaça, se for pirita) ao quebrar o bloco em `block_pos`.
    pub fn spawn_break(&mut self, block_pos: IVec3, block: BlockType, reduced: bool) {
        if block.is_air() || block == BlockType::Water {
            return;
        }
        let center = block_pos.as_vec3() + Vec3::splat(0.5);
        // Cor dos detritos = cor média da textura do bloco (minério quebrado solta lascas de pedra e veio)
        let avg = crate::world::textures::average_color(block, crate::world::textures::FaceKind::Side);
        let base = [avg[0], avg[1], avg[2], 1.0];
        let n = if reduced { 4 } else { 14 };
        for _ in 0..n {
            let shade = 0.75 + 0.35 * self.rand();
            let color = [base[0] * shade, base[1] * shade, base[2] * shade, 1.0];
            let off = Vec3::new(self.rand() - 0.5, self.rand() - 0.5, self.rand() - 0.5) * 0.8;
            let vel = Vec3::new(self.rand() - 0.5, 0.3 + self.rand(), self.rand() - 0.5) * 4.0;
            let life = (if reduced { 0.5 } else { 0.9 }) + self.rand() * 0.5;
            let size = 0.10 + self.rand() * 0.10;
            self.push(Particle { pos: center + off, vel, age: 0.0, life, color, size, kind: Kind::Debris }, reduced);
        }
        if block == BlockType::Pyrite {
            let n = if reduced { 2 } else { 7 };
            for _ in 0..n {
                let off = Vec3::new(self.rand() - 0.5, self.rand() * 0.5, self.rand() - 0.5) * 0.8;
                let vel = Vec3::new(self.rand() - 0.5, 0.8 + self.rand() * 0.8, self.rand() - 0.5) * 0.6;
                let tint = 0.55 + 0.2 * self.rand();
                let life = 1.6 + self.rand();
                self.push(Particle {
                    pos: center + off,
                    vel,
                    age: 0.0,
                    life,
                    color: [tint + 0.25, tint + 0.2, tint - 0.1, 0.7],
                    size: 0.3,
                    kind: Kind::Smoke,
                }, reduced);
            }
        }
    }

    pub fn update(&mut self, dt: f32, world: &dyn BlockQuery, reduced: bool) {
        let cap = Self::cap(reduced);
        if self.list.len() > cap {
            let extra = self.list.len() - cap;
            self.list.drain(0..extra);
        }
        let solid = |p: Vec3| world.block_at(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32).is_solid();
        for p in &mut self.list {
            p.age += dt;
            match p.kind {
                Kind::Debris => {
                    p.vel.y -= GRAVITY * dt;
                    let ny = p.pos + Vec3::new(0.0, p.vel.y * dt, 0.0);
                    if solid(ny) {
                        p.vel.y = -p.vel.y * 0.3;
                        p.vel.x *= 0.6;
                        p.vel.z *= 0.6;
                    } else {
                        p.pos = ny;
                    }
                    let nxz = p.pos + Vec3::new(p.vel.x, 0.0, p.vel.z) * dt;
                    if solid(nxz) {
                        p.vel.x = 0.0;
                        p.vel.z = 0.0;
                    } else {
                        p.pos = nxz;
                    }
                }
                Kind::Smoke => {
                    p.vel *= 1.0 - (0.8 * dt).min(1.0);
                    p.pos += p.vel * dt;
                    p.size += 0.25 * dt;
                }
            }
        }
        self.list.retain(|p| p.age < p.life);
    }

    /// Quads voltados para a câmera. A fumaça desvanece com a idade.
    /// `visible(centro, raio)` aplica o entity culling (partículas só usam distância + frustum).
    pub fn build_mesh(&self, camera: &Camera, visible: &mut dyn FnMut(Vec3, f32) -> bool) -> (Vec<VoxelVertex>, Vec<u32>) {
        let forward = camera.forward();
        let right = camera.right();
        let up = right.cross(forward);
        let normal = (-forward).to_array();
        let mut vs = Vec::with_capacity(self.list.len() * 4);
        let mut is = Vec::with_capacity(self.list.len() * 6);
        for p in &self.list {
            if !visible(p.pos, p.size) {
                continue;
            }
            let mut color = p.color;
            if p.kind == Kind::Smoke {
                color[3] *= (1.0 - p.age / p.life).clamp(0.0, 1.0);
            }
            let r = right * (p.size * 0.5);
            let u = up * (p.size * 0.5);
            let base = vs.len() as u32;
            for c in [p.pos - r - u, p.pos + r - u, p.pos + r + u, p.pos - r + u] {
                vs.push(VoxelVertex::flat(c.to_array(), color, normal));
            }
            is.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        (vs, is)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chão sólido em y < 10, ar acima.
    struct Floor;
    impl BlockQuery for Floor {
        fn block_at(&self, _x: i32, y: i32, _z: i32) -> BlockType {
            if y < 10 { BlockType::Stone } else { BlockType::Air }
        }
    }

    #[test]
    fn break_spawns_debris_with_block_color() {
        let mut ps = ParticleSystem::new();
        ps.spawn_break(IVec3::new(0, 20, 0), BlockType::Stone, false);
        assert_eq!(ps.list.len(), 14);
        assert!(ps.list.iter().all(|p| p.kind == Kind::Debris));
        // cor da pedra (cinza) com variação de brilho
        assert!(ps.list.iter().all(|p| (p.color[0] - p.color[1]).abs() < 0.01));
        ps.clear();
        ps.spawn_break(IVec3::ZERO, BlockType::Air, false);
        ps.spawn_break(IVec3::ZERO, BlockType::Water, false);
        assert!(ps.list.is_empty(), "ar e água não geram partículas");
    }

    #[test]
    fn pyrite_also_emits_smoke() {
        let mut ps = ParticleSystem::new();
        ps.spawn_break(IVec3::new(0, 20, 0), BlockType::Pyrite, false);
        assert!(ps.list.iter().any(|p| p.kind == Kind::Smoke));
    }

    #[test]
    fn reduced_spawns_fewer_and_caps_lower() {
        let (mut a, mut b) = (ParticleSystem::new(), ParticleSystem::new());
        a.spawn_break(IVec3::ZERO, BlockType::Pyrite, false);
        b.spawn_break(IVec3::ZERO, BlockType::Pyrite, true);
        assert!(b.list.len() * 2 < a.list.len());
        for _ in 0..500 {
            a.spawn_break(IVec3::ZERO, BlockType::Dirt, true);
        }
        assert!(a.list.len() <= MAX_PARTICLES_REDUCED);
        let mut c = ParticleSystem::new();
        for _ in 0..500 {
            c.spawn_break(IVec3::ZERO, BlockType::Dirt, false);
        }
        assert_eq!(c.list.len(), MAX_PARTICLES);
        // ligar "reduzir" com muitas partículas vivas poda imediatamente
        c.update(0.0, &Floor, true);
        assert_eq!(c.list.len(), MAX_PARTICLES_REDUCED);
    }

    #[test]
    fn debris_falls_and_never_enters_the_floor() {
        let mut ps = ParticleSystem::new();
        ps.spawn_break(IVec3::new(0, 14, 0), BlockType::Dirt, false);
        for _ in 0..40 {
            ps.update(1.0 / 60.0, &Floor, false);
            assert!(ps.list.iter().all(|p| p.pos.y >= 10.0 - 1e-3), "atravessou o chão");
        }
        assert!(ps.list.iter().all(|p| p.pos.y < 14.5), "deveria ter caído");
    }

    #[test]
    fn particles_expire() {
        let mut ps = ParticleSystem::new();
        ps.spawn_break(IVec3::new(0, 14, 0), BlockType::Pyrite, false);
        for _ in 0..(5 * 60) {
            ps.update(1.0 / 60.0, &Floor, false);
        }
        assert!(ps.list.is_empty());
    }

    #[test]
    fn frozen_without_update_calls() {
        // A pausa simplesmente não chama `update`: o estado fica idêntico.
        let mut ps = ParticleSystem::new();
        ps.spawn_break(IVec3::new(0, 14, 0), BlockType::Dirt, false);
        ps.update(0.1, &Floor, false);
        let before: Vec<_> = ps.list.iter().map(|p| (p.pos, p.age)).collect();
        let _ = ps.build_mesh(&Camera::new(Vec3::ZERO, 1.0), &mut |_, _| true);
        let after: Vec<_> = ps.list.iter().map(|p| (p.pos, p.age)).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn mesh_quads_face_the_camera() {
        let mut ps = ParticleSystem::new();
        ps.spawn_break(IVec3::new(0, 14, -10), BlockType::Dirt, false);
        let mut cam = Camera::new(Vec3::ZERO, 1.0);
        cam.yaw = 0.7;
        cam.pitch = 0.3;
        let (vs, is) = ps.build_mesh(&cam, &mut |_, _| true);
        assert_eq!(vs.len(), ps.list.len() * 4);
        assert_eq!(is.len(), ps.list.len() * 6);
        let to_cam = -cam.forward();
        for q in vs.chunks(4) {
            let (a, b, c) = (Vec3::from(q[0].position), Vec3::from(q[1].position), Vec3::from(q[2].position));
            let n = (b - a).cross(c - a).normalize();
            assert!(n.dot(to_cam) > 0.99, "enrolamento CCW voltado ao observador (backface culling)");
            assert!(Vec3::from(q[0].normal).dot(to_cam) > 0.99);
        }
    }

    #[test]
    fn culled_particles_are_skipped() {
        let mut ps = ParticleSystem::new();
        ps.spawn_break(IVec3::new(0, 14, -10), BlockType::Dirt, false);
        let cam = Camera::new(Vec3::ZERO, 1.0);
        let (vs, _) = ps.build_mesh(&cam, &mut |_, _| false);
        assert!(vs.is_empty());
        let mut n = 0;
        let (vs, _) = ps.build_mesh(&cam, &mut |_, _| { n += 1; n % 2 == 0 });
        assert_eq!(vs.len(), (ps.list.len() / 2) * 4);
    }
}
