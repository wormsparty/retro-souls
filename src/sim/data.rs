//! Données de tuning chargées depuis `assets/config/*.ron`.
//!
//! Toutes les durées sont en **ticks** (60 par seconde). Les distances sont en mètres,
//! les vitesses en m/s, les angles en degrés.
//! Repère local d'un combattant : x = droite, y = haut, z = avant.

use bevy::prelude::*;
use serde::Deserialize;

use crate::lang::LText;

/// Capsule en repère local (segment `a`–`b` de rayon `r`).
#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Capsule {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub r: f32,
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reaction {
    /// Petite interruption.
    #[default]
    Light,
    /// Mise au sol.
    Heavy,
}

/// Fenêtre de frappe active pendant `[start, end)`.
#[derive(Deserialize, Clone, Debug)]
pub struct HitWindow {
    pub start: u32,
    pub end: u32,
    pub capsule: Capsule,
    /// Rotation de la capsule autour de l'axe vertical pendant la fenêtre (degrés,
    /// début → fin). Positif = vers la gauche. Sert aux tailles horizontales et aux balayages.
    #[serde(default)]
    pub arc: Option<[f32; 2]>,
    pub damage: f32,
    /// Dégâts infligés à la jauge de stagger de la cible.
    #[serde(default)]
    pub stagger: f32,
    #[serde(default)]
    pub reaction: Reaction,
    /// Attaque furie : imparable en garde normale.
    #[serde(default)]
    pub fury: bool,
    /// Ticks de hitstop appliqués aux deux combattants à l'impact.
    #[serde(default = "default_hitstop")]
    pub hitstop: u8,
}

fn default_hitstop() -> u8 {
    4
}

/// Déplacement « root motion » pendant `[start, end)`.
#[derive(Deserialize, Clone, Debug)]
pub struct Motion {
    pub start: u32,
    pub end: u32,
    /// Vitesse vers l'avant (négatif = recul).
    #[serde(default)]
    pub speed: f32,
    /// Si vrai, la vitesse est calculée pour atterrir à `stop_dist` de la cible
    /// (distance figée au début du mouvement), plafonnée par `speed`.
    #[serde(default)]
    pub to_target: bool,
    #[serde(default)]
    pub stop_dist: f32,
}

/// Définition d'une action (attaque, esquive, réaction…).
#[derive(Deserialize, Clone, Debug)]
pub struct MoveDef {
    /// Nom du clip d'animation correspondant.
    pub anim: String,
    pub total: u32,
    #[serde(default)]
    pub hits: Vec<HitWindow>,
    #[serde(default)]
    pub motion: Vec<Motion>,
    /// L'orientation suit la cible / le stick jusqu'à ce tick.
    #[serde(default)]
    pub track_until: u32,
    /// Vitesse de rotation pendant le suivi (degrés/s).
    #[serde(default = "default_track_rate")]
    pub track_rate: f32,
    /// Coût d'endurance. Absent sur une attaque d'arme : calculé à partir des dégâts
    /// (`stamina_per_damage`), pour qu'un même total de dégâts coûte pareil quelle que soit l'arme.
    #[serde(default)]
    pub stamina: Option<f32>,
    /// Vitesse de marche autorisée pendant l'action (soin), 0 = immobile.
    #[serde(default)]
    pub walk: f32,
    /// Tick à partir duquel l'action suivante (combo, input bufferisé) peut démarrer.
    #[serde(default)]
    pub chain_from: Option<u32>,
    /// Tick à partir duquel esquive et garde peuvent interrompre l'action.
    #[serde(default)]
    pub cancel_from: Option<u32>,
    #[serde(default)]
    pub hyperarmor: Option<[u32; 2]>,
    #[serde(default)]
    pub iframes: Option<[u32; 2]>,
    /// Fenêtre pendant laquelle un coup reçu est contré (posture de l'épée longue).
    #[serde(default)]
    pub counter: Option<[u32; 2]>,
}

fn default_track_rate() -> f32 {
    360.0
}

impl MoveDef {
    pub fn total_damage(&self) -> f32 {
        self.hits.iter().map(|h| h.damage).sum()
    }
    pub fn chain_tick(&self) -> u32 {
        self.chain_from.unwrap_or(self.total)
    }
    pub fn cancel_tick(&self) -> u32 {
        self.cancel_from.or(self.chain_from).unwrap_or(self.total)
    }
    pub fn in_window(w: Option<[u32; 2]>, tick: u32) -> bool {
        w.is_some_and(|[s, e]| tick >= s && tick < e)
    }
    /// Premier tick actif et fin du dernier coup, utilisés pour caler l'animation.
    pub fn strike_span(&self) -> Option<(u32, u32)> {
        let s = self.hits.iter().map(|h| h.start).min()?;
        let e = self.hits.iter().map(|h| h.end).max()?;
        Some((s, e))
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct GuardDef {
    /// Fenêtre de garde parfaite (ticks depuis le début de la garde).
    pub perfect_window: u32,
    /// Pression répétée en moins de `spam_window` ticks : la fenêtre rétrécit de `spam_penalty`.
    pub spam_window: u32,
    pub spam_penalty: u32,
    pub min_window: u32,
    /// Part des dégâts subis en garde normale (le reste est annulé).
    pub damage_ratio: f32,
    /// Endurance perdue par point de dégât bloqué.
    pub stamina_ratio: f32,
    /// Stagger infligé à l'attaquant par une garde parfaite.
    pub perfect_stagger: f32,
    /// Jauge spéciale gagnée par une garde parfaite.
    pub perfect_special: f32,
    pub perfect_hitstop: u8,
    /// Arc frontal protégé par la garde (degrés de chaque côté).
    pub arc: f32,
    pub walk_speed: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PlayerDef {
    pub max_hp: f32,
    pub max_stamina: f32,
    /// Endurance consommée par point de dégât des attaques d'arme.
    pub stamina_per_damage: f32,
    /// L'endurance peut descendre jusque-là ; il faut au moins 1 point pour agir.
    pub stamina_floor: f32,
    pub stamina_regen: f32,
    pub stamina_regen_guarding: f32,
    pub stamina_delay: u32,
    pub run_speed: f32,
    pub sprint_speed: f32,
    pub sprint_stamina: f32,
    /// Maintenir esquive au-delà de ce nombre de ticks déclenche le sprint.
    pub sprint_hold: u32,
    pub accel: f32,
    pub turn_rate: f32,
    pub radius: f32,
    pub height: f32,
    pub lock_range: f32,
    pub input_buffer: u32,
    /// Durée pendant laquelle le regain peut être récupéré.
    pub regain_ticks: u32,
    /// PV rendus par point de dégât infligé.
    pub regain_ratio: f32,
    pub special_segments: u32,
    pub special_per_segment: f32,
    pub special_per_damage: f32,
    /// Distance max et arc pour déclencher le coup fatal.
    pub fatal_range: f32,
    pub fatal_arc: f32,
    pub guard: GuardDef,
    pub dodge: MoveDef,
    pub backstep: MoveDef,
    pub guard_hit: MoveDef,
    pub perfect_guard: MoveDef,
    pub guard_break: MoveDef,
    pub hit_light: MoveDef,
    pub hit_heavy: MoveDef,
    pub switch: MoveDef,
    /// Tick du changement d'arme effectif pendant `switch`.
    pub switch_at: u32,
    pub death: MoveDef,
    /// Charges de soin (rechargées au checkpoint).
    pub heal_charges: u8,
    /// Part des PV max rendue par un soin.
    pub heal_ratio: f32,
    pub heal: MoveDef,
    /// Tick où le soin s'applique : touché avant, la charge est perdue.
    pub heal_at: u32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct WeaponDef {
    pub name: LText,
    pub light: Vec<MoveDef>,
    pub heavy: MoveDef,
    pub heavy_charged: MoveDef,
    /// Ticks de maintien pour une charge complète : l'attaque chargée part alors d'elle-même.
    pub charge_ticks: u32,
    pub charge_anim: String,
    pub special: MoveDef,
    /// Riposte déclenchée si la fenêtre `counter` de la spéciale est touchée.
    #[serde(default)]
    pub special_counter: Option<MoveDef>,
    pub fatal: MoveDef,
}

#[derive(Deserialize, Clone, Debug)]
pub struct WeaponsDef {
    pub weapons: Vec<WeaponDef>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct BossAttack {
    pub name: String,
    pub mv: MoveDef,
    pub min_range: f32,
    pub max_range: f32,
    /// Angle max entre l'avant du boss et la cible pour lancer l'attaque.
    pub max_angle: f32,
    pub weight: f32,
    pub cooldown: u32,
    /// Phases où l'attaque est disponible (1 et/ou 2).
    pub phases: Vec<u8>,
    /// Enchaînement possible : (nom de l'attaque suivante, probabilité).
    #[serde(default)]
    pub next: Option<(String, f32)>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct BossDef {
    pub name: LText,
    pub max_hp: f32,
    pub phase2_at: f32,
    pub radius: f32,
    pub height: f32,
    pub walk_speed: f32,
    pub strafe_speed: f32,
    pub turn_rate: f32,
    /// Distance que le boss cherche à garder avec sa cible.
    pub preferred_range: f32,
    pub stagger_max: f32,
    pub stagger_delay: u32,
    pub stagger_decay: f32,
    /// Pause (min, max) entre deux attaques : c'est là qu'on punit.
    pub idle_ticks: [u32; 2],
    /// Durée pendant laquelle le dernier attaquant garde l'aggro.
    pub aggro_ticks: u32,
    /// Âmes gagnées en le battant.
    pub souls: u32,
    pub groggy: MoveDef,
    pub fatal_received: MoveDef,
    pub roar: MoveDef,
    pub death: MoveDef,
    pub attacks: Vec<BossAttack>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ArenaDef {
    pub radius: f32,
    /// Piliers : (x, z, rayon).
    pub pillars: Vec<[f32; 3]>,
    /// Point de réapparition du joueur (devant le checkpoint).
    pub player_spawn: [f32; 2],
    pub boss_spawn: [f32; 2],
    /// Couloir d'accès, au sud de l'arène (vers -z) : demi-largeur et z du mur du fond.
    pub corridor_half_width: f32,
    pub corridor_end: f32,
    /// Checkpoint (x, z), au bout du couloir.
    pub checkpoint: [f32; 2],
    /// Nom du checkpoint (menu de voyage).
    pub checkpoint_name: LText,
    /// Vue du checkpoint dans le menu de voyage : position de la caméra puis point visé (x, y, z).
    pub checkpoint_view: [[f32; 3]; 2],
}

/// Ensemble des données de tuning utilisées par la simulation.
#[derive(Resource, Clone, Debug)]
pub struct Tuning {
    pub player: PlayerDef,
    pub weapons: Vec<WeaponDef>,
    pub boss: BossDef,
    pub arena: ArenaDef,
}

pub const PLAYER_RON: &str = include_str!("../../assets/config/player.ron");
pub const WEAPONS_RON: &str = include_str!("../../assets/config/weapons.ron");
pub const BOSS_RON: &str = include_str!("../../assets/config/boss.ron");
pub const ARENA_RON: &str = include_str!("../../assets/config/arena.ron");

impl Tuning {
    pub fn parse(player: &str, weapons: &str, boss: &str, arena: &str) -> Result<Self, String> {
        let opts = ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
        let p = |name: &str, e: ron::error::SpannedError| format!("{name}: {e}");
        Ok(Self {
            player: opts.from_str(player).map_err(|e| p("player.ron", e))?,
            weapons: opts
                .from_str::<WeaponsDef>(weapons)
                .map_err(|e| p("weapons.ron", e))?
                .weapons,
            boss: opts.from_str(boss).map_err(|e| p("boss.ron", e))?,
            arena: opts.from_str(arena).map_err(|e| p("arena.ron", e))?,
        })
    }

    /// Données compilées dans le binaire (utilisées au démarrage et par les tests).
    pub fn builtin() -> Self {
        Self::parse(PLAYER_RON, WEAPONS_RON, BOSS_RON, ARENA_RON).expect("tuning intégré invalide")
    }

    pub fn get(&self, r: MoveRef) -> &MoveDef {
        match r {
            MoveRef::Player(m) => {
                let p = &self.player;
                match m {
                    PlayerMove::Dodge => &p.dodge,
                    PlayerMove::Backstep => &p.backstep,
                    PlayerMove::GuardHit => &p.guard_hit,
                    PlayerMove::PerfectGuard => &p.perfect_guard,
                    PlayerMove::GuardBreak => &p.guard_break,
                    PlayerMove::HitLight => &p.hit_light,
                    PlayerMove::HitHeavy => &p.hit_heavy,
                    PlayerMove::Switch => &p.switch,
                    PlayerMove::Death => &p.death,
                    PlayerMove::Heal => &p.heal,
                }
            }
            MoveRef::Weapon(w, m) => {
                let w = &self.weapons[w as usize];
                match m {
                    WeaponMove::Light(i) => &w.light[i as usize],
                    WeaponMove::Heavy => &w.heavy,
                    WeaponMove::HeavyCharged => &w.heavy_charged,
                    WeaponMove::Special => &w.special,
                    WeaponMove::SpecialCounter => {
                        w.special_counter.as_ref().unwrap_or(&w.special)
                    }
                    WeaponMove::Fatal => &w.fatal,
                }
            }
            MoveRef::BossAttack(i) => &self.boss.attacks[i as usize].mv,
            MoveRef::Boss(m) => match m {
                BossMove::Groggy => &self.boss.groggy,
                BossMove::FatalReceived => &self.boss.fatal_received,
                BossMove::Roar => &self.boss.roar,
                BossMove::Death => &self.boss.death,
            },
        }
    }
}

/// Référence compacte (Copy) vers une `MoveDef` : c'est ce qui est stocké dans l'état de la sim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MoveRef {
    Player(PlayerMove),
    Weapon(u8, WeaponMove),
    BossAttack(u16),
    Boss(BossMove),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlayerMove {
    Dodge,
    Backstep,
    GuardHit,
    PerfectGuard,
    GuardBreak,
    HitLight,
    HitHeavy,
    Switch,
    Death,
    Heal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WeaponMove {
    Light(u8),
    Heavy,
    HeavyCharged,
    Special,
    SpecialCounter,
    Fatal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BossMove {
    Groggy,
    FatalReceived,
    Roar,
    Death,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_tuning_parses() {
        let t = Tuning::builtin();
        assert_eq!(t.weapons.len(), 2);
        assert!(!t.boss.attacks.is_empty());
        // Les enchaînements du boss pointent vers des attaques existantes.
        for a in &t.boss.attacks {
            if let Some((n, _)) = &a.next {
                assert!(t.boss.attacks.iter().any(|b| &b.name == n), "next inconnu: {n}");
            }
            for h in &a.mv.hits {
                assert!(h.start < h.end && h.end <= a.mv.total, "{}: fenêtre invalide", a.name);
            }
        }
    }
}
