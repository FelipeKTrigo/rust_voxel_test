struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _padding: f32,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;

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
    let sky_color = vec3<f32>(0.53, 0.81, 0.98);

    let ambient = 0.25;
    let diffuse = max(dot(in.normal, sun_dir), 0.0);
    let light = ambient + diffuse * 0.75;

    let lit_color = in.color.rgb * light;

    // Fog
    let dist = length(in.world_pos - camera.camera_pos);
    let fog_start = 200.0;
    let fog_end = 400.0;
    let fog_factor = clamp((dist - fog_start) / (fog_end - fog_start), 0.0, 1.0);

    let final_color = mix(lit_color, sky_color, fog_factor);
    return vec4<f32>(final_color, in.color.a);
}
