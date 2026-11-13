use super::settings::{settings_panel, GraphicsSettings};
use super::{big_button, PlayerModeUi};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseAction {
    Resume,
    SaveAndQuit,
    QuitWithoutSaving,
    SetMode(PlayerModeUi),
}

#[derive(Default, Debug)]
pub struct PauseState {
    pub confirm_quit_without_saving: bool,
}

pub fn pause_menu_screen(
    ctx: &egui::Context,
    current_mode: PlayerModeUi,
    settings: &mut GraphicsSettings,
    state: &mut PauseState,
) -> Option<PauseAction> {
    let mut action = None;
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200)))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(30.0);
                    ui.heading(egui::RichText::new("Jogo Pausado").size(36.0));
                    ui.add_space(16.0);
                    if big_button(ui, "Despausar") {
                        action = Some(PauseAction::Resume);
                    }
                    ui.add_space(12.0);
                    ui.label("Modo de jogo");
                    ui.horizontal(|ui| {
                        // centraliza aproximadamente os 3 botões
                        ui.add_space(((ui.available_width() - 3.0 * 110.0) / 2.0).max(0.0));
                        for m in PlayerModeUi::ALL {
                            let b = egui::SelectableLabel::new(current_mode == m, m.label());
                            if ui.add_sized([100.0, 28.0], b).clicked() && m != current_mode {
                                action = Some(PauseAction::SetMode(m));
                            }
                        }
                    });
                    ui.add_space(12.0);
                    egui::CollapsingHeader::new("Gráficos")
                        .default_open(false)
                        .show(ui, |ui| {
                            settings_panel(ui, settings);
                        });
                    ui.add_space(16.0);
                    if big_button(ui, "Salvar mundo e sair") {
                        action = Some(PauseAction::SaveAndQuit);
                    }
                    ui.add_space(8.0);
                    if state.confirm_quit_without_saving {
                        ui.colored_label(
                            egui::Color32::from_rgb(240, 120, 100),
                            "Progresso não salvo será perdido. Tem certeza?",
                        );
                        ui.horizontal(|ui| {
                            ui.add_space(((ui.available_width() - 2.0 * 130.0) / 2.0).max(0.0));
                            if ui.add_sized([125.0, 30.0], egui::Button::new("Sim, sair")).clicked() {
                                state.confirm_quit_without_saving = false;
                                action = Some(PauseAction::QuitWithoutSaving);
                            }
                            if ui.add_sized([125.0, 30.0], egui::Button::new("Cancelar")).clicked() {
                                state.confirm_quit_without_saving = false;
                            }
                        });
                    } else if big_button(ui, "Sair sem salvar") {
                        state.confirm_quit_without_saving = true;
                    }
                });
            });
        });
    if action == Some(PauseAction::Resume) {
        state.confirm_quit_without_saving = false;
    }
    action
}
