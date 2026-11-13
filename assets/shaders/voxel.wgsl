@group(1) @binding(0) var block_tex: texture_2d_array<f32>;
@group(1) @binding(1) var block_samp: sampler;

const NO_TEXTURE: u32 = 65535u;
const TEX_LAYERS: u32 = 96u;
// Bit na camada: não variar o tile por bloco (itens soltos e outros cubos pequenos).
const NO_VARIATION: u32 = 16384u;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) layer: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) @interpolate(flat) layer: u32,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    out.normal = in.normal;
    out.world_pos = in.position;
    // v cresce para cima no mundo; a linha 0 da textura é o topo.
    out.uv = vec2<f32>(in.uv.x, -in.uv.y);
    out.layer = in.layer;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let raw = in.layer;
    let layer = min(raw & 16383u, TEX_LAYERS - 1u);

    // Variação por bloco: espelha/gira o tile conforme a posição do bloco para a pedra, a terra
    // etc. não formarem uma grade repetida. Laterais só espelham (grama/madeira têm "cima").
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    var f = fract(in.uv);
    if ((raw & NO_VARIATION) == 0u) {
        let bp = floor(in.world_pos - n * 0.01);
        let h = u32(hash21(bp.xz + vec2<f32>(bp.y * 7.31, bp.y * 3.17)) * 8.0);
        let kind = layer % 3u;
        if ((h & 1u) != 0u) { f.x = 1.0 - f.x; }
        if (kind != 1u) {
            if ((h & 2u) != 0u) { f.y = 1.0 - f.y; }
            if ((h & 4u) != 0u) { f = vec2<f32>(f.y, f.x); }
        }
    }
    // Amostra sempre (derivadas em fluxo uniforme) e escolhe depois.
    let tex = textureSampleGrad(block_tex, block_samp, f, i32(layer), dx, dy).rgb;
    let base = select(in.color.rgb, tex, raw != NO_TEXTURE);

    // Abaixo do nível do mar a luz vai sumindo: o fundo do oceano é escuro.
    let depth = clamp((camera.params.y - in.world_pos.y) / 26.0, 0.0, 1.0);
    let dark = mix(1.0, 0.4, depth);

    let lit_color = base * scene_light(n) * dark;
    return vec4<f32>(apply_fog(lit_color, in.world_pos), in.color.a);
}
