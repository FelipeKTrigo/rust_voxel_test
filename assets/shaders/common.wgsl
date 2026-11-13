// Prelúdio compartilhado (concatenado antes de voxel/water/fish no Rust).
struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    time: f32,
    // x = câmera debaixo d'água (0/1), y = nível do mar, zw = reservado
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;

const SKY_COLOR: vec3<f32> = vec3<f32>(0.53, 0.81, 0.98);
const DEEP_COLOR: vec3<f32> = vec3<f32>(0.01, 0.05, 0.12);
const UNDERWATER_FOG_END: f32 = 36.0;

// Mesma fórmula de `wave_height` em src/fauna/mod.rs (barcos acompanham a onda).
fn wave_height(x: f32, z: f32, t: f32) -> f32 {
    return 0.05 * sin(x * 0.9 + t * 1.6)
         + 0.04 * sin(z * 1.1 + t * 1.3)
         + 0.03 * sin((x + z) * 0.5 + t * 0.9);
}

fn apply_fog(color: vec3<f32>, world_pos: vec3<f32>) -> vec3<f32> {
    let dist = length(world_pos - camera.camera_pos);
    if (camera.params.x > 0.5) {
        // Submerso: pouca visibilidade e escuro.
        let f = clamp(dist / UNDERWATER_FOG_END, 0.0, 1.0);
        return mix(color * 0.75, DEEP_COLOR, f * f * 0.6 + f * 0.4);
    }
    let f = clamp((dist - 200.0) / 200.0, 0.0, 1.0);
    return mix(color, SKY_COLOR, f);
}
