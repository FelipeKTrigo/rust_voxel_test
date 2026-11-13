use glam::{Mat4, Vec3, Vec4};
use crate::input::InputState;
use winit::keyboard::KeyCode;

pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn new(position: Vec3, aspect: f32) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 70.0_f32.to_radians(),
            aspect,
            near: 0.1,
            far: 500.0,
        }
    }

    /// Forward vector in Y-up right-handed coordinates
    /// yaw=0 → looking toward -Z, positive yaw turns right
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(cp * sy, sp, -cp * cy)
    }

    pub fn right(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        Vec3::new(cy, 0.0, sy)
    }

    pub fn view_matrix(&self) -> Mat4 {
        let target = self.position + self.forward();
        Mat4::look_at_rh(self.position, target, Vec3::Y)
    }

    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far)
    }

    pub fn view_proj(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    /// Apply mouse look: dx = horizontal (yaw), dy = vertical (pitch)
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        const SENSITIVITY: f32 = 0.002;
        self.yaw += dx * SENSITIVITY;
        self.pitch -= dy * SENSITIVITY;
        self.pitch = self.pitch.clamp(-1.5, 1.5); // ~86 degrees
    }

    /// Move camera based on input
    /// `horizontal_only`: when true (Survival), only move on the XZ plane
    pub fn update_movement(
        &mut self,
        input: &InputState,
        dt: f32,
        speed: f32,
        horizontal_only: bool,
    ) {
        let mut move_dir = Vec3::ZERO;

        let forward = if horizontal_only {
            let f = self.forward();
            Vec3::new(f.x, 0.0, f.z).normalize_or_zero()
        } else {
            self.forward()
        };
        let right = self.right();
        let up = Vec3::Y;

        if input.is_key_held(KeyCode::KeyW) { move_dir += forward; }
        if input.is_key_held(KeyCode::KeyS) { move_dir -= forward; }
        if input.is_key_held(KeyCode::KeyA) { move_dir -= right; }
        if input.is_key_held(KeyCode::KeyD) { move_dir += right; }

        if !horizontal_only {
            if input.is_key_held(KeyCode::Space) { move_dir += up; }
            if input.is_key_held(KeyCode::ShiftLeft) { move_dir -= up; }
        } else {
            // Survival: Space = jump, Shift = duck (simple vertical for now)
            if input.is_key_held(KeyCode::Space) { move_dir += up; }
            if input.is_key_held(KeyCode::ShiftLeft) { move_dir -= up; }
        }

        if move_dir != Vec3::ZERO {
            self.position += move_dir.normalize() * speed * dt;
        }
    }

    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }

    /// Ajusta o plano distante para cobrir a distância de renderização
    /// (diagonal do quadrado de chunks + folga), nunca abaixo de 500.
    pub fn set_far_for_render_distance(&mut self, render_distance: i32) {
        let blocks = (render_distance.max(1) + 2) as f32 * crate::world::chunk::CHUNK_SIZE as f32;
        self.far = (blocks * std::f32::consts::SQRT_2).max(500.0);
    }

    pub fn frustum(&self) -> Frustum {
        Frustum::from_view_proj(&self.view_proj())
    }
}

/// Frustum de visão (6 planos) para descartar chunks fora da câmera.
#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    /// Extração de planos (Gribb–Hartmann) para profundidade clip 0..1 do wgpu.
    pub fn from_view_proj(m: &Mat4) -> Self {
        let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
        let raw = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2];
        let planes = raw.map(|p| {
            let len = p.truncate().length();
            if len > 0.0 { p / len } else { p }
        });
        Self { planes }
    }

    /// `false` somente se a caixa está totalmente fora de algum plano.
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        self.planes.iter().all(|p| {
            let v = Vec3::new(
                if p.x >= 0.0 { max.x } else { min.x },
                if p.y >= 0.0 { max.y } else { min.y },
                if p.z >= 0.0 { max.z } else { min.z },
            );
            p.truncate().dot(v) + p.w >= 0.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam() -> Camera {
        // Em (0,0,0) olhando para -Z (yaw=0, pitch=0)
        Camera::new(Vec3::ZERO, 16.0 / 9.0)
    }

    #[test]
    fn box_in_front_is_visible_behind_is_not() {
        let f = cam().frustum();
        let cube = |c: Vec3| (c - Vec3::splat(1.0), c + Vec3::splat(1.0));
        let (a, b) = cube(Vec3::new(0.0, 0.0, -20.0));
        assert!(f.intersects_aabb(a, b), "à frente");
        let (a, b) = cube(Vec3::new(0.0, 0.0, 20.0));
        assert!(!f.intersects_aabb(a, b), "atrás");
        let (a, b) = cube(Vec3::new(500.0, 0.0, -20.0));
        assert!(!f.intersects_aabb(a, b), "muito à direita");
        let (a, b) = cube(Vec3::new(0.0, 0.0, -2000.0));
        assert!(!f.intersects_aabb(a, b), "além do plano distante");
    }

    #[test]
    fn straddling_box_is_kept() {
        let f = cam().frustum();
        // Grande caixa que contém a câmera
        assert!(f.intersects_aabb(Vec3::splat(-50.0), Vec3::splat(50.0)));
    }

    #[test]
    fn rotation_changes_visibility() {
        let mut c = cam();
        let target = (Vec3::new(30.0, -1.0, 0.0), Vec3::new(34.0, 1.0, 4.0)); // lado +X
        assert!(!c.frustum().intersects_aabb(target.0, target.1));
        c.yaw = std::f32::consts::FRAC_PI_2; // olhando para +X
        assert!(c.frustum().intersects_aabb(target.0, target.1));
    }

    #[test]
    fn far_plane_grows_with_render_distance() {
        let mut c = cam();
        c.set_far_for_render_distance(4);
        assert_eq!(c.far, 500.0);
        c.set_far_for_render_distance(24);
        assert!(c.far >= 24.0 * 32.0 * 1.4);
    }
}
