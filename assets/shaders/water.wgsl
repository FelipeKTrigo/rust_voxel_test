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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sun_dir = normalize(vec3<f32>(0.6, 1.0, 0.4));
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
    let diffuse = max(dot(n, sun_dir), 0.0);
    var rgb = in.color.rgb * (0.45 + 0.55 * diffuse);

    // Brilho do sol e espuma leve nas cristas.
    let spec = pow(max(dot(reflect(-sun_dir, n), view), 0.0), 48.0);
    rgb = rgb + vec3<f32>(spec * 0.6);
    if (in.wave > 0.5) {
        let crest = smoothstep(0.07, 0.115, wave_height(in.world_pos.x, in.world_pos.z, t));
        rgb = mix(rgb, vec3<f32>(0.9, 0.96, 1.0), crest * 0.5);
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
