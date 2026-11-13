pub mod inventory_ui;
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

/// Cor de destaque dos menus (a mesma do título).
pub(crate) const ACCENT: egui::Color32 = egui::Color32::from_rgb(230, 190, 90);

/// Painel centralizado e estilizado igual em todos os menus. `dim` escurece o fundo (pausa).
/// O fechamento do conteúdo devolve o que `content` retornar.
pub(crate) fn centered_panel<R>(
    ctx: &egui::Context,
    dim: bool,
    width: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let fill = if dim { egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200) } else { ctx.style().visuals.panel_fill };
    egui::CentralPanel::default().frame(egui::Frame::none().fill(fill)).show(ctx, |_| {});
    egui::Area::new(egui::Id::new("centered_panel"))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(26, 28, 40))
                .stroke(egui::Stroke::new(2.0, ACCENT))
                .rounding(10.0)
                .inner_margin(24.0)
                .show(ui, |ui| {
                    ui.set_width(width);
                    content(ui)
                })
                .inner
        })
        .inner
}
