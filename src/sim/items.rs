//! Objets, inventaire et emplacements rapides (façon Lies of P) : les objets équipés dans les
//! emplacements se choisissent en jeu (croix bas / C) et s'utilisent avec un seul bouton.
//!
//! Trois familles, comme dans les souls-like :
//! - consommables (emplacements rapides) : fiole de soin (rechargée au repos), braises à
//!   écraser, mousse, résine ;
//! - talismans (un emplacement dédié) : bonus permanents tant qu'ils sont portés ;
//! - objets clés, appliqués dès qu'on les ramasse (éclat de fiole : une charge de plus).

use serde::{Deserialize, Serialize};

use super::data::Tuning;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Item {
    HealFlask,
    /// Braise ternie : à écraser pour gagner des braises.
    FadedEmber,
    /// Braise vive : comme la braise ternie, en mieux.
    LivelyEmber,
    /// Mousse dorée : régénère des PV pendant quelques secondes.
    GoldenMoss,
    /// Résine ardente : l'arme fait plus de dégâts pendant une minute.
    EmberResin,
    /// Éclat de fiole : une charge de soin de plus, dès qu'on le ramasse.
    FlaskShard,
    /// Talisman : dégâts subis réduits.
    IronBrooch,
    /// Talisman : esquives moins coûteuses en endurance (`CarouselFeather` : son ancien nom,
    /// dans les sauvegardes).
    #[serde(alias = "CarouselFeather")]
    CrestPlume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Consumable,
    Talisman,
    Key,
}

/// Braises gagnées en écrasant une braise ternie / vive.
pub const FADED_EMBERS: u32 = 200;
pub const LIVELY_EMBERS: u32 = 600;
/// Mousse dorée : PV rendus par seconde, et durée (ticks).
pub const MOSS_HP_PER_SEC: f32 = 6.0;
pub const MOSS_TICKS: u32 = 25 * 60;
/// Résine ardente : bonus de dégâts et durée (ticks).
pub const RESIN_DAMAGE: f32 = 1.2;
pub const RESIN_TICKS: u32 = 60 * 60;
/// Broche de fer : part des dégâts subis.
pub const BROOCH_DAMAGE: f32 = 0.85;
/// Plume de cimier : part du coût d'endurance des esquives.
pub const FEATHER_DODGE: f32 = 0.7;

impl Item {
    pub const ALL: [Item; 8] = [
        Item::HealFlask,
        Item::FadedEmber,
        Item::LivelyEmber,
        Item::GoldenMoss,
        Item::EmberResin,
        Item::FlaskShard,
        Item::IronBrooch,
        Item::CrestPlume,
    ];

    pub fn kind(self) -> Kind {
        match self {
            Item::IronBrooch | Item::CrestPlume => Kind::Talisman,
            Item::FlaskShard => Kind::Key,
            _ => Kind::Consumable,
        }
    }

    pub fn name(self) -> &'static str {
        use crate::lang::tr;
        match self {
            Item::HealFlask => tr("Healing Flask", "Fiole de soin"),
            Item::FadedEmber => tr("Faded Ember", "Braise ternie"),
            Item::LivelyEmber => tr("Lively Ember", "Braise vive"),
            Item::GoldenMoss => tr("Golden Moss", "Mousse dorée"),
            Item::EmberResin => tr("Ember Resin", "Résine ardente"),
            Item::FlaskShard => tr("Flask Shard", "Éclat de fiole"),
            Item::IronBrooch => tr("Iron Brooch", "Broche de fer"),
            Item::CrestPlume => tr("Crest Plume", "Plume de cimier"),
        }
    }

    /// Description courte (fenêtre d'objet ramassé, menu d'équipement).
    pub fn description(self) -> &'static str {
        use crate::lang::tr;
        match self {
            Item::HealFlask => tr("Restores HP. Refilled when resting.", "Rend des PV. Remplie au repos."),
            Item::FadedEmber => tr("Crush it to gain embers.", "À écraser pour gagner des braises."),
            Item::LivelyEmber => tr("Crush it to gain many embers.", "À écraser pour gagner beaucoup de braises."),
            Item::GoldenMoss => tr("Slowly restores HP for a while.", "Rend lentement des PV pendant un moment."),
            Item::EmberResin => tr("Weapon deals more damage for a minute.", "L'arme frappe plus fort pendant une minute."),
            Item::FlaskShard => tr("Healing Flask: one more charge.", "Fiole de soin : une charge de plus."),
            Item::IronBrooch => tr("Talisman. Damage taken reduced by 15%.", "Talisman. Dégâts subis réduits de 15 %."),
            Item::CrestPlume => tr("Talisman. Dodging costs 30% less stamina.", "Talisman. Esquiver coûte 30 % d'endurance en moins."),
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
    /// Talisman porté.
    pub talisman: Option<Item>,
    /// Charges de soin gagnées avec les éclats de fiole.
    pub flask_bonus: u8,
}

impl Default for Inventory {
    fn default() -> Self {
        Self { items: Vec::new(), slots: [None; QUICK_SLOTS], active: 0, talisman: None, flask_bonus: 0 }
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

    /// Quantité rendue au repos et à la mort (`None` : l'objet ne se recharge pas).
    pub fn refill_amount(&self, item: Item, t: &Tuning) -> Option<u8> {
        match item {
            Item::HealFlask => Some(t.player.heal_charges + self.flask_bonus),
            _ => None,
        }
    }

    /// Ajoute des objets ramassés. Les objets clés s'appliquent tout de suite ; un nouveau
    /// consommable va dans le premier emplacement rapide libre, un talisman est porté si on
    /// n'en portait pas.
    pub fn add(&mut self, item: Item, n: u8) {
        match item.kind() {
            Kind::Key => {
                if item == Item::FlaskShard {
                    self.flask_bonus = self.flask_bonus.saturating_add(n);
                    // La charge gagnée est utilisable tout de suite.
                    if let Some((_, c)) = self.items.iter_mut().find(|(i, _)| *i == Item::HealFlask) {
                        *c = c.saturating_add(n);
                    }
                }
                return;
            }
            Kind::Talisman | Kind::Consumable => {}
        }
        let new = !self.owns(item);
        match self.items.iter_mut().find(|(i, _)| *i == item) {
            Some((_, c)) => *c = c.saturating_add(n).min(99),
            None => self.items.push((item, n.min(99))),
        }
        if item.kind() == Kind::Talisman && self.talisman.is_none() {
            self.talisman = Some(item);
        }
        if new && item.kind() == Kind::Consumable && !self.slots.contains(&Some(item)) {
            if let Some(s) = self.slots.iter_mut().find(|s| s.is_none()) {
                *s = Some(item);
            }
            if self.active_item().is_none() {
                self.cycle();
            }
        }
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

    /// Équipe un consommable dans un emplacement (il quitte son ancien emplacement s'il en avait un).
    pub fn equip(&mut self, slot: usize, item: Option<Item>) {
        if slot >= QUICK_SLOTS || item.is_some_and(|i| !self.owns(i) || i.kind() != Kind::Consumable) {
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

    pub fn equip_talisman(&mut self, item: Option<Item>) {
        if item.is_none_or(|i| self.owns(i) && i.kind() == Kind::Talisman) {
            self.talisman = item;
        }
    }

    pub fn wears(&self, item: Item) -> bool {
        self.talisman == Some(item)
    }

    /// Recharge les objets rechargeables (repos au checkpoint, mort).
    pub fn refill(&mut self, t: &Tuning) {
        let amounts: Vec<Option<u8>> = self.items.iter().map(|(i, _)| self.refill_amount(*i, t)).collect();
        for ((_, n), max) in self.items.iter_mut().zip(amounts) {
            if let Some(max) = max {
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

    #[test]
    fn pickups_fill_slots_and_shards_add_charges() {
        let t = Tuning::builtin();
        let mut inv = Inventory::new_game(&t);
        inv.add(Item::GoldenMoss, 2);
        assert_eq!(inv.slots[1], Some(Item::GoldenMoss));
        inv.add(Item::GoldenMoss, 1);
        assert_eq!(inv.count(Item::GoldenMoss), 3);
        // Un talisman ne va pas dans les emplacements rapides ; le premier est porté d'office.
        inv.add(Item::IronBrooch, 1);
        assert!(!inv.slots.contains(&Some(Item::IronBrooch)));
        assert!(inv.wears(Item::IronBrooch));
        inv.equip(2, Some(Item::IronBrooch));
        assert_eq!(inv.slots[2], None);
        inv.add(Item::CrestPlume, 1);
        assert!(inv.wears(Item::IronBrooch));
        inv.equip_talisman(Some(Item::CrestPlume));
        assert!(inv.wears(Item::CrestPlume));
        // Éclat de fiole : une charge de plus, gardée au repos ; les consommables ne se rechargent pas.
        inv.add(Item::FlaskShard, 1);
        assert!(!inv.owns(Item::FlaskShard));
        assert_eq!(inv.count(Item::HealFlask), t.player.heal_charges + 1);
        while inv.consume(Item::HealFlask) {}
        inv.consume(Item::GoldenMoss);
        inv.refill(&t);
        assert_eq!(inv.count(Item::HealFlask), t.player.heal_charges + 1);
        assert_eq!(inv.count(Item::GoldenMoss), 2);
    }
}
