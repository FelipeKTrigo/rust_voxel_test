// Ruído comum (simplex 2D com hash inteiro, sem tabelas). Espelhado em Rust (cpu_columns).
struct Params {
    seed: u32,
    count: u32,
    _p0: u32,
    _p1: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> chunks: array<vec2<i32>>;
@group(0) @binding(2) var<storage, read_write> cols: array<u32>;

fn pcg(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

fn hash2(ix: i32, iz: i32, salt: u32) -> u32 {
    var h = params.seed ^ (salt * 2654435769u);
    h = pcg(h + u32(ix));
    h = pcg(h + u32(iz));
    return h;
}

fn grad(h: u32) -> vec2<f32> {
    let a = f32(h & 1023u) * (6.2831853 / 1024.0);
    return vec2<f32>(cos(a), sin(a));
}

fn snoise(p: vec2<f32>, salt: u32) -> f32 {
    let K1 = 0.3660254;
    let K2 = 0.21132487;
    let s = (p.x + p.y) * K1;
    let i = floor(p.x + s);
    let j = floor(p.y + s);
    let t = (i + j) * K2;
    let x0 = p.x - (i - t);
    let y0 = p.y - (j - t);
    var i1x = 0;
    var i1y = 1;
    if (x0 > y0) { i1x = 1; i1y = 0; }
    let x1 = x0 - f32(i1x) + K2;
    let y1 = y0 - f32(i1y) + K2;
    let x2 = x0 - 1.0 + 2.0 * K2;
    let y2 = y0 - 1.0 + 2.0 * K2;
    let ii = i32(i);
    let jj = i32(j);
    var n = 0.0;
    var tt = 0.5 - x0 * x0 - y0 * y0;
    if (tt > 0.0) {
        tt = tt * tt;
        n += tt * tt * dot(grad(hash2(ii, jj, salt)), vec2<f32>(x0, y0));
    }
    tt = 0.5 - x1 * x1 - y1 * y1;
    if (tt > 0.0) {
        tt = tt * tt;
        n += tt * tt * dot(grad(hash2(ii + i1x, jj + i1y, salt)), vec2<f32>(x1, y1));
    }
    tt = 0.5 - x2 * x2 - y2 * y2;
    if (tt > 0.0) {
        tt = tt * tt;
        n += tt * tt * dot(grad(hash2(ii + 1, jj + 1, salt)), vec2<f32>(x2, y2));
    }
    return clamp(n * 70.0, -1.0, 1.0);
}

fn fbm(p: vec2<f32>, salt: u32, oct: u32) -> f32 {
    var sum = 0.0;
    var amp = 1.0;
    var f = 1.0;
    var norm = 0.0;
    for (var o = 0u; o < oct; o++) {
        sum += amp * snoise(p * f, salt + o);
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    return sum / norm;
}

// Ridged multifractal: 0..1, cristas nítidas.
fn ridgef(p: vec2<f32>, salt: u32, oct: u32) -> f32 {
    var sum = 0.0;
    var amp = 1.0;
    var f = 1.0;
    var norm = 0.0;
    for (var o = 0u; o < oct; o++) {
        var n = 1.0 - abs(snoise(p * f, salt + o));
        n = n * n;
        sum += amp * n;
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    return sum / norm;
}
