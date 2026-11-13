pub mod main_menu;
pub mod pause_menu;
pub mod settings;
pub mod world_select;

/// Modo de jogo exibido na UI (o orquestrador converte para o modo real do jogador).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerModeUi {
    Survival,
    Creative,
    Spectator,
}

impl PlayerModeUi {
    pub const ALL: [PlayerModeUi; 3] = [Self::Survival, Self::Creative, Self::Spectator];

    pub fn label(self) -> &'static str {
        match self {
            Self::Survival => "Sobrevivência",
            Self::Creative => "Criativo",
            Self::Spectator => "Espectador",
        }
    }
}

/// Botão grande padrão dos menus.
pub(crate) fn big_button(ui: &mut egui::Ui, text: &str) -> bool {
    ui.add_sized([260.0, 38.0], egui::Button::new(egui::RichText::new(text).size(18.0)))
        .clicked()
}
