use super::big_button;
use super::settings::{graphics_screen, GraphicsSettings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuAction {
    Play,
    Settings,
    Quit,
}

pub fn main_menu_screen(ctx: &egui::Context) -> Option<MainMenuAction> {
    let mut action = None;
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.25).max(20.0));
            ui.label(
                egui::RichText::new("Dungeon n Evolve")
                    .size(56.0)
                    .strong()
                    .color(egui::Color32::from_rgb(230, 190, 90)),
            );
            ui.label(egui::RichText::new("Aventura voxel medieval").size(18.0).weak());
            ui.add_space(40.0);
            if big_button(ui, "Iniciar Jogo") {
                action = Some(MainMenuAction::Play);
            }
            ui.add_space(10.0);
            if big_button(ui, "Configurações") {
                action = Some(MainMenuAction::Settings);
            }
            ui.add_space(10.0);
            if big_button(ui, "Sair") {
                action = Some(MainMenuAction::Quit);
            }
        });
    });
    action
}

/// Tela de configurações (gráficos), centralizada. Retorna true quando o usuário clica em "Voltar".
/// (Mudanças são aplicadas direto em `settings`; o chamador decide quando salvar.)
pub fn settings_screen(ctx: &egui::Context, settings: &mut GraphicsSettings) -> bool {
    graphics_screen(ctx, settings, false).0
}
