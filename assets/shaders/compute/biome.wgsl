// Biomas: Voronoi (células jitter) + clima (temperatura/umidade/continentalidade).
// IDs = enum Biome: 0 Plains,1 Forest,2 Mountains,3 Beach,4 Desert,5 Ocean
const CELL: f32 = 160.0;
const BLEND_SCALE: f32 = 20.0;

fn cell_point(cx: i32, cz: i32) -> vec2<f32> {
    let h = hash2(cx, cz, 100u);
    let fx = f32(h & 0xffffu) / 65535.0;
    let fz = f32(h >> 16u) / 65535.0;
    return vec2<f32>(f32(cx) + 0.2 + 0.6 * fx, f32(cz) + 0.2 + 0.6 * fz) * CELL;
}

fn classify(p: vec2<f32>) -> u32 {
    let temp = clamp(fbm(p * 0.0012, 1u, 3u) * 1.8, -1.0, 1.0);
    let humid = clamp(fbm(p * 0.0012, 2u, 3u) * 1.8, -1.0, 1.0);
    let cont = clamp(fbm(p * 0.0008, 3u, 3u) * 1.8, -1.0, 1.0);
    let mnt = clamp(fbm(p * 0.0014, 4u, 3u) * 1.8, -1.0, 1.0);
    if (cont < -0.2) { return 5u; }
    if (mnt > 0.25 || (temp < -0.4 && humid < 0.0)) { return 2u; }
    if (temp > 0.2 && humid < 0.1) { return 4u; }
    if (humid > 0.0) { return 1u; }
    return 0u;
}
