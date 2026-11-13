use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LodLevel {
    Off,
    Low,
    Medium,
    High,
}

impl LodLevel {
    pub const ALL: [LodLevel; 4] = [Self::Off, Self::Low, Self::Medium, Self::High];
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Desligado",
            Self::Low => "Baixo",
            Self::Medium => "Médio",
            Self::High => "Alto",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GraphicsSettings {
    pub render_distance: i32,
    pub reduce_particles: bool,
    pub lod_level: LodLevel,
    pub gen_threads: usize,
    /// 0.0 = tudo GPU .. 1.0 = tudo CPU
    pub cpu_gpu_balance: f32,
    /// 0 = desligado
    pub auto_save_minutes: u32,
}

pub fn max_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).max(1)
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            render_distance: 8,
            reduce_particles: false,
            lod_level: LodLevel::Medium,
            gen_threads: (max_threads() / 2).max(1),
            cpu_gpu_balance: 0.2,
            auto_save_minutes: 5,
        }
    }
}

impl GraphicsSettings {
    pub fn clamp(&mut self) {
        self.render_distance = self.render_distance.clamp(4, 24);
        self.gen_threads = self.gen_threads.clamp(1, max_threads());
        self.cpu_gpu_balance = if self.cpu_gpu_balance.is_finite() {
            self.cpu_gpu_balance.clamp(0.0, 1.0)
        } else {
            0.2
        };
        self.auto_save_minutes = self.auto_save_minutes.min(120);
    }

    /// Carrega de `path`; qualquer erro resulta nos valores padrão.
    pub fn load_or_default(path: &Path) -> Self {
        let mut s = std::fs::read(path)
            .ok()
            .and_then(|b| bincode::deserialize::<GraphicsSettings>(&b).ok())
            .unwrap_or_default();
        s.clamp();
        s
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let bytes = bincode::serialize(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        std::fs::write(path, bytes)
    }
}

/// Painel de configurações gráficas. Retorna true se algo mudou.
pub fn settings_panel(ui: &mut egui::Ui, s: &mut GraphicsSettings) -> bool {
    let mut changed = false;
    egui::Grid::new("graphics_settings_grid")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label("Distância de renderização");
            changed |= ui
                .add(egui::Slider::new(&mut s.render_distance, 4..=24).suffix(" chunks"))
                .changed();
            ui.end_row();

            ui.label("Reduzir partículas");
            changed |= ui.checkbox(&mut s.reduce_particles, "").changed();
            ui.end_row();

            ui.label("Nível de LOD");
            egui::ComboBox::from_id_salt("lod_level")
                .selected_text(s.lod_level.label())
                .show_ui(ui, |ui| {
                    for l in LodLevel::ALL {
                        changed |= ui.selectable_value(&mut s.lod_level, l, l.label()).changed();
                    }
                });
            ui.end_row();

            ui.label("Threads de geração");
            changed |= ui
                .add(egui::Slider::new(&mut s.gen_threads, 1..=max_threads()))
                .changed();
            ui.end_row();

            ui.label("Balanço GPU \u{2194} CPU");
            changed |= ui
                .add(egui::Slider::new(&mut s.cpu_gpu_balance, 0.0..=1.0).custom_formatter(|v, _| {
                    format!("{:.0}% CPU", v * 100.0)
                }))
                .changed();
            ui.end_row();

            ui.label("Salvamento automático");
            changed |= ui
                .add(
                    egui::Slider::new(&mut s.auto_save_minutes, 0..=60).custom_formatter(|v, _| {
                        if v < 0.5 { "Desligado".to_string() } else { format!("{:.0} min", v) }
                    }),
                )
                .changed();
            ui.end_row();
        });
    if changed {
        s.clamp();
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_limits() {
        let mut s = GraphicsSettings {
            render_distance: 100,
            gen_threads: 0,
            cpu_gpu_balance: 5.0,
            ..Default::default()
        };
        s.clamp();
        assert_eq!(s.render_distance, 24);
        assert_eq!(s.gen_threads, 1);
        assert_eq!(s.cpu_gpu_balance, 1.0);
        s.render_distance = -3;
        s.gen_threads = 10_000;
        s.clamp();
        assert_eq!(s.render_distance, 4);
        assert_eq!(s.gen_threads, max_threads());
    }

    #[test]
    fn bincode_roundtrip() {
        let dir = std::env::temp_dir().join(format!("dne_ui_settings_{}", std::process::id()));
        let path = dir.join("settings.bin");
        let mut s = GraphicsSettings::default();
        s.render_distance = 12;
        s.lod_level = LodLevel::High;
        s.auto_save_minutes = 0;
        s.save(&path).unwrap();
        assert_eq!(GraphicsSettings::load_or_default(&path), s);
        assert_eq!(
            GraphicsSettings::load_or_default(&dir.join("nao_existe.bin")),
            GraphicsSettings::default()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
