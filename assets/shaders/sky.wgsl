// Céu: gradiente + sol quadrado, lua com fases, estrelas, nuvens pixeladas e cometas.
struct SkyOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> SkyOut {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: SkyOut;
    o.pos = vec4<f32>(p[vi], 1.0, 1.0);
    o.ndc = p[vi];
    return o;
}

const CELLS: f32 = 210.0; // células de "pixel" por radiano

// Hash inteiro por célula (sem perda de precisão para coordenadas grandes).
fn cell_hash(c: vec2<f32>, seed: f32) -> f32 {
    let p = vec2<i32>(c) + vec2<i32>(i32(seed) * 131, i32(seed) * 71);
    var h = u32(p.x) * 374761393u + u32(p.y) * 668265263u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    h = h ^ (h >> 16u);
    return f32(h & 65535u) / 65536.0;
}


// Células de "pixel" por projeção em cubo: quase quadradas em qualquer parte do céu.
fn sky_cell(d: vec3<f32>, n: f32) -> vec2<f32> {
    let a = abs(d);
    var uv: vec2<f32>;
    var face: f32;
    if (a.x >= a.y && a.x >= a.z) {
        uv = d.yz / a.x;
        face = select(0.0, 1.0, d.x < 0.0);
    } else if (a.y >= a.z) {
        uv = d.xz / a.y;
        face = select(2.0, 3.0, d.y < 0.0);
    } else {
        uv = d.xy / a.z;
        face = select(4.0, 5.0, d.z < 0.0);
    }
    return floor(uv * n) + vec2<f32>(face * 4096.0, 0.0);
}

fn rot_z(d: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(d.x * c - d.y * s, d.x * s + d.y * c, d.z);
}

fn star_layer(d: vec3<f32>, n: f32, thresh: f32, seed: f32) -> vec4<f32> {
    let c = sky_cell(d, n);
    let h = cell_hash(c, seed);
    if (h > thresh) { return vec4<f32>(0.0); }
    let b = cell_hash(c, seed + 17.0);
    let tw = 0.78 + 0.22 * sin(camera.time * (0.8 + b * 2.5) + b * 40.0);
    let tint = cell_hash(c, seed + 29.0);
    var col = vec3<f32>(1.0, 1.0, 1.0);
    if (tint < 0.25) { col = vec3<f32>(0.75, 0.85, 1.0); }
    else if (tint > 0.8) { col = vec3<f32>(1.0, 0.88, 0.7); }
    return vec4<f32>(col, (0.30 + 0.70 * b) * tw);
}

fn disc_basis(center: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    let right = normalize(cross(center, vec3<f32>(0.0, 1.0, 0.0)));
    let up = cross(right, center);
    return vec3<f32>(dot(d, right), dot(d, up), dot(d, center));
}

fn vnoise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm2(p: vec2<f32>) -> f32 {
    return 0.55 * vnoise2(p) + 0.30 * vnoise2(p * 2.1 + 7.3) + 0.15 * vnoise2(p * 4.3 + 19.1);
}

// Nuvens em células quadradas que derivam com o vento. rgb + alpha.
fn clouds(d: vec3<f32>) -> vec4<f32> {
    if (d.y < 0.012) { return vec4<f32>(0.0); }
    let proj = d.xz / (d.y + 0.07);
    let wind = vec2<f32>(camera.time * 0.07, camera.time * 0.028);
    let q = proj * 8.0 + wind;
    let cell = floor(q) + 0.5;
    let n = fbm2(cell * 0.16);
    let weather = 0.58 + 0.06 * sin(camera.sky.y * 1.7);
    if (n < weather) { return vec4<f32>(0.0); }
    // Borda iluminada do lado do sol
    let toward = fbm2((cell + camera.sun.xz * 1.5) * 0.16);
    let shade = clamp((n - toward) * 7.0 + 0.5, 0.0, 1.0);
    let day = daylight();
    let ml = moon_light();
    let lit = mix(vec3<f32>(0.70, 0.72, 0.80), vec3<f32>(1.0, 1.0, 1.0), shade);
    let night_col = vec3<f32>(0.018, 0.022, 0.04) + vec3<f32>(0.05, 0.06, 0.10) * ml;
    var col = mix(night_col, lit * (0.85 + 0.15 * day), day);
    let sh = camera.sun.y;
    let twi = clamp(1.0 - abs(sh - 0.04) / 0.34, 0.0, 1.0);
    col = mix(col, vec3<f32>(1.0, 0.55, 0.38) * (0.6 + 0.4 * shade), twi * 0.65);
    let a = smoothstep(0.012, 0.20, d.y) * mix(0.62, 0.95, day);
    return vec4<f32>(col, a);
}

@fragment
fn fs_main(in: SkyOut) -> @location(0) vec4<f32> {
    if (camera.params.x > 0.5) {
        return vec4<f32>(deep_color(), 1.0);
    }
    let w = camera.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let d = normalize(w.xyz / w.w - camera.camera_pos);
    let day = daylight();
    var col = sky_base(d);

    // --- Estrelas (somem ao amanhecer; a lua cheia ofusca as mais fracas) ---
    let night = (1.0 - day) * (1.0 - day);
    if (night > 0.01 && d.y > -0.05) {
        let sun_a = (camera.sky.x - 0.25) * 2.0 * PI;
        let sd = rot_z(d, -sun_a);
        let wash = moon_light() * 0.85;
        let horizon = smoothstep(-0.02, 0.16, d.y);
        var s = star_layer(sd, CELLS, 0.016, 1.0);
        var best = max(s.a - wash * 0.8, 0.0) * 0.55;
        var scol = s.rgb;
        s = star_layer(sd, CELLS * 0.5, 0.006, 5.0);
        let a2 = max(s.a - wash * 0.5, 0.0) * 0.85;
        if (a2 > best) { best = a2; scol = s.rgb; }
        s = star_layer(sd, CELLS * 0.25, 0.0016, 9.0);
        let a3 = max(s.a - wash * 0.25, 0.0);
        if (a3 > best) { best = a3; scol = s.rgb; }
        col = col + scol * best * night * horizon;
    }

    // --- Cometa raro: cabeça + rastro em células ---
    if (camera.comet_a.w >= 0.0 && day < 0.1) {
        let cell = sky_cell(d, CELLS);
        var b = 0.0;
        for (var k = 0; k < 40; k = k + 1) {
            let pk = camera.comet_a.w - f32(k) * 0.006;
            if (pk < 0.0) { break; }
            let pos = normalize(mix(camera.comet_a.xyz, camera.comet_b.xyz, pk));
            let diff = abs(sky_cell(pos, CELLS) - cell);
            let head = k < 3;
            let wide = k < 14;
            if ((head && diff.x <= 2.0 && diff.y <= 2.0) || (wide && diff.x <= 1.0 && diff.y <= 1.0) || (!head && diff.x < 0.5 && diff.y < 0.5)) {
                b = max(b, pow(1.0 - f32(k) / 40.0, 1.7));
            }
        }
        let life = pow(sin(PI * clamp(camera.comet_a.w, 0.0, 1.0)), 0.5);
        col = col + mix(vec3<f32>(0.6, 0.8, 1.0), vec3<f32>(1.0, 1.0, 1.0), b) * min(b * 2.2, 1.0) * life * (1.0 - day);
    }

    // --- Lua com fases ---
    let m = disc_basis(camera.moon.xyz, d);
    if (m.z > 0.0) {
        let q = 0.0085;
        let R = 0.058;
        let qc = (floor(m.xy / q) + 0.5) * q;
        if (length(qc) < R) {
            let nrm = vec3<f32>(qc / R, sqrt(max(1.0 - dot(qc / R, qc / R), 0.0)));
            let p = camera.moon.w * 2.0 * PI;
            let L = vec3<f32>(sin(p), 0.0, -cos(p));
            let lit = dot(nrm, L);
            if (lit > 0.0) {
                let crater = cell_hash(floor(m.xy / q), 41.0);
                var mc = vec3<f32>(0.93, 0.93, 0.85) * (0.55 + 0.45 * clamp(lit * 1.6, 0.0, 1.0));
                if (crater > 0.80) { mc = mc * 0.72; }
                if (crater < 0.06) { mc = mc * 0.85; }
                col = mix(col, mc, 1.0 - 0.45 * day);
            }
        }
    }

    // --- Sol (quadrado, pixelado) com halo ---
    let s = disc_basis(camera.sun.xyz, d);
    if (s.z > 0.0) {
        let ang = length(s.xy);
        let warm = smoothstep(0.0, 0.45, camera.sun.y);
        let sun_col = mix(vec3<f32>(1.0, 0.55, 0.25), vec3<f32>(1.0, 0.95, 0.7), warm);
        col = col + sun_col * exp(-ang * 11.0) * 0.55 * smoothstep(-0.15, 0.1, camera.sun.y);
        let q = 0.009;
        let qc = (floor(s.xy / q) + 0.5) * q;
        let hs = 0.045;
        let r = max(abs(qc.x), abs(qc.y));
        if (r < hs) {
            var sc = mix(sun_col, vec3<f32>(1.0, 1.0, 0.92), warm);
            if (r > hs - q * 1.1) { sc = sc * vec3<f32>(1.0, 0.82, 0.55); }
            col = sc * 1.25;
        }
    }

    // --- Nuvens por cima de tudo ---
    let c = clouds(d);
    col = mix(col, c.rgb, c.a);
    return vec4<f32>(col, 1.0);
}
