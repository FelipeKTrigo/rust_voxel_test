//! Parte 2 — texturas de bloco 16×16 geradas proceduralmente (pixel art determinística).
//! Cada bloco tem 3 tiles: topo, lado e base. Camada do array de texturas = `bloco * 3 + tipo`.
use super::block::BlockType;
use super::palette::ColorPalette;

pub const TILE: usize = 16;
pub const KINDS: usize = 3;
pub const NUM_BLOCKS: usize = 32;
pub const NUM_LAYERS: usize = NUM_BLOCKS * KINDS;
pub const MIP_LEVELS: usize = 5; // 16, 8, 4, 2, 1
/// Valor de `layer` nos vértices sem textura (usam a cor do vértice).
pub const NO_TEXTURE: u32 = 0xFFFF;
/// Bit extra na camada: o shader não espelha/gira o tile por bloco (itens soltos). DEVE ser
/// igual a `NO_VARIATION` em voxel.wgsl.
pub const NO_VARIATION_BIT: u32 = 0x4000;

pub type Rgba = [u8; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceKind {
    Top = 0,
    Side = 1,
    Bottom = 2,
}

impl FaceKind {
    /// Índice de face do mesher (+X,-X,+Y,-Y,+Z,-Z) -> tipo de tile.
    pub fn from_face(face: usize) -> Self {
        match face {
            2 => Self::Top,
            3 => Self::Bottom,
            _ => Self::Side,
        }
    }
    fn normal(self) -> [f32; 3] {
        match self {
            Self::Top => [0.0, 1.0, 0.0],
            Self::Side => [1.0, 0.0, 0.0],
            Self::Bottom => [0.0, -1.0, 0.0],
        }
    }
}

pub fn layer_for(block: BlockType, kind: FaceKind) -> u32 {
    block as u32 * KINDS as u32 + kind as u32
}

pub fn layer_for_face(block: BlockType, face: usize) -> u32 {
    layer_for(block, FaceKind::from_face(face))
}

// ---------------------------------------------------------------- utilidades de pixel

fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x27d4_eb2d)
        ^ (y as u32).wrapping_mul(0x1656_67b1)
        ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^ (h >> 16)
}

/// Aleatório em [0,1) por pixel (com repetição de 16 px).
fn rnd(x: usize, y: usize, seed: u32) -> f32 {
    (hash((x % TILE) as i32, (y % TILE) as i32, seed) >> 8) as f32 / (1u32 << 24) as f32
}

type V3 = [f32; 3];

fn mul(c: V3, k: f32) -> V3 {
    [c[0] * k, c[1] * k, c[2] * k]
}
fn mixc(a: V3, b: V3, t: f32) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
fn rgb(c: [f32; 4]) -> V3 {
    [c[0], c[1], c[2]]
}
fn to_u8(c: V3) -> Rgba {
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    [f(c[0]), f(c[1]), f(c[2]), 255]
}

/// Cor base com variação por pixel de ±`amt`.
fn noisy(base: V3, x: usize, y: usize, seed: u32, amt: f32) -> V3 {
    mul(base, 1.0 + (rnd(x, y, seed) - 0.5) * 2.0 * amt)
}

/// Ruído de valor suave (células de `cell` px), em [0,1).
fn vnoise(x: usize, y: usize, seed: u32, cell: usize) -> f32 {
    let (cx, cy) = (x / cell, y / cell);
    let (fx, fy) = ((x % cell) as f32 / cell as f32, (y % cell) as f32 / cell as f32);
    let n = TILE / cell;
    let g = |i: usize, j: usize| rnd((i % n) * cell, (j % n) * cell, seed);
    let top = g(cx, cy) * (1.0 - fx) + g(cx + 1, cy) * fx;
    let bot = g(cx, cy + 1) * (1.0 - fx) + g(cx + 1, cy + 1) * fx;
    top * (1.0 - fy) + bot * fy
}

fn base_of(block: BlockType, kind: FaceKind) -> V3 {
    rgb(ColorPalette::get_color(block, kind.normal()))
}

// ---------------------------------------------------------------- padrões reutilizáveis

const STONE: V3 = [0.50, 0.50, 0.52];
const DIRT: V3 = [0.50, 0.34, 0.20];

fn stone(x: usize, y: usize, seed: u32) -> V3 {
    let mut c = noisy(STONE, x, y, seed, 0.10);
    // manchas de tom suaves
    c = mul(c, 0.88 + 0.24 * vnoise(x, y, seed ^ 7, 4));
    // rachaduras: pixels escuros em linhas curtas pseudoaleatórias
    let r = rnd(x / 2, y / 4, seed ^ 99);
    if r > 0.86 && rnd(x, y, seed ^ 5) > 0.35 {
        c = mul(c, 0.68);
    }
    // lascas claras
    if rnd(x, y, seed ^ 31) > 0.95 {
        c = mul(c, 1.18);
    }
    c
}

fn dirt(x: usize, y: usize, seed: u32) -> V3 {
    let mut c = noisy(DIRT, x, y, seed, 0.12);
    c = mul(c, 0.9 + 0.2 * vnoise(x, y, seed ^ 3, 4));
    if rnd(x, y, seed ^ 17) > 0.90 {
        c = mul(c, 0.72); // torrões escuros
    } else if rnd(x, y, seed ^ 23) > 0.94 {
        c = mixc(c, [0.62, 0.55, 0.45], 0.7); // pedrinhas
    }
    c
}

/// Pedra com manchas de minério: `blobs` pequenos de `ore` (2×2 / 3×2).
fn ore(x: usize, y: usize, seed: u32, ore_col: V3, density: f32, shine: bool) -> V3 {
    let mut c = stone(x, y, seed);
    // centros de veio em uma grade 4×4 com deslocamento aleatório
    let (gx, gy) = (x / 4, y / 4);
    let r = rnd(gx * 4, gy * 4, seed ^ 0x51);
    if r < density {
        let ox = gx * 4 + (rnd(gx * 4, gy * 4, seed ^ 0x52) * 2.0) as usize;
        let oy = gy * 4 + (rnd(gx * 4, gy * 4, seed ^ 0x53) * 2.0) as usize;
        let (w, h) = if rnd(gx * 4, gy * 4, seed ^ 0x54) > 0.5 { (3, 2) } else { (2, 3) };
        if x >= ox && x < ox + w && y >= oy && y < oy + h {
            let hi = shine && (x == ox && y == oy);
            let t = rnd(x, y, seed ^ 0x55);
            c = if hi { mul(ore_col, 1.35) } else { mul(ore_col, 0.85 + 0.3 * t) };
        }
    }
    c
}

fn leaves(x: usize, y: usize, seed: u32, base: V3) -> V3 {
    let mut c = noisy(base, x, y, seed, 0.14);
    c = mul(c, 0.78 + 0.44 * vnoise(x, y, seed ^ 0x11, 4));
    // "buracos" escuros entre folhas (opacos!) e folhas claras
    let r = rnd(x, y, seed ^ 0x12);
    if r > 0.88 {
        c = mul(c, 0.55);
    } else if r < 0.07 {
        c = mul(c, 1.35);
    }
    c
}

fn grass_top(x: usize, y: usize, seed: u32) -> V3 {
    let base = base_of(BlockType::Grass, FaceKind::Top);
    let mut c = noisy(base, x, y, seed, 0.09);
    c = mul(c, 0.9 + 0.2 * vnoise(x, y, seed ^ 5, 4));
    if rnd(x, y, seed ^ 8) > 0.9 {
        c = mul(c, 1.25); // pontas de grama claras
    } else if rnd(x, y, seed ^ 9) > 0.9 {
        c = mul(c, 0.75);
    }
    c
}

fn bricks(x: usize, y: usize, seed: u32, base: V3, mortar: V3, rows: usize, brick_w: usize) -> V3 {
    let row_h = TILE / rows;
    let ry = y / row_h;
    let off = if ry % 2 == 1 { brick_w / 2 } else { 0 };
    let bx = (x + off) % brick_w;
    if y % row_h == row_h - 1 || bx == brick_w - 1 {
        return noisy(mortar, x, y, seed ^ 1, 0.08);
    }
    let id = ((x + off) / brick_w) as u32 + ry as u32 * 7;
    let tone = 0.82 + 0.36 * ((hash(id as i32, ry as i32, seed) >> 8) as f32 / (1u32 << 24) as f32);
    let mut c = mul(noisy(base, x, y, seed, 0.07), tone);
    if y % row_h == 0 {
        c = mul(c, 1.12); // brilho no topo do tijolo
    }
    c
}

// ---------------------------------------------------------------- um pixel de cada bloco

/// Cor do pixel `(x, y)` (origem no canto superior esquerdo) do tile do bloco.
fn paint(block: BlockType, kind: FaceKind, x: usize, y: usize) -> V3 {
    use BlockType::*;
    let seed = block as u32 * 131 + kind as u32 * 17 + 5;
    match block {
        Air => {
            // tile "ausente": xadrez magenta (nunca é desenhado)
            if (x / 4 + y / 4) % 2 == 0 { [1.0, 0.0, 1.0] } else { [0.1, 0.0, 0.1] }
        }
        Grass => match kind {
            FaceKind::Top => grass_top(x, y, seed),
            FaceKind::Bottom => dirt(x, y, 1000),
            FaceKind::Side => {
                // franja de grama irregular (3–5 px) sobre a terra
                let depth = 3 + (rnd(x, 0, seed ^ 0x77) * 3.0) as usize;
                if y < depth { grass_top(x, y, seed) } else { dirt(x, y, 1000) }
            }
        },
        Dirt => dirt(x, y, 1000),
        Stone => stone(x, y, 2000),
        Sand => {
            let mut c = noisy(base_of(Sand, kind), x, y, seed, 0.05);
            c = mul(c, 0.96 + 0.08 * vnoise(x, y, seed, 8));
            if rnd(x, y, seed ^ 4) > 0.92 { c = mul(c, 0.86); }
            if rnd(x, y, seed ^ 6) > 0.96 { c = mul(c, 1.08); }
            c
        }
        Water => {
            let mut c = noisy([0.16, 0.38, 0.78], x, y, seed, 0.08);
            if (x + y * 2 + (rnd(x, y, seed) * 3.0) as usize) % 7 == 0 { c = mul(c, 1.25); }
            c
        }
        Wood => match kind {
            FaceKind::Side => {
                // casca: sulcos verticais escuros e irregulares
                let groove = rnd(x, 0, seed ^ 0x31);
                let mut c = noisy([0.36, 0.25, 0.14], x, y, seed, 0.10);
                if groove > 0.55 { c = mul(c, 0.72); }
                if rnd(x, y / 3, seed ^ 0x32) > 0.82 { c = mul(c, 0.8); }
                if rnd(x, y, seed ^ 0x33) > 0.93 { c = mul(c, 1.2); }
                c
            }
            _ => {
                // anéis de crescimento
                let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
                let d = (dx * dx + dy * dy).sqrt();
                if d > 6.8 {
                    noisy([0.32, 0.22, 0.12], x, y, seed, 0.08) // casca na borda
                } else {
                    let ring = ((d * 1.3).floor() as i32 % 2 == 0) as i32 as f32;
                    let c = mixc([0.72, 0.56, 0.32], [0.58, 0.42, 0.22], ring);
                    noisy(c, x, y, seed, 0.05)
                }
            }
        },
        Leaves => leaves(x, y, seed, [0.20, 0.52, 0.16]),
        PineLeaves => {
            // agulhas: traços diagonais mais escuros
            let mut c = leaves(x, y, seed, [0.09, 0.32, 0.18]);
            if (x + y) % 4 == 0 && rnd(x, y, seed ^ 0x40) > 0.35 { c = mul(c, 1.3); }
            c
        }
        PalmLeaves => {
            // folha de palmeira: nervura central e nervuras laterais
            let mut c = leaves(x, y, seed, [0.30, 0.64, 0.20]);
            if x == 7 || x == 8 { c = mixc(c, [0.55, 0.78, 0.30], 0.8); }
            else if (y + x / 2) % 4 == 0 { c = mul(c, 0.8); }
            c
        }
        Snow => {
            let mut c = noisy([0.94, 0.96, 1.0], x, y, seed, 0.03);
            if kind == FaceKind::Side { c = mixc(c, [0.80, 0.86, 0.94], 0.35); }
            if rnd(x, y, seed ^ 4) > 0.9 { c = mixc(c, [0.78, 0.86, 0.98], 0.7); }
            c
        }
        Gravel => {
            // seixos de tons diferentes
            let id = hash((x / 3) as i32, (y / 3) as i32, seed);
            let tone = 0.65 + 0.5 * ((id >> 8) as f32 / (1u32 << 24) as f32);
            let mut c = mul([0.56, 0.53, 0.50], tone);
            if x % 3 == 2 || y % 3 == 2 { c = mul(c, 0.7); }
            noisy(c, x, y, seed, 0.05)
        }
        Cobblestone => bricks(x, y, seed, [0.52, 0.52, 0.50], [0.25, 0.25, 0.24], 4, 8),
        Bedrock => {
            let mut c = noisy([0.17, 0.17, 0.18], x, y, seed, 0.30);
            if vnoise(x, y, seed, 4) > 0.62 { c = mul(c, 0.5); }
            if rnd(x, y, seed ^ 1) > 0.9 { c = mul(c, 1.8); }
            c
        }
        Coal => ore(x, y, 2000, [0.07, 0.07, 0.08], 0.75, false),
        Iron => ore(x, y, 2000, [0.80, 0.62, 0.50], 0.65, false),
        Gold => ore(x, y, 2000, [1.0, 0.82, 0.18], 0.55, true),
        Feldspar => ore(x, y, 2000, [0.90, 0.62, 0.58], 0.85, false),
        Pyrite => ore(x, y, 2000, [0.86, 0.78, 0.28], 0.8, true),
        Bauxite => ore(x, y, 2000, [0.72, 0.36, 0.24], 0.85, false),
        Cactus => match kind {
            FaceKind::Side => {
                // gomos verticais com espinhos claros
                let mut c = noisy([0.16, 0.48, 0.17], x, y, seed, 0.07);
                if x % 4 == 0 { c = mul(c, 0.72); }
                if (x + y * 3) % 8 == 2 && x % 4 != 0 { c = [0.92, 0.92, 0.70]; }
                c
            }
            _ => {
                let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
                let d = (dx * dx + dy * dy).sqrt();
                let mut c = noisy([0.20, 0.55, 0.18], x, y, seed, 0.06);
                if d < 3.0 { c = mul(c, 1.25); }
                if d > 6.0 { c = mul(c, 0.7); }
                c
            }
        },
        Moss => {
            let mut c = noisy([0.22, 0.40, 0.18], x, y, seed, 0.16);
            c = mul(c, 0.8 + 0.4 * vnoise(x, y, seed, 2));
            if rnd(x, y, seed ^ 6) > 0.93 { c = [0.55, 0.65, 0.30]; }
            c
        }
        Coconut => {
            let mut c = noisy([0.38, 0.26, 0.14], x, y, seed, 0.12);
            if (y % 3) == 1 && rnd(x, y, seed ^ 9) > 0.4 { c = mul(c, 0.8); } // fibras
            // três "olhos" do coco
            for (ex, ey) in [(5usize, 4usize), (9, 4), (7, 7)] {
                if x >= ex && x < ex + 2 && y >= ey && y < ey + 2 { c = [0.10, 0.07, 0.05]; }
            }
            c
        }
        Planks => {
            // 4 tábuas horizontais com veios e pregos
            let row = y / 4;
            let mut c = if y % 4 == 3 {
                [0.34, 0.23, 0.12]
            } else {
                let shift = (row as u32 * 5 + 2) as usize;
                let tone = 0.9 + 0.2 * vnoise((x + shift) % TILE, y, seed ^ row as u32, 8);
                let c = mul(noisy([0.72, 0.55, 0.32], x, y, seed, 0.04), tone);
                if rnd(x + shift, y / 2, seed ^ 0x61) > 0.85 { mul(c, 0.82) } else { c }
            };
            if (x == 1 || x == 14) && y % 4 == 1 { c = [0.30, 0.28, 0.27]; }
            c
        }
        FlowerRed | FlowerYellow => {
            let petal = if block == FlowerRed { [0.92, 0.14, 0.20] } else { [0.98, 0.86, 0.12] };
            let on_top = kind == FaceKind::Top;
            let mut c = match kind {
                FaceKind::Bottom => return dirt(x, y, 1000),
                FaceKind::Top => grass_top(x, y, seed),
                FaceKind::Side => {
                    let depth = 3 + (rnd(x, 0, 0x77) * 3.0) as usize;
                    if y < depth { grass_top(x, y, 5) } else { dirt(x, y, 1000) }
                }
            };
            // flores: cruz de 3×3 com miolo claro
            let spots: [(usize, usize); 3] = [(3, 3), (10, 5), (6, 11)];
            for (i, (cx, cy)) in spots.iter().enumerate() {
                if !on_top && i != 0 { continue; }
                let (cy, ox) = if on_top { (*cy, 0usize) } else { (1usize, i * 6) };
                let (dx, dy) = (x as i32 - (*cx + ox) as i32, y as i32 - cy as i32);
                if dx.abs() + dy.abs() == 1 { c = petal; }
                if dx == 0 && dy == 0 { c = [1.0, 0.95, 0.55]; }
            }
            if !on_top && kind == FaceKind::Side {
                for cx in [3usize, 9, 13] {
                    if x == cx && (2..5).contains(&y) { c = petal; }
                }
            }
            c
        }
        BerryBushRed | BerryBushBlue => {
            let berry = if block == BerryBushRed { [0.80, 0.08, 0.16] } else { [0.18, 0.26, 0.88] };
            let mut c = leaves(x, y, seed ^ 0x700, [0.14, 0.42, 0.14]);
            for (bx, by) in [(2usize, 2usize), (9, 1), (5, 7), (12, 6), (3, 12), (10, 11), (14, 14)] {
                let by = (by + kind as usize * 3) % TILE;
                if x >= bx && x < bx + 2 && y >= by && y < by + 2 {
                    c = if x == bx && y == by { mul(berry, 1.6) } else { berry };
                }
            }
            c
        }
        SulfuricAcid => {
            let mut c = noisy([0.55, 0.88, 0.12], x, y, seed, 0.10);
            c = mul(c, 0.8 + 0.4 * vnoise(x, y, seed, 4));
            if rnd(x / 2, y / 2, seed ^ 3) > 0.9 { c = [0.85, 1.0, 0.45]; } // bolhas
            c
        }
        DeadBush => {
            // graveto seco sobre solo árido
            let mut c = noisy([0.62, 0.52, 0.34], x, y, seed, 0.08);
            let twig = (x as i32 - 8).abs() == (y as i32 / 2 % 5)
                || (x as i32 - 8).abs() == 1 + (y as i32 / 3 % 3)
                || x == 8;
            if twig && rnd(x, y, seed ^ 2) > 0.2 { c = [0.30, 0.20, 0.10]; }
            c
        }
        CactusFruit => {
            let mut c = noisy([0.88, 0.26, 0.52], x, y, seed, 0.10);
            if (x + y) % 5 == 0 { c = [0.98, 0.78, 0.35]; } // sementes/espinhos
            if (x % 8 == 0) || (y % 8 == 0) { c = mul(c, 0.8); }
            c
        }
        Roof => {
            // telhas de barro em escama
            let row = y / 4;
            let off = if row % 2 == 1 { 4 } else { 0 };
            let bx = (x + off) % 8;
            let mut c = mul(noisy([0.66, 0.24, 0.18], x, y, seed, 0.06), 0.86 + 0.28 * rnd(((x + off) / 8) * 8, row * 4, seed));
            if y % 4 == 3 { c = mul(c, 0.55); }
            if bx == 7 { c = mul(c, 0.7); }
            if y % 4 == 0 { c = mul(c, 1.15); }
            c
        }
    }
}

/// Pixels RGBA (16×16, linha a linha) do tile.
pub fn generate_tile(block: BlockType, kind: FaceKind) -> Vec<Rgba> {
    let mut out = Vec::with_capacity(TILE * TILE);
    for y in 0..TILE {
        for x in 0..TILE {
            out.push(to_u8(paint(block, kind, x, y)));
        }
    }
    out
}

/// Reduz pela metade com filtro de caixa.
fn downsample(src: &[Rgba], size: usize) -> Vec<Rgba> {
    let n = size / 2;
    let mut out = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let mut acc = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = src[(y * 2 + dy) * size + x * 2 + dx];
                for i in 0..4 {
                    acc[i] += p[i] as u32;
                }
            }
            out.push([(acc[0] / 4) as u8, (acc[1] / 4) as u8, (acc[2] / 4) as u8, (acc[3] / 4) as u8]);
        }
    }
    out
}

/// Cadeia de mipmaps: 16×16, 8×8, 4×4, 2×2, 1×1.
pub fn mip_chain(tile: &[Rgba]) -> Vec<Vec<Rgba>> {
    let mut levels = vec![tile.to_vec()];
    let mut size = TILE;
    while size > 1 {
        let next = downsample(levels.last().unwrap(), size);
        levels.push(next);
        size /= 2;
    }
    levels
}

/// Todas as camadas do array de texturas (ordem: bloco*3 + tipo).
pub fn all_tiles() -> Vec<Vec<Rgba>> {
    let mut v = Vec::with_capacity(NUM_LAYERS);
    for b in 0..NUM_BLOCKS as u8 {
        let block = BlockType::from_u8(b);
        for kind in [FaceKind::Top, FaceKind::Side, FaceKind::Bottom] {
            v.push(generate_tile(block, kind));
        }
    }
    v
}

/// Cor média do tile (cor dos detritos de partículas).
pub fn average_color(block: BlockType, kind: FaceKind) -> [f32; 3] {
    let t = generate_tile(block, kind);
    let mut acc = [0.0f32; 3];
    for p in &t {
        for i in 0..3 {
            acc[i] += p[i] as f32 / 255.0;
        }
    }
    acc.map(|v| v / t.len() as f32)
}

/// Dados para `create_texture_with_data` (layer-major, mip-major dentro de cada camada).
pub fn gpu_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    for tile in all_tiles() {
        for level in mip_chain(&tile) {
            for p in level {
                bytes.extend_from_slice(&p);
            }
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn all_blocks() -> impl Iterator<Item = BlockType> {
        (0..NUM_BLOCKS as u8).map(BlockType::from_u8)
    }
    const KINDS_ALL: [FaceKind; 3] = [FaceKind::Top, FaceKind::Side, FaceKind::Bottom];

    #[test]
    fn every_tile_is_16x16_and_fully_opaque() {
        let tiles = all_tiles();
        assert_eq!(tiles.len(), NUM_LAYERS);
        for (i, t) in tiles.iter().enumerate() {
            assert_eq!(t.len(), TILE * TILE, "camada {i}");
            assert!(t.iter().all(|p| p[3] == 255), "camada {i} tem pixel translúcido (veria através do bloco)");
        }
    }

    #[test]
    fn generation_is_deterministic() {
        for b in all_blocks() {
            for k in KINDS_ALL {
                assert_eq!(generate_tile(b, k), generate_tile(b, k));
            }
        }
    }

    #[test]
    fn layer_indexing_is_consistent() {
        assert_eq!(layer_for(BlockType::Air, FaceKind::Top), 0);
        assert_eq!(layer_for(BlockType::Roof, FaceKind::Bottom) as usize, NUM_LAYERS - 1);
        assert_eq!(layer_for_face(BlockType::Grass, 2), layer_for(BlockType::Grass, FaceKind::Top));
        assert_eq!(layer_for_face(BlockType::Grass, 3), layer_for(BlockType::Grass, FaceKind::Bottom));
        for f in [0, 1, 4, 5] {
            assert_eq!(layer_for_face(BlockType::Grass, f), layer_for(BlockType::Grass, FaceKind::Side));
        }
        assert!(NO_TEXTURE as usize >= NUM_LAYERS);
        assert!(NO_VARIATION_BIT as usize > NUM_LAYERS && NO_VARIATION_BIT & NO_TEXTURE == NO_VARIATION_BIT);
        assert_eq!(NO_VARIATION_BIT & (NO_VARIATION_BIT - 1), 0, "um único bit");
    }

    #[test]
    fn all_blocks_have_a_distinct_look() {
        // Nenhum par de blocos (exceto o ar) compartilha o tile lateral.
        let mut seen: Vec<(BlockType, Vec<Rgba>)> = Vec::new();
        for b in all_blocks().filter(|b| !b.is_air()) {
            let t = generate_tile(b, FaceKind::Side);
            for (o, ot) in &seen {
                assert_ne!(&t, ot, "{b:?} e {o:?} têm o mesmo tile lateral");
            }
            seen.push((b, t));
        }
    }

    #[test]
    fn multi_face_blocks_differ_per_face() {
        use BlockType::*;
        for b in [Grass, Wood, Cactus, FlowerRed, FlowerYellow] {
            let (t, s) = (generate_tile(b, FaceKind::Top), generate_tile(b, FaceKind::Side));
            assert_ne!(t, s, "{b:?}: topo = lado");
        }
        assert_ne!(generate_tile(Grass, FaceKind::Top), generate_tile(Grass, FaceKind::Bottom));
        assert_eq!(generate_tile(Grass, FaceKind::Bottom), generate_tile(Dirt, FaceKind::Bottom), "base da grama é terra");
    }

    #[test]
    fn tiles_have_real_texture_detail() {
        // Pixel art de verdade: cada bloco usa várias cores distintas (não é cor chapada).
        for b in all_blocks().filter(|b| !b.is_air()) {
            for k in KINDS_ALL {
                let colors: HashSet<_> = generate_tile(b, k).into_iter().collect();
                assert!(colors.len() >= 6, "{b:?}/{k:?} muito liso: {} cores", colors.len());
            }
        }
    }

    #[test]
    fn tile_color_matches_what_the_block_should_be() {
        // A cor média do tile fica perto da cor "conceitual" do bloco na paleta.
        use BlockType::*;
        // Minérios são pedra com veios: a paleta guarda a cor do veio, testada em `ores_are_stone_with_speckles`.
        // Flores e arbustos de bagas são folhagem/grama com pontos coloridos (a paleta guarda a cor do ponto).
        let ores = [Coal, Iron, Gold, Feldspar, Pyrite, Bauxite, FlowerRed, FlowerYellow, BerryBushRed, BerryBushBlue];
        for b in all_blocks().filter(|b| !b.is_air() && !ores.contains(b)) {
            for k in KINDS_ALL {
                let avg = average_color(b, k);
                let pal = ColorPalette::get_color(b, k.normal());
                let dist = ((avg[0] - pal[0]).powi(2) + (avg[1] - pal[1]).powi(2) + (avg[2] - pal[2]).powi(2)).sqrt();
                assert!(dist < 0.42, "{b:?}/{k:?}: tile {avg:?} longe da paleta {pal:?} ({dist:.2})");
            }
        }
    }

    #[test]
    fn dominant_hues_make_sense() {
        use BlockType::*;
        let a = |b, k| average_color(b, k);
        let g = a(Grass, FaceKind::Top);
        assert!(g[1] > g[0] && g[1] > g[2], "grama é verde: {g:?}");
        let l = a(Leaves, FaceKind::Side);
        assert!(l[1] > l[0] && l[1] > l[2], "folhas verdes");
        let w = a(Water, FaceKind::Top);
        assert!(w[2] > w[0] && w[2] > w[1], "água azul");
        let s = a(Snow, FaceKind::Top);
        assert!(s.iter().all(|c| *c > 0.85), "neve branca");
        let sd = a(Sand, FaceKind::Top);
        assert!(sd[0] > sd[2] + 0.2, "areia amarelada");
        let wood = a(Wood, FaceKind::Side);
        assert!(wood[0] > wood[2], "madeira marrom");
        let bed = a(Bedrock, FaceKind::Side);
        assert!(bed.iter().all(|c| *c < 0.3), "bedrock escura");
        let st = a(Stone, FaceKind::Side);
        assert!((st[0] - st[2]).abs() < 0.1, "pedra cinza");
        let roof = a(Roof, FaceKind::Top);
        assert!(roof[0] > roof[1] * 1.5, "telhado avermelhado");
    }

    #[test]
    fn ores_are_stone_with_speckles() {
        use BlockType::*;
        let stone_tile = generate_tile(Stone, FaceKind::Side);
        let stone_px: HashSet<_> = stone_tile.iter().copied().collect();
        for ore in [Coal, Iron, Gold, Feldspar, Pyrite, Bauxite] {
            let t = generate_tile(ore, FaceKind::Side);
            let diff = t.iter().zip(&stone_tile).filter(|(a, b)| a != b).count();
            assert!(diff >= 8, "{ore:?} sem veios visíveis ({diff} px)");
            assert!(diff < 130, "{ore:?} deve continuar sendo majoritariamente pedra ({diff} px)");
            assert!(t.iter().filter(|p| stone_px.contains(*p)).count() > 100);
        }
    }

    #[test]
    fn mip_chain_sizes_and_average() {
        let tile = generate_tile(BlockType::Stone, FaceKind::Top);
        let chain = mip_chain(&tile);
        assert_eq!(chain.len(), MIP_LEVELS);
        let sizes: Vec<_> = chain.iter().map(|l| l.len()).collect();
        assert_eq!(sizes, vec![256, 64, 16, 4, 1]);
        let avg = average_color(BlockType::Stone, FaceKind::Top);
        let last = chain[4][0];
        for i in 0..3 {
            assert!((last[i] as f32 / 255.0 - avg[i]).abs() < 0.02);
        }
        assert!(chain.iter().all(|l| l.iter().all(|p| p[3] == 255)));
    }

    #[test]
    fn gpu_upload_size_matches_layout() {
        let per_layer = (256 + 64 + 16 + 4 + 1) * 4;
        assert_eq!(gpu_bytes().len(), NUM_LAYERS * per_layer);
    }

    /// Gera uma imagem de conferência: DNE_ATLAS_OUT=/caminho/atlas.ppm cargo test dump_atlas -- --ignored
    #[test]
    #[ignore]
    fn dump_atlas() {
        let Some(path) = std::env::var_os("DNE_ATLAS_OUT") else { return };
        let tiles = all_tiles();
        let (cols, scale) = (12usize, 6usize); // 4 blocos por linha × 3 faces
        let rows = NUM_LAYERS / cols;
        let (w, h) = (cols * (TILE + 1) * scale, rows * (TILE + 1) * scale);
        let mut img = vec![30u8; w * h * 3];
        for (i, t) in tiles.iter().enumerate() {
            let (tx, ty) = ((i % cols) * (TILE + 1), (i / cols) * (TILE + 1));
            for y in 0..TILE * scale {
                for x in 0..TILE * scale {
                    let p = t[(y / scale) * TILE + x / scale];
                    let o = ((ty * scale + y) * w + tx * scale + x) * 3;
                    img[o..o + 3].copy_from_slice(&p[..3]);
                }
            }
        }
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        out.extend(img);
        std::fs::write(path, out).unwrap();
    }
}
