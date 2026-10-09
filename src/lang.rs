//! Interface language (English by default, French).
//!
//! The current language is global: texts are translated where they're displayed with
//! `tr("English", "Français")`, without having to pass the settings around everywhere. It follows
//! `Settings::language` (see `settings::apply_settings`).

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

    /// Name of the language in that language (the same whatever the current language).
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

/// Picks the text for the current language.
pub fn tr(en: &'static str, fr: &'static str) -> &'static str {
    match current() {
        Lang::En => en,
        Lang::Fr => fr,
    }
}

/// Text from data files: a single string, or one per language
/// (`name: (en: "Rapier", fr: "Rapière")`).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
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

/// Fixed text of a UI node, re-translated when the language changes.
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
        // After `apply_settings` (PostUpdate), which sets the language.
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
