//! Déroulement de la partie : checkpoints (repos, découverte, voyage), entrée dans l'arène
//! (le boss se réveille et la brume ferme l'escalier), victoire (braises), mort et
//! réapparition au dernier checkpoint, objets ramassés, commandes venues des menus.
//!
//! C'est de la simulation : tout ce qui change l'état passe par ici, au tick près.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::boss::Boss;
use super::data::{BossMove, MoveRef, Tuning};
use super::fighter::{Action, Body, Health};
use super::items::{Inventory, Item};
use super::player::{PState, Player};
use super::{ResetFight, SimEvent, SimEvents, math, world};

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Encounter {
    pub boss_defeated: bool,
    /// Combat en cours : le boss est réveillé et la brume bloque l'escalier.
    pub active: bool,
    /// Un ennemi du chemin est à la poursuite d'un joueur : impossible de se reposer.
    pub hunted: bool,
    /// Les ennemis doivent revenir à leur poste (repos au checkpoint).
    pub respawn_enemies: bool,
}

/// Délai entre la fin de l'animation de mort et la réapparition.
pub const RESPAWN_TICKS: u32 = 150;
/// Distance au checkpoint pour pouvoir s'y reposer.
pub const REST_RANGE: f32 = 2.2;
/// Distance pour ramasser un objet.
pub const PICKUP_RANGE: f32 = 1.4;
/// Rayon de collision du checkpoint.
pub const CHECKPOINT_RADIUS: f32 = 0.6;
/// Distance pour récupérer les braises laissées à la mort.
pub const RECOVER_RANGE: f32 = 1.6;
/// Les braises laissées par une chute restent au moins à cette distance du bord.
const DROP_MARGIN: f32 = 0.9;
/// Il faut s'enfoncer d'autant dans l'arène pour réveiller le boss.
const ENTER_MARGIN: f32 = 1.5;

/// Braises laissées sur place à la mort (avec le cadavre). Les récupérer les rend ; mourir
/// avant les fait perdre pour de bon.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dropped {
    /// Position au sol (x, y, z).
    pub at: [f32; 3],
    pub embers: u32,
}

impl Dropped {
    pub fn pos(&self) -> Vec3 {
        Vec3::from(self.at)
    }
}

/// Assez près des braises laissées pour les récupérer.
pub fn near_dropped(d: &Dropped, pos: Vec3) -> bool {
    let p = d.pos();
    math::flat_len(pos - p) <= RECOVER_RANGE && (pos.y - p.y).abs() < 1.5
}

/// Progression persistante d'un joueur : c'est ce que contient la sauvegarde.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    /// Monnaie (braises). L'ancien nom `souls` est accepté pour les vieilles sauvegardes.
    #[serde(alias = "souls")]
    pub embers: u32,
    pub boss_defeated: bool,
    pub weapon: u8,
    pub inventory: Inventory,
    /// PV (`None` : pleins).
    pub hp: Option<f32>,
    /// Position (x, z) et orientation (`None` : devant le dernier checkpoint).
    pub pos: Option<[f32; 3]>,
    /// Dernier checkpoint où l'on s'est reposé : on y réapparaît.
    pub checkpoint: u8,
    /// Checkpoints découverts (bit i : `level.checkpoints[i]`).
    pub found: u32,
    /// Objets ramassés (bit i : `level.pickups[i]`).
    pub picked: u64,
    /// Ennemis uniques vaincus (bit i : `level.enemies[i]`).
    pub slain: u64,
    /// Braises laissées à la dernière mort.
    pub dropped: Option<Dropped>,
}

impl Progress {
    pub fn new_game(t: &Tuning) -> Self {
        Self { inventory: Inventory::new_game(t), found: 1, ..default() }
    }

    /// État à reprendre pour ce joueur. Mort (ou en pleine chute) : comme après la
    /// réapparition (dernier checkpoint, objets rechargés), ses braises restent là où il est
    /// tombé (et celles qu'il n'avait pas récupérées sont perdues). En plein combat de boss :
    /// devant la brume, le boss sera réinitialisé.
    pub fn of_player(p: &Player, body: &Body, hp: &Health, enc: &Encounter, t: &Tuning) -> Self {
        let dead = matches!(p.state, PState::Dead | PState::Falling) || hp.dead();
        let mut inventory = p.inventory.clone();
        if dead {
            inventory.refill(t);
        }
        let pos = if dead {
            None
        } else if enc.active {
            let g = world::gate_outside(&t.arena);
            Some([g.x, g.z, math::yaw_of(Vec3::Z)])
        } else {
            Some([body.pos.x, body.pos.z, body.yaw])
        };
        let (embers, dropped) = if dead {
            // Une chute : au bord d'où l'on est tombé, un peu en retrait du vide.
            let at = world::settle(t, if p.falling { p.fall_at } else { body.pos }, DROP_MARGIN);
            (0, (p.embers > 0).then_some(Dropped { at: at.to_array(), embers: p.embers }))
        } else {
            (p.embers, p.dropped)
        };
        Self {
            embers,
            dropped,
            boss_defeated: enc.boss_defeated,
            weapon: p.weapon,
            inventory,
            hp: (!dead).then_some(hp.cur),
            pos,
            checkpoint: p.checkpoint,
            found: p.found,
            picked: p.picked,
            slain: p.slain,
        }
    }
}

/// Brasier du checkpoint `i`, au sol.
pub fn checkpoint_pos(t: &Tuning, i: usize) -> Vec3 {
    let c = &t.level.checkpoints[i.min(t.level.checkpoints.len() - 1)];
    let y = world::floor_at(t, c.pos[0], c.pos[1], 0.0).unwrap_or(0.0);
    Vec3::new(c.pos[0], y, c.pos[1])
}

/// Point de réapparition au checkpoint `i` (position, orientation) : à côté du brasier (le feu à
/// sa gauche), tourné vers la suite du chemin (`look`) ; la caméra, derrière, voit le feu de côté.
pub fn checkpoint_spawn(t: &Tuning, i: usize) -> (Vec3, f32) {
    let c = &t.level.checkpoints[i.min(t.level.checkpoints.len() - 1)];
    let fire = checkpoint_pos(t, i);
    let yaw = math::yaw_of(Vec3::new(c.look[0], 0.0, c.look[1]) - fire);
    let p = fire + math::forward(yaw) * 0.5 + math::right(yaw) * 1.4;
    let y = world::floor_at(t, p.x, p.z, p.y).unwrap_or(p.y);
    (Vec3::new(p.x, y, p.z), yaw)
}

/// Checkpoint à portée de repos.
pub fn near_checkpoint(t: &Tuning, pos: Vec3) -> Option<u8> {
    (0..t.level.checkpoints.len()).find_map(|i| {
        let c = checkpoint_pos(t, i);
        (math::flat_len(pos - c) <= REST_RANGE && (pos.y - c.y).abs() < 1.5).then_some(i as u8)
    })
}

/// Position au sol d'un objet à ramasser.
pub fn pickup_pos(t: &Tuning, i: usize) -> Vec3 {
    let p = &t.level.pickups[i];
    Vec3::new(p.pos[0], world::floor_at(t, p.pos[0], p.pos[1], 0.0).unwrap_or(0.0), p.pos[1])
}

/// Objet pas encore ramassé (bits de `picked`) à portée.
pub fn near_pickup(t: &Tuning, picked: u64, pos: Vec3) -> Option<u16> {
    (0..t.level.pickups.len())
        .filter(|i| picked & (1u64 << i) == 0)
        .map(|i| (i, pickup_pos(t, i)))
        .filter(|(_, p)| math::flat_len(pos - *p) <= PICKUP_RANGE && (pos.y - p.y).abs() < 1.5)
        .min_by(|a, b| math::flat_len(pos - a.1).total_cmp(&math::flat_len(pos - b.1)))
        .map(|(i, _)| i as u16)
}

pub fn in_arena(a: &super::data::ArenaDef, pos: Vec3) -> bool {
    math::flat_len(Vec3::new(pos.x, 0.0, pos.z)) < a.radius - ENTER_MARGIN && pos.y > -1.0
}

/// Entrée dans l'arène, victoire, réapparition.
#[allow(clippy::type_complexity)]
pub fn encounter_tick(
    tuning: Res<Tuning>,
    mut enc: ResMut<Encounter>,
    mut reset: ResMut<ResetFight>,
    mut events: ResMut<SimEvents>,
    mut players: Query<(&mut Player, &Body, &Health), Without<Boss>>,
    mut bosses: Query<(&mut Action, &Health), With<Boss>>,
) {
    let t = &*tuning;
    for (mut p, _, _) in &mut players {
        p.dead_ticks = if p.state == PState::Dead { p.dead_ticks + 1 } else { 0 };
    }
    let boss_alive = bosses.iter().any(|(_, h)| !h.dead());

    // Entrée dans l'arène : le boss se réveille en rugissant et la brume se referme.
    if !enc.active
        && boss_alive
        && players.iter().any(|(_, b, h)| !h.dead() && in_arena(&t.arena, b.pos))
    {
        enc.active = true;
        for (mut a, h) in &mut bosses {
            if !h.dead() && a.mv.is_none() {
                a.start(MoveRef::Boss(BossMove::Roar), 0.0);
            }
        }
        events.push(SimEvent::BossAwake);
    }

    if enc.active && !boss_alive {
        enc.active = false;
        enc.boss_defeated = true;
        for (mut p, ..) in &mut players {
            p.embers = p.embers.saturating_add(t.boss.embers);
        }
        events.push(SimEvent::BossDefeated { embers: t.boss.embers });
    }

    // Tout le monde est mort : retour au checkpoint, le boss repart de zéro.
    let all_dead = players.iter().all(|(p, ..)| p.state == PState::Dead && p.dead_ticks >= RESPAWN_TICKS);
    if all_dead
        && !reset.requested
        && let Some((p, b, h)) = players.iter().min_by_key(|(p, ..)| p.id)
    {
        reset.requested = true;
        reset.progress = Some(Progress::of_player(p, b, h, &enc, t));
    }
}

/// Actions décidées dans les menus. En réseau, elles transiteront avec les inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimCommand {
    Equip { player: u8, slot: u8, item: Option<Item> },
    EquipTalisman { player: u8, item: Option<Item> },
    /// Fait revenir le boss vaincu (depuis le checkpoint).
    ReviveBoss,
    /// Voyage vers un checkpoint découvert : on y réapparaît reposé, le monde est réinitialisé.
    Travel { player: u8, checkpoint: u8 },
}

#[derive(Resource, Default, Debug)]
pub struct SimCommands(pub Vec<SimCommand>);

#[allow(clippy::too_many_arguments)]
pub fn apply_commands(
    mut commands: Commands,
    tuning: Res<Tuning>,
    mut queue: ResMut<SimCommands>,
    mut enc: ResMut<Encounter>,
    mut events: ResMut<SimEvents>,
    mut reset: ResMut<ResetFight>,
    mut players: Query<(&mut Player, &Body, &Health)>,
    bosses: Query<Entity, With<Boss>>,
) {
    for c in std::mem::take(&mut queue.0) {
        match c {
            SimCommand::Equip { player, slot, item } => {
                for (mut p, ..) in &mut players {
                    if p.id == player {
                        p.inventory.equip(slot as usize, item);
                    }
                }
            }
            SimCommand::EquipTalisman { player, item } => {
                for (mut p, ..) in &mut players {
                    if p.id == player {
                        p.inventory.equip_talisman(item);
                    }
                }
            }
            SimCommand::Travel { player, checkpoint } => {
                let Some((p, b, h)) = players.iter().find(|(p, ..)| p.id == player) else { continue };
                if enc.active || p.found & (1u32 << checkpoint) == 0 || checkpoint as usize >= tuning.level.checkpoints.len() {
                    continue;
                }
                let mut progress = Progress::of_player(p, b, h, &enc, &tuning);
                progress.checkpoint = checkpoint;
                progress.pos = None;
                progress.hp = None;
                progress.inventory.refill(&tuning);
                reset.requested = true;
                reset.progress = Some(progress);
            }
            SimCommand::ReviveBoss => {
                if enc.boss_defeated && !enc.active {
                    enc.boss_defeated = false;
                    for e in &bosses {
                        commands.entity(e).despawn();
                    }
                    super::spawn_boss(&mut commands, &tuning);
                    events.push(SimEvent::BossRevived);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoints_spawn_points_and_gate() {
        let t = Tuning::builtin();
        for i in 0..t.level.checkpoints.len() {
            let (p, _) = checkpoint_spawn(&t, i);
            assert_eq!(near_checkpoint(&t, p), Some(i as u8));
            assert!(world::floor_at(&t, p.x, p.z, p.y).is_some());
        }
        assert!(!in_arena(&t.arena, world::gate_outside(&t.arena)));
        assert!(in_arena(&t.arena, Vec3::new(0.0, 0.0, -10.0)));
    }
}
