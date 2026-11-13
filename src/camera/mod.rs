use glam::{Mat4, Vec3};
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
}
