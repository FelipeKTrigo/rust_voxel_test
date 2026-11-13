// Prelúdio compartilhado (concatenado antes de voxel/water/fish/sky no Rust).
struct CameraUniform {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    time: f32,
    // x = câmera debaixo d'água (0/1), y = nível do mar, z = início do fog, w = fim do fog
    params: vec4<f32>,
    // xyz = direção do sol, w = luz do dia (0 noite .. 1 dia)
    sun: vec4<f32>,
    // xyz = direção da lua, w = fase (0 nova, 0.5 cheia)
    moon: vec4<f32>,
    // x = hora do dia (0..1), y = dia, zw = reservado
    sky: vec4<f32>,
    // cometa: xyz = origem, w = progresso (<0 = sem cometa)
    comet_a: vec4<f32>,
    // xyz = destino
    comet_b: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;

const PI: f32 = 3.14159265;
const DEEP_COLOR: vec3<f32> = vec3<f32>(0.01, 0.05, 0.12);
const UNDERWATER_FOG_END: f32 = 36.0;

// Mesma fórmula de `wave_height` em src/fauna/mod.rs (barcos acompanham a onda).
fn wave_height(x: f32, z: f32, t: f32) -> f32 {
    return 0.05 * sin(x * 0.9 + t * 1.6)
         + 0.04 * sin(z * 1.1 + t * 1.3)
         + 0.03 * sin((x + z) * 0.5 + t * 0.9);
}

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 456.21));
    q = q + dot(q, q + 45.32);
    return fract(q.x * q.y);
}

fn daylight() -> f32 {
    return camera.sun.w;
}

// 0 na lua nova, 1 na cheia
fn moon_phase_light() -> f32 {
    return 0.5 - 0.5 * cos(camera.moon.w * 2.0 * PI);
}

// Luz efetiva da lua (fase × altura no céu)
fn moon_light() -> f32 {
    return moon_phase_light() * smoothstep(-0.05, 0.2, camera.moon.y);
}

// Cor do céu na direção `dir`: gradiente dia/noite + brilho de nascer/pôr do sol.
// O fog usa a MESMA função, então não há emenda entre terreno distante e céu.
fn sky_base(dir: vec3<f32>) -> vec3<f32> {
    let day = daylight();
    let ml = moon_light();
    let t = pow(clamp(dir.y, 0.0, 1.0), 0.55);
    let day_col = mix(vec3<f32>(0.66, 0.82, 0.98), vec3<f32>(0.20, 0.42, 0.84), t);
    let night_h = vec3<f32>(0.035, 0.05, 0.10) + vec3<f32>(0.03, 0.04, 0.07) * ml;
    let night_z = vec3<f32>(0.004, 0.008, 0.030) + vec3<f32>(0.010, 0.015, 0.030) * ml;
    var col = mix(mix(night_h, night_z, t), day_col, day);

    // Crepúsculo: faixa laranja/rosa no horizonte, mais forte do lado do sol.
    let sh = camera.sun.y;
    let twi = clamp(1.0 - abs(sh - 0.04) / 0.34, 0.0, 1.0);
    let hdir = normalize(vec2<f32>(dir.x, dir.z) + vec2<f32>(1e-5, 0.0));
    let sdir = normalize(vec2<f32>(camera.sun.x, camera.sun.z) + vec2<f32>(1e-5, 0.0));
    let toward = clamp(dot(hdir, sdir), 0.0, 1.0);
    let band = exp(-abs(dir.y) * 3.5);
    let glow = twi * band * (0.30 + 0.70 * pow(toward, 2.5));
    let warm = mix(vec3<f32>(1.0, 0.52, 0.20), vec3<f32>(0.95, 0.36, 0.38), clamp(0.5 - sh * 2.0, 0.0, 1.0));
    col = mix(col, warm, clamp(glow, 0.0, 0.95));
    col = col + vec3<f32>(0.10, 0.03, 0.14) * twi * t * 0.6;
    return col;
}

// Iluminação da cena: sol (com cor quente ao amanhecer/entardecer), lua e ambiente.
fn scene_light(n: vec3<f32>) -> vec3<f32> {
    let day = daylight();
    let ml = moon_light();
    let ambient = mix(vec3<f32>(0.07, 0.09, 0.17) + vec3<f32>(0.05, 0.07, 0.11) * ml,
                      vec3<f32>(0.34, 0.35, 0.40), day);
    let warm = smoothstep(0.0, 0.45, camera.sun.y);
    let sun_col = mix(vec3<f32>(1.0, 0.55, 0.28), vec3<f32>(1.0, 0.96, 0.88), warm) * 0.70;
    let moon_col = vec3<f32>(0.30, 0.40, 0.70) * ml * 0.35;
    let sun_term = max(dot(n, camera.sun.xyz), 0.0) * day;
    let moon_term = max(dot(n, camera.moon.xyz), 0.0);
    return ambient + sun_col * sun_term + moon_col * moon_term;
}

fn deep_color() -> vec3<f32> {
    return DEEP_COLOR * (0.2 + 0.8 * daylight());
}

fn apply_fog(color: vec3<f32>, world_pos: vec3<f32>) -> vec3<f32> {
    let to = world_pos - camera.camera_pos;
    let dist = length(to);
    if (camera.params.x > 0.5) {
        // Submerso: pouca visibilidade e escuro.
        let f = clamp(dist / UNDERWATER_FOG_END, 0.0, 1.0);
        return mix(color * 0.75, deep_color(), f * f * 0.6 + f * 0.4);
    }
    let f = clamp((dist - camera.params.z) / max(camera.params.w - camera.params.z, 1.0), 0.0, 1.0);
    let d = to / max(dist, 1e-4);
    let fog = sky_base(normalize(vec3<f32>(d.x, 0.04 + max(d.y, 0.0) * 0.3, d.z)));
    return mix(color, fog, f);
}
