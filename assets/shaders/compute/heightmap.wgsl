// Heightmap + resultado final por coluna. Concatenado após noise.wgsl e biome.wgsl.
const SEA: i32 = 63;

fn biome_height(b: u32, rolling: f32, detail: f32, dunes: f32, ridge: f32) -> f32 {
    switch b {
        case 0u: { return clamp(70.0 + 4.0 * rolling + 1.0 * detail, 65.0, 75.0); }
        case 1u: { return clamp(80.0 + 8.0 * rolling + 2.0 * detail, 70.0, 90.0); }
        case 2u: { return clamp(100.0 + 100.0 * ridge + 6.0 * rolling, 100.0, 200.0); }
        case 3u: { return 65.5 + 1.5 * detail; }
        case 4u: { return clamp(80.0 + 12.0 * dunes + 3.0 * rolling, 65.0, 95.0); }
        default: { return clamp(50.0 + 6.0 * rolling + 2.0 * detail, 30.0, 61.0); }
    }
}

// Retorna (altura, bioma, bioma2, blend)
fn column(p: vec2<f32>) -> vec4<u32> {
    let cx0 = i32(floor(p.x / CELL));
    let cz0 = i32(floor(p.y / CELL));
    var ds: array<f32, 9>;
    var bs: array<u32, 9>;
    var i1 = 0;
    var i2 = -1;
    for (var k = 0; k < 9; k++) {
        let cx = cx0 + (k % 3) - 1;
        let cz = cz0 + (k / 3) - 1;
        let cp = cell_point(cx, cz);
        ds[k] = distance(p, cp);
        bs[k] = classify(cp);
    }
    for (var k = 1; k < 9; k++) {
        if (ds[k] < ds[i1]) { i2 = i1; i1 = k; }
        else if (i2 < 0 || ds[k] < ds[i2]) { i2 = k; }
    }
    let d1 = ds[i1];
    let d2 = ds[i2];

    let rolling = clamp(fbm(p * 0.004, 5u, 4u) * 1.5, -1.0, 1.0);
    let detail = snoise(p * 0.03, 6u);
    let dunes = snoise(vec2<f32>(p.x * 0.012, p.y * 0.03), 7u);
    let ridge = ridgef(p * 0.0035, 8u, 4u);

    var wsum = 0.0;
    var hsum = 0.0;
    var wocean = 0.0;
    var wmount = 0.0;
    for (var k = 0; k < 9; k++) {
        let w = exp(-(ds[k] - d1) / BLEND_SCALE);
        wsum += w;
        hsum += w * biome_height(bs[k], rolling, detail, dunes, ridge);
        if (bs[k] == 5u) { wocean += w; }
        if (bs[k] == 2u) { wmount += w; }
    }
    var h = hsum / wsum;
    wocean = wocean / wsum;
    wmount = wmount / wsum;

    // Ravinas
    let rv = abs(snoise(p * 0.0045, 9u));
    let RW = 0.09;
    if (rv < RW) {
        let kk = 1.0 - rv / RW;
        let carve = kk * kk * (8.0 + 22.0 * wmount);
        h -= carve * clamp((h - 66.0) / 10.0, 0.0, 1.0);
    }
    h = clamp(h, 1.0, 255.0);
    let hi = i32(floor(h));

    let b1 = bs[i1];
    let b2 = bs[i2];
    var bio = b1;
    var bio2 = b2;
    if (hi < SEA) {
        if (b1 != 5u) { bio2 = b1; }
        bio = 5u;
    } else if (b1 == 5u && hi > 68) {
        bio = select(b2, 0u, b2 == 5u);
        bio2 = b1;
    } else if (hi <= 68 && b1 != 2u && (b1 == 5u || wocean > 0.02)) {
        bio = 3u;
        if (b1 != 5u) { bio2 = b1; }
    }
    let bl = u32(clamp(127.5 * exp(-(d2 - d1) / BLEND_SCALE), 0.0, 255.0));
    return vec4<u32>(u32(hi), bio, bio2, bl);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.z >= params.count) { return; }
    let cp = chunks[gid.z];
    let p = vec2<f32>(f32(cp.x * 32 + i32(gid.x)), f32(cp.y * 32 + i32(gid.y)));
    let c = column(p);
    let idx = gid.z * 1024u + gid.y * 32u + gid.x;
    cols[idx * 2u] = c.x | (c.y << 16u) | (c.z << 24u);
    cols[idx * 2u + 1u] = c.w;
}
