//! Déroulement de la partie autour du combat : checkpoint au bout du couloir, entrée dans
//! l'arène (le boss se réveille et la brume ferme le couloir), victoire (âmes), mort et
//! réapparition au checkpoint, commandes venues des menus.
//!
//! C'est de la simulation : tout ce qui change l'état passe par ici, au tick près.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::boss::Boss;
use super::data::{ArenaDef, BossMove, MoveRef, Tuning};
use super::fighter::{Action, Body, Health};
use super::items::{Inventory, Item};
use super::player::{PState, Player};
use super::{ResetFight, SimEvent, SimEvents, math};

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Encounter {
    pub boss_defeated: bool,
    /// Combat en cours : le boss est réveillé et la brume bloque le couloir.
    pub active: bool,
}

/// Délai entre la fin de l'animation de mort et la réapparition.
pub const RESPAWN_TICKS: u32 = 150;
/// Distance au checkpoint pour pouvoir s'y reposer.
pub const REST_RANGE: f32 = 2.2;
/// Rayon de collision du checkpoint.
pub const CHECKPOINT_RADIUS: f32 = 0.45;
/// Il faut s'enfoncer d'autant dans l'arène pour réveiller le boss.
const ENTER_MARGIN: f32 = 1.5;

/// Progression persistante d'un joueur : c'est ce que contient la sauvegarde.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub souls: u32,
    pub boss_defeated: bool,
    pub weapon: u8,
    pub inventory: Inventory,
    /// PV (`None` : pleins).
    pub hp: Option<f32>,
    /// Position (x, z) et orientation (`None` : devant le checkpoint).
    pub pos: Option<[f32; 3]>,
}

impl Progress {
    pub fn new_game(t: &Tuning) -> Self {
        Self { inventory: Inventory::new_game(t), ..default() }
    }

    /// État à reprendre pour ce joueur. Mort : comme après la réapparition (checkpoint,
    /// objets rechargés). En plein combat : devant la brume, le boss sera réinitialisé.
    pub fn of_player(p: &Player, body: &Body, hp: &Health, enc: &Encounter, t: &Tuning) -> Self {
        let dead = p.state == PState::Dead || hp.dead();
        let mut inventory = p.inventory.clone();
        if dead {
            inventory.refill(t);
        }
        let pos = if dead {
            None
        } else if enc.active {
            let g = gate_outside(&t.arena);
            Some([g.x, g.z, math::yaw_of(Vec3::Z)])
        } else {
            Some([body.pos.x, body.pos.z, body.yaw])
        };
        Self {
            souls: p.souls,
            boss_defeated: enc.boss_defeated,
            weapon: p.weapon,
            inventory,
            hp: (!dead).then_some(hp.cur),
            pos,
        }
    }
}

pub fn checkpoint_pos(a: &ArenaDef) -> Vec3 {
    Vec3::new(a.checkpoint[0], 0.0, a.checkpoint[1])
}

pub fn near_checkpoint(a: &ArenaDef, pos: Vec3) -> bool {
    math::flat_len(pos - checkpoint_pos(a)) <= REST_RANGE
}

pub fn in_arena(a: &ArenaDef, pos: Vec3) -> bool {
    math::flat_len(Vec3::new(pos.x, 0.0, pos.z)) < a.radius - ENTER_MARGIN
}

/// Centre de la brume, dans l'ouverture du mur de l'arène.
pub fn fog_gate(a: &ArenaDef) -> Vec3 {
    let hw = a.corridor_half_width;
    Vec3::new(0.0, 0.0, -libm::sqrtf(a.radius * a.radius - hw * hw) - 0.3)
}

/// Point du couloir juste devant la brume.
pub fn gate_outside(a: &ArenaDef) -> Vec3 {
    Vec3::new(0.0, 0.0, -(a.radius + 1.5))
}

/// Ramène une position (corps de rayon `r`) dans la zone praticable : l'arène, plus le
/// couloir si `corridor` (fermé par la brume pendant le combat, interdit au boss).
pub fn clamp_walkable(a: &ArenaDef, pos: Vec3, r: f32, corridor: bool) -> Vec3 {
    let flat = Vec3::new(pos.x, 0.0, pos.z);
    let max = a.radius - r;
    let d = math::flat_len(flat);
    if d <= max {
        return pos;
    }
    let c = flat / d * max;
    let in_circle = Vec3::new(c.x, pos.y, c.z);
    if !corridor {
        return in_circle;
    }
    let hw = (a.corridor_half_width - r).max(0.0);
    let in_corridor = Vec3::new(pos.x.clamp(-hw, hw), pos.y, pos.z.clamp(a.corridor_end + r, -(a.radius - 2.0)));
    if in_corridor == pos || pos.distance_squared(in_corridor) < pos.distance_squared(in_circle) {
        in_corridor
    } else {
        in_circle
    }
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
            p.souls = p.souls.saturating_add(t.boss.souls);
        }
        events.push(SimEvent::BossDefeated { souls: t.boss.souls });
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
    /// Fait revenir le boss vaincu (depuis le checkpoint).
    ReviveBoss,
}

#[derive(Resource, Default, Debug)]
pub struct SimCommands(pub Vec<SimCommand>);

pub fn apply_commands(
    mut commands: Commands,
    tuning: Res<Tuning>,
    mut queue: ResMut<SimCommands>,
    mut enc: ResMut<Encounter>,
    mut events: ResMut<SimEvents>,
    mut players: Query<&mut Player>,
    bosses: Query<Entity, With<Boss>>,
) {
    for c in std::mem::take(&mut queue.0) {
        match c {
            SimCommand::Equip { player, slot, item } => {
                for mut p in &mut players {
                    if p.id == player {
                        p.inventory.equip(slot as usize, item);
                    }
                }
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
    fn walkable_area_is_arena_plus_corridor() {
        let t = Tuning::builtin();
        let a = &t.arena;
        let r = 0.4;
        // Dans le couloir : libre, sauf pour le boss ou pendant le combat.
        let p = Vec3::new(0.5, 0.0, -24.0);
        assert_eq!(clamp_walkable(a, p, r, true), p);
        assert!(math::flat_len(clamp_walkable(a, p, r, false)) <= a.radius - r + 1e-4);
        // Contre le mur du couloir et au fond.
        assert_eq!(clamp_walkable(a, Vec3::new(5.0, 0.0, -24.0), r, true).x, a.corridor_half_width - r);
        assert_eq!(clamp_walkable(a, Vec3::new(0.0, 0.0, -40.0), r, true).z, a.corridor_end + r);
        // Hors de l'ouverture : on reste dans l'arène.
        let q = clamp_walkable(a, Vec3::new(6.0, 0.0, -16.5), r, true);
        assert!((math::flat_len(q) - (a.radius - r)).abs() < 1e-3);
        assert!(near_checkpoint(a, Vec3::new(a.player_spawn[0], 0.0, a.player_spawn[1])));
        assert!(!in_arena(a, gate_outside(a)));
    }
}
