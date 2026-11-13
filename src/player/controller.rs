use super::modes::PlayerMode;
use crate::input::InputState;
use crate::physics::{
    move_and_collide, raycast, BlockQuery, RayHit, EYE_HEIGHT, GRAVITY, JUMP_VELOCITY, PLAYER_HEIGHT,
    PLAYER_WIDTH, REACH, TERMINAL_VELOCITY,
};
use glam::Vec3;
use winit::keyboard::KeyCode;

const DOUBLE_TAP_WINDOW: f32 = 0.3;
const CREATIVE_FLY_MULT: f32 = 1.8;
/// Ctrl: corrida no chão.
pub const SPRINT_MULT: f32 = 1.6;
/// Ctrl: velocidade de voo (Criativo e Espectador).
pub const FLY_BOOST_MULT: f32 = 2.5;

pub struct Player {
    /// Posição dos pés (centro da base da hitbox).
    pub position: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub mode: PlayerMode,
    pub flying: bool,
    /// Correndo (Ctrl segurado e andando no chão).
    pub sprinting: bool,
    space_was_down: bool,
    since_space_press: f32,
}

impl Player {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            velocity: Vec3::ZERO,
            on_ground: false,
            mode: PlayerMode::default(),
            flying: false,
            sprinting: false,
            space_was_down: false,
            since_space_press: f32::INFINITY,
        }
    }

    pub fn eye_position(&self) -> Vec3 {
        self.position + Vec3::Y * EYE_HEIGHT
    }

    pub fn set_mode(&mut self, mode: PlayerMode) {
        self.mode = mode;
        self.flying = mode == PlayerMode::Spectator;
        self.velocity = Vec3::ZERO;
    }

    /// Alterna voo (apenas Creative; Spectator sempre voa).
    pub fn toggle_fly(&mut self) {
        if self.mode == PlayerMode::Creative {
            self.flying = !self.flying;
            self.velocity.y = 0.0;
        }
    }

    pub fn target_block(&self, camera_forward: Vec3, world: &dyn BlockQuery) -> Option<RayHit> {
        raycast(self.eye_position(), camera_forward, REACH, world)
    }

    pub fn update(&mut self, dt: f32, input: &InputState, yaw: f32, world: &dyn BlockQuery) {
        let (sy, cy) = yaw.sin_cos();
        let forward = Vec3::new(sy, 0.0, -cy);
        let right = Vec3::new(cy, 0.0, sy);

        let mut wish = Vec3::ZERO;
        if input.is_key_held(KeyCode::KeyW) { wish += forward; }
        if input.is_key_held(KeyCode::KeyS) { wish -= forward; }
        if input.is_key_held(KeyCode::KeyA) { wish -= right; }
        if input.is_key_held(KeyCode::KeyD) { wish += right; }
        let wish = wish.normalize_or_zero();
        let space = input.is_key_held(KeyCode::Space);
        let shift = input.is_key_held(KeyCode::ShiftLeft);
        let ctrl = input.is_key_held(KeyCode::ControlLeft) || input.is_key_held(KeyCode::ControlRight);

        // Duplo toque de Space (Creative) alterna voo.
        self.since_space_press += dt;
        if space && !self.space_was_down {
            if self.since_space_press <= DOUBLE_TAP_WINDOW {
                self.toggle_fly();
                self.since_space_press = f32::INFINITY;
            } else {
                self.since_space_press = 0.0;
            }
        }
        self.space_was_down = space;

        let mut speed = self.mode.speed();
        let vert = (space as i32 - shift as i32) as f32;

        self.sprinting = false;
        if self.mode.noclip() {
            self.flying = true;
            if ctrl { speed *= FLY_BOOST_MULT; }
            self.velocity = wish * speed + Vec3::Y * vert * speed;
            self.position += self.velocity * dt;
            self.on_ground = false;
            return;
        }

        let old_vy = self.velocity.y;
        let mut new_vy;
        if self.flying {
            speed *= CREATIVE_FLY_MULT;
            if ctrl { speed *= FLY_BOOST_MULT; }
            new_vy = vert * speed;
        } else {
            if ctrl && wish != Vec3::ZERO {
                speed *= SPRINT_MULT;
                self.sprinting = true;
            }
            new_vy = (old_vy - GRAVITY * dt).max(-TERMINAL_VELOCITY);
            if space && self.on_ground {
                new_vy = JUMP_VELOCITY;
            }
        }
        let hvel = wish * speed;
        let old_vy = if self.flying { new_vy } else if new_vy == JUMP_VELOCITY && old_vy <= 0.0 { new_vy } else { old_vy };
        // Velocidade média no passo (integração exata para aceleração constante).
        let move_vel = Vec3::new(hvel.x, (old_vy + new_vy) * 0.5, hvel.z);

        let r = move_and_collide(
            self.position - Vec3::new(PLAYER_WIDTH / 2.0, 0.0, PLAYER_WIDTH / 2.0),
            Vec3::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_WIDTH),
            move_vel,
            dt,
            world,
        );
        self.position = r.position + Vec3::new(PLAYER_WIDTH / 2.0, 0.0, PLAYER_WIDTH / 2.0);
        if r.hit_y {
            new_vy = 0.0;
        }
        self.velocity = Vec3::new(if r.hit_x { 0.0 } else { hvel.x }, new_vy, if r.hit_z { 0.0 } else { hvel.z });
        self.on_ground = r.on_ground;
        if self.flying && self.on_ground && self.mode == PlayerMode::Creative && shift {
            self.flying = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_world::FakeWorld;

    fn held(keys: &[KeyCode]) -> InputState {
        let mut i = InputState::new();
        for k in keys { i.update_key(*k, true); }
        i
    }

    fn survival(pos: Vec3) -> Player {
        let mut p = Player::new(pos);
        p.set_mode(PlayerMode::Survival);
        p
    }

    #[test]
    fn falls_and_lands() {
        let w = FakeWorld::flat();
        let mut p = survival(Vec3::new(0.5, 70.0, 0.5));
        let input = InputState::new();
        for _ in 0..120 { p.update(1.0 / 60.0, &input, 0.0, &w); }
        assert!(p.on_ground);
        assert!((p.position.y - 64.0).abs() < 0.01, "{}", p.position.y);
    }

    #[test]
    fn no_tunneling_dt_01() {
        let w = FakeWorld::flat();
        let mut p = survival(Vec3::new(0.5, 100.0, 0.5));
        let input = InputState::new();
        for _ in 0..100 { p.update(0.1, &input, 0.0, &w); }
        assert!(p.position.y >= 64.0 && p.position.y < 64.01, "{}", p.position.y);
    }

    #[test]
    fn jump_height_and_wall() {
        let mut w = FakeWorld::flat();
        let mut p = survival(Vec3::new(0.5, 64.0, 0.5));
        p.update(1.0 / 60.0, &InputState::new(), 0.0, &w);
        assert!(p.on_ground);
        let input = held(&[KeyCode::Space]);
        let mut max_y: f32 = 0.0;
        for _ in 0..60 {
            p.update(1.0 / 60.0, &input, 0.0, &w);
            max_y = max_y.max(p.position.y - 64.0);
            if !p.on_ground { /* segue */ }
        }
        // com Space segurado repete pulos; mede só o primeiro
        assert!(max_y > 1.1 && max_y < 1.4, "{max_y}");

        // Parede de 2 blocos em z-1 (andando para -Z com yaw 0): não escala.
        for y in 64..66 { for x in -1..=1 { w.set(x, y, -2); } }
        let mut p = survival(Vec3::new(0.5, 64.0, 0.5));
        let input = held(&[KeyCode::KeyW, KeyCode::Space]);
        for _ in 0..120 { p.update(1.0 / 60.0, &input, 0.0, &w); }
        assert!(p.position.y < 65.5, "subiu parede de 2: {}", p.position.y);
        assert!(p.position.z > -1.0 + 0.29, "{}", p.position.z);

        // Parede de 1 bloco: sobe.
        let mut w = FakeWorld::flat();
        for x in -1..=1 { w.set(x, 64, -2); }
        let mut p = survival(Vec3::new(0.5, 64.0, 0.5));
        for _ in 0..120 { p.update(1.0 / 60.0, &input, 0.0, &w); }
        assert!(p.position.y > 64.9 || p.position.z < -1.5, "{:?}", p.position);
    }

    #[test]
    fn lateral_collision_forward_convention() {
        let mut w = FakeWorld::flat();
        for y in 64..67 { w.set(0, y, -3); }
        let mut p = survival(Vec3::new(0.5, 64.0, 0.5));
        let input = held(&[KeyCode::KeyW]);
        for _ in 0..120 { p.update(1.0 / 60.0, &input, 0.0, &w); }
        assert!((p.position.z - (-2.0 + 0.3)).abs() < 0.01, "{}", p.position.z);
        // D com yaw 0 vai para +X
        let mut p = survival(Vec3::new(0.5, 64.0, 0.5));
        let input = held(&[KeyCode::KeyD]);
        for _ in 0..30 { p.update(1.0 / 60.0, &input, 0.0, &w); }
        assert!(p.position.x > 1.5);
    }

    #[test]
    fn creative_flight_no_gravity() {
        let w = FakeWorld::flat();
        let mut p = Player::new(Vec3::new(0.5, 80.0, 0.5));
        p.set_mode(PlayerMode::Creative);
        p.toggle_fly();
        assert!(p.flying);
        let input = InputState::new();
        for _ in 0..60 { p.update(1.0 / 60.0, &input, 0.0, &w); }
        assert!((p.position.y - 80.0).abs() < 1e-3);
        // desce com shift e colide com o chão
        let input = held(&[KeyCode::ShiftLeft]);
        for _ in 0..300 { p.update(1.0 / 60.0, &input, 0.0, &w); }
        assert!(p.position.y >= 64.0);
    }

    #[test]
    fn double_tap_space_toggles_fly() {
        let w = FakeWorld::flat();
        let mut p = Player::new(Vec3::new(0.5, 64.0, 0.5));
        let down = held(&[KeyCode::Space]);
        let up = InputState::new();
        p.update(0.016, &down, 0.0, &w);
        p.update(0.016, &up, 0.0, &w);
        p.update(0.016, &down, 0.0, &w);
        assert!(p.flying);
    }

    #[test]
    fn spectator_passes_through() {
        let w = FakeWorld::flat();
        let mut p = Player::new(Vec3::new(0.5, 70.0, 0.5));
        p.set_mode(PlayerMode::Spectator);
        assert!(p.flying);
        let input = held(&[KeyCode::ShiftLeft]);
        for _ in 0..120 { p.update(1.0 / 60.0, &input, 0.0, &w); }
        assert!(p.position.y < 60.0, "{}", p.position.y);
    }

    #[test]
    fn target_block_works() {
        let w = FakeWorld::flat();
        let p = survival(Vec3::new(0.5, 64.0, 0.5));
        let h = p.target_block(Vec3::new(0.0, -1.0, 0.0), &w).unwrap();
        assert_eq!(h.block.y, 63);
    }

    fn run(mut p: Player, keys: &[KeyCode], secs: f32) -> Player {
        let w = FakeWorld::flat();
        let input = held(keys);
        for _ in 0..(secs * 60.0) as usize { p.update(1.0 / 60.0, &input, 0.0, &w); }
        p
    }

    #[test]
    fn ctrl_makes_the_player_run() {
        let walk = run(survival(Vec3::new(0.5, 64.0, 0.5)), &[KeyCode::KeyW], 1.0);
        let sprint = run(survival(Vec3::new(0.5, 64.0, 0.5)), &[KeyCode::KeyW, KeyCode::ControlLeft], 1.0);
        let (dw, ds) = ((walk.position.z - 0.5).abs(), (sprint.position.z - 0.5).abs());
        assert!(sprint.sprinting && !walk.sprinting);
        assert!((ds / dw - SPRINT_MULT).abs() < 0.05, "razão {}", ds / dw);
    }

    #[test]
    fn right_ctrl_also_runs() {
        let a = run(survival(Vec3::new(0.5, 64.0, 0.5)), &[KeyCode::KeyW, KeyCode::ControlLeft], 1.0);
        let b = run(survival(Vec3::new(0.5, 64.0, 0.5)), &[KeyCode::KeyW, KeyCode::ControlRight], 1.0);
        assert!(b.sprinting);
        assert!((a.position.z - b.position.z).abs() < 1e-4);
    }

    #[test]
    fn ctrl_alone_does_not_sprint() {
        let p = run(survival(Vec3::new(0.5, 64.0, 0.5)), &[KeyCode::ControlLeft], 0.5);
        assert!(!p.sprinting);
        assert!((p.position.z - 0.5).abs() < 1e-3);
    }

    #[test]
    fn ctrl_speeds_up_creative_flight() {
        let mk = || {
            let mut p = Player::new(Vec3::new(0.5, 90.0, 0.5));
            p.set_mode(PlayerMode::Creative);
            p.toggle_fly();
            p
        };
        let slow = run(mk(), &[KeyCode::KeyW], 1.0);
        let fast = run(mk(), &[KeyCode::KeyW, KeyCode::ControlLeft], 1.0);
        assert!(slow.flying && fast.flying);
        let ratio = (fast.position.z - 0.5).abs() / (slow.position.z - 0.5).abs();
        assert!((ratio - FLY_BOOST_MULT).abs() < 0.05, "razão {ratio}");
        // também sobe mais rápido
        let up_slow = run(mk(), &[KeyCode::Space], 0.5);
        let up_fast = run(mk(), &[KeyCode::Space, KeyCode::ControlLeft], 0.5);
        assert!(up_fast.position.y - 90.0 > (up_slow.position.y - 90.0) * 2.0);
    }

    #[test]
    fn ctrl_speeds_up_spectator_flight() {
        let mk = || {
            let mut p = Player::new(Vec3::new(0.5, 90.0, 0.5));
            p.set_mode(PlayerMode::Spectator);
            p
        };
        let slow = run(mk(), &[KeyCode::KeyW], 1.0);
        let fast = run(mk(), &[KeyCode::KeyW, KeyCode::ControlLeft], 1.0);
        let ratio = (fast.position.z - 0.5).abs() / (slow.position.z - 0.5).abs();
        assert!((ratio - FLY_BOOST_MULT).abs() < 0.05, "razão {ratio}");
    }
}
