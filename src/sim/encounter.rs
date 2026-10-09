//! Game flow: checkpoints (rest, discovery, travel), entering the arena
//! (the boss wakes up and the fog closes the stairs), victory (embers), death and
//! respawn at the last checkpoint, picked-up items, commands coming from the menus.
//!
//! This is simulation: everything that changes the state goes through here, tick-accurate.

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
    /// Fight in progress: the boss is awake and the fog blocks the stairs.
    pub active: bool,
    /// A path enemy is chasing a player: resting is impossible.
    pub hunted: bool,
    /// Enemies must return to their post (rest at the checkpoint).
    pub respawn_enemies: bool,
    /// Encounter waiting in the arena (`Tuning::encounters`).
    pub boss_choice: u8,
}

/// Delay between the end of the death animation and the respawn.
pub const RESPAWN_TICKS: u32 = 150;
/// Distance to the checkpoint to be able to rest there.
pub const REST_RANGE: f32 = 2.2;
/// Distance to pick up an item.
pub const PICKUP_RANGE: f32 = 1.4;
/// Collision radius of the checkpoint.
pub const CHECKPOINT_RADIUS: f32 = 0.6;
/// Distance to recover the embers dropped on death.
pub const RECOVER_RANGE: f32 = 1.6;
/// Embers dropped by a fall stay at least this far from the edge.
const DROP_MARGIN: f32 = 0.9;
/// You have to go this deep into the arena to wake the boss.
const ENTER_MARGIN: f32 = 1.5;

/// Embers left on the spot on death (with the corpse). Recovering them gives them back; dying
/// before that loses them for good.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dropped {
    /// Ground position (x, y, z).
    pub at: [f32; 3],
    pub embers: u32,
}

impl Dropped {
    pub fn pos(&self) -> Vec3 {
        Vec3::from(self.at)
    }
}

/// Close enough to the dropped embers to recover them.
pub fn near_dropped(d: &Dropped, pos: Vec3) -> bool {
    let p = d.pos();
    math::flat_len(pos - p) <= RECOVER_RANGE && (pos.y - p.y).abs() < 1.5
}

/// A player's persistent progress: this is what the save contains.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    /// Currency (embers). The old name `souls` is accepted for old saves.
    #[serde(alias = "souls")]
    pub embers: u32,
    pub boss_defeated: bool,
    /// Encounter chosen at the checkpoint (`Tuning::encounters`).
    #[serde(default)]
    pub boss_choice: u8,
    pub weapon: u8,
    pub inventory: Inventory,
    /// HP (`None`: full).
    pub hp: Option<f32>,
    /// Position (x, z) and orientation (`None`: in front of the last checkpoint).
    pub pos: Option<[f32; 3]>,
    /// Last checkpoint rested at: respawn there.
    pub checkpoint: u8,
    /// Discovered checkpoints (bit i: `level.checkpoints[i]`).
    pub found: u32,
    /// Picked-up items (bit i: `level.pickups[i]`).
    pub picked: u64,
    /// Defeated unique enemies (bit i: `level.enemies[i]`).
    pub slain: u64,
    /// Embers dropped on the last death.
    pub dropped: Option<Dropped>,
}

impl Progress {
    pub fn new_game(t: &Tuning) -> Self {
        Self { inventory: Inventory::new_game(t), found: 1, ..default() }
    }

    /// State to resume for this player. Dead (or mid-fall): as after the
    /// respawn (last checkpoint, items refilled), their embers stay where they
    /// fell (and those they hadn't recovered are lost). Mid boss fight:
    /// in front of the fog, the boss will be reset.
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
            // A fall: at the edge you fell from, a little back from the void.
            let at = world::settle(t, if p.falling { p.fall_at } else { body.pos }, DROP_MARGIN);
            (0, (p.embers > 0).then_some(Dropped { at: at.to_array(), embers: p.embers }))
        } else {
            (p.embers, p.dropped)
        };
        Self {
            embers,
            dropped,
            boss_defeated: enc.boss_defeated,
            boss_choice: enc.boss_choice,
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

/// Brazier of checkpoint `i`, on the ground.
pub fn checkpoint_pos(t: &Tuning, i: usize) -> Vec3 {
    let c = &t.level.checkpoints[i.min(t.level.checkpoints.len() - 1)];
    let y = world::floor_at(t, c.pos[0], c.pos[1], 0.0).unwrap_or(0.0);
    Vec3::new(c.pos[0], y, c.pos[1])
}

/// Respawn point at checkpoint `i` (position, orientation): next to the brazier (the fire on
/// its left), facing the way forward (`look`); the camera, behind, sees the fire from the side.
pub fn checkpoint_spawn(t: &Tuning, i: usize) -> (Vec3, f32) {
    let c = &t.level.checkpoints[i.min(t.level.checkpoints.len() - 1)];
    let fire = checkpoint_pos(t, i);
    let yaw = math::yaw_of(Vec3::new(c.look[0], 0.0, c.look[1]) - fire);
    let p = fire + math::forward(yaw) * 0.5 + math::right(yaw) * 1.4;
    let y = world::floor_at(t, p.x, p.z, p.y).unwrap_or(p.y);
    (Vec3::new(p.x, y, p.z), yaw)
}

/// Checkpoint within resting range.
pub fn near_checkpoint(t: &Tuning, pos: Vec3) -> Option<u8> {
    (0..t.level.checkpoints.len()).find_map(|i| {
        let c = checkpoint_pos(t, i);
        (math::flat_len(pos - c) <= REST_RANGE && (pos.y - c.y).abs() < 1.5).then_some(i as u8)
    })
}

/// Ground position of an item to pick up.
pub fn pickup_pos(t: &Tuning, i: usize) -> Vec3 {
    let p = &t.level.pickups[i];
    Vec3::new(p.pos[0], world::floor_at(t, p.pos[0], p.pos[1], 0.0).unwrap_or(0.0), p.pos[1])
}

/// Item not yet picked up (bits of `picked`) within range.
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

/// Entering the arena, victory, respawn.
#[allow(clippy::type_complexity)]
pub fn encounter_tick(
    tuning: Res<Tuning>,
    mut enc: ResMut<Encounter>,
    mut reset: ResMut<ResetFight>,
    mut events: ResMut<SimEvents>,
    mut players: Query<(&mut Player, &Body, &Health), Without<Boss>>,
    mut bosses: Query<(&Boss, &mut Action, &mut Health)>,
) {
    let t = &*tuning;
    for (mut p, _, _) in &mut players {
        p.dead_ticks = if p.state == PState::Dead { p.dead_ticks + 1 } else { 0 };
    }
    // Supporting roles (the butcher's dogs) don't count for victory.
    let boss_alive = bosses.iter().any(|(b, _, h)| !h.dead() && !b.def(t).minor);

    // Entering the arena: the boss wakes up roaring and the fog closes again.
    if !enc.active
        && boss_alive
        && players.iter().any(|(_, b, h)| !h.dead() && in_arena(&t.arena, b.pos))
    {
        enc.active = true;
        for (b, mut a, h) in &mut bosses {
            if !h.dead() && a.mv.is_none() {
                a.start(MoveRef::Boss(b.def, BossMove::Roar), 0.0);
            }
        }
        events.push(SimEvent::BossAwake);
    }

    if enc.active && !boss_alive {
        enc.active = false;
        enc.boss_defeated = true;
        // With the master fallen, his dogs collapse with him.
        for (b, mut a, mut h) in &mut bosses {
            if !h.dead() {
                h.cur = 0.0;
                a.start(MoveRef::Boss(b.def, BossMove::Death), 0.0);
            }
        }
        let embers = t.encounters.get(enc.boss_choice as usize).map_or(0, |e| e.embers);
        for (mut p, ..) in &mut players {
            p.embers = p.embers.saturating_add(embers);
        }
        events.push(SimEvent::BossDefeated { embers });
    }

    // Everyone is dead: back to the checkpoint, the boss starts over.
    let all_dead = players.iter().all(|(p, ..)| p.state == PState::Dead && p.dead_ticks >= RESPAWN_TICKS);
    if all_dead
        && !reset.requested
        && let Some((p, b, h)) = players.iter().min_by_key(|(p, ..)| p.id)
    {
        reset.requested = true;
        reset.progress = Some(Progress::of_player(p, b, h, &enc, t));
    }
}

/// Actions decided in the menus. Over the network, they'll travel with the inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimCommand {
    Equip { player: u8, slot: u8, item: Option<Item> },
    EquipTalisman { player: u8, item: Option<Item> },
    /// Brings back the defeated boss (from the checkpoint).
    ReviveBoss,
    /// Chooses the boss waiting in the arena (from the checkpoint): it appears fresh.
    ChooseBoss(u8),
    /// Travel to a discovered checkpoint: you respawn there rested, the world is reset.
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
            SimCommand::ChooseBoss(choice) => {
                if !enc.active && (choice as usize) < tuning.encounters.len() {
                    enc.boss_choice = choice;
                    enc.boss_defeated = false;
                    for e in &bosses {
                        commands.entity(e).despawn();
                    }
                    super::spawn_boss(&mut commands, &tuning, choice);
                    events.push(SimEvent::BossRevived);
                }
            }
            SimCommand::ReviveBoss => {
                if enc.boss_defeated && !enc.active {
                    enc.boss_defeated = false;
                    for e in &bosses {
                        commands.entity(e).despawn();
                    }
                    super::spawn_boss(&mut commands, &tuning, enc.boss_choice);
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
