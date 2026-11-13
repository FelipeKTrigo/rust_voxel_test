struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    out.normal = in.normal;
    out.world_pos = in.position;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sun_dir = normalize(vec3<f32>(0.6, 1.0, 0.4));

    let ambient = 0.25;
    let diffuse = max(dot(in.normal, sun_dir), 0.0);
    let light = ambient + diffuse * 0.75;

    // Abaixo do nível do mar a luz vai sumindo: o fundo do oceano é escuro.
    let depth = clamp((camera.params.y - in.world_pos.y) / 26.0, 0.0, 1.0);
    let dark = mix(1.0, 0.4, depth);

    let lit_color = in.color.rgb * light * dark;
    return vec4<f32>(apply_fog(lit_color, in.world_pos), in.color.a);
}
