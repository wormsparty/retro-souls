//! Données de tuning chargées depuis `assets/config/*.ron`.
//!
//! Toutes les durées sont en **ticks** (60 par seconde). Les distances sont en mètres,
//! les vitesses en m/s, les angles en degrés.
//! Repère local d'un combattant : x = droite, y = haut, z = avant.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::lang::LText;
use super::items::Item;

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
    /// Zone d'effet (onde de choc) : ni garde ni garde parfaite, seulement la fuite ou les
    /// i-frames. La capsule est affichée au sol pendant l'anticipation.
    #[serde(default)]
    pub aoe: bool,
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
    /// (distance figée au début de l'action), plafonnée par `speed`.
    #[serde(default)]
    pub to_target: bool,
    #[serde(default)]
    pub stop_dist: f32,
    /// Avec `to_target` : la distance est re-mesurée à chaque tick jusqu'au début du
    /// mouvement (et non figée au début de l'action). Sert aux sauts qui retombent sur la cible.
    #[serde(default)]
    pub retarget: bool,
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
pub struct JumpDef {
    /// Vitesse verticale au départ (m/s) : hauteur = v² / (2 × gravité).
    pub speed: f32,
    pub stamina: f32,
    /// Accélération horizontale en l'air (m/s²) : on corrige un peu sa trajectoire, sans plus.
    pub air_control: f32,
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
    pub jump: JumpDef,
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
    /// Phases où l'attaque est disponible (1 et/ou 2). Les ennemis n'ont qu'une phase.
    #[serde(default = "default_phases")]
    pub phases: Vec<u8>,
    /// Enchaînement possible : (nom de l'attaque suivante, probabilité).
    #[serde(default)]
    pub next: Option<(String, f32)>,
}

fn default_phases() -> Vec<u8> {
    vec![1, 2]
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
    /// Braises gagnées en le battant.
    #[serde(alias = "souls")]
    pub embers: u32,
    pub groggy: MoveDef,
    pub fatal_received: MoveDef,
    pub roar: MoveDef,
    pub death: MoveDef,
    pub attacks: Vec<BossAttack>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ArenaDef {
    pub radius: f32,
    /// Piliers : (x, z, rayon).
    pub pillars: Vec<[f32; 3]>,
    pub boss_spawn: [f32; 2],
    /// Demi-largeur de l'ouverture du mur, au sud (vers -z), où se forme la brume.
    pub gate_half_width: f32,
}

/// Forme d'un morceau de sol praticable. Repère jeu : (x, z) au sol, y = hauteur.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
pub enum Shape {
    /// Ellipse horizontale (un disque si les deux rayons sont égaux).
    Ellipse { center: [f32; 2], radii: [f32; 2], y: f32 },
    /// Bande droite de `from` à `to` (x, z, y) : pont, rampe ou escalier si les hauteurs diffèrent.
    Strip { from: [f32; 3], to: [f32; 3], half_width: f32 },
}

/// Aspect d'un morceau de sol (décor uniquement).
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FloorStyle {
    /// Dallage sur un socle de roche.
    #[default]
    Paved,
    /// Pont de pierre sur arches.
    Bridge,
    /// Passerelle de planches.
    Planks,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct FloorDef {
    pub shape: Shape,
    /// Bords murés : on y bute. Sinon, au-delà du bord, c'est le vide (et la chute).
    #[serde(default)]
    pub walled: bool,
    /// Fait partie de l'arène (interdit aux ennemis du chemin).
    #[serde(default)]
    pub arena: bool,
    /// Nombre de marches dessinées (décor ; la pente est continue pour la simulation).
    #[serde(default)]
    pub steps: u32,
    #[serde(default)]
    pub style: FloorStyle,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CheckpointDef {
    pub name: LText,
    /// Position (x, z) du brasier.
    pub pos: [f32; 2],
    /// Point (x, z) vers lequel regarde le joueur qui réapparaît au brasier (et la caméra,
    /// derrière lui) : la suite du chemin. Il se tient à côté du feu, pas devant, pour que le
    /// brasier et ses braises ne masquent pas la vue.
    pub look: [f32; 2],
    /// Vue du lieu dans le menu de voyage : position de la caméra puis point visé (x, y, z).
    pub view: [[f32; 3]; 2],
}

/// Décor posé au sol. Les collisions sont données par `Prop::colliders`.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    /// Réverbère (allumé).
    Lamp,
    /// Réverbère éteint, tordu.
    DeadLamp,
    /// Fontaine sèche.
    Fountain,
    Bench,
    /// Statue de cheval de manège renversée.
    Horse,
    /// Guichet de foire (cabane de bois).
    Booth,
    /// Colonne brisée (kiosque à musique).
    Column,
    Crates,
}

impl Prop {
    /// Cercles de collision (dx, dz, rayon), en repère local (z = avant).
    pub fn colliders(self) -> &'static [[f32; 3]] {
        match self {
            Prop::Lamp | Prop::DeadLamp => &[[0.0, 0.0, 0.18]],
            Prop::Fountain => &[[0.0, 0.0, 1.75]],
            Prop::Bench => &[[-0.5, 0.0, 0.32], [0.5, 0.0, 0.32]],
            Prop::Horse => &[[0.0, -0.45, 0.4], [0.0, 0.45, 0.4]],
            Prop::Booth => &[[0.0, 0.0, 0.95]],
            Prop::Column => &[[0.0, 0.0, 0.3]],
            Prop::Crates => &[[0.0, 0.0, 0.55]],
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
pub struct PropDef {
    pub kind: Prop,
    pub pos: [f32; 2],
    /// Orientation (degrés).
    #[serde(default)]
    pub yaw: f32,
}

/// Ennemi placé dans le niveau.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct EnemySpawn {
    /// Clé du type d'ennemi (`enemies.ron`).
    pub kind: String,
    pub pos: [f32; 2],
    #[serde(default)]
    pub yaw: f32,
    /// Endormi : il faut s'approcher davantage pour le réveiller, mais il voit dans toutes les
    /// directions. Sinon, il guette devant lui.
    #[serde(default)]
    pub asleep: bool,
    /// Les ennemis d'un même groupe (> 0) donnent l'alerte ensemble.
    #[serde(default)]
    pub group: u8,
    /// Ne réapparaît pas une fois vaincu.
    #[serde(default)]
    pub unique: bool,
}

/// Objet qui brille au sol, ramassé une seule fois par partie.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PickupDef {
    pub pos: [f32; 2],
    pub items: Vec<(Item, u8)>,
}

/// Le niveau autour de l'arène : sols, checkpoints, décor, ennemis, objets.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct LevelDef {
    pub floors: Vec<FloorDef>,
    pub checkpoints: Vec<CheckpointDef>,
    #[serde(default)]
    pub props: Vec<PropDef>,
    #[serde(default)]
    pub enemies: Vec<EnemySpawn>,
    #[serde(default)]
    pub pickups: Vec<PickupDef>,
}

/// Type d'ennemi (chien, pantin…).
#[derive(Deserialize, Clone, Debug)]
pub struct EnemyDef {
    pub key: String,
    /// Modèle (`assets/models/<model>.glb`) ; plusieurs types peuvent partager un modèle.
    pub model: String,
    /// Échelle du modèle (les portées des coups sont à donner à cette échelle).
    #[serde(default = "one")]
    pub scale: f32,
    /// Teinte permanente du modèle (r, g, b, force).
    #[serde(default)]
    pub tint: Option<[f32; 4]>,
    pub name: LText,
    pub max_hp: f32,
    pub radius: f32,
    pub height: f32,
    pub mass: f32,
    pub walk_speed: f32,
    pub run_speed: f32,
    pub turn_rate: f32,
    /// Distance de détection (réduite de moitié s'il dort, et alors dans toutes les directions).
    pub sight: f32,
    /// Distance max à son point de départ avant d'abandonner la poursuite.
    pub leash: f32,
    pub preferred_range: f32,
    pub idle_ticks: [u32; 2],
    /// Dégâts encaissés (sur ~1 s) avant d'être interrompu ; 0 = toujours.
    pub poise: f32,
    pub embers: u32,
    /// Cri d'alerte.
    pub alert: MoveDef,
    pub hit: MoveDef,
    pub death: MoveDef,
    pub attacks: Vec<BossAttack>,
}

fn one() -> f32 {
    1.0
}

#[derive(Deserialize, Clone, Debug)]
pub struct EnemiesDef {
    pub kinds: Vec<EnemyDef>,
}

/// Ensemble des données de tuning utilisées par la simulation.
#[derive(Resource, Clone, Debug)]
pub struct Tuning {
    pub player: PlayerDef,
    pub weapons: Vec<WeaponDef>,
    pub boss: BossDef,
    pub arena: ArenaDef,
    pub level: LevelDef,
    pub enemies: Vec<EnemyDef>,
}

pub const PLAYER_RON: &str = include_str!("../../assets/config/player.ron");
pub const WEAPONS_RON: &str = include_str!("../../assets/config/weapons.ron");
pub const BOSS_RON: &str = include_str!("../../assets/config/boss.ron");
pub const ARENA_RON: &str = include_str!("../../assets/config/arena.ron");
pub const LEVEL_RON: &str = include_str!("../../assets/config/level.ron");
pub const ENEMIES_RON: &str = include_str!("../../assets/config/enemies.ron");

/// Contenu des fichiers de tuning, dans l'ordre de `Tuning::parse`.
pub struct TuningSources<'a> {
    pub player: &'a str,
    pub weapons: &'a str,
    pub boss: &'a str,
    pub arena: &'a str,
    pub level: &'a str,
    pub enemies: &'a str,
}

impl Tuning {
    pub fn parse(src: &TuningSources) -> Result<Self, String> {
        let opts = ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
        let p = |name: &str, e: ron::error::SpannedError| format!("{name}: {e}");
        let t = Self {
            player: opts.from_str(src.player).map_err(|e| p("player.ron", e))?,
            weapons: opts
                .from_str::<WeaponsDef>(src.weapons)
                .map_err(|e| p("weapons.ron", e))?
                .weapons,
            boss: opts.from_str(src.boss).map_err(|e| p("boss.ron", e))?,
            arena: opts.from_str(src.arena).map_err(|e| p("arena.ron", e))?,
            level: opts.from_str(src.level).map_err(|e| p("level.ron", e))?,
            enemies: opts.from_str::<EnemiesDef>(src.enemies).map_err(|e| p("enemies.ron", e))?.kinds,
        };
        for e in &t.level.enemies {
            if t.enemy_kind(&e.kind).is_none() {
                return Err(format!("level.ron: type d'ennemi inconnu « {} »", e.kind));
            }
        }
        if t.level.checkpoints.is_empty() {
            return Err("level.ron: il faut au moins un checkpoint".into());
        }
        Ok(t)
    }

    /// Données compilées dans le binaire (utilisées au démarrage et par les tests).
    pub fn builtin() -> Self {
        Self::parse(&TuningSources {
            player: PLAYER_RON,
            weapons: WEAPONS_RON,
            boss: BOSS_RON,
            arena: ARENA_RON,
            level: LEVEL_RON,
            enemies: ENEMIES_RON,
        })
        .expect("tuning intégré invalide")
    }

    /// Index du type d'ennemi `key`.
    pub fn enemy_kind(&self, key: &str) -> Option<u8> {
        self.enemies.iter().position(|k| k.key == key).map(|i| i as u8)
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
            MoveRef::Enemy(k, m) => {
                let e = &self.enemies[k as usize];
                match m {
                    EnemyMove::Attack(i) => &e.attacks[i as usize].mv,
                    EnemyMove::Alert => &e.alert,
                    EnemyMove::Hit => &e.hit,
                    EnemyMove::Death => &e.death,
                }
            }
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
    /// Action d'un ennemi : (type, action).
    Enemy(u8, EnemyMove),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EnemyMove {
    Attack(u8),
    Alert,
    Hit,
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
        for k in &t.enemies {
            for a in &k.attacks {
                for h in &a.mv.hits {
                    assert!(h.start < h.end && h.end <= a.mv.total, "{}/{}: fenêtre invalide", k.key, a.name);
                }
                if let Some((n, _)) = &a.next {
                    assert!(k.attacks.iter().any(|b| &b.name == n), "{}: next inconnu: {n}", k.key);
                }
            }
        }
    }
}
