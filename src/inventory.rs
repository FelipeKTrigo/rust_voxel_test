//! Parte 2 — inventário do personagem: hotbar, grade principal, equipamentos (armadura, anéis,
//! colar, luvas, bolsa, cinto) e bugigangas liberadas pelo cinto. Lógica pura e testável;
//! a UI (egui) só traduz cliques em `click_slot`.
use serde::{Deserialize, Serialize};

use crate::world::BlockType;

pub const STACK_MAX: u32 = 64;
/// Hotbar padrão (configurável em `Inventory::with_hotbar`).
pub const DEFAULT_HOTBAR_SLOTS: usize = 5;
pub const MAIN_COLS: usize = 9;
pub const MAIN_BASE_SLOTS: usize = MAIN_COLS * 3;
/// A bolsa equipada libera mais uma fileira.
pub const BAG_EXTRA_SLOTS: usize = MAIN_COLS;
pub const MAIN_MAX_SLOTS: usize = MAIN_BASE_SLOTS + BAG_EXTRA_SLOTS;
/// Máximo de slots de bugigangas que qualquer cinto pode liberar.
pub const MAX_TRINKET_SLOTS: usize = 6;

/// Onde um equipamento pode ser vestido.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GearKind {
    Helmet,
    Chestplate,
    Leggings,
    Boots,
    Gloves,
    Ring,
    Necklace,
    Bag,
    Belt,
    /// Item de melhoria que vai nos slots do cinto (lanterna, amuleto…).
    Trinket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gear {
    IronHelmet,
    IronChestplate,
    IronLeggings,
    IronBoots,
    LeatherGloves,
    GoldRing,
    SilverRing,
    Pendant,
    TravelerBag,
    /// Libera 2 slots de bugigangas.
    LeatherBelt,
    /// Libera 4 slots de bugigangas.
    AdventurerBelt,
    Lantern,
    Amulet,
    Compass,
    LuckyCoin,
}

impl Gear {
    pub const ALL: [Gear; 15] = [
        Gear::IronHelmet, Gear::IronChestplate, Gear::IronLeggings, Gear::IronBoots,
        Gear::LeatherGloves, Gear::GoldRing, Gear::SilverRing, Gear::Pendant, Gear::TravelerBag,
        Gear::LeatherBelt, Gear::AdventurerBelt, Gear::Lantern, Gear::Amulet, Gear::Compass,
        Gear::LuckyCoin,
    ];

    pub fn kind(self) -> GearKind {
        use Gear::*;
        match self {
            IronHelmet => GearKind::Helmet,
            IronChestplate => GearKind::Chestplate,
            IronLeggings => GearKind::Leggings,
            IronBoots => GearKind::Boots,
            LeatherGloves => GearKind::Gloves,
            GoldRing | SilverRing => GearKind::Ring,
            Pendant => GearKind::Necklace,
            TravelerBag => GearKind::Bag,
            LeatherBelt | AdventurerBelt => GearKind::Belt,
            Lantern | Amulet | Compass | LuckyCoin => GearKind::Trinket,
        }
    }

    /// Slots de bugigangas liberados por este cinto (0 para o que não é cinto).
    /// Tabela de configuração: ajuste aqui para balancear.
    pub fn belt_slots(self) -> usize {
        match self {
            Gear::LeatherBelt => 2,
            Gear::AdventurerBelt => 4,
            _ => 0,
        }
    }

    pub fn label(self) -> &'static str {
        use Gear::*;
        match self {
            IronHelmet => "Elmo de ferro",
            IronChestplate => "Peitoral de ferro",
            IronLeggings => "Grevas de ferro",
            IronBoots => "Botas de ferro",
            LeatherGloves => "Luvas de couro",
            GoldRing => "Anel de ouro",
            SilverRing => "Anel de prata",
            Pendant => "Pingente",
            TravelerBag => "Bolsa de viajante",
            LeatherBelt => "Cinto de couro",
            AdventurerBelt => "Cinto de aventureiro",
            Lantern => "Lanterna",
            Amulet => "Amuleto",
            Compass => "Bússola",
            LuckyCoin => "Moeda da sorte",
        }
    }

    /// Cor do ícone (arte provisória até haver texturas de equipamento).
    pub fn color(self) -> [u8; 3] {
        use Gear::*;
        match self {
            IronHelmet | IronChestplate | IronLeggings | IronBoots => [170, 175, 185],
            LeatherGloves | LeatherBelt => [130, 85, 45],
            GoldRing | Pendant | LuckyCoin => [235, 200, 60],
            SilverRing | Compass => [200, 205, 215],
            TravelerBag => [150, 110, 70],
            AdventurerBelt => [95, 60, 35],
            Lantern => [250, 190, 90],
            Amulet => [120, 200, 160],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Item {
    Block(BlockType),
    Gear(Gear),
}

impl Item {
    pub fn max_stack(self) -> u32 {
        match self {
            Item::Block(_) => STACK_MAX,
            Item::Gear(_) => 1,
        }
    }

    pub fn label(self) -> String {
        match self {
            Item::Block(b) => format!("{b:?}"),
            Item::Gear(g) => g.label().to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    pub item: Item,
    pub count: u32,
}

impl ItemStack {
    pub fn new(item: Item, count: u32) -> Self {
        Self { item, count }
    }
}

pub type Slot = Option<ItemStack>;

/// Slots de equipamento (anel tem dois).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EquipSlot {
    Helmet,
    Chestplate,
    Leggings,
    Boots,
    Gloves,
    RingLeft,
    RingRight,
    Necklace,
    Bag,
    Belt,
}

impl EquipSlot {
    pub const ALL: [EquipSlot; 10] = [
        Self::Helmet, Self::Chestplate, Self::Leggings, Self::Boots, Self::Gloves,
        Self::RingLeft, Self::RingRight, Self::Necklace, Self::Bag, Self::Belt,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap()
    }

    pub fn accepts(self, kind: GearKind) -> bool {
        matches!(
            (self, kind),
            (Self::Helmet, GearKind::Helmet)
                | (Self::Chestplate, GearKind::Chestplate)
                | (Self::Leggings, GearKind::Leggings)
                | (Self::Boots, GearKind::Boots)
                | (Self::Gloves, GearKind::Gloves)
                | (Self::RingLeft | Self::RingRight, GearKind::Ring)
                | (Self::Necklace, GearKind::Necklace)
                | (Self::Bag, GearKind::Bag)
                | (Self::Belt, GearKind::Belt)
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Helmet => "Elmo",
            Self::Chestplate => "Peitoral",
            Self::Leggings => "Grevas",
            Self::Boots => "Botas",
            Self::Gloves => "Luvas",
            Self::RingLeft | Self::RingRight => "Anel",
            Self::Necklace => "Colar",
            Self::Bag => "Bolsa",
            Self::Belt => "Cinto",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SlotId {
    Hotbar(usize),
    Main(usize),
    Equip(EquipSlot),
    Trinket(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// Pega/solta tudo (troca se forem itens diferentes).
    Left,
    /// Pega metade / solta um.
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    pub hotbar: Vec<Slot>,
    /// Sempre `MAIN_MAX_SLOTS`; só os primeiros `main_slots_active()` são usáveis.
    pub main: Vec<Slot>,
    pub equip: [Slot; 10],
    /// Sempre `MAX_TRINKET_SLOTS`; só os primeiros `trinket_slots_active()` são usáveis.
    pub trinkets: [Slot; MAX_TRINKET_SLOTS],
    pub selected: usize,
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new()
    }
}

impl Inventory {
    pub fn new() -> Self {
        Self::with_hotbar(DEFAULT_HOTBAR_SLOTS)
    }

    pub fn with_hotbar(slots: usize) -> Self {
        Self {
            hotbar: vec![None; slots.clamp(1, 9)],
            main: vec![None; MAIN_MAX_SLOTS],
            equip: [None; 10],
            trinkets: [None; MAX_TRINKET_SLOTS],
            selected: 0,
        }
    }

    // ------------------------------------------------------------ equipamentos e slots ativos

    pub fn equipped(&self, slot: EquipSlot) -> Option<Gear> {
        match self.equip[slot.index()]?.item {
            Item::Gear(g) => Some(g),
            _ => None,
        }
    }

    pub fn main_slots_active(&self) -> usize {
        if self.equipped(EquipSlot::Bag).is_some() { MAIN_MAX_SLOTS } else { MAIN_BASE_SLOTS }
    }

    pub fn trinket_slots_active(&self) -> usize {
        self.equipped(EquipSlot::Belt).map_or(0, |b| b.belt_slots()).min(MAX_TRINKET_SLOTS)
    }

    pub fn slot_available(&self, id: SlotId) -> bool {
        match id {
            SlotId::Hotbar(i) => i < self.hotbar.len(),
            SlotId::Main(i) => i < self.main_slots_active(),
            SlotId::Equip(_) => true,
            SlotId::Trinket(i) => i < self.trinket_slots_active(),
        }
    }

    pub fn slot(&self, id: SlotId) -> &Slot {
        match id {
            SlotId::Hotbar(i) => &self.hotbar[i],
            SlotId::Main(i) => &self.main[i],
            SlotId::Equip(e) => &self.equip[e.index()],
            SlotId::Trinket(i) => &self.trinkets[i],
        }
    }

    fn slot_mut(&mut self, id: SlotId) -> &mut Slot {
        match id {
            SlotId::Hotbar(i) => &mut self.hotbar[i],
            SlotId::Main(i) => &mut self.main[i],
            SlotId::Equip(e) => &mut self.equip[e.index()],
            SlotId::Trinket(i) => &mut self.trinkets[i],
        }
    }

    /// O item pode ser colocado neste slot?
    pub fn can_place(&self, id: SlotId, item: Item) -> bool {
        if !self.slot_available(id) {
            return false;
        }
        match id {
            SlotId::Hotbar(_) | SlotId::Main(_) => true,
            SlotId::Equip(e) => matches!(item, Item::Gear(g) if e.accepts(g.kind())),
            SlotId::Trinket(_) => matches!(item, Item::Gear(g) if g.kind() == GearKind::Trinket),
        }
    }

    /// Nenhum item fica "preso" em slot que deixou de existir (cinto/bolsa removidos).
    pub fn layout_ok(&self) -> bool {
        self.trinkets[self.trinket_slots_active()..].iter().all(|s| s.is_none())
            && self.main[self.main_slots_active()..].iter().all(|s| s.is_none())
    }

    // ------------------------------------------------------------ hotbar

    pub fn select(&mut self, i: usize) {
        if i < self.hotbar.len() {
            self.selected = i;
        }
    }

    /// Roda a seleção (roda do mouse): +1 = próximo slot.
    pub fn scroll(&mut self, delta: i32) {
        let n = self.hotbar.len() as i32;
        self.selected = (self.selected as i32 + delta).rem_euclid(n) as usize;
    }

    pub fn selected_item(&self) -> Option<ItemStack> {
        self.hotbar[self.selected]
    }

    // ------------------------------------------------------------ coleta

    fn main_has(&self, item: Item) -> bool {
        self.main[..self.main_slots_active()].iter().flatten().any(|s| s.item == item)
    }

    /// Preenche pilhas parciais do mesmo item em `slots`; devolve o que sobrou.
    fn merge_into(slots: &mut [Slot], item: Item, mut count: u32) -> u32 {
        for s in slots.iter_mut() {
            if count == 0 {
                break;
            }
            if let Some(st) = s {
                if st.item == item && st.count < item.max_stack() {
                    let add = (item.max_stack() - st.count).min(count);
                    st.count += add;
                    count -= add;
                }
            }
        }
        count
    }

    /// Coloca o que sobrar em slots vazios de `slots`.
    fn fill_empty(slots: &mut [Slot], item: Item, mut count: u32) -> u32 {
        for s in slots.iter_mut() {
            if count == 0 {
                break;
            }
            if s.is_none() {
                let put = item.max_stack().min(count);
                *s = Some(ItemStack::new(item, put));
                count -= put;
            }
        }
        count
    }

    /// Guarda itens pegos do chão. Devolve quantos NÃO couberam.
    ///
    /// Regra: se a grade principal já tem o mesmo item, ele vai para a grade; senão vai primeiro
    /// para a hotbar (e depois para a grade). Em ambos os casos, completa pilhas existentes antes
    /// de usar slots vazios, e transborda para o outro lado quando enche.
    pub fn add_item(&mut self, item: Item, count: u32) -> u32 {
        let active = self.main_slots_active();
        let mut left = count;
        let main_first = self.main_has(item);
        let order: [bool; 2] = if main_first { [true, false] } else { [false, true] };
        for into_main in order {
            let slots: &mut [Slot] = if into_main { &mut self.main[..active] } else { &mut self.hotbar[..] };
            left = Self::merge_into(slots, item, left);
            left = Self::fill_empty(slots, item, left);
        }
        left
    }

    /// Cabe `count` deste item (sem alterar nada)?
    #[allow(dead_code)]
    pub fn can_hold(&self, item: Item, count: u32) -> bool {
        let mut probe = self.clone();
        probe.add_item(item, count) == 0
    }

    /// Quantos itens deste tipo existem na hotbar + grade.
    #[allow(dead_code)]
    pub fn count_of(&self, item: Item) -> u32 {
        self.hotbar.iter().chain(self.main[..self.main_slots_active()].iter())
            .flatten().filter(|s| s.item == item).map(|s| s.count).sum()
    }

    // ------------------------------------------------------------ cliques da UI

    /// Clique em um slot com o item "na mão" (`held`). Retorna `false` se a ação foi recusada
    /// (slot inexistente, item que não serve, ou deixaria bugigangas/itens sem slot).
    pub fn click_slot(&mut self, id: SlotId, held: &mut Slot, click: Click) -> bool {
        if !self.slot_available(id) {
            return false;
        }
        let backup = (self.clone(), *held);
        let ok = self.click_inner(id, held, click) && self.layout_ok();
        if !ok {
            *self = backup.0;
            *held = backup.1;
        }
        ok
    }

    fn click_inner(&mut self, id: SlotId, held: &mut Slot, click: Click) -> bool {
        let cur = *self.slot(id);
        match (click, *held, cur) {
            (_, None, None) => false,
            // Pegar
            (Click::Left, None, Some(_)) => {
                *held = cur;
                *self.slot_mut(id) = None;
                true
            }
            (Click::Right, None, Some(st)) => {
                let take = st.count.div_ceil(2);
                *held = Some(ItemStack::new(st.item, take));
                *self.slot_mut(id) = (st.count > take).then(|| ItemStack::new(st.item, st.count - take));
                true
            }
            // Soltar
            (Click::Left, Some(h), None) => {
                if !self.can_place(id, h.item) {
                    return false;
                }
                *self.slot_mut(id) = Some(h);
                *held = None;
                true
            }
            (Click::Right, Some(h), None) => {
                if !self.can_place(id, h.item) {
                    return false;
                }
                *self.slot_mut(id) = Some(ItemStack::new(h.item, 1));
                *held = (h.count > 1).then(|| ItemStack::new(h.item, h.count - 1));
                true
            }
            (Click::Left, Some(h), Some(s)) => {
                if !self.can_place(id, h.item) {
                    return false;
                }
                if s.item == h.item {
                    // Junta (o que passar do limite continua na mão)
                    let room = h.item.max_stack().saturating_sub(s.count);
                    if room == 0 {
                        return false;
                    }
                    let add = room.min(h.count);
                    *self.slot_mut(id) = Some(ItemStack::new(s.item, s.count + add));
                    *held = (h.count > add).then(|| ItemStack::new(h.item, h.count - add));
                } else {
                    // Troca
                    *self.slot_mut(id) = Some(h);
                    *held = Some(s);
                }
                true
            }
            (Click::Right, Some(h), Some(s)) => {
                if s.item == h.item && self.can_place(id, h.item) && s.count < h.item.max_stack() {
                    *self.slot_mut(id) = Some(ItemStack::new(s.item, s.count + 1));
                    *held = (h.count > 1).then(|| ItemStack::new(h.item, h.count - 1));
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Devolve o que está "na mão" ao inventário (ao fechar a tela). O que não couber é devolvido.
    pub fn stash_held(&mut self, held: &mut Slot) -> Option<ItemStack> {
        let h = held.take()?;
        let left = self.add_item(h.item, h.count);
        (left > 0).then(|| ItemStack::new(h.item, left))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use BlockType::*;

    fn blk(b: BlockType) -> Item {
        Item::Block(b)
    }

    #[test]
    fn defaults_hotbar_5_main_27() {
        let inv = Inventory::new();
        assert_eq!(inv.hotbar.len(), 5, "hotbar padrão de 5 slots");
        assert_eq!(inv.main_slots_active(), 27);
        assert_eq!(inv.trinket_slots_active(), 0);
        assert!(inv.layout_ok());
        assert_eq!(Inventory::with_hotbar(7).hotbar.len(), 7, "configurável");
        assert_eq!(Inventory::with_hotbar(0).hotbar.len(), 1);
        assert_eq!(Inventory::with_hotbar(99).hotbar.len(), 9);
    }

    #[test]
    fn new_items_go_to_the_hotbar_first() {
        let mut inv = Inventory::new();
        assert_eq!(inv.add_item(blk(Stone), 3), 0);
        assert_eq!(inv.hotbar[0], Some(ItemStack::new(blk(Stone), 3)));
        assert!(inv.main.iter().all(|s| s.is_none()));
        inv.add_item(blk(Dirt), 1);
        assert_eq!(inv.hotbar[1].unwrap().item, blk(Dirt));
    }

    #[test]
    fn item_already_in_inventory_goes_to_the_inventory() {
        let mut inv = Inventory::new();
        inv.main[4] = Some(ItemStack::new(blk(Stone), 10));
        inv.add_item(blk(Stone), 5);
        assert_eq!(inv.main[4].unwrap().count, 15, "completa a pilha da grade");
        assert!(inv.hotbar.iter().all(|s| s.is_none()), "não toca a hotbar");
        // item diferente continua indo para a hotbar
        inv.add_item(blk(Sand), 1);
        assert_eq!(inv.hotbar[0].unwrap().item, blk(Sand));
    }

    #[test]
    fn stacks_cap_at_64_and_overflow_continues() {
        let mut inv = Inventory::new();
        assert_eq!(inv.add_item(blk(Stone), 64), 0);
        assert_eq!(inv.add_item(blk(Stone), 10), 0);
        // O mesmo item já está na hotbar (não na grade): completa pilhas e abre o próximo slot da hotbar
        assert_eq!(inv.hotbar[0].unwrap().count, 64);
        assert_eq!(inv.hotbar[1].unwrap().count, 10);
        assert_eq!(inv.count_of(blk(Stone)), 74);
    }

    #[test]
    fn hotbar_full_overflows_into_the_grid() {
        let mut inv = Inventory::new();
        for (i, b) in [Stone, Dirt, Sand, Gravel, Snow].into_iter().enumerate() {
            inv.add_item(blk(b), 1);
            assert!(inv.hotbar[i].is_some());
        }
        assert_eq!(inv.add_item(blk(Coal), 1), 0);
        assert!(inv.hotbar.iter().flatten().all(|s| s.item != blk(Coal)));
        assert_eq!(inv.main[0].unwrap().item, blk(Coal));
    }

    #[test]
    fn full_inventory_returns_leftover_and_loses_nothing() {
        let mut inv = Inventory::new();
        let total_slots = inv.hotbar.len() + MAIN_BASE_SLOTS;
        for i in 0..total_slots {
            let b = BlockType::from_u8(1 + (i % 30) as u8);
            // um tipo diferente por slot não cabe (só 30 tipos): usa pilhas cheias do mesmo tipo
            let _ = b;
            assert_eq!(inv.add_item(blk(Stone), 64), 0);
        }
        assert_eq!(inv.count_of(blk(Stone)), total_slots as u32 * 64);
        assert_eq!(inv.add_item(blk(Stone), 7), 7, "tudo cheio: nada é perdido, volta para o chamador");
        assert!(!inv.can_hold(blk(Dirt), 1));
        assert!(inv.can_hold(blk(Stone), 0));
    }

    #[test]
    fn gear_does_not_stack() {
        let mut inv = Inventory::new();
        inv.add_item(Item::Gear(Gear::Lantern), 1);
        inv.add_item(Item::Gear(Gear::Lantern), 1);
        assert_eq!(inv.hotbar[0].unwrap().count, 1);
        assert_eq!(inv.hotbar[1].unwrap().count, 1);
    }

    #[test]
    fn select_and_scroll_wrap() {
        let mut inv = Inventory::new();
        inv.select(3);
        assert_eq!(inv.selected, 3);
        inv.select(9);
        assert_eq!(inv.selected, 3, "ignora índice inválido");
        inv.scroll(1);
        inv.scroll(1);
        assert_eq!(inv.selected, 0, "dá a volta");
        inv.scroll(-1);
        assert_eq!(inv.selected, 4);
        inv.hotbar[4] = Some(ItemStack::new(blk(Gold), 2));
        assert_eq!(inv.selected_item().unwrap().item, blk(Gold));
    }

    #[test]
    fn move_items_between_hotbar_and_inventory() {
        let mut inv = Inventory::new();
        inv.add_item(blk(Stone), 20);
        let mut held = None;
        // pega da hotbar…
        assert!(inv.click_slot(SlotId::Hotbar(0), &mut held, Click::Left));
        assert_eq!(held, Some(ItemStack::new(blk(Stone), 20)));
        assert!(inv.hotbar[0].is_none());
        // …e solta na grade
        assert!(inv.click_slot(SlotId::Main(10), &mut held, Click::Left));
        assert!(held.is_none());
        assert_eq!(inv.main[10], Some(ItemStack::new(blk(Stone), 20)));
        // e de volta
        assert!(inv.click_slot(SlotId::Main(10), &mut held, Click::Left));
        assert!(inv.click_slot(SlotId::Hotbar(2), &mut held, Click::Left));
        assert_eq!(inv.hotbar[2], Some(ItemStack::new(blk(Stone), 20)));
        assert_eq!(inv.count_of(blk(Stone)), 20, "nada some nem duplica");
    }

    #[test]
    fn click_swaps_merges_and_splits() {
        let mut inv = Inventory::new();
        inv.hotbar[0] = Some(ItemStack::new(blk(Stone), 10));
        inv.hotbar[1] = Some(ItemStack::new(blk(Dirt), 5));
        inv.main[0] = Some(ItemStack::new(blk(Stone), 60));
        let mut held = None;
        // troca Stone <-> Dirt
        inv.click_slot(SlotId::Hotbar(0), &mut held, Click::Left);
        inv.click_slot(SlotId::Hotbar(1), &mut held, Click::Left);
        assert_eq!(inv.hotbar[1], Some(ItemStack::new(blk(Stone), 10)));
        assert_eq!(held, Some(ItemStack::new(blk(Dirt), 5)));
        inv.click_slot(SlotId::Hotbar(0), &mut held, Click::Left);
        assert_eq!(inv.hotbar[0].unwrap().item, blk(Dirt));
        // junta com limite: 10 + 60 -> 64 na grade, 6 ficam na mão
        inv.click_slot(SlotId::Hotbar(1), &mut held, Click::Left);
        inv.click_slot(SlotId::Main(0), &mut held, Click::Left);
        assert_eq!(inv.main[0].unwrap().count, 64);
        assert_eq!(held, Some(ItemStack::new(blk(Stone), 6)));
        // clique direito pega metade
        let mut h2 = None;
        inv.click_slot(SlotId::Main(0), &mut h2, Click::Right);
        assert_eq!(h2.unwrap().count, 32);
        assert_eq!(inv.main[0].unwrap().count, 32);
        // clique direito solta um
        inv.click_slot(SlotId::Main(5), &mut h2, Click::Right);
        assert_eq!(inv.main[5].unwrap().count, 1);
        assert_eq!(h2.unwrap().count, 31);
    }

    #[test]
    fn clicking_nothing_does_nothing() {
        let mut inv = Inventory::new();
        let mut held = None;
        assert!(!inv.click_slot(SlotId::Main(0), &mut held, Click::Left));
        assert!(!inv.click_slot(SlotId::Main(MAIN_BASE_SLOTS), &mut held, Click::Left), "slot da bolsa inexistente");
        assert_eq!(inv, Inventory::new());
    }

    #[test]
    fn equipment_slots_only_accept_matching_gear() {
        let mut inv = Inventory::new();
        inv.main[0] = Some(ItemStack::new(Item::Gear(Gear::IronHelmet), 1));
        inv.main[1] = Some(ItemStack::new(blk(Stone), 1));
        inv.main[2] = Some(ItemStack::new(Item::Gear(Gear::GoldRing), 1));
        let mut held = None;
        // bloco não vai no elmo
        inv.click_slot(SlotId::Main(1), &mut held, Click::Left);
        assert!(!inv.click_slot(SlotId::Equip(EquipSlot::Helmet), &mut held, Click::Left));
        assert_eq!(held, Some(ItemStack::new(blk(Stone), 1)), "continua na mão");
        inv.click_slot(SlotId::Main(1), &mut held, Click::Left);
        // elmo no peitoral não
        inv.click_slot(SlotId::Main(0), &mut held, Click::Left);
        assert!(!inv.click_slot(SlotId::Equip(EquipSlot::Chestplate), &mut held, Click::Left));
        assert!(inv.click_slot(SlotId::Equip(EquipSlot::Helmet), &mut held, Click::Left));
        assert_eq!(inv.equipped(EquipSlot::Helmet), Some(Gear::IronHelmet));
        // anel vai em qualquer dos dois slots de anel
        inv.click_slot(SlotId::Main(2), &mut held, Click::Left);
        assert!(inv.click_slot(SlotId::Equip(EquipSlot::RingRight), &mut held, Click::Left));
        assert_eq!(inv.equipped(EquipSlot::RingRight), Some(Gear::GoldRing));
        assert_eq!(inv.equipped(EquipSlot::RingLeft), None);
    }

    fn equip_belt(inv: &mut Inventory, g: Gear) {
        inv.equip[EquipSlot::Belt.index()] = Some(ItemStack::new(Item::Gear(g), 1));
    }

    #[test]
    fn belt_unlocks_configurable_trinket_slots() {
        let mut inv = Inventory::new();
        let lantern = Item::Gear(Gear::Lantern);
        assert!(!inv.slot_available(SlotId::Trinket(0)));
        assert!(!inv.can_place(SlotId::Trinket(0), lantern), "sem cinto não há slots");
        equip_belt(&mut inv, Gear::LeatherBelt);
        assert_eq!(inv.trinket_slots_active(), 2);
        assert!(inv.can_place(SlotId::Trinket(1), lantern));
        assert!(!inv.can_place(SlotId::Trinket(2), lantern));
        assert!(!inv.can_place(SlotId::Trinket(0), blk(Stone)), "só bugigangas");
        equip_belt(&mut inv, Gear::AdventurerBelt);
        assert_eq!(inv.trinket_slots_active(), 4);
        assert!(inv.can_place(SlotId::Trinket(3), lantern));
        assert!(Gear::ALL.iter().all(|g| g.belt_slots() <= MAX_TRINKET_SLOTS));
    }

    #[test]
    fn removing_a_belt_with_trinkets_is_refused() {
        let mut inv = Inventory::new();
        equip_belt(&mut inv, Gear::AdventurerBelt);
        inv.trinkets[3] = Some(ItemStack::new(Item::Gear(Gear::Amulet), 1));
        let before = inv.clone();
        let mut held = None;
        // tirar o cinto deixaria o amuleto sem slot
        assert!(!inv.click_slot(SlotId::Equip(EquipSlot::Belt), &mut held, Click::Left));
        assert_eq!(inv, before);
        assert!(held.is_none());
        // trocar por um cinto menor também (slot 3 sumiria)
        inv.main[0] = Some(ItemStack::new(Item::Gear(Gear::LeatherBelt), 1));
        inv.click_slot(SlotId::Main(0), &mut held, Click::Left);
        assert!(!inv.click_slot(SlotId::Equip(EquipSlot::Belt), &mut held, Click::Left));
        assert_eq!(held, Some(ItemStack::new(Item::Gear(Gear::LeatherBelt), 1)), "estado restaurado");
        assert_eq!(inv.equipped(EquipSlot::Belt), Some(Gear::AdventurerBelt));
        // esvaziando o slot 3, a troca é permitida
        inv.trinkets[3] = None;
        assert!(inv.click_slot(SlotId::Equip(EquipSlot::Belt), &mut held, Click::Left));
        assert_eq!(inv.equipped(EquipSlot::Belt), Some(Gear::LeatherBelt));
        assert_eq!(held.unwrap().item, Item::Gear(Gear::AdventurerBelt));
    }

    #[test]
    fn bag_adds_a_row_and_cannot_be_removed_while_used() {
        let mut inv = Inventory::new();
        inv.equip[EquipSlot::Bag.index()] = Some(ItemStack::new(Item::Gear(Gear::TravelerBag), 1));
        assert_eq!(inv.main_slots_active(), MAIN_BASE_SLOTS + BAG_EXTRA_SLOTS);
        assert!(inv.slot_available(SlotId::Main(MAIN_MAX_SLOTS - 1)));
        inv.main[MAIN_MAX_SLOTS - 1] = Some(ItemStack::new(blk(Gold), 3));
        let mut held = None;
        assert!(!inv.click_slot(SlotId::Equip(EquipSlot::Bag), &mut held, Click::Left));
        assert_eq!(inv.main_slots_active(), MAIN_MAX_SLOTS);
        inv.main[MAIN_MAX_SLOTS - 1] = None;
        assert!(inv.click_slot(SlotId::Equip(EquipSlot::Bag), &mut held, Click::Left));
        assert_eq!(inv.main_slots_active(), MAIN_BASE_SLOTS);
    }

    #[test]
    fn pickup_uses_bag_slots_when_equipped() {
        let mut inv = Inventory::new();
        for i in 0..inv.hotbar.len() { inv.hotbar[i] = Some(ItemStack::new(blk(Stone), 64)); }
        for i in 0..MAIN_BASE_SLOTS { inv.main[i] = Some(ItemStack::new(blk(Stone), 64)); }
        assert_eq!(inv.add_item(blk(Dirt), 1), 1, "cheio");
        inv.equip[EquipSlot::Bag.index()] = Some(ItemStack::new(Item::Gear(Gear::TravelerBag), 1));
        assert_eq!(inv.add_item(blk(Dirt), 1), 0, "agora cabe na fileira da bolsa");
        assert!(inv.main[MAIN_BASE_SLOTS].is_some());
    }

    #[test]
    fn stash_held_returns_items_on_close() {
        let mut inv = Inventory::new();
        let mut held = Some(ItemStack::new(blk(Sand), 12));
        assert!(inv.stash_held(&mut held).is_none());
        assert!(held.is_none());
        assert_eq!(inv.count_of(blk(Sand)), 12);
        // cheio: o que sobra volta para o chamador (que pode soltar no chão)
        let mut full = Inventory::new();
        for i in 0..full.hotbar.len() { full.hotbar[i] = Some(ItemStack::new(blk(Stone), 64)); }
        for i in 0..MAIN_BASE_SLOTS { full.main[i] = Some(ItemStack::new(blk(Stone), 64)); }
        let mut held = Some(ItemStack::new(blk(Sand), 12));
        assert_eq!(full.stash_held(&mut held), Some(ItemStack::new(blk(Sand), 12)));
    }

    #[test]
    fn serde_roundtrip() {
        let mut inv = Inventory::new();
        inv.add_item(blk(Gold), 9);
        equip_belt(&mut inv, Gear::LeatherBelt);
        inv.trinkets[0] = Some(ItemStack::new(Item::Gear(Gear::Lantern), 1));
        inv.select(2);
        let bytes = bincode::serialize(&inv).unwrap();
        let back: Inventory = bincode::deserialize(&bytes).unwrap();
        assert_eq!(inv, back);
    }
}
