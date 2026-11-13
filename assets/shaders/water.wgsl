struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) wave: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(3) wave: f32,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    var p = in.position;
    // Só a superfície se move; usa coordenadas de MUNDO, então vértices
    // compartilhados entre quads/chunks se movem juntos (sem frestas).
    if (in.wave > 0.5) {
        p.y = p.y - 0.12 + wave_height(p.x, p.z, camera.time);
    }
    out.clip_position = camera.view_proj * vec4<f32>(p, 1.0);
    out.color = in.color;
    out.normal = in.normal;
    out.world_pos = p;
    out.wave = in.wave;
    return out;
}

// Mar "pixelado": quadrados com tom aleatório que derivam devagar + brilhos que piscam,
// em vez de círculos concêntricos.
fn sea_pixels(p: vec2<f32>, t: f32) -> f32 {
    let drift = vec2<f32>(t * 0.30, t * 0.17);
    let big = hash21(floor(p * 0.5 + drift * 0.5) + 3.0);     // quadrados de 2 blocos
    let mid = hash21(floor(p + drift) + 11.0);                // quadrados de 1 bloco
    let small = hash21(floor(p * 2.0) + floor(t * 1.3) * 7.31); // 0,5 bloco, troca no tempo
    var tone = 0.90 + 0.10 * big + 0.12 * (mid - 0.5);
    if (small > 0.94) { tone = tone + 0.22; }
    if (small < 0.06) { tone = tone - 0.14; }
    return tone;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = camera.time;
    let e = 0.2;

    // Normal da onda por diferenças finitas (só na superfície).
    var n = normalize(in.normal);
    if (in.wave > 0.5) {
        let h = wave_height(in.world_pos.x, in.world_pos.z, t);
        let hx = wave_height(in.world_pos.x + e, in.world_pos.z, t);
        let hz = wave_height(in.world_pos.x, in.world_pos.z + e, t);
        n = normalize(vec3<f32>(-(hx - h) / e, 1.0, -(hz - h) / e));
    }

    let view = normalize(camera.camera_pos - in.world_pos);
    let day = daylight();
    let ml = moon_light();
    var p = in.world_pos.xz;
    if (abs(in.normal.y) < 0.5) { p = vec2<f32>(in.world_pos.x + in.world_pos.z, in.world_pos.y); }

    var rgb = in.color.rgb * sea_pixels(p, t) * scene_light(n) * 1.15;

    // Reflexo do sol (de dia) ou da lua (de noite): faíscas em quadrados aleatórios, sem
    // depender do desenho das ondas (que formaria círculos).
    let light_dir = select(camera.moon.xyz, camera.sun.xyz, day > 0.15);
    let light_amt = select(ml * 0.5, day * 0.8, day > 0.15);
    let spec = pow(max(dot(reflect(-light_dir, n), view), 0.0), 12.0);
    let glint = hash21(floor(p * 2.0) + floor(t * 2.0) * 3.7);
    if (glint > 1.0 - 0.5 * spec) {
        rgb = rgb + vec3<f32>(0.60, 0.56, 0.48) * light_amt;
    }
    // Poucas cristas brancas, também quadradas e aleatórias
    if (in.wave > 0.5 && hash21(floor(p * 1.0) + floor(t * 0.7) * 5.3) > 0.975) {
        rgb = mix(rgb, vec3<f32>(0.9, 0.96, 1.0) * (0.35 + 0.65 * day), 0.55);
    }

    // Mais opaca em ângulos rasantes (fresnel) e quando vista de baixo.
    let fres = pow(1.0 - clamp(abs(dot(view, n)), 0.0, 1.0), 3.0);
    var alpha = mix(in.color.a, 0.95, fres);
    if (camera.params.x > 0.5) {
        rgb = rgb * 0.6;
        alpha = 0.85;
    }
    return vec4<f32>(apply_fog(rgb, in.world_pos), alpha);
}
