//! Objets, inventaire et emplacements rapides (façon Lies of P) : les objets équipés dans les
//! emplacements se choisissent en jeu (croix bas / C) et s'utilisent avec un seul bouton.

use serde::{Deserialize, Serialize};

use super::data::Tuning;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Item {
    HealFlask,
}

impl Item {
    pub const ALL: [Item; 1] = [Item::HealFlask];

    pub fn name(self) -> &'static str {
        match self {
            Item::HealFlask => crate::lang::tr("Healing Flask", "Fiole de soin"),
        }
    }

    /// Quantité rendue au repos et à la mort (`None` : l'objet ne se recharge pas).
    pub fn refill(self, t: &Tuning) -> Option<u8> {
        match self {
            Item::HealFlask => Some(t.player.heal_charges),
        }
    }
}

pub const QUICK_SLOTS: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Inventory {
    /// Objets possédés et leur quantité (une entrée par objet, dans l'ordre d'obtention).
    pub items: Vec<(Item, u8)>,
    pub slots: [Option<Item>; QUICK_SLOTS],
    /// Emplacement rapide sélectionné.
    pub active: u8,
}

impl Default for Inventory {
    fn default() -> Self {
        Self { items: Vec::new(), slots: [None; QUICK_SLOTS], active: 0 }
    }
}

impl Inventory {
    pub fn new_game(t: &Tuning) -> Self {
        let mut inv = Self::default();
        inv.items.push((Item::HealFlask, t.player.heal_charges));
        inv.slots[0] = Some(Item::HealFlask);
        inv
    }

    pub fn count(&self, item: Item) -> u8 {
        self.items.iter().find(|(i, _)| *i == item).map_or(0, |(_, n)| *n)
    }

    pub fn owns(&self, item: Item) -> bool {
        self.items.iter().any(|(i, _)| *i == item)
    }

    pub fn active_item(&self) -> Option<Item> {
        self.slots.get(self.active as usize).copied().flatten()
    }

    /// Retire un exemplaire ; faux s'il n'y en a plus.
    pub fn consume(&mut self, item: Item) -> bool {
        match self.items.iter_mut().find(|(i, _)| *i == item) {
            Some((_, n)) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        }
    }

    /// Passe à l'emplacement équipé suivant (ne fait rien s'il n'y en a pas d'autre).
    pub fn cycle(&mut self) {
        for k in 1..=QUICK_SLOTS {
            let i = (self.active as usize + k) % QUICK_SLOTS;
            if self.slots[i].is_some() {
                self.active = i as u8;
                return;
            }
        }
    }

    /// Équipe un objet dans un emplacement (il quitte son ancien emplacement s'il en avait un).
    pub fn equip(&mut self, slot: usize, item: Option<Item>) {
        if slot >= QUICK_SLOTS || item.is_some_and(|i| !self.owns(i)) {
            return;
        }
        if let Some(it) = item {
            for s in &mut self.slots {
                if *s == Some(it) {
                    *s = None;
                }
            }
        }
        self.slots[slot] = item;
        if self.active_item().is_none() {
            self.cycle();
        }
    }

    /// Recharge les objets rechargeables (repos au checkpoint, mort).
    pub fn refill(&mut self, t: &Tuning) {
        for (i, n) in &mut self.items {
            if let Some(max) = i.refill(t) {
                *n = max;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equip_cycle_and_consume() {
        let t = Tuning::builtin();
        let mut inv = Inventory::new_game(&t);
        assert_eq!(inv.active_item(), Some(Item::HealFlask));
        // Un seul objet équipé : changer d'emplacement ne fait rien.
        inv.cycle();
        assert_eq!(inv.active, 0);
        // Déplacer l'objet vers l'emplacement 3 libère le 1 et suit la sélection.
        inv.equip(2, Some(Item::HealFlask));
        assert_eq!(inv.slots, [None, None, Some(Item::HealFlask), None]);
        assert_eq!(inv.active, 2);
        while inv.consume(Item::HealFlask) {}
        assert_eq!(inv.count(Item::HealFlask), 0);
        inv.refill(&t);
        assert_eq!(inv.count(Item::HealFlask), t.player.heal_charges);
    }
}
