//! Parte 2 — blocos quebrados viram itens no chão: pequenos (¼ do bloco), girando e flutuando.
//! Ao chegar perto, o jogador os recolhe para o inventário (regra de hotbar/grade em `Inventory`).
use glam::{IVec3, Mat3, Vec3};

use crate::inventory::{Inventory, Item};
use crate::physics::BlockQuery;
use crate::renderer::mesh::{plane_axes, VoxelVertex, FACE_NORMALS, FACE_VERTICES};
use crate::world::textures::{layer_for_face, NO_TEXTURE, NO_VARIATION_BIT};
use crate::world::{BlockType, ColorPalette};

/// Lado do cubo do item solto, em blocos (bem menor que um bloco real).
pub const DROP_SCALE: f32 = 0.25;
pub const PICKUP_RADIUS: f32 = 1.6;
/// Tempo antes de poder recolher (evita pegar de volta o bloco no mesmo instante).
pub const PICKUP_DELAY: f32 = 0.6;
/// Itens esquecidos somem depois de 5 minutos.
pub const DROP_LIFETIME: f32 = 300.0;
pub const MAX_DROPS: usize = 300;
const GRAVITY: f32 = 22.0;
const TERMINAL: f32 = 30.0;
/// Altura do centro do cubo acima do chão quando em repouso (flutua).
pub const HOVER: f32 = 0.22;
const BOB_AMPLITUDE: f32 = 0.06;
const SPIN_SPEED: f32 = 1.8;
const MERGE_DIST: f32 = 0.9;

#[derive(Clone, Debug)]
pub struct ItemDrop {
    pub item: Item,
    pub count: u32,
    /// Centro do cubo (sem o balanço).
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: f32,
    pub phase: f32,
    pub grounded: bool,
}

impl ItemDrop {
    /// Posição desenhada: com o balanço senoidal quando está no chão.
    pub fn render_pos(&self) -> Vec3 {
        let bob = if self.grounded { (self.age * 2.2 + self.phase).sin() * BOB_AMPLITUDE } else { 0.0 };
        self.pos + Vec3::Y * bob
    }

    pub fn spin(&self) -> f32 {
        self.age * SPIN_SPEED + self.phase
    }
}

/// O que um bloco quebrado deixa cair (`None` = nada).
pub fn drop_for_block(b: BlockType) -> Option<Item> {
    match b {
        BlockType::Air | BlockType::Water | BlockType::Bedrock | BlockType::SulfuricAcid => None,
        other => Some(Item::Block(other)),
    }
}

#[derive(Default)]
pub struct DropSystem {
    pub drops: Vec<ItemDrop>,
    rng: u32,
}

impl DropSystem {
    pub fn new() -> Self {
        Self { drops: Vec::new(), rng: 0x2545_f491 }
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn clear(&mut self) {
        self.drops.clear();
    }

    pub fn len(&self) -> usize {
        self.drops.len()
    }

    /// Solta um item no mundo com um pequeno "pulo".
    pub fn spawn(&mut self, item: Item, count: u32, pos: Vec3) {
        let vel = Vec3::new(self.rand() - 0.5, 2.0 + self.rand() * 1.5, self.rand() - 0.5) * Vec3::new(2.0, 1.0, 2.0);
        let phase = self.rand() * std::f32::consts::TAU;
        if self.drops.len() >= MAX_DROPS {
            self.drops.remove(0); // o mais antigo some
        }
        self.drops.push(ItemDrop { item, count, pos, vel, age: 0.0, phase, grounded: false });
    }

    /// Bloco quebrado em `block_pos`: solta o item correspondente.
    pub fn spawn_from_block(&mut self, block_pos: IVec3, block: BlockType) {
        if let Some(item) = drop_for_block(block) {
            self.spawn(item, 1, block_pos.as_vec3() + Vec3::splat(0.5));
        }
    }

    fn solid(world: &dyn BlockQuery, p: Vec3) -> bool {
        world.block_at(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32).is_solid()
    }

    /// Física, envelhecimento, fusão de itens iguais próximos e coleta pelo jogador.
    /// `player_center`: ponto do corpo do jogador. `can_pickup`: falso no modo Espectador.
    /// Retorna `(item, quantidade)` recolhidos neste passo.
    pub fn update(
        &mut self,
        dt: f32,
        world: &dyn BlockQuery,
        player_center: Vec3,
        inventory: &mut Inventory,
        can_pickup: bool,
    ) -> Vec<(Item, u32)> {
        for d in &mut self.drops {
            d.age += dt;
            let probe = d.pos - Vec3::Y * (HOVER + 0.02);
            let ground_below = Self::solid(world, probe);
            if ground_below && d.vel.y <= 0.0 {
                // Em repouso: encaixa a flutuar HOVER acima do topo do bloco
                d.grounded = true;
                d.vel = Vec3::ZERO;
                d.pos.y = probe.y.floor() + 1.0 + HOVER;
            } else {
                // Caindo (ou subindo do "pulo"); sem chão embaixo volta a cair
                d.grounded = false;
                d.vel.y = (d.vel.y - GRAVITY * dt).max(-TERMINAL);
                let next = d.pos + d.vel * dt;
                let hx = Vec3::new(next.x, d.pos.y, d.pos.z);
                let hz = Vec3::new(d.pos.x, d.pos.y, next.z);
                if Self::solid(world, hx) { d.vel.x = 0.0; } else { d.pos.x = next.x; }
                if Self::solid(world, hz) { d.vel.z = 0.0; } else { d.pos.z = next.z; }
                if d.vel.y > 0.0 && Self::solid(world, Vec3::new(d.pos.x, next.y + 0.15, d.pos.z)) {
                    d.vel.y = 0.0; // bateu a cabeça no teto
                } else {
                    d.pos.y = next.y;
                }
                d.vel.x *= 1.0 - (2.0 * dt).min(1.0);
                d.vel.z *= 1.0 - (2.0 * dt).min(1.0);
            }
        }
        self.drops.retain(|d| d.age < DROP_LIFETIME && d.pos.y > -30.0);

        // Funde itens iguais próximos (parados) para não encher o chão
        let mut i = 0;
        while i < self.drops.len() {
            let mut j = i + 1;
            while j < self.drops.len() {
                let (a, b) = (&self.drops[i], &self.drops[j]);
                if a.grounded && b.grounded && a.item == b.item
                    && a.pos.distance(b.pos) < MERGE_DIST
                    && a.count + b.count <= a.item.max_stack()
                {
                    let c = self.drops[j].count;
                    self.drops[i].count += c;
                    self.drops.swap_remove(j);
                } else {
                    j += 1;
                }
            }
            i += 1;
        }

        let mut picked = Vec::new();
        if can_pickup {
            let r2 = PICKUP_RADIUS * PICKUP_RADIUS;
            for d in &mut self.drops {
                if d.age < PICKUP_DELAY || d.pos.distance_squared(player_center) > r2 {
                    continue;
                }
                let left = inventory.add_item(d.item, d.count);
                if left < d.count {
                    picked.push((d.item, d.count - left));
                    d.count = left;
                }
            }
            self.drops.retain(|d| d.count > 0);
        }
        picked
    }

    /// Malha dos itens: cubos de `DROP_SCALE` girando, com a textura do bloco.
    /// `visible(índice, centro, raio)` decide se o item entra (entity culling).
    pub fn build_mesh(&self, visible: &mut dyn FnMut(usize, Vec3, f32) -> bool) -> (Vec<VoxelVertex>, Vec<u32>) {
        let mut vs = Vec::new();
        let mut is = Vec::new();
        for (idx, d) in self.drops.iter().enumerate() {
            let center = d.render_pos();
            if !visible(idx, center, DROP_SCALE) {
                continue;
            }
            let rot = Mat3::from_rotation_y(d.spin());
            for face in 0..6 {
                let base = vs.len() as u32;
                let n = rot * Vec3::from_array(FACE_NORMALS[face]);
                let (layer, color) = match d.item {
                    Item::Block(b) => (layer_for_face(b, face) | NO_VARIATION_BIT, ColorPalette::get_color(b, FACE_NORMALS[face])),
                    Item::Gear(g) => {
                        let c = g.color();
                        (NO_TEXTURE, [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0])
                    }
                };
                let (ua, va) = plane_axes(face / 2);
                for v in &FACE_VERTICES[face] {
                    let local = (Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) - Vec3::splat(0.5)) * DROP_SCALE;
                    vs.push(VoxelVertex {
                        position: (center + rot * local).to_array(),
                        color,
                        normal: n.to_array(),
                        uv: [v[ua] as f32, v[va] as f32],
                        layer,
                    });
                }
                is.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        (vs, is)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{Gear, ItemStack};

    /// Chão de pedra em y < 10; `hole` remove um bloco do chão.
    struct Floor {
        hole: Option<(i32, i32, i32)>,
    }
    impl BlockQuery for Floor {
        fn block_at(&self, x: i32, y: i32, z: i32) -> BlockType {
            if Some((x, y, z)) == self.hole { return BlockType::Air; }
            if y < 10 { BlockType::Stone } else { BlockType::Air }
        }
    }
    const FLOOR: Floor = Floor { hole: None };
    const FAR: Vec3 = Vec3::new(500.0, 11.0, 500.0);

    fn settle(ds: &mut DropSystem, world: &dyn BlockQuery, secs: f32) {
        let mut inv = Inventory::new();
        for _ in 0..(secs * 60.0) as usize {
            ds.update(1.0 / 60.0, world, FAR, &mut inv, true);
        }
    }

    #[test]
    fn broken_block_drops_its_item() {
        let mut ds = DropSystem::new();
        ds.spawn_from_block(IVec3::new(3, 12, 3), BlockType::Stone);
        assert_eq!(ds.len(), 1);
        assert_eq!(ds.drops[0].item, Item::Block(BlockType::Stone));
        assert_eq!(ds.drops[0].pos, Vec3::new(3.5, 12.5, 3.5));
        for b in [BlockType::Air, BlockType::Water, BlockType::Bedrock, BlockType::SulfuricAcid] {
            ds.spawn_from_block(IVec3::ZERO, b);
        }
        assert_eq!(ds.len(), 1, "ar, água, bedrock e ácido não deixam item");
    }

    #[test]
    fn drop_is_much_smaller_than_a_block() {
        assert!(DROP_SCALE <= 0.3);
        let mut ds = DropSystem::new();
        ds.spawn_from_block(IVec3::new(0, 12, 0), BlockType::Dirt);
        let (vs, is) = ds.build_mesh(&mut |_, _, _| true);
        assert_eq!(vs.len(), 24);
        assert_eq!(is.len(), 36);
        let (mn, mx) = vs.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), v| {
            let p = Vec3::from(v.position);
            (a.min(p), b.max(p))
        });
        // Cabe numa esfera de raio ~DROP_SCALE*0.87 (cubo girado): bem menor que 1 bloco
        assert!((mx - mn).max_element() < DROP_SCALE * 1.5);
    }

    #[test]
    fn falls_lands_and_floats_above_the_ground() {
        let mut ds = DropSystem::new();
        ds.spawn(Item::Block(BlockType::Dirt), 1, Vec3::new(0.5, 15.5, 0.5));
        settle(&mut ds, &FLOOR, 3.0);
        let d = &ds.drops[0];
        assert!(d.grounded);
        // topo do chão em y=10, flutua HOVER acima
        assert!((d.pos.y - (10.0 + HOVER)).abs() < 1e-3, "{}", d.pos.y);
        // balança: a posição desenhada varia ao longo do tempo
        let ys: Vec<f32> = (0..30).map(|i| {
            let mut c = d.clone();
            c.age += i as f32 * 0.1;
            c.render_pos().y
        }).collect();
        let (lo, hi) = ys.iter().fold((f32::MAX, f32::MIN), |(l, h), y| (l.min(*y), h.max(*y)));
        assert!(hi - lo > 0.05 && hi - lo <= BOB_AMPLITUDE * 2.0 + 1e-3, "balanço {}", hi - lo);
        assert!(lo > 10.0, "nunca entra no chão");
    }

    #[test]
    fn spins_over_time() {
        let mut ds = DropSystem::new();
        ds.spawn_from_block(IVec3::new(0, 12, 0), BlockType::Gold);
        let m0 = ds.build_mesh(&mut |_, _, _| true).0;
        ds.drops[0].age += 0.5;
        let m1 = ds.build_mesh(&mut |_, _, _| true).0;
        assert_ne!(m0[0].position, m1[0].position);
        assert!(ds.drops[0].spin() > 0.9 * 0.5 * SPIN_SPEED);
    }

    #[test]
    fn falls_again_when_the_floor_is_removed() {
        let mut ds = DropSystem::new();
        ds.spawn(Item::Block(BlockType::Sand), 1, Vec3::new(0.5, 11.0, 0.5));
        settle(&mut ds, &FLOOR, 2.0);
        assert!(ds.drops[0].grounded);
        let hole = Floor { hole: Some((0, 9, 0)) };
        settle(&mut ds, &hole, 2.0);
        assert!((ds.drops[0].pos.y - (9.0 + HOVER)).abs() < 1e-3, "caiu um bloco: {}", ds.drops[0].pos.y);
    }

    #[test]
    fn player_picks_up_nearby_items_after_the_delay() {
        let mut ds = DropSystem::new();
        let mut inv = Inventory::new();
        let player = Vec3::new(0.5, 11.0, 0.5);
        ds.spawn(Item::Block(BlockType::Coal), 1, Vec3::new(0.5, 10.5, 0.5));
        // dentro do atraso: não recolhe
        assert!(ds.update(0.1, &FLOOR, player, &mut inv, true).is_empty());
        assert_eq!(inv.count_of(Item::Block(BlockType::Coal)), 0);
        let mut got = Vec::new();
        for _ in 0..60 {
            got.extend(ds.update(1.0 / 60.0, &FLOOR, player, &mut inv, true));
        }
        assert_eq!(got, vec![(Item::Block(BlockType::Coal), 1)]);
        assert_eq!(ds.len(), 0);
        assert_eq!(inv.hotbar[0].unwrap().item, Item::Block(BlockType::Coal), "vai primeiro para a hotbar");
    }

    #[test]
    fn far_items_and_spectators_do_not_pick_up() {
        let mut ds = DropSystem::new();
        let mut inv = Inventory::new();
        ds.spawn(Item::Block(BlockType::Coal), 1, Vec3::new(30.5, 10.5, 0.5));
        ds.drops[0].age = 5.0;
        ds.update(0.016, &FLOOR, Vec3::new(0.5, 11.0, 0.5), &mut inv, true);
        assert_eq!(ds.len(), 1, "longe demais");
        ds.drops[0].pos = Vec3::new(0.5, 10.5, 0.5);
        ds.update(0.016, &FLOOR, Vec3::new(0.5, 11.0, 0.5), &mut inv, false);
        assert_eq!(ds.len(), 1, "espectador não interage");
        assert_eq!(inv, Inventory::new());
    }

    #[test]
    fn full_inventory_leaves_the_item_on_the_ground() {
        let mut inv = Inventory::new();
        for i in 0..inv.hotbar.len() { inv.hotbar[i] = Some(ItemStack::new(Item::Block(BlockType::Stone), 64)); }
        for i in 0..27 { inv.main[i] = Some(ItemStack::new(Item::Block(BlockType::Stone), 64)); }
        let mut ds = DropSystem::new();
        ds.spawn(Item::Block(BlockType::Coal), 3, Vec3::new(0.5, 10.5, 0.5));
        ds.drops[0].age = 5.0;
        let got = ds.update(0.016, &FLOOR, Vec3::new(0.5, 11.0, 0.5), &mut inv, true);
        assert!(got.is_empty());
        assert_eq!(ds.len(), 1, "nada é apagado");
        assert_eq!(ds.drops[0].count, 3);
    }

    #[test]
    fn partial_pickup_keeps_the_remainder() {
        let mut inv = Inventory::new();
        for i in 0..inv.hotbar.len() { inv.hotbar[i] = Some(ItemStack::new(Item::Block(BlockType::Stone), 64)); }
        for i in 0..27 { inv.main[i] = Some(ItemStack::new(Item::Block(BlockType::Stone), 64)); }
        inv.main[3] = Some(ItemStack::new(Item::Block(BlockType::Stone), 60));
        let mut ds = DropSystem::new();
        ds.spawn(Item::Block(BlockType::Stone), 10, Vec3::new(0.5, 10.5, 0.5));
        ds.drops[0].age = 5.0;
        let got = ds.update(0.016, &FLOOR, Vec3::new(0.5, 11.0, 0.5), &mut inv, true);
        assert_eq!(got, vec![(Item::Block(BlockType::Stone), 4)]);
        assert_eq!(ds.drops[0].count, 6);
    }

    #[test]
    fn nearby_identical_drops_merge() {
        let mut ds = DropSystem::new();
        for i in 0..4 {
            ds.spawn(Item::Block(BlockType::Sand), 1, Vec3::new(0.5 + i as f32 * 0.1, 10.5, 0.5));
        }
        settle(&mut ds, &FLOOR, 2.0);
        assert_eq!(ds.len(), 1);
        assert_eq!(ds.drops[0].count, 4);
    }

    #[test]
    fn old_drops_despawn_and_the_cap_holds() {
        let mut ds = DropSystem::new();
        ds.spawn(Item::Block(BlockType::Sand), 1, Vec3::new(0.5, 10.5, 0.5));
        ds.drops[0].age = DROP_LIFETIME + 1.0;
        settle(&mut ds, &FLOOR, 0.1);
        assert_eq!(ds.len(), 0);
        for i in 0..(MAX_DROPS + 50) {
            ds.spawn(Item::Block(BlockType::Stone), 1, Vec3::new(i as f32, 20.0, 0.0));
        }
        assert_eq!(ds.len(), MAX_DROPS);
    }

    #[test]
    fn mesh_uses_the_block_texture_and_winds_outward() {
        let mut ds = DropSystem::new();
        ds.spawn_from_block(IVec3::new(0, 12, 0), BlockType::Wood);
        ds.drops[0].age = 0.37;
        let (vs, is) = ds.build_mesh(&mut |_, _, _| true);
        assert!(vs.iter().all(|v| v.layer != NO_TEXTURE && (v.layer & !NO_VARIATION_BIT) < 96 && v.layer & NO_VARIATION_BIT != 0));
        assert!(vs.iter().all(|v| v.uv.iter().all(|c| (0.0..=1.0).contains(c))));
        for tri in is.chunks(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| Vec3::from(vs[i as usize].position));
            let n = Vec3::from(vs[tri[0] as usize].normal);
            assert!((b - a).cross(c - a).dot(n) > 0.0, "triângulo voltado para dentro");
        }
        // Gear usa cor chapada
        ds.clear();
        ds.spawn(Item::Gear(Gear::Lantern), 1, Vec3::new(0.0, 12.0, 0.0));
        let (vs, _) = ds.build_mesh(&mut |_, _, _| true);
        assert!(vs.iter().all(|v| v.layer == NO_TEXTURE));
    }

    #[test]
    fn culled_drops_are_not_meshed() {
        let mut ds = DropSystem::new();
        for i in 0..5 {
            ds.spawn(Item::Block(BlockType::Stone), 1, Vec3::new(i as f32 * 5.0, 12.0, 0.0));
        }
        let mut asked = 0;
        let (vs, _) = ds.build_mesh(&mut |i, _, r| { asked += 1; assert_eq!(r, DROP_SCALE); i % 2 == 0 });
        assert_eq!(asked, 5);
        assert_eq!(vs.len(), 3 * 24);
    }
}
