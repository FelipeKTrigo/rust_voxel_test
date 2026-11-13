use super::big_button;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct WorldEntry {
    pub name: String,
    pub seed: u32,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldSelectAction {
    Create { name: String, seed: u32 },
    Load(usize),
    Delete(usize),
    Back,
}

#[derive(Default, Debug)]
pub struct WorldSelectState {
    pub new_name: String,
    pub new_seed_text: String,
    pub selected: Option<usize>,
    pub confirm_delete: Option<usize>,
}

/// Lê metadados do mundo. Isolado: o formato real de save virá na Fase 7.
fn read_world_meta(_path: &Path) -> u32 {
    0
}

/// Lista mundos: subpastas ou arquivos `.dne`. Tolerante a erros.
pub fn list_worlds(saves_dir: &Path) -> Vec<WorldEntry> {
    let Ok(rd) = std::fs::read_dir(saves_dir) else { return Vec::new() };
    let mut out: Vec<WorldEntry> = rd
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = if path.is_dir() {
                path.file_name()?.to_str()?.to_string()
            } else if path.extension().and_then(|x| x.to_str()) == Some("dne") {
                path.file_stem()?.to_str()?.to_string()
            } else {
                return None;
            };
            let seed = read_world_meta(&path);
            Some(WorldEntry { name, seed, path })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Seed numérica, hash FNV-1a estável do texto, ou aleatória (tempo) se vazio.
pub fn seed_from_text(text: &str) -> u32 {
    let t = text.trim();
    if t.is_empty() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(12345);
        return (n ^ (n >> 32) ^ (n >> 64)) as u32;
    }
    if let Ok(n) = t.parse::<u32>() {
        return n;
    }
    let mut h: u32 = 0x811c9dc5;
    for b in t.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    h
}

/// Valida nome de novo mundo; Err com mensagem em português.
pub fn validate_world_name(name: &str, existing: &[WorldEntry]) -> Result<(), String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("O nome não pode ser vazio.".into());
    }
    if n.contains('/') || n.contains('\\') || n.contains("..") {
        return Err("O nome não pode conter / \\ ou ..".into());
    }
    if existing.iter().any(|w| w.name.eq_ignore_ascii_case(n)) {
        return Err("Já existe um mundo com esse nome.".into());
    }
    Ok(())
}

pub fn world_select_screen(
    ctx: &egui::Context,
    state: &mut WorldSelectState,
    worlds: &[WorldEntry],
) -> Option<WorldSelectAction> {
    let mut action = None;
    if state.selected.map_or(false, |i| i >= worlds.len()) {
        state.selected = None;
    }
    if state.confirm_delete.map_or(false, |i| i >= worlds.len()) {
        state.confirm_delete = None;
    }
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.heading("Selecionar Mundo");
            ui.add_space(10.0);
        });
        ui.separator();
        ui.label("Mundos salvos");
        egui::ScrollArea::vertical().max_height((ui.available_height() - 230.0).max(80.0)).show(ui, |ui| {
            if worlds.is_empty() {
                ui.weak("Nenhum mundo encontrado. Crie um abaixo.");
            }
            for (i, w) in worlds.iter().enumerate() {
                let sel = state.selected == Some(i);
                if ui.selectable_label(sel, format!("{}   (seed {})", w.name, w.seed)).clicked() {
                    state.selected = Some(i);
                    state.confirm_delete = None;
                }
            }
        });
        ui.horizontal(|ui| {
            let has = state.selected.is_some();
            if ui.add_enabled(has, egui::Button::new("Jogar")).clicked() {
                action = state.selected.map(WorldSelectAction::Load);
            }
            if ui.add_enabled(has, egui::Button::new("Excluir")).clicked() {
                state.confirm_delete = state.selected;
            }
            if let Some(i) = state.confirm_delete {
                ui.colored_label(
                    egui::Color32::from_rgb(240, 120, 100),
                    format!("Excluir \"{}\"?", worlds[i].name),
                );
                if ui.button("Sim").clicked() {
                    state.confirm_delete = None;
                    state.selected = None;
                    action = Some(WorldSelectAction::Delete(i));
                }
                if ui.button("Não").clicked() {
                    state.confirm_delete = None;
                }
            }
        });
        ui.separator();
        ui.label("Novo mundo");
        egui::Grid::new("new_world_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Nome");
            ui.text_edit_singleline(&mut state.new_name);
            ui.end_row();
            ui.label("Seed");
            ui.add(egui::TextEdit::singleline(&mut state.new_seed_text).hint_text("vazio = aleatória"));
            ui.end_row();
        });
        let validation = validate_world_name(&state.new_name, worlds);
        if !state.new_name.is_empty() {
            if let Err(msg) = &validation {
                ui.colored_label(egui::Color32::from_rgb(240, 120, 100), msg);
            }
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(validation.is_ok(), egui::Button::new("Criar mundo")).clicked() {
                action = Some(WorldSelectAction::Create {
                    name: state.new_name.trim().to_string(),
                    seed: seed_from_text(&state.new_seed_text),
                });
                state.new_name.clear();
                state.new_seed_text.clear();
            }
        });
        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            if big_button(ui, "Voltar") {
                action = Some(WorldSelectAction::Back);
            }
        });
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_numeric_and_stable() {
        assert_eq!(seed_from_text("12345"), 12345);
        assert_eq!(seed_from_text(" 7 "), 7);
        assert_eq!(seed_from_text("dragão"), seed_from_text("dragão"));
        assert_ne!(seed_from_text("a"), seed_from_text("b"));
        // valor fixo FNV-1a de "a"
        assert_eq!(seed_from_text("a"), 0xe40c292c);
        let _ = seed_from_text(""); // aleatória, não deve panicar
    }

    #[test]
    fn name_validation() {
        let ex = vec![WorldEntry { name: "Mundo".into(), seed: 0, path: PathBuf::new() }];
        assert!(validate_world_name("", &ex).is_err());
        assert!(validate_world_name("   ", &ex).is_err());
        assert!(validate_world_name("a/b", &ex).is_err());
        assert!(validate_world_name("a\\b", &ex).is_err());
        assert!(validate_world_name("..", &ex).is_err());
        assert!(validate_world_name("mundo", &ex).is_err());
        assert!(validate_world_name("Novo", &ex).is_ok());
    }

    #[test]
    fn list_worlds_temp() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dne_ui_worlds_{}_{}", std::process::id(), nanos));
        std::fs::create_dir_all(dir.join("beta")).unwrap();
        std::fs::write(dir.join("alfa.dne"), b"x").unwrap();
        std::fs::write(dir.join("lixo.txt"), b"x").unwrap();
        let w = list_worlds(&dir);
        let names: Vec<_> = w.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, vec!["alfa", "beta"]);
        assert!(list_worlds(&dir.join("nao_existe")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
