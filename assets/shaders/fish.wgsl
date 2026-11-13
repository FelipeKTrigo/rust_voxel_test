// Peixes 2D (pixel art) como billboards cilíndricos instanciados.
@group(1) @binding(0) var fish_tex: texture_2d<f32>;
@group(1) @binding(1) var fish_samp: sampler;

struct Instance {
    @location(0) pos_size: vec4<f32>,   // xyz = centro, w = tamanho (largura em blocos)
    @location(1) dir: vec2<f32>,        // direção horizontal de nado (xz)
    @location(2) frame: f32,            // 0 ou 1 (frame da cauda)
    @location(3) tint: f32,             // 0..1 variação de cor
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: f32,
    @location(2) world_pos: vec3<f32>,
}

const FRAMES: f32 = 2.0;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corners[vi];
    let center = inst.pos_size.xyz;
    let size = inst.pos_size.w;

    let to_cam = camera.camera_pos - center;
    var f = normalize(vec3<f32>(to_cam.x, 0.0, to_cam.z) + vec3<f32>(0.0001, 0.0, 0.0));
    let right = vec3<f32>(f.z, 0.0, -f.x);

    // Sprite desenhado olhando para a direita da tela: espelha conforme o rumo.
    let facing_right = dot(vec3<f32>(inst.dir.x, 0.0, inst.dir.y), right) >= 0.0;
    var u = c.x;
    if (!facing_right) { u = 1.0 - c.x; }

    let local = vec2<f32>(c.x - 0.5, c.y - 0.5);
    let world = center + right * (local.x * size) + vec3<f32>(0.0, 1.0, 0.0) * (local.y * size * 0.5);

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world, 1.0);
    out.uv = vec2<f32>((u + inst.frame) / FRAMES, 1.0 - c.y);
    out.tint = inst.tint;
    out.world_pos = world;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(fish_tex, fish_samp, in.uv);
    if (texel.a < 0.5) {
        discard;
    }
    let hue = mix(vec3<f32>(1.0, 0.55, 0.25), vec3<f32>(0.5, 0.75, 1.0), in.tint);
    let rgb = texel.rgb * hue;
    return vec4<f32>(apply_fog(rgb, in.world_pos), 1.0);
}
