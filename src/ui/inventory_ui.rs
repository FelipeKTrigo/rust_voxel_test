//! Parte 2 — interface do inventário (egui): hotbar no HUD, tela do inventário com a visão do
//! personagem e equipamentos, painel do modo Criativo e relógio do mundo.
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use crate::inventory::{
    Click, EquipSlot, Gear, Inventory, Item, Slot, SlotId, MAIN_COLS, MAX_TRINKET_SLOTS,
};
use crate::world::block::BlockType;
use crate::world::textures::{self as tx, FaceKind};

const GOLD: Color32 = Color32::from_rgb(230, 190, 90);
const PANEL: Color32 = Color32::from_rgb(26, 28, 40);
const SLOT_BG: Color32 = Color32::from_rgb(44, 47, 64);
const SLOT_DISABLED: Color32 = Color32::from_rgb(30, 31, 40);

// ------------------------------------------------------------------------- ícones

/// Atlas com as texturas dos blocos como textura do egui (mesmos tiles do jogo 3D).
pub struct BlockIcons {
    texture: egui::TextureHandle,
}

const ATLAS_COLS: usize = 12;

impl BlockIcons {
    pub fn new(ctx: &egui::Context) -> Self {
        let rows = tx::NUM_LAYERS.div_ceil(ATLAS_COLS);
        let (w, h) = (ATLAS_COLS * tx::TILE, rows * tx::TILE);
        let mut pixels = vec![Color32::TRANSPARENT; w * h];
        for (layer, tile) in tx::all_tiles().iter().enumerate() {
            let (ox, oy) = ((layer % ATLAS_COLS) * tx::TILE, (layer / ATLAS_COLS) * tx::TILE);
            for y in 0..tx::TILE {
                for x in 0..tx::TILE {
                    let p = tile[y * tx::TILE + x];
                    pixels[(oy + y) * w + ox + x] = Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]);
                }
            }
        }
        let image = egui::ColorImage { size: [w, h], pixels };
        Self { texture: ctx.load_texture("block_icons", image, egui::TextureOptions::NEAREST) }
    }

    fn uv(&self, layer: usize) -> Rect {
        let rows = tx::NUM_LAYERS.div_ceil(ATLAS_COLS) as f32;
        let (cx, cy) = ((layer % ATLAS_COLS) as f32, (layer / ATLAS_COLS) as f32);
        Rect::from_min_max(
            Pos2::new(cx / ATLAS_COLS as f32, cy / rows),
            Pos2::new((cx + 1.0) / ATLAS_COLS as f32, (cy + 1.0) / rows),
        )
    }

    pub fn paint_block(&self, painter: &egui::Painter, rect: Rect, block: BlockType) {
        let layer = tx::layer_for(block, FaceKind::Side) as usize;
        painter.image(self.texture.id(), rect, self.uv(layer), Color32::WHITE);
    }
}

fn paint_item(painter: &egui::Painter, rect: Rect, item: Item, count: u32, icons: &BlockIcons) {
    let inner = rect.shrink(rect.width() * 0.16);
    match item {
        Item::Block(b) => {
            icons.paint_block(painter, inner, b);
            painter.rect_stroke(inner, 1.0, Stroke::new(1.0, Color32::from_black_alpha(120)));
        }
        Item::Gear(g) => {
            let c = g.color();
            let col = Color32::from_rgb(c[0], c[1], c[2]);
            painter.rect_filled(inner, 6.0, col);
            painter.rect_stroke(inner, 6.0, Stroke::new(1.5, Color32::from_black_alpha(140)));
            let letter: String = g.label().chars().take(2).collect();
            painter.text(inner.center(), Align2::CENTER_CENTER, letter, FontId::proportional(rect.width() * 0.34), Color32::from_black_alpha(190));
        }
    }
    if count > 1 {
        painter.text(
            rect.right_bottom() - Vec2::new(4.0, 2.0),
            Align2::RIGHT_BOTTOM,
            count.to_string(),
            FontId::monospace(13.0),
            Color32::WHITE,
        );
    }
}

/// Desenha um slot e devolve a resposta de clique.
fn slot_ui(
    ui: &mut egui::Ui,
    id: egui::Id,
    rect: Rect,
    slot: &Slot,
    enabled: bool,
    highlight: bool,
    icons: &BlockIcons,
) -> egui::Response {
    let resp = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    p.rect_filled(rect, 4.0, if enabled { SLOT_BG } else { SLOT_DISABLED });
    let stroke = if highlight {
        Stroke::new(2.5, GOLD)
    } else if resp.hovered() && enabled {
        Stroke::new(1.5, Color32::from_rgb(160, 170, 210))
    } else {
        Stroke::new(1.0, Color32::from_rgb(70, 74, 98))
    };
    p.rect_stroke(rect, 4.0, stroke);
    if !enabled {
        p.text(rect.center(), Align2::CENTER_CENTER, "×", FontId::proportional(18.0), Color32::from_gray(80));
    }
    if let Some(st) = slot {
        paint_item(p, rect, st.item, st.count, icons);
        if resp.hovered() {
            resp.clone().on_hover_text(st.item.label());
        }
    }
    resp
}

// ------------------------------------------------------------------------- HUD

pub const HUD_SLOT: f32 = 52.0;

/// Hotbar centralizada na parte inferior da tela.
pub fn hotbar_hud(ctx: &egui::Context, inv: &Inventory, icons: &BlockIcons) {
    egui::Area::new(egui::Id::new("hotbar_hud"))
        .anchor(Align2::CENTER_BOTTOM, [0.0, -14.0])
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            if let Some(st) = inv.selected_item() {
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(st.item.label()).size(15.0).color(Color32::WHITE).strong());
                });
            }
            egui::Frame::none()
                .fill(Color32::from_black_alpha(150))
                .rounding(8.0)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    let n = inv.hotbar.len();
                    let gap = 5.0;
                    let (area, _) = ui.allocate_exact_size(
                        Vec2::new(n as f32 * (HUD_SLOT + gap) - gap, HUD_SLOT),
                        Sense::hover(),
                    );
                    for i in 0..n {
                        let rect = Rect::from_min_size(
                            area.min + Vec2::new(i as f32 * (HUD_SLOT + gap), 0.0),
                            Vec2::splat(HUD_SLOT),
                        );
                        // (a área do HUD é `interactable(false)`: os slots apenas exibem)
                        slot_ui(ui, egui::Id::new(("hb", i)), rect, &inv.hotbar[i], true, i == inv.selected, icons);
                        ui.painter().text(
                            rect.left_top() + Vec2::new(4.0, 2.0),
                            Align2::LEFT_TOP,
                            (i + 1).to_string(),
                            FontId::monospace(11.0),
                            Color32::from_white_alpha(150),
                        );
                    }
                });
        });
}

/// Dia, semana, hora e fase da lua (canto superior direito).
pub fn clock_hud(ctx: &egui::Context, line1: &str, line2: &str) {
    egui::Area::new(egui::Id::new("clock_hud"))
        .anchor(Align2::RIGHT_TOP, [-12.0, 10.0])
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::none().fill(Color32::from_black_alpha(120)).rounding(6.0).inner_margin(6.0).show(ui, |ui| {
                ui.label(egui::RichText::new(line1).color(Color32::WHITE).size(14.0));
                ui.label(egui::RichText::new(line2).color(Color32::from_rgb(200, 210, 240)).size(13.0));
            });
        });
}

// ------------------------------------------------------------------------- tela do inventário

const S: f32 = 46.0;
const G: f32 = 5.0;

fn cell(origin: Pos2, col: usize, row: usize) -> Rect {
    Rect::from_min_size(origin + Vec2::new(col as f32 * (S + G), row as f32 * (S + G)), Vec2::splat(S))
}

fn apply_click(inv: &mut Inventory, held: &mut Slot, id: SlotId, resp: &egui::Response) {
    if resp.clicked() {
        inv.click_slot(id, held, Click::Left);
    } else if resp.secondary_clicked() {
        inv.click_slot(id, held, Click::Right);
    }
}

/// Figura do personagem; peças equipadas mudam a aparência.
fn paint_character(painter: &egui::Painter, area: Rect, inv: &Inventory) {
    let u = 7.0;
    let c = area.center();
    let at = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(Pos2::new(c.x + x * u, c.y + y * u), Vec2::new(w * u, h * u));
    let skin = Color32::from_rgb(238, 198, 158);
    let tunic = Color32::from_rgb(70, 100, 170);
    let pants = Color32::from_rgb(92, 72, 52);
    let iron = Color32::from_rgb(168, 174, 186);
    let leather = Color32::from_rgb(120, 80, 44);
    let boots = if inv.equipped(EquipSlot::Boots).is_some() { iron } else { Color32::from_rgb(60, 45, 35) };
    let has = |s| inv.equipped(s).is_some();

    // sombra
    painter.rect_filled(at(-5.0, 10.2, 10.0, 1.0), 6.0, Color32::from_black_alpha(80));
    // bolsa nas costas (aparece ao lado)
    if has(EquipSlot::Bag) {
        painter.rect_filled(at(3.2, -1.0, 3.0, 5.0), 3.0, leather);
        painter.rect_stroke(at(3.2, -1.0, 3.0, 5.0), 3.0, Stroke::new(1.0, Color32::BLACK));
    }
    // pernas
    let leg = if has(EquipSlot::Leggings) { iron } else { pants };
    painter.rect_filled(at(-2.5, 4.0, 2.4, 6.0), 1.0, leg);
    painter.rect_filled(at(0.1, 4.0, 2.4, 6.0), 1.0, leg);
    painter.rect_filled(at(-2.8, 9.0, 2.7, 1.4), 1.0, boots);
    painter.rect_filled(at(0.1, 9.0, 2.7, 1.4), 1.0, boots);
    // tronco
    painter.rect_filled(at(-3.0, -2.0, 6.0, 6.2), 1.0, if has(EquipSlot::Chestplate) { iron } else { tunic });
    // braços e mãos
    painter.rect_filled(at(-5.0, -2.0, 2.0, 5.4), 1.0, if has(EquipSlot::Chestplate) { iron } else { tunic });
    painter.rect_filled(at(3.0, -2.0, 2.0, 5.4), 1.0, if has(EquipSlot::Chestplate) { iron } else { tunic });
    let hand = if has(EquipSlot::Gloves) { leather } else { skin };
    painter.rect_filled(at(-5.0, 3.0, 2.0, 1.6), 1.0, hand);
    painter.rect_filled(at(3.0, 3.0, 2.0, 1.6), 1.0, hand);
    // anéis
    let gold = Color32::from_rgb(240, 205, 70);
    if has(EquipSlot::RingLeft) { painter.rect_filled(at(-4.5, 3.6, 0.9, 0.9), 0.0, gold); }
    if has(EquipSlot::RingRight) { painter.rect_filled(at(3.6, 3.6, 0.9, 0.9), 0.0, gold); }
    // cinto e bugigangas penduradas
    if has(EquipSlot::Belt) {
        painter.rect_filled(at(-3.0, 2.6, 6.0, 1.1), 0.0, leather);
        painter.rect_filled(at(-0.6, 2.5, 1.2, 1.3), 0.0, gold);
        let n = inv.trinket_slots_active().min(MAX_TRINKET_SLOTS);
        for i in 0..n {
            if let Some(st) = inv.trinkets[i] {
                if let Item::Gear(g) = st.item {
                    let col = g.color();
                    let x = -2.8 + i as f32 * 1.5;
                    painter.rect_filled(at(x, 3.7, 1.1, 1.4), 1.0, Color32::from_rgb(col[0], col[1], col[2]));
                }
            }
        }
    }
    // cabeça
    painter.rect_filled(at(-2.5, -7.5, 5.0, 5.5), 1.0, skin);
    painter.rect_filled(at(-2.5, -7.5, 5.0, 1.6), 0.0, Color32::from_rgb(86, 56, 32)); // cabelo
    painter.rect_filled(at(-1.6, -5.2, 0.8, 0.8), 0.0, Color32::BLACK);
    painter.rect_filled(at(0.8, -5.2, 0.8, 0.8), 0.0, Color32::BLACK);
    if has(EquipSlot::Helmet) {
        painter.rect_filled(at(-2.9, -8.1, 5.8, 3.0), 1.0, iron);
        painter.rect_filled(at(-0.4, -5.2, 0.8, 2.0), 0.0, iron);
    }
    if has(EquipSlot::Necklace) {
        painter.rect_filled(at(-1.8, -2.2, 3.6, 0.7), 0.0, gold);
        painter.rect_filled(at(-0.4, -1.5, 0.8, 0.9), 0.0, Color32::from_rgb(120, 200, 230));
    }
}

/// Itens do painel do modo Criativo: todos os blocos e equipamentos.
fn creative_catalog() -> Vec<Item> {
    let mut v: Vec<Item> = (1..tx::NUM_BLOCKS as u8)
        .map(BlockType::from_u8)
        .filter(|b| *b != BlockType::Water && *b != BlockType::SulfuricAcid)
        .map(Item::Block)
        .collect();
    v.extend(Gear::ALL.iter().map(|g| Item::Gear(*g)));
    v
}

/// Tela do inventário. `held` é o item preso ao cursor. Devolve `true` quando o jogador pede
/// para fechar (botão "Fechar").
pub fn inventory_screen(
    ctx: &egui::Context,
    inv: &mut Inventory,
    held: &mut Slot,
    creative: bool,
    icons: &BlockIcons,
) -> bool {
    let mut close = false;
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(Color32::from_black_alpha(150)))
        .show(ctx, |_| {});

    egui::Area::new(egui::Id::new("inventory_area"))
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(PANEL)
                .stroke(Stroke::new(2.0, GOLD))
                .rounding(10.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(egui::RichText::new("Inventário").color(GOLD));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Fechar (E)").clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.add_space(8.0);
                    ui.horizontal_top(|ui| {
                        // ---------------------------------------------- coluna 1: mochila + hotbar
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Mochila").strong());
                            let active = inv.main_slots_active();
                            let rows = active / MAIN_COLS;
                            let (area, _) = ui.allocate_exact_size(
                                Vec2::new(MAIN_COLS as f32 * (S + G) - G, rows as f32 * (S + G) - G),
                                Sense::hover(),
                            );
                            for i in 0..active {
                                let rect = cell(area.min, i % MAIN_COLS, i / MAIN_COLS);
                                let resp = slot_ui(ui, egui::Id::new(("main", i)), rect, &inv.main[i], true, false, icons);
                                apply_click(inv, held, SlotId::Main(i), &resp);
                            }
                            ui.add_space(10.0);
                            ui.label(egui::RichText::new(format!("Barra rápida (teclas 1–{})", inv.hotbar.len())).strong());
                            let n = inv.hotbar.len();
                            let (area, _) = ui.allocate_exact_size(
                                Vec2::new(MAIN_COLS as f32 * (S + G) - G, S),
                                Sense::hover(),
                            );
                            for i in 0..n {
                                let rect = cell(area.min, i, 0);
                                let resp = slot_ui(ui, egui::Id::new(("hotbar", i)), rect, &inv.hotbar[i], true, i == inv.selected, icons);
                                apply_click(inv, held, SlotId::Hotbar(i), &resp);
                            }
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Clique esquerdo: pegar/soltar · direito: metade/um")
                                    .small()
                                    .color(Color32::from_gray(150)),
                            );
                        });
                        ui.add_space(22.0);

                        // ---------------------------------------------- coluna 2: personagem
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Personagem").strong());
                            let fig_w = 3.0 * (S + G);
                            let total_w = 2.0 * (S + G) + fig_w;
                            let (area, _) = ui.allocate_exact_size(Vec2::new(total_w - G, 6.0 * (S + G) - G), Sense::hover());
                            let left = [EquipSlot::Helmet, EquipSlot::Chestplate, EquipSlot::Leggings, EquipSlot::Boots];
                            let right = [
                                EquipSlot::Gloves, EquipSlot::Necklace, EquipSlot::RingLeft,
                                EquipSlot::RingRight, EquipSlot::Bag, EquipSlot::Belt,
                            ];
                            let fig_rect = Rect::from_min_size(
                                area.min + Vec2::new(S + G, 0.0),
                                Vec2::new(fig_w - G, 6.0 * (S + G) - G),
                            );
                            ui.painter().rect_filled(fig_rect, 6.0, Color32::from_rgb(34, 37, 54));
                            ui.painter().rect_stroke(fig_rect, 6.0, Stroke::new(1.0, Color32::from_rgb(70, 74, 98)));
                            paint_character(ui.painter(), fig_rect, inv);
                            for (i, e) in left.iter().enumerate() {
                                let rect = cell(area.min, 0, i);
                                let resp = slot_ui(ui, egui::Id::new(("eq", i)), rect, &inv.equip[equip_idx(*e)], true, false, icons);
                                if inv.equip[equip_idx(*e)].is_none() {
                                    ui.painter().text(rect.center_bottom() + Vec2::new(0.0, -2.0), Align2::CENTER_BOTTOM, e.label(), FontId::proportional(9.0), Color32::from_white_alpha(110));
                                }
                                apply_click(inv, held, SlotId::Equip(*e), &resp);
                            }
                            for (i, e) in right.iter().enumerate() {
                                let rect = cell(area.min, 4, i);
                                let resp = slot_ui(ui, egui::Id::new(("eqr", i)), rect, &inv.equip[equip_idx(*e)], true, false, icons);
                                if inv.equip[equip_idx(*e)].is_none() {
                                    ui.painter().text(rect.center_bottom() + Vec2::new(0.0, -2.0), Align2::CENTER_BOTTOM, e.label(), FontId::proportional(9.0), Color32::from_white_alpha(110));
                                }
                                apply_click(inv, held, SlotId::Equip(*e), &resp);
                            }
                            ui.add_space(8.0);
                            let slots = inv.trinket_slots_active();
                            ui.label(
                                egui::RichText::new(format!("Cinto — bugigangas ({slots} slots)")).strong(),
                            );
                            let (area, _) = ui.allocate_exact_size(
                                Vec2::new(MAX_TRINKET_SLOTS as f32 * (S + G) - G, S),
                                Sense::hover(),
                            );
                            for i in 0..MAX_TRINKET_SLOTS {
                                let rect = cell(area.min, i, 0);
                                let resp = slot_ui(ui, egui::Id::new(("tr", i)), rect, &inv.trinkets[i], i < slots, false, icons);
                                if i < slots {
                                    apply_click(inv, held, SlotId::Trinket(i), &resp);
                                }
                            }
                            if slots == 0 {
                                ui.label(egui::RichText::new("Equipe um cinto para liberar slots.").small().color(Color32::from_gray(150)));
                            }
                        });

                        // ---------------------------------------------- coluna 3: criativo
                        if creative {
                            ui.add_space(22.0);
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("Criativo").strong().color(Color32::from_rgb(130, 220, 170)));
                                let catalog = creative_catalog();
                                let cols = 6;
                                egui::ScrollArea::vertical().max_height(6.0 * (S + G) + 30.0).id_salt("creative_scroll").show(ui, |ui| {
                                    let rows = catalog.len().div_ceil(cols);
                                    let (area, _) = ui.allocate_exact_size(
                                        Vec2::new(cols as f32 * (S + G) - G, rows as f32 * (S + G) - G),
                                        Sense::hover(),
                                    );
                                    for (i, item) in catalog.iter().enumerate() {
                                        let rect = cell(area.min, i % cols, i / cols);
                                        let st = Some(crate::inventory::ItemStack::new(*item, 1));
                                        let resp = slot_ui(ui, egui::Id::new(("cr", i)), rect, &st, true, false, icons);
                                        if resp.clicked() {
                                            *held = Some(crate::inventory::ItemStack::new(*item, item.max_stack()));
                                        }
                                    }
                                });
                                ui.add_space(6.0);
                                let (rect, _) = ui.allocate_exact_size(Vec2::new(cols as f32 * (S + G) - G, 30.0), Sense::hover());
                                let resp = ui.interact(rect, egui::Id::new("trash"), Sense::click());
                                ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(70, 36, 36));
                                ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_rgb(190, 90, 90)));
                                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Lixeira (solte aqui)", FontId::proportional(13.0), Color32::from_rgb(240, 180, 180));
                                if resp.clicked() {
                                    *held = None;
                                }
                            });
                        }
                    });
                });
        });

    // Item preso ao cursor
    if let (Some(st), Some(pos)) = (*held, ctx.pointer_hover_pos()) {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("held_item")));
        let rect = Rect::from_center_size(pos + Vec2::new(8.0, 8.0), Vec2::splat(S));
        paint_item(&painter, rect, st.item, st.count, icons);
    }
    close
}

fn equip_idx(e: EquipSlot) -> usize {
    EquipSlot::ALL.iter().position(|s| *s == e).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creative_catalog_has_blocks_and_all_gear_once() {
        let c = creative_catalog();
        assert!(c.iter().all(|i| !matches!(i, Item::Block(BlockType::Air | BlockType::Water))));
        for g in Gear::ALL {
            assert_eq!(c.iter().filter(|i| **i == Item::Gear(g)).count(), 1);
        }
        assert!(c.contains(&Item::Block(BlockType::Stone)) && c.contains(&Item::Block(BlockType::Gold)));
        let mut unique = c.clone();
        unique.dedup();
        assert_eq!(unique.len(), c.len());
    }

    #[test]
    fn atlas_layout_covers_all_layers() {
        assert!(ATLAS_COLS * tx::NUM_LAYERS.div_ceil(ATLAS_COLS) >= tx::NUM_LAYERS);
    }

    #[test]
    fn equip_index_is_stable() {
        for (i, e) in EquipSlot::ALL.iter().enumerate() {
            assert_eq!(equip_idx(*e), i);
        }
    }

    fn frame(ctx: &egui::Context, f: impl FnMut(&egui::Context)) {
        let mut raw = egui::RawInput::default();
        raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0)));
        let _ = ctx.run(raw, f);
    }

    #[test]
    fn screens_render_headless_without_panicking() {
        use crate::inventory::{EquipSlot, ItemStack};
        let ctx = egui::Context::default();
        let icons = BlockIcons::new(&ctx);
        let mut inv = Inventory::new();
        inv.add_item(Item::Block(BlockType::Stone), 40);
        inv.equip[EquipSlot::ALL.iter().position(|s| *s == EquipSlot::Belt).unwrap()] =
            Some(ItemStack::new(Item::Gear(Gear::AdventurerBelt), 1));
        inv.trinkets[0] = Some(ItemStack::new(Item::Gear(Gear::Lantern), 1));
        let mut held = Some(ItemStack::new(Item::Block(BlockType::Gold), 5));
        for creative in [false, true] {
            let mut closed = true;
            for _ in 0..2 {
                frame(&ctx, |c| {
                    closed = inventory_screen(c, &mut inv, &mut held, creative, &icons);
                    hotbar_hud(c, &inv, &icons);
                    clock_hud(c, "Dia 1", "Lua cheia");
                });
            }
            assert!(!closed, "o botão Fechar só retorna true quando clicado");
        }
        assert!(held.is_some() && inv.count_of(Item::Block(BlockType::Stone)) == 40, "sem cliques nada muda");
    }

    #[test]
    fn graphics_screen_renders_and_reports_no_change_without_input() {
        let ctx = egui::Context::default();
        let mut s = crate::ui::settings::GraphicsSettings::default();
        let before = s.clone();
        let mut res = (true, true);
        for _ in 0..2 {
            frame(&ctx, |c| res = crate::ui::settings::graphics_screen(c, &mut s, true));
        }
        assert_eq!(res, (false, false));
        assert_eq!(s, before);
    }

    // ------------------------------------------------------------------------------------------
    // Cliques de verdade: eventos de ponteiro do egui -> slot desenhado -> `Inventory::click_slot`.
    // Garante que cada slot na tela está ligado ao `SlotId` certo (colunas de equipamento, bugigangas…).

    struct Ui {
        ctx: egui::Context,
        icons: BlockIcons,
        inv: Inventory,
        held: Slot,
        creative: bool,
        time: f64,
        closed: bool,
    }

    impl Ui {
        fn new(creative: bool) -> Self {
            let ctx = egui::Context::default();
            let icons = BlockIcons::new(&ctx);
            let mut u = Self { ctx, icons, inv: Inventory::new(), held: None, creative, time: 0.0, closed: false };
            for _ in 0..3 {
                u.frame(vec![]); // aquecimento: Areas centralizadas medem o tamanho no 1º quadro
            }
            u
        }

        fn frame(&mut self, events: Vec<egui::Event>) {
            self.time += 0.05;
            let mut raw = egui::RawInput::default();
            raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0)));
            raw.time = Some(self.time);
            raw.events = events;
            let (inv, held, icons, creative) = (&mut self.inv, &mut self.held, &self.icons, self.creative);
            let mut closed = false;
            let _ = self.ctx.run(raw, |c| {
                closed |= inventory_screen(c, inv, held, creative, icons);
            });
            self.closed |= closed;
        }

        fn center(&self, id: impl std::hash::Hash) -> Pos2 {
            self.ctx
                .read_response(egui::Id::new(id))
                .expect("slot não foi desenhado")
                .rect
                .center()
        }

        fn click_at(&mut self, pos: Pos2, button: egui::PointerButton) {
            let m = egui::Modifiers::NONE;
            self.frame(vec![egui::Event::PointerMoved(pos)]);
            self.frame(vec![egui::Event::PointerButton { pos, button, pressed: true, modifiers: m }]);
            self.frame(vec![egui::Event::PointerButton { pos, button, pressed: false, modifiers: m }]);
            self.frame(vec![]);
        }

        fn click(&mut self, id: impl std::hash::Hash) {
            let p = self.center(id);
            self.click_at(p, egui::PointerButton::Primary);
        }

        fn right_click(&mut self, id: impl std::hash::Hash) {
            let p = self.center(id);
            self.click_at(p, egui::PointerButton::Secondary);
        }
    }

    fn st(item: Item, n: u32) -> Slot {
        Some(crate::inventory::ItemStack::new(item, n))
    }

    #[test]
    fn clicking_moves_items_between_hotbar_and_inventory() {
        let mut u = Ui::new(false);
        u.inv.hotbar[0] = st(Item::Block(BlockType::Stone), 20);
        u.click(("hotbar", 0));
        assert_eq!(u.held, st(Item::Block(BlockType::Stone), 20), "pegou da hotbar");
        assert!(u.inv.hotbar[0].is_none());
        u.click(("main", 10));
        assert!(u.held.is_none());
        assert_eq!(u.inv.main[10], st(Item::Block(BlockType::Stone), 20), "soltou na mochila");
        // de volta para outro slot da hotbar
        u.click(("main", 10));
        u.click(("hotbar", 3));
        assert_eq!(u.inv.hotbar[3], st(Item::Block(BlockType::Stone), 20));
        assert_eq!(u.inv.count_of(Item::Block(BlockType::Stone)), 20);
    }

    #[test]
    fn right_click_splits_a_stack() {
        let mut u = Ui::new(false);
        u.inv.main[0] = st(Item::Block(BlockType::Dirt), 10);
        u.right_click(("main", 0));
        assert_eq!(u.held, st(Item::Block(BlockType::Dirt), 5));
        assert_eq!(u.inv.main[0], st(Item::Block(BlockType::Dirt), 5));
        u.right_click(("main", 1));
        assert_eq!(u.inv.main[1], st(Item::Block(BlockType::Dirt), 1), "direito solta um");
        assert_eq!(u.held, st(Item::Block(BlockType::Dirt), 4));
    }

    #[test]
    fn left_column_equipment_slots_are_wired_to_the_right_slots() {
        let mut u = Ui::new(false);
        u.inv.main[0] = st(Item::Gear(Gear::IronHelmet), 1);
        u.inv.main[1] = st(Item::Gear(Gear::IronBoots), 1);
        u.inv.main[2] = st(Item::Block(BlockType::Stone), 1);
        // Elmo no slot "Peitoral" (coluna esquerda, índice 1): recusado
        u.click(("main", 0));
        u.click(("eq", 1));
        assert!(u.inv.equipped(EquipSlot::Chestplate).is_none(), "elmo não vai no peitoral");
        assert_eq!(u.held, st(Item::Gear(Gear::IronHelmet), 1), "continua na mão");
        // Elmo no slot "Elmo" (índice 0): aceito
        u.click(("eq", 0));
        assert_eq!(u.inv.equipped(EquipSlot::Helmet), Some(Gear::IronHelmet));
        assert!(u.held.is_none());
        // Botas no slot de botas (índice 3)
        u.click(("main", 1));
        u.click(("eq", 3));
        assert_eq!(u.inv.equipped(EquipSlot::Boots), Some(Gear::IronBoots));
        // Bloco no elmo: recusado
        u.click(("main", 2));
        u.click(("eq", 0));
        assert_eq!(u.inv.equipped(EquipSlot::Helmet), Some(Gear::IronHelmet));
        assert_eq!(u.held, st(Item::Block(BlockType::Stone), 1));
    }

    #[test]
    fn right_column_equipment_slots_are_wired_to_the_right_slots() {
        // luvas, colar, anel, anel, bolsa, cinto = ("eqr", 0..=5)
        let cases = [
            (Gear::LeatherGloves, 0, EquipSlot::Gloves),
            (Gear::Pendant, 1, EquipSlot::Necklace),
            (Gear::GoldRing, 2, EquipSlot::RingLeft),
            (Gear::SilverRing, 3, EquipSlot::RingRight),
            (Gear::TravelerBag, 4, EquipSlot::Bag),
            (Gear::LeatherBelt, 5, EquipSlot::Belt),
        ];
        let mut u = Ui::new(false);
        for (i, (g, _, _)) in cases.iter().enumerate() {
            u.inv.main[i] = st(Item::Gear(*g), 1);
        }
        for (i, (g, col, slot)) in cases.iter().enumerate() {
            u.click(("main", i));
            u.click(("eqr", *col));
            assert_eq!(u.inv.equipped(*slot), Some(*g), "{g:?} deveria ir para {slot:?}");
        }
        assert!(u.held.is_none());
        // A bolsa liberou a 4ª fileira da mochila: o slot 27 passa a existir na tela
        assert!(u.ctx.read_response(egui::Id::new(("main", 27))).is_some());
        // item errado na coluna direita: anel nas luvas é recusado
        u.inv.main[10] = st(Item::Gear(Gear::GoldRing), 1);
        u.click(("main", 10));
        u.click(("eqr", 0));
        assert_eq!(u.inv.equipped(EquipSlot::Gloves), Some(Gear::LeatherGloves));
        assert_eq!(u.held, st(Item::Gear(Gear::GoldRing), 1));
    }

    #[test]
    fn belt_unlocks_trinket_slots_on_screen() {
        let mut u = Ui::new(false);
        u.inv.main[0] = st(Item::Gear(Gear::Lantern), 1);
        u.inv.main[1] = st(Item::Gear(Gear::AdventurerBelt), 1);
        // sem cinto, a bugiganga não entra
        u.click(("main", 0));
        u.click(("tr", 0));
        assert!(u.inv.trinkets[0].is_none());
        assert_eq!(u.held, st(Item::Gear(Gear::Lantern), 1));
        u.click(("main", 0)); // devolve
        // veste o cinto (4 slots) e agora entra, até o 4º slot e não além
        u.click(("main", 1));
        u.click(("eqr", 5));
        assert_eq!(u.inv.trinket_slots_active(), 4);
        u.click(("main", 0));
        u.click(("tr", 3));
        assert_eq!(u.inv.trinkets[3], st(Item::Gear(Gear::Lantern), 1));
        // tirar o cinto com bugiganga dentro é recusado
        u.click(("eqr", 5));
        assert_eq!(u.inv.equipped(EquipSlot::Belt), Some(Gear::AdventurerBelt));
        assert!(u.held.is_none(), "nada fica preso na mão");
    }

    #[test]
    fn creative_catalog_gives_items_and_trash_deletes_them() {
        let mut u = Ui::new(true);
        let idx = creative_catalog().iter().position(|i| *i == Item::Block(BlockType::Gold)).unwrap();
        u.click(("cr", idx));
        assert_eq!(u.held, st(Item::Block(BlockType::Gold), 64));
        u.click(("hotbar", 2));
        assert_eq!(u.inv.hotbar[2], st(Item::Block(BlockType::Gold), 64));
        // equipamento direto do catálogo para a ranhura certa
        let g = creative_catalog().iter().position(|i| *i == Item::Gear(Gear::IronHelmet)).unwrap();
        u.click(("cr", g));
        assert_eq!(u.held, st(Item::Gear(Gear::IronHelmet), 1));
        u.click(("eq", 0));
        assert_eq!(u.inv.equipped(EquipSlot::Helmet), Some(Gear::IronHelmet));
        // lixeira apaga o que está na mão
        u.click(("cr", idx));
        assert!(u.held.is_some());
        let p = u.ctx.read_response(egui::Id::new("trash")).expect("lixeira").rect.center();
        u.click_at(p, egui::PointerButton::Primary);
        assert!(u.held.is_none());
    }

    #[test]
    fn survival_has_no_creative_panel() {
        let u = Ui::new(false);
        assert!(u.ctx.read_response(egui::Id::new(("cr", 0))).is_none());
        assert!(u.ctx.read_response(egui::Id::new("trash")).is_none());
    }
}
