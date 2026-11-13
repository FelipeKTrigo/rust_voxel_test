//! Ferramentas de diagnóstico via variáveis de ambiente (nenhuma afeta o jogo normal).
//!
//! - `DNE_SMOKE=<seed>`  entra direto num mundo temporário (sem salvar)
//! - `DNE_TOD=0.5`       fixa a hora do dia (0 = meia-noite, .25 = nascer do sol, .75 = pôr do sol)
//! - `DNE_DAY=12`        fixa o dia (afeta a fase da lua)
//! - `DNE_COMET=0.6`     força um cometa no progresso dado (0..1)
//! - `DNE_POS=x,y,z`     posição do jogador;  `DNE_YAW`/`DNE_PITCH` em graus
//! - `DNE_MODE=survival|creative|spectator`
//! - `DNE_LOOK=sun|moon` aponta a câmera para o sol/lua
//! - `DNE_GIVE=1`        inventário de exemplo;  `DNE_NOCULL=1` desliga o entity culling
//! - `DNE_KEYS=60:e,90:esc` roteiro de teclas por número do quadro;  `DNE_BREAK=1.5` quebra blocos
//! - `DNE_DROPS=1` solta itens junto ao jogador
//! - `DNE_UI=inventory|pause|graphics`  abre uma tela
//! - `DNE_SHOT=arquivo.ppm` salva um quadro (após `DNE_SHOT_FRAMES`, padrão 120) e sai
use std::path::PathBuf;

use glam::Vec3;

#[derive(Default, Clone, Debug)]
pub struct DebugOpts {
    pub time_of_day: Option<f32>,
    pub day: Option<u32>,
    pub comet: Option<f32>,
    pub pos: Option<Vec3>,
    pub yaw_deg: Option<f32>,
    pub pitch_deg: Option<f32>,
    pub mode: Option<String>,
    pub ui: Option<String>,
    /// `sun` ou `moon`: aponta a câmera para o astro.
    pub look: Option<String>,
    /// Dá itens e equipamentos de exemplo ao jogador (para capturas de tela).
    pub give: bool,
    /// Solta alguns itens perto do jogador (diagnóstico da coleta).
    pub drops: bool,
    /// Roteiro de entradas: `quadro:acao,...` (ex.: `60:e,90:3,120:esc`). Ações: e, esc, 1-9,
    /// wheel+, wheel-, click.
    pub keys: Vec<(u32, String)>,
    /// Quebra o bloco mirado a cada N segundos (diagnóstico de itens soltos).
    pub break_every: Option<f32>,
    pub shot: Option<PathBuf>,
    pub shot_frames: u32,
}

fn env_f32(k: &str) -> Option<f32> {
    std::env::var(k).ok()?.trim().parse().ok()
}

impl DebugOpts {
    pub fn from_env() -> Self {
        let pos = std::env::var("DNE_POS").ok().and_then(|v| {
            let p: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            (p.len() == 3).then(|| Vec3::new(p[0], p[1], p[2]))
        });
        Self {
            time_of_day: env_f32("DNE_TOD"),
            day: std::env::var("DNE_DAY").ok().and_then(|v| v.parse().ok()),
            comet: env_f32("DNE_COMET"),
            pos,
            yaw_deg: env_f32("DNE_YAW"),
            pitch_deg: env_f32("DNE_PITCH"),
            mode: std::env::var("DNE_MODE").ok().map(|s| s.to_lowercase()),
            ui: std::env::var("DNE_UI").ok().map(|s| s.to_lowercase()),
            look: std::env::var("DNE_LOOK").ok().map(|s| s.to_lowercase()),
            give: std::env::var_os("DNE_GIVE").is_some(),
            drops: std::env::var_os("DNE_DROPS").is_some(),
            keys: std::env::var("DNE_KEYS")
                .map(|v| {
                    v.split(',')
                        .filter_map(|e| {
                            let (f, k) = e.split_once(':')?;
                            Some((f.trim().parse().ok()?, k.trim().to_lowercase()))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            break_every: env_f32("DNE_BREAK"),
            shot: std::env::var_os("DNE_SHOT").map(PathBuf::from),
            shot_frames: std::env::var("DNE_SHOT_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(120),
        }
    }

    /// Com a hora fixada o relógio do mundo fica parado.
    pub fn freezes_clock(&self) -> bool {
        self.time_of_day.is_some()
    }
}

/// Copia a textura da tela para um arquivo PPM (P6). Converte BGRA -> RGB quando preciso.
pub fn save_frame_ppm(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    path: &std::path::Path,
) -> std::io::Result<()> {
    let bgra = matches!(format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    let padded = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Frame capture"),
        size: (padded * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::ImageCopyTexture { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::ImageCopyBuffer {
            buffer: &buffer,
            layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(height) },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit(Some(enc.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().map_err(|e| std::io::Error::other(e.to_string()))?.map_err(|e| std::io::Error::other(e.to_string()))?;
    let data = slice.get_mapped_range();
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    for y in 0..height as usize {
        let row = &data[y * padded as usize..y * padded as usize + width as usize * 4];
        for px in row.chunks_exact(4) {
            if bgra {
                out.extend_from_slice(&[px[2], px[1], px[0]]);
            } else {
                out.extend_from_slice(&px[..3]);
            }
        }
    }
    drop(data);
    buffer.unmap();
    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_all_off() {
        let d = DebugOpts::default();
        assert!(d.time_of_day.is_none() && d.shot.is_none() && !d.freezes_clock());
    }
}
