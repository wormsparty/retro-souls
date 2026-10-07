//! Sauvegarde automatique façon Dark Souls : un seul emplacement, réécrit dès qu'il se passe
//! quelque chose d'important (repos, entrée dans l'arène, victoire, mort, objet utilisé,
//! ouverture/fermeture d'un menu), toutes les 5 secondes, et en quittant. Le fichier n'est
//! réécrit que s'il a changé. On reprend où on s'était arrêté — sauf en plein combat de boss :
//! on revient alors devant la brume, et le boss repart de zéro.
//!
//! Emplacement : `save.ron` à côté des options (voir `storage`).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::fx::FxState;
use crate::menu::MenuState;
use crate::render::AppState;
use crate::sim::SimEvent;
use crate::sim::data::Tuning;
use crate::sim::encounter::{Encounter, Progress};
use crate::sim::fighter::{Body, Health};
use crate::sim::player::Player;

pub const VERSION: u32 = 1;
const FILE: &str = "save";
const AUTOSAVE_SECS: f32 = 5.0;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SaveData {
    pub version: u32,
    /// Temps de jeu, en secondes.
    pub play_time: f64,
    pub progress: Progress,
}

#[derive(Resource, Default)]
pub struct SaveSlot {
    /// Dernière sauvegarde lue ou écrite.
    pub data: Option<SaveData>,
    /// N'écrit rien sur le disque (captures automatiques).
    pub disabled: bool,
    pub play_time: f64,
    pending: bool,
    timer: f32,
    last_text: Option<String>,
}

impl SaveSlot {
    fn load() -> Self {
        let text = crate::storage::read(FILE);
        let data = text.as_deref().and_then(|t| match ron::from_str::<SaveData>(t) {
            Ok(d) if d.version == VERSION => Some(d),
            Ok(d) => {
                warn!("sauvegarde de version {} ignorée (attendu {VERSION})", d.version);
                None
            }
            Err(e) => {
                warn!("sauvegarde illisible, ignorée : {e}");
                None
            }
        });
        Self { last_text: data.is_some().then_some(text).flatten(), data, ..default() }
    }

    /// Demande une écriture à la fin de la frame.
    pub fn request(&mut self) {
        self.pending = true;
    }

    pub fn store(&mut self, data: SaveData) {
        self.pending = false;
        self.timer = 0.0;
        let text = match ron::ser::to_string_pretty(&data, ron::ser::PrettyConfig::default()) {
            Ok(t) => t,
            Err(e) => {
                warn!("impossible de sérialiser la sauvegarde : {e}");
                return;
            }
        };
        self.data = Some(data);
        if self.disabled || self.last_text.as_deref() == Some(text.as_str()) {
            return;
        }
        crate::storage::write(FILE, &text);
        self.last_text = Some(text);
    }
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SaveSlot::load())
            .add_systems(Last, (autosave_triggers, flush).chain().run_if(in_state(AppState::Playing)));
    }
}

fn autosave_triggers(
    time: Res<Time>,
    fx: Res<FxState>,
    menu: Res<MenuState>,
    mut exit: MessageReader<AppExit>,
    mut slot: ResMut<SaveSlot>,
    mut was_open: Local<bool>,
) {
    let dt = time.delta_secs();
    slot.play_time += dt as f64;
    slot.timer += dt;
    let important = fx.last.iter().any(|e| {
        matches!(
            e,
            SimEvent::Rested { .. }
                | SimEvent::BossAwake
                | SimEvent::BossDefeated { .. }
                | SimEvent::BossRevived
                | SimEvent::PlayerDied
                | SimEvent::Respawned
                | SimEvent::Heal { .. }
                | SimEvent::WeaponSwitched { .. }
                | SimEvent::ItemCycled { .. }
        )
    });
    let menu_toggled = menu.open != *was_open;
    *was_open = menu.open;
    if important || menu_toggled || slot.timer >= AUTOSAVE_SECS || exit.read().next().is_some() {
        slot.request();
    }
}

fn flush(world: &mut World) {
    if world.resource::<SaveSlot>().pending {
        save_now(world);
    }
}

/// Progression actuelle du joueur local (`None` hors partie).
pub fn capture(world: &mut World) -> Option<Progress> {
    let enc = *world.resource::<Encounter>();
    let tuning = world.resource::<Tuning>().clone();
    let mut q = world.query::<(&Player, &Body, &Health)>();
    q.iter(world)
        .min_by_key(|(p, ..)| p.id)
        .map(|(p, b, h)| Progress::of_player(p, b, h, &enc, &tuning))
}

/// Écrit la sauvegarde tout de suite (si une partie est en cours).
pub fn save_now(world: &mut World) {
    let Some(progress) = capture(world) else { return };
    let mut slot = world.resource_mut::<SaveSlot>();
    let data = SaveData { version: VERSION, play_time: slot.play_time, progress };
    slot.store(data);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_roundtrip() {
        let t = Tuning::builtin();
        let mut progress = Progress::new_game(&t);
        progress.souls = 1000;
        progress.pos = Some([0.5, -20.0, 3.0]);
        let data = SaveData { version: VERSION, play_time: 75.0, progress };
        let txt = ron::ser::to_string_pretty(&data, ron::ser::PrettyConfig::default()).unwrap();
        assert_eq!(ron::from_str::<SaveData>(&txt).unwrap(), data);
    }
}
