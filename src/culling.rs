//! Parte 2 — entity culling: entidades (peixes, barcos, itens soltos, partículas) que não estão
//! no campo de visão, estão longe demais ou estão escondidas atrás de blocos não são desenhadas.
//!
//! Três testes em ordem de custo: distância → frustum → linha de visão (raycast por blocos,
//! em vários pontos da entidade). Uma entidade que acabou de ser tapada continua visível por
//! `LOS_HOLD_SECS` para não piscar quando a borda de um bloco passa por ela.
use std::collections::HashMap;

use glam::Vec3;

use crate::camera::{Camera, Frustum};
use crate::physics::{raycast, BlockQuery};

pub const LOS_HOLD_SECS: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityKind {
    Fish = 0,
    Boat = 1,
    Drop = 2,
    Particle = 3,
}

impl EntityKind {
    pub const ALL: [EntityKind; 4] = [Self::Fish, Self::Boat, Self::Drop, Self::Particle];

    /// Distância máxima de desenho (sprites pequenos somem antes).
    pub fn max_distance(self) -> f32 {
        match self {
            Self::Fish => 110.0,
            Self::Boat => 260.0,
            Self::Drop => 64.0,
            Self::Particle => 80.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Fish => "peixes",
            Self::Boat => "barcos",
            Self::Drop => "itens",
            Self::Particle => "partículas",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CullCounts {
    pub total: u32,
    pub visible: u32,
}

pub struct EntityCuller {
    frustum: Option<Frustum>,
    cam: Vec3,
    right: Vec3,
    now: f32,
    /// Último instante em que cada entidade passou em TODOS os testes.
    last_seen: HashMap<(EntityKind, u64), f32>,
    stats: [CullCounts; 4],
    /// `false` desenha tudo (diagnóstico: `DNE_NOCULL=1`).
    pub enabled: bool,
}

impl Default for EntityCuller {
    fn default() -> Self {
        Self::new()
    }
}

/// Há linha de visão entre `from` e `to`? Blocos sólidos (qualquer coisa que não seja ar/água)
/// tapam; a água não. Uma pequena folga ignora o bloco onde a própria entidade está.
pub fn line_of_sight(from: Vec3, to: Vec3, world: &dyn BlockQuery) -> bool {
    let d = to - from;
    let dist = d.length();
    if dist < 1.0 {
        return true;
    }
    match raycast(from, d / dist, dist, world) {
        Some(hit) => hit.distance >= dist - 0.6,
        None => true,
    }
}

impl EntityCuller {
    pub fn new() -> Self {
        Self {
            frustum: None,
            cam: Vec3::ZERO,
            right: Vec3::X,
            now: 0.0,
            last_seen: HashMap::new(),
            stats: [CullCounts::default(); 4],
            enabled: true,
        }
    }

    /// Chamar uma vez por quadro, antes dos testes.
    pub fn begin_frame(&mut self, camera: &Camera, now: f32) {
        self.frustum = Some(camera.frustum());
        self.cam = camera.position;
        self.right = camera.right();
        self.now = now;
        self.stats = [CullCounts::default(); 4];
        if self.last_seen.len() > 4096 {
            let cutoff = now - 2.0;
            self.last_seen.retain(|_, t| *t >= cutoff);
        }
    }

    pub fn stats(&self, kind: EntityKind) -> CullCounts {
        self.stats[kind as usize]
    }

    fn count(&mut self, kind: EntityKind, visible: bool) -> bool {
        let s = &mut self.stats[kind as usize];
        s.total += 1;
        s.visible += visible as u32;
        visible
    }

    /// Distância + frustum (barato; basta para partículas).
    pub fn in_view(&mut self, kind: EntityKind, center: Vec3, radius: f32) -> bool {
        let ok = !self.enabled || self.in_view_raw(kind, center, radius);
        self.count(kind, ok)
    }

    fn in_view_raw(&self, kind: EntityKind, center: Vec3, radius: f32) -> bool {
        if center.distance(self.cam) > kind.max_distance() + radius {
            return false;
        }
        match &self.frustum {
            Some(f) => f.intersects_aabb(center - Vec3::splat(radius), center + Vec3::splat(radius)),
            None => true,
        }
    }

    /// Teste completo: distância, frustum e linha de visão em 4 pontos da entidade.
    /// `key` identifica a entidade entre quadros (para o `LOS_HOLD_SECS`).
    pub fn test(&mut self, kind: EntityKind, key: u64, center: Vec3, radius: f32, world: &dyn BlockQuery) -> bool {
        if !self.enabled {
            return self.count(kind, true);
        }
        if !self.in_view_raw(kind, center, radius) {
            return self.count(kind, false);
        }
        // Muito perto da câmera: sempre visível (e o raycast seria instável).
        if center.distance(self.cam) <= radius + 1.0 {
            self.last_seen.insert((kind, key), self.now);
            return self.count(kind, true);
        }
        let up = Vec3::Y * radius;
        let side = self.right * radius;
        let seen = [center, center + up, center + side, center - side]
            .into_iter()
            .any(|p| line_of_sight(self.cam, p, world));
        let visible = if seen {
            self.last_seen.insert((kind, key), self.now);
            true
        } else {
            self.last_seen.get(&(kind, key)).is_some_and(|t| self.now - *t < LOS_HOLD_SECS)
        };
        self.count(kind, visible)
    }
}

/// Chave estável a partir de dois inteiros (chunk do cardume/barco + índice).
pub fn entity_key(a: i32, b: i32, c: u32) -> u64 {
    (a as u32 as u64) << 40 ^ (b as u32 as u64) << 16 ^ c as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_world::FakeWorld;
    use crate::world::BlockType;

    /// Câmera em (0.5, 70, 0.5) olhando para -Z.
    fn cam() -> Camera {
        Camera::new(Vec3::new(0.5, 70.0, 0.5), 16.0 / 9.0)
    }

    fn culler(now: f32) -> EntityCuller {
        let mut c = EntityCuller::new();
        c.begin_frame(&cam(), now);
        c
    }

    #[test]
    fn visible_in_front_hidden_behind_and_beyond_range() {
        let w = FakeWorld::flat();
        let mut c = culler(0.0);
        assert!(c.test(EntityKind::Boat, 1, Vec3::new(0.5, 72.0, -30.0), 1.0, &w), "à frente");
        assert!(!c.test(EntityKind::Boat, 2, Vec3::new(0.5, 72.0, 30.0), 1.0, &w), "atrás da câmera");
        assert!(!c.test(EntityKind::Boat, 3, Vec3::new(200.0, 72.0, -30.0), 1.0, &w), "fora do frustum (lado)");
        assert!(!c.test(EntityKind::Drop, 4, Vec3::new(0.5, 72.0, -100.0), 0.25, &w), "itens somem a 64 blocos");
        assert!(c.test(EntityKind::Boat, 5, Vec3::new(0.5, 72.0, -100.0), 1.0, &w), "barcos aparecem até mais longe");
        assert!(!c.test(EntityKind::Fish, 6, Vec3::new(0.5, 72.0, -150.0), 0.5, &w), "peixes somem a 110");
    }

    #[test]
    fn entities_behind_blocks_are_hidden() {
        let mut w = FakeWorld::flat();
        // Parede 20x20 entre a câmera e o alvo
        for x in -10..=10 { for y in 60..=80 { w.set(x, y, -10); } }
        let mut c = culler(0.0);
        assert!(!c.test(EntityKind::Fish, 1, Vec3::new(0.5, 70.0, -30.0), 0.5, &w), "atrás da parede");
        assert!(c.test(EntityKind::Fish, 2, Vec3::new(0.5, 70.0, -5.0), 0.5, &w), "antes da parede");
    }

    #[test]
    fn water_does_not_hide_entities() {
        struct Sea;
        impl BlockQuery for Sea {
            fn block_at(&self, _x: i32, y: i32, _z: i32) -> BlockType {
                if y < 60 { BlockType::Stone } else if y < 80 { BlockType::Water } else { BlockType::Air }
            }
        }
        let mut c = culler(0.0);
        assert!(c.test(EntityKind::Fish, 1, Vec3::new(0.5, 65.0, -40.0), 0.5, &Sea), "peixe visto através da água");
    }

    #[test]
    fn entity_on_the_ground_is_visible_when_a_block_only_touches_its_own_cell() {
        let mut w = FakeWorld::flat();
        // Barco apoiado num bloco: o bloco do próprio alvo não conta como oclusor
        w.set(0, 70, -20);
        let mut c = culler(0.0);
        assert!(c.test(EntityKind::Boat, 1, Vec3::new(0.5, 70.5, -19.5), 1.0, &w));
    }

    #[test]
    fn partially_visible_entities_count_as_visible() {
        let mut w = FakeWorld::flat();
        // Muro baixo (altura 70..70): tapa o centro, mas o topo do alvo aparece por cima
        for x in -3..=3 { w.set(x, 70, -10); }
        let mut c = culler(0.0);
        assert!(c.test(EntityKind::Boat, 1, Vec3::new(0.5, 70.5, -30.0), 3.0, &w), "topo visível por cima do muro");
    }

    #[test]
    fn briefly_occluded_entities_stay_visible_then_vanish() {
        let w_open = FakeWorld::flat();
        let mut w_wall = FakeWorld::flat();
        for x in -10..=10 { for y in 60..=80 { w_wall.set(x, y, -10); } }
        let target = Vec3::new(0.5, 70.0, -30.0);
        let mut c = culler(0.0);
        assert!(c.test(EntityKind::Fish, 7, target, 0.5, &w_open));
        // 0,1 s depois a parede surge: ainda visível (evita piscar)
        let mut c2 = EntityCuller { last_seen: c.last_seen.clone(), ..EntityCuller::new() };
        c2.begin_frame(&cam(), 0.1);
        assert!(c2.test(EntityKind::Fish, 7, target, 0.5, &w_wall));
        // 0,5 s depois: oculto
        c2.begin_frame(&cam(), 0.5);
        assert!(!c2.test(EntityKind::Fish, 7, target, 0.5, &w_wall));
        // outra entidade (chave diferente) nunca vista: oculta de imediato
        assert!(!c2.test(EntityKind::Fish, 8, target, 0.5, &w_wall));
    }

    #[test]
    fn camera_inside_solid_ground_sees_nothing() {
        let w = FakeWorld::flat();
        let mut inside = EntityCuller::new();
        let mut cam = cam();
        cam.position = Vec3::new(0.5, 50.0, 0.5); // dentro da pedra
        inside.begin_frame(&cam, 0.0);
        assert!(!inside.test(EntityKind::Boat, 1, Vec3::new(0.5, 50.0, -30.0), 1.0, &w));
    }

    #[test]
    fn disabled_culler_shows_everything_and_counts() {
        let w = FakeWorld::flat();
        let mut c = culler(0.0);
        c.enabled = false;
        assert!(c.test(EntityKind::Fish, 1, Vec3::new(0.5, 72.0, 500.0), 0.5, &w));
        assert!(c.in_view(EntityKind::Particle, Vec3::new(0.5, 72.0, 500.0), 0.1));
        assert_eq!(c.stats(EntityKind::Fish), CullCounts { total: 1, visible: 1 });
    }

    #[test]
    fn stats_count_total_and_visible_per_kind() {
        let w = FakeWorld::flat();
        let mut c = culler(0.0);
        c.test(EntityKind::Boat, 1, Vec3::new(0.5, 72.0, -30.0), 1.0, &w);
        c.test(EntityKind::Boat, 2, Vec3::new(0.5, 72.0, 30.0), 1.0, &w);
        c.in_view(EntityKind::Particle, Vec3::new(0.5, 72.0, -5.0), 0.1);
        assert_eq!(c.stats(EntityKind::Boat), CullCounts { total: 2, visible: 1 });
        assert_eq!(c.stats(EntityKind::Particle), CullCounts { total: 1, visible: 1 });
        assert_eq!(c.stats(EntityKind::Fish), CullCounts::default());
        c.begin_frame(&cam(), 1.0);
        assert_eq!(c.stats(EntityKind::Boat), CullCounts::default(), "zera a cada quadro");
    }

    #[test]
    fn entity_keys_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for a in -3..3 { for b in -3..3 { for c in 0..12 { assert!(seen.insert(entity_key(a, b, c))); } } }
    }

    #[test]
    fn line_of_sight_basics() {
        let mut w = FakeWorld::flat();
        w.set(5, 70, 0);
        assert!(!line_of_sight(Vec3::new(0.5, 70.5, 0.5), Vec3::new(20.5, 70.5, 0.5), &w), "bloco no caminho");
        assert!(line_of_sight(Vec3::new(0.5, 72.5, 0.5), Vec3::new(20.5, 72.5, 0.5), &w), "por cima do bloco");
        assert!(line_of_sight(Vec3::new(0.5, 70.5, 0.5), Vec3::new(0.7, 70.5, 0.5), &w), "muito perto");
    }
}
