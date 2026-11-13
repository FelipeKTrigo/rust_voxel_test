//! Fase 7 — Save & Load. Um arquivo `.dne` por mundo contendo apenas o delta
//! (blocos modificados) sobre o mundo procedural, mais o estado do jogador.
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::world::chunk::{Chunk, ChunkPos, CHUNK_VOLUME};
use crate::world::BlockType;

pub const MAGIC: &[u8; 4] = b"DNE1";
pub const VERSION: u32 = 1;

/// Blocos alterados por chunk: índice linear do bloco -> `BlockType as u8`.
pub type ChunkEdits = HashMap<u32, u8>;
pub type WorldEdits = HashMap<ChunkPos, ChunkEdits>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SaveHeader {
    pub version: u32,
    pub name: String,
    pub seed: u32,
    pub player_pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// 0 = Survival, 1 = Creative, 2 = Spectator
    pub mode: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Delta {
    x: i32,
    z: i32,
    blocks: Vec<(u32, u8)>,
}

#[derive(Clone, Debug)]
pub struct WorldSave {
    pub header: SaveHeader,
    pub edits: WorldEdits,
}

/// Caminho do arquivo de um mundo.
pub fn world_path(saves_dir: &Path, name: &str) -> PathBuf {
    saves_dir.join(format!("{name}.dne"))
}

fn io_err<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
}

/// Grava de forma atômica (arquivo temporário + rename).
pub fn write_world(path: &Path, save: &WorldSave) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    let mut deltas: Vec<Delta> = save
        .edits
        .iter()
        .filter(|(_, e)| !e.is_empty())
        .map(|(p, e)| {
            let mut blocks: Vec<(u32, u8)> = e.iter().map(|(i, b)| (*i, *b)).collect();
            blocks.sort_unstable();
            Delta { x: p.x, z: p.z, blocks }
        })
        .collect();
    deltas.sort_by_key(|d| (d.x, d.z));

    let mut bytes = MAGIC.to_vec();
    bytes.extend(bincode::serialize(&save.header).map_err(io_err)?);
    bytes.extend(bincode::serialize(&deltas).map_err(io_err)?);

    let tmp = path.with_extension("dne.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

fn parse_header(cur: &mut &[u8]) -> std::io::Result<SaveHeader> {
    let mut magic = [0u8; 4];
    cur.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(io_err("arquivo .dne inválido"));
    }
    let h: SaveHeader = bincode::deserialize_from(&mut *cur).map_err(io_err)?;
    if h.version > VERSION {
        return Err(io_err("versão de save mais nova que o jogo"));
    }
    Ok(h)
}

/// Lê apenas o cabeçalho (nome, seed, jogador).
pub fn read_header(path: &Path) -> std::io::Result<SaveHeader> {
    let bytes = std::fs::read(path)?;
    parse_header(&mut bytes.as_slice())
}

pub fn read_world(path: &Path) -> std::io::Result<WorldSave> {
    let bytes = std::fs::read(path)?;
    let mut cur = bytes.as_slice();
    let header = parse_header(&mut cur)?;
    let deltas: Vec<Delta> = bincode::deserialize_from(&mut cur).map_err(io_err)?;
    let edits = deltas
        .into_iter()
        .map(|d| {
            let m: ChunkEdits = d
                .blocks
                .into_iter()
                .filter(|(i, _)| (*i as usize) < CHUNK_VOLUME)
                .collect();
            (ChunkPos::new(d.x, d.z), m)
        })
        .collect();
    Ok(WorldSave { header, edits })
}

/// Aplica o delta de um chunk sobre o terreno recém-gerado.
pub fn apply_edits(chunk: &mut Chunk, edits: &ChunkEdits) {
    for (&idx, &b) in edits {
        if let Some(slot) = chunk.blocks.get_mut(idx as usize) {
            *slot = BlockType::from_u8(b);
        }
    }
    chunk.is_dirty = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::{CHUNK_HEIGHT, CHUNK_SIZE};
    use crate::world::gen::{biome, gpu_gen::cpu_columns};

    fn tmpdir(tag: &str) -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("dne_save_{tag}_{}_{n}", std::process::id()))
    }

    fn sample(seed: u32) -> WorldSave {
        let mut edits = WorldEdits::new();
        let mut e = ChunkEdits::new();
        e.insert(Chunk::block_index(3, 70, 4) as u32, BlockType::Air as u8);
        e.insert(Chunk::block_index(5, 71, 6) as u32, BlockType::Planks as u8);
        edits.insert(ChunkPos::new(-2, 7), e);
        WorldSave {
            header: SaveHeader {
                version: VERSION,
                name: "Mundo".into(),
                seed,
                player_pos: [1.5, 80.0, -3.25],
                yaw: 1.0,
                pitch: -0.5,
                mode: 2,
            },
            edits,
        }
    }

    #[test]
    fn roundtrip_file() {
        let dir = tmpdir("rt");
        let path = world_path(&dir, "Mundo");
        let save = sample(4242);
        write_world(&path, &save).unwrap();
        assert!(!path.with_extension("dne.tmp").exists());
        let back = read_world(&path).unwrap();
        assert_eq!(back.header, save.header);
        assert_eq!(back.edits, save.edits);
        assert_eq!(read_header(&path).unwrap().seed, 4242);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_garbage() {
        let dir = tmpdir("bad");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.dne");
        std::fs::write(&p, b"lixo").unwrap();
        assert!(read_header(&p).is_err());
        assert!(read_world(&p).is_err());
        assert!(read_world(&dir.join("nao_existe.dne")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn world_list_reads_real_seed() {
        let dir = tmpdir("list");
        write_world(&world_path(&dir, "Alfa"), &sample(99)).unwrap();
        let w = crate::ui::world_select::list_worlds(&dir);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].seed, 99);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delta_survives_regeneration() {
        let seed = 7;
        let pos = ChunkPos::new(0, 0);
        let cols = cpu_columns(seed, pos);
        let gen = || {
            let mut c = Chunk::new(pos);
            biome::populate_chunk(&mut c, &cols, seed);
            c
        };
        let mut base = gen();
        let (x, z) = (4usize, 4usize);
        let y = (0..CHUNK_HEIGHT).rev().find(|&y| base.get(x, y, z).is_solid()).unwrap();
        let original = base.get(x, y, z);
        assert_ne!(original, BlockType::Air);

        // "Quebra" o bloco e registra o delta
        base.set(x, y, z, BlockType::Air);
        let mut e = ChunkEdits::new();
        e.insert(Chunk::block_index(x, y, z) as u32, BlockType::Air as u8);

        let mut regen = gen();
        assert_eq!(regen.get(x, y, z), original);
        apply_edits(&mut regen, &e);
        assert_eq!(regen.get(x, y, z), BlockType::Air);
        // Nenhum outro bloco muda
        let reference = gen();
        let diff = (0..CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE)
            .filter(|&i| regen.blocks[i] != reference.blocks[i])
            .count();
        assert_eq!(diff, 1);
    }

    #[test]
    fn out_of_range_edit_ignored() {
        let mut c = Chunk::new(ChunkPos::new(0, 0));
        let mut e = ChunkEdits::new();
        e.insert(u32::MAX, 3);
        apply_edits(&mut c, &e);
    }
}
