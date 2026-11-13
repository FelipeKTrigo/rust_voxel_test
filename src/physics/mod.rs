//! Física pura: AABB, gravidade, colisão com blocos e raycast DDA.
use crate::world::block::BlockType;
use glam::{IVec3, Vec3};

/// Consulta de blocos. Chunks não carregados devem ser retornados como sólidos.
pub trait BlockQuery {
    fn block_at(&self, x: i32, y: i32, z: i32) -> BlockType;
}

pub const GRAVITY: f32 = 32.0;
pub const TERMINAL_VELOCITY: f32 = 78.0;
/// Altura de pulo alvo (~1.25 blocos): v = sqrt(2 g h).
pub const JUMP_HEIGHT: f32 = 1.25;
pub const JUMP_VELOCITY: f32 = 8.944_272; // sqrt(2 * 32 * 1.25)
pub const PLAYER_WIDTH: f32 = 0.6;
pub const PLAYER_HEIGHT: f32 = 1.8;
pub const EYE_HEIGHT: f32 = 1.62;
pub const REACH: f32 = 6.0;

const EPS: f32 = 1e-3;
const MAX_STEP: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, size: Vec3) -> Self {
        Self { min, max: min + size }
    }

    /// Hitbox do jogador a partir da posição dos pés (centro da base).
    pub fn player(feet: Vec3) -> Self {
        Self::new(
            feet - Vec3::new(PLAYER_WIDTH / 2.0, 0.0, PLAYER_WIDTH / 2.0),
            Vec3::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_WIDTH),
        )
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    /// Algum bloco sólido intersecta a caixa (intervalo aberto)?
    pub fn intersects_solid(&self, world: &dyn BlockQuery) -> bool {
        let lo = self.min.floor().as_ivec3();
        let hi = (self.max.ceil().as_ivec3()) - IVec3::ONE;
        for x in lo.x..=hi.x {
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    if world.block_at(x, y, z).is_solid() {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MoveResult {
    /// Nova posição do canto mínimo da AABB.
    pub position: Vec3,
    pub on_ground: bool,
    pub hit_x: bool,
    pub hit_y: bool,
    pub hit_z: bool,
}

/// Move uma AABB (canto mínimo `pos_min`, tamanho `size`) por `velocity * dt`,
/// resolvendo colisões por eixo (X, Z, depois Y) em substeps (sem tunneling).
pub fn move_and_collide(
    pos_min: Vec3,
    size: Vec3,
    velocity: Vec3,
    dt: f32,
    world: &dyn BlockQuery,
) -> MoveResult {
    let total = velocity * dt;
    let n = ((total.abs().max_element() / MAX_STEP).ceil() as usize).max(1);
    let step = total / n as f32;
    let mut pos = pos_min;
    let (mut hx, mut hy, mut hz) = (false, false, false);
    for _ in 0..n {
        for axis in [0usize, 2, 1] {
            let d = step[axis];
            if d == 0.0 {
                continue;
            }
            let mut np = pos;
            np[axis] += d;
            if !Aabb::new(np, size).intersects_solid(world) {
                pos = np;
                continue;
            }
            // Encaixa a face líder na fronteira do bloco colidido.
            if d > 0.0 {
                let edge = np[axis] + size[axis];
                pos[axis] = edge.floor() - size[axis] - EPS;
            } else {
                pos[axis] = np[axis].floor() + 1.0 + EPS;
            }
            match axis {
                0 => hx = true,
                1 => hy = true,
                _ => hz = true,
            }
        }
    }
    MoveResult {
        position: pos,
        on_ground: hy && velocity.y < 0.0,
        hit_x: hx,
        hit_y: hy,
        hit_z: hz,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub block: IVec3,
    pub face_normal: IVec3,
    pub distance: f32,
}

/// Raycast DDA; ignora Air e Water.
pub fn raycast(origin: Vec3, dir: Vec3, max_dist: f32, world: &dyn BlockQuery) -> Option<RayHit> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    let mut cell = origin.floor().as_ivec3();
    let mut step = IVec3::ZERO;
    let mut t_max = Vec3::splat(f32::INFINITY);
    let mut t_delta = Vec3::splat(f32::INFINITY);
    for i in 0..3 {
        if dir[i] > 0.0 {
            step[i] = 1;
            t_delta[i] = 1.0 / dir[i];
            t_max[i] = ((cell[i] + 1) as f32 - origin[i]) / dir[i];
        } else if dir[i] < 0.0 {
            step[i] = -1;
            t_delta[i] = -1.0 / dir[i];
            t_max[i] = (origin[i] - cell[i] as f32) / -dir[i];
        }
    }
    let mut normal = IVec3::ZERO;
    let mut t = 0.0f32;
    loop {
        let b = world.block_at(cell.x, cell.y, cell.z);
        if !matches!(b, BlockType::Air | BlockType::Water) {
            return Some(RayHit { block: cell, face_normal: normal, distance: t });
        }
        let axis = if t_max.x <= t_max.y && t_max.x <= t_max.z {
            0
        } else if t_max.y <= t_max.z {
            1
        } else {
            2
        };
        t = t_max[axis];
        if t > max_dist {
            return None;
        }
        cell[axis] += step[axis];
        t_max[axis] += t_delta[axis];
        normal = IVec3::ZERO;
        normal[axis] = -step[axis];
    }
}

#[cfg(test)]
pub mod test_world {
    use super::*;
    use std::collections::HashSet;

    /// Plano de pedra em y<=63 mais blocos extras.
    pub struct FakeWorld {
        pub extra: HashSet<(i32, i32, i32)>,
        pub removed: HashSet<(i32, i32, i32)>,
    }
    impl FakeWorld {
        pub fn flat() -> Self {
            Self { extra: HashSet::new(), removed: HashSet::new() }
        }
        pub fn set(&mut self, x: i32, y: i32, z: i32) {
            self.extra.insert((x, y, z));
        }
    }
    impl BlockQuery for FakeWorld {
        fn block_at(&self, x: i32, y: i32, z: i32) -> BlockType {
            if self.extra.contains(&(x, y, z)) || (y <= 63 && !self.removed.contains(&(x, y, z))) {
                BlockType::Stone
            } else {
                BlockType::Air
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_world::FakeWorld;
    use super::*;

    #[test]
    fn no_tunneling_big_dt() {
        let w = FakeWorld::flat();
        let size = Vec3::new(0.6, 1.8, 0.6);
        let r = move_and_collide(Vec3::new(0.2, 70.0, 0.2), size, Vec3::new(0.0, -78.0, 0.0), 0.1, &w);
        assert!(r.position.y >= 64.0 && r.position.y < 64.01, "{}", r.position.y);
        assert!(r.on_ground && r.hit_y);
    }

    #[test]
    fn side_collision() {
        let mut w = FakeWorld::flat();
        w.set(3, 64, 0);
        let size = Vec3::new(0.6, 1.8, 0.6);
        let r = move_and_collide(Vec3::new(1.0, 64.0, 0.2), size, Vec3::new(20.0, 0.0, 0.0), 0.1, &w);
        assert!(r.hit_x);
        assert!(r.position.x + 0.6 <= 3.0);
    }

    #[test]
    fn raycast_hits_face() {
        let w = FakeWorld::flat();
        let h = raycast(Vec3::new(0.5, 66.5, 0.5), Vec3::new(0.0, -1.0, 0.0), 6.0, &w).unwrap();
        assert_eq!(h.block, IVec3::new(0, 63, 0));
        assert_eq!(h.face_normal, IVec3::Y);
        assert!((h.distance - 2.5).abs() < 1e-4);

        let mut w = FakeWorld::flat();
        w.set(3, 64, 0);
        let h = raycast(Vec3::new(0.5, 64.5, 0.5), Vec3::new(1.0, 0.0, 0.0), 6.0, &w).unwrap();
        assert_eq!(h.block, IVec3::new(3, 64, 0));
        assert_eq!(h.face_normal, IVec3::new(-1, 0, 0));
        assert!(raycast(Vec3::new(0.5, 64.5, 0.5), Vec3::new(1.0, 0.0, 0.0), 1.0, &w).is_none());
        assert!(raycast(Vec3::new(0.5, 64.5, 0.5), Vec3::new(0.0, 1.0, 0.0), 6.0, &w).is_none());
    }
}
