//! Langue de l'interface (anglais par défaut, français).
//!
//! La langue courante est globale : les textes se traduisent là où ils sont affichés avec
//! `tr("English", "Français")`, sans avoir à faire circuler les options partout. Elle suit
//! `Settings::language` (voir `settings::apply_settings`).

use std::sync::atomic::{AtomicU8, Ordering};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Fr];

    /// Nom de la langue dans cette langue (identique quelle que soit la langue courante).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Fr => "Français",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::Fr,
        _ => Lang::En,
    }
}

pub fn set(l: Lang) {
    CURRENT.store(l as u8, Ordering::Relaxed);
}

/// Choisit le texte de la langue courante.
pub fn tr(en: &'static str, fr: &'static str) -> &'static str {
    match current() {
        Lang::En => en,
        Lang::Fr => fr,
    }
}

/// Texte des fichiers de données : une chaîne unique, ou une par langue
/// (`name: (en: "Rapier", fr: "Rapière")`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum LText {
    Tr { en: String, fr: String },
    Same(String),
}

impl LText {
    pub fn get(&self) -> &str {
        match self {
            LText::Same(s) => s,
            LText::Tr { en, fr } => match current() {
                Lang::En => en,
                Lang::Fr => fr,
            },
        }
    }
}

/// Texte fixe d'un nœud d'interface, retraduit quand la langue change.
#[derive(Component, Clone)]
pub struct Localized(pub LText);

impl Localized {
    pub fn tr(en: &str, fr: &str) -> Self {
        Self(LText::Tr { en: en.into(), fr: fr.into() })
    }
}

pub struct LangPlugin;

impl Plugin for LangPlugin {
    fn build(&self, app: &mut App) {
        // Après `apply_settings` (PostUpdate), qui fixe la langue.
        app.add_systems(Last, update_localized);
    }
}

fn update_localized(mut q: Query<(Ref<Localized>, &mut Text)>, mut last: Local<Option<Lang>>) {
    let lang = current();
    let changed = *last != Some(lang);
    *last = Some(lang);
    for (l, mut t) in &mut q {
        if changed || l.is_added() {
            let s = l.0.get();
            if t.0 != s {
                t.0 = s.into();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ltext_parses_both_forms() {
        let a: LText = ron::from_str(r#""Plain""#).unwrap();
        assert_eq!(a, LText::Same("Plain".into()));
        let b: LText = ron::from_str(r#"(en: "Rapier", fr: "Rapière")"#).unwrap();
        assert_eq!(b, LText::Tr { en: "Rapier".into(), fr: "Rapière".into() });
    }
}
