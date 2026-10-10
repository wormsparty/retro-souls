//! Game flow: checkpoints (rest, discovery, travel), the bosses' fogs (into their arena and back),
//! the torches (reviving a defeated boss), victory (embers), the final door, death and respawn at
//! the last checkpoint, picked-up items, commands coming from the menus.
//!
//! This is simulation: everything that changes the state goes through here, tick-accurate.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::boss::Boss;
use super::data::{BossMove, MoveRef, Tuning};
use super::fighter::{Action, Body, Health};
use super::items::{Inventory, Item};
use super::player::{PState, Player};
use super::world::{self, Zone};
use super::{ResetFight, SimEvent, SimEvents, math};

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Encounter {
    /// Defeated bosses (bit i: `Tuning::encounters[i]`, fought in `Tuning::arenas[i]`).
    pub defeated: u32,
    /// Arena whose bosses are there: a player went through its fog.
    pub arena: Option<u8>,
    /// Fight in progress: the bosses are awake and the fog blocks the way out.
    pub active: bool,
    /// A path enemy is chasing a player: resting is impossible.
    pub hunted: bool,
    /// Enemies must return to their post (rest at the checkpoint).
    pub respawn_enemies: bool,
}

impl Encounter {
    pub fn is_defeated(&self, i: u8) -> bool {
        self.defeated & (1u32 << i) != 0
    }
}

/// All the bosses are defeated: the final door is open.
pub fn door_open(t: &Tuning, defeated: u32) -> bool {
    let all = if t.encounters.len() >= 32 { u32::MAX } else { (1u32 << t.encounters.len()) - 1 };
    defeated & all == all
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
/// Distance to go through a fog, to rekindle a torch, to read the sign.
pub const FOG_RANGE: f32 = 1.8;
pub const TORCH_RANGE: f32 = 1.6;
pub const SIGN_RANGE: f32 = 1.8;
/// Embers dropped by a fall stay at least this far from the edge.
const DROP_MARGIN: f32 = 0.9;

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
pub struct Progress {
    /// Currency (embers).
    pub embers: u32,
    /// Defeated bosses (bit i: `Tuning::encounters[i]`).
    pub defeated: u32,
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
    /// Start in this arena, in front of its bosses (debug: `SOULS_BOSS`). Never saved.
    #[serde(skip)]
    pub arena: Option<u8>,
}

impl Progress {
    pub fn new_game(t: &Tuning) -> Self {
        Self { inventory: Inventory::new_game(t), found: 1, ..default() }
    }

    /// State to resume for this player. Dead (or mid-fall): as after the
    /// respawn (last checkpoint, items refilled), their embers stay where they
    /// fell (and those they hadn't recovered are lost); fallen in an arena, in front of its
    /// fog. In an arena: in front of its fog, the boss will be reset.
    pub fn of_player(p: &Player, body: &Body, hp: &Health, enc: &Encounter, t: &Tuning) -> Self {
        let dead = matches!(p.state, PState::Dead | PState::Falling) || hp.dead();
        let mut inventory = p.inventory.clone();
        if dead {
            inventory.refill(t);
        }
        let outside = match p.zone {
            Zone::Arena(i) => Some(gate_outside(t, i as usize)),
            Zone::Level => None,
        };
        let pos = if dead {
            None
        } else if let Some((g, yaw)) = outside {
            Some([g.x, g.z, yaw])
        } else {
            Some([body.pos.x, body.pos.z, body.yaw])
        };
        let (embers, dropped) = if dead {
            // A fall: at the edge you fell from, a little back from the void.
            let at = match outside {
                Some((g, _)) => g,
                None => world::settle(t, Zone::Level, if p.falling { p.fall_at } else { body.pos }, DROP_MARGIN),
            };
            (0, (p.embers > 0).then_some(Dropped { at: at.to_array(), embers: p.embers }))
        } else {
            (p.embers, p.dropped)
        };
        Self {
            embers,
            dropped,
            defeated: enc.defeated,
            weapon: p.weapon,
            inventory,
            hp: (!dead).then_some(hp.cur),
            pos,
            checkpoint: p.checkpoint,
            found: p.found,
            picked: p.picked,
            slain: p.slain,
            ..default()
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

fn within(pos: Vec3, p: Vec3, range: f32) -> bool {
    math::flat_len(pos - p) <= range && (pos.y - p.y).abs() < 1.5
}

/// Fog at the end of boss `i`'s corridor, in the level (ground point, direction into it).
pub fn gate(t: &Tuning, i: usize) -> (Vec3, Vec3) {
    world::portal(t, &t.arenas[i].gate)
}

/// Where you come back out of arena `i`: in front of its fog in the level, your back to it.
pub fn gate_outside(t: &Tuning, i: usize) -> (Vec3, f32) {
    let (g, dir) = gate(t, i);
    let p = g - dir * 1.4;
    let y = world::floor_at(t, p.x, p.z, g.y).unwrap_or(g.y);
    (Vec3::new(p.x, y, p.z), math::yaw_of(-dir))
}

/// Where you enter arena `i`: past its fog, facing the bosses.
pub fn door_entry(t: &Tuning, i: usize) -> (Vec3, f32) {
    let (d, dir) = world::portal(t, &t.arenas[i].door);
    let p = d + dir * 2.4;
    let y = world::floor_at(t, p.x, p.z, d.y).unwrap_or(d.y);
    (Vec3::new(p.x, y, p.z), math::yaw_of(dir))
}

/// Boss fog within range, in the level.
pub fn near_gate(t: &Tuning, pos: Vec3) -> Option<u8> {
    (0..t.arenas.len()).find(|&i| within(pos, gate(t, i).0, FOG_RANGE)).map(|i| i as u8)
}

/// In arena `i`, close to the fog you came in through.
pub fn near_door(t: &Tuning, i: usize, pos: Vec3) -> bool {
    within(pos, world::portal(t, &t.arenas[i].door).0, FOG_RANGE)
}

/// Torch of boss `i`, on the ground.
pub fn torch_pos(t: &Tuning, i: usize) -> Vec3 {
    let [x, z] = t.arenas[i].torch;
    Vec3::new(x, world::floor_at(t, x, z, 0.0).unwrap_or(0.0), z)
}

pub fn near_torch(t: &Tuning, pos: Vec3) -> Option<u8> {
    (0..t.arenas.len()).find(|&i| within(pos, torch_pos(t, i), TORCH_RANGE)).map(|i| i as u8)
}

/// The sign behind the final door, on the ground.
pub fn sign_pos(t: &Tuning) -> Vec3 {
    let [x, z] = t.level.sign;
    Vec3::new(x, world::floor_at(t, x, z, 0.0).unwrap_or(0.0), z)
}

pub fn near_sign(t: &Tuning, pos: Vec3) -> bool {
    within(pos, sign_pos(t), SIGN_RANGE)
}

/// Bosses waiting in the arena a player went into; the fight starts at once (the fog closes
/// behind them); victory; the arena empties once everyone has left it; respawn.
#[allow(clippy::type_complexity)]
pub fn encounter_tick(
    mut commands: Commands,
    tuning: Res<Tuning>,
    mut enc: ResMut<Encounter>,
    mut reset: ResMut<ResetFight>,
    mut events: ResMut<SimEvents>,
    mut players: Query<(&mut Player, &Body, &Health), Without<Boss>>,
    mut bosses: Query<(Entity, &Boss, &mut Action, &mut Health)>,
) {
    let t = &*tuning;
    for (mut p, _, _) in &mut players {
        p.dead_ticks = if p.state == PState::Dead { p.dead_ticks + 1 } else { 0 };
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
    let alive_in = |p: &Player, h: &Health| match p.zone {
        Zone::Arena(i) if !h.dead() => Some(i),
        _ => None,
    };

    // A player went through a fog: its bosses are there (created at the next tick).
    if let Some(i) = players.iter().find_map(|(p, _, h)| alive_in(p, h))
        && enc.arena != Some(i)
        && !enc.is_defeated(i)
    {
        for (e, ..) in &bosses {
            commands.entity(e).despawn();
        }
        super::spawn_boss(&mut commands, t, i);
        enc.arena = Some(i);
        enc.active = false;
        return;
    }
    let Some(arena) = enc.arena else { return };
    // Supporting roles (the butcher's dogs) don't count for victory.
    let boss_alive = bosses.iter().any(|(_, b, _, h)| !h.dead() && !b.def(t).minor);

    // The bosses wake up roaring, the fog closes again.
    if !enc.active && boss_alive && players.iter().any(|(p, _, h)| alive_in(p, h) == Some(arena)) {
        enc.active = true;
        for (_, b, mut a, h) in &mut bosses {
            if !h.dead() && a.mv.is_none() {
                a.start(MoveRef::Boss(b.def, BossMove::Roar), 0.0);
            }
        }
        events.push(SimEvent::BossAwake);
    }

    if enc.active && !boss_alive {
        enc.active = false;
        enc.defeated |= 1u32 << arena;
        // With the master fallen, his dogs collapse with him.
        for (_, b, mut a, mut h) in &mut bosses {
            if !h.dead() {
                h.cur = 0.0;
                a.start(MoveRef::Boss(b.def, BossMove::Death), 0.0);
            }
        }
        let embers = t.encounters.get(arena as usize).map_or(0, |e| e.embers);
        for (mut p, ..) in &mut players {
            p.embers = p.embers.saturating_add(embers);
        }
        events.push(SimEvent::BossDefeated { embers });
        if door_open(t, enc.defeated) {
            events.push(SimEvent::DoorOpened);
        }
    }

    // Everyone has left the arena (after the victory): it empties.
    if !enc.active && !players.iter().any(|(p, ..)| p.zone == Zone::Arena(arena)) {
        for (e, ..) in &bosses {
            commands.entity(e).despawn();
        }
        enc.arena = None;
    }
}

/// Actions decided in the menus. Over the network, they'll travel with the inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimCommand {
    Equip { player: u8, slot: u8, item: Option<Item> },
    EquipTalisman { player: u8, item: Option<Item> },
    /// Brings back a defeated boss (rekindling its torch): it awaits again in its arena.
    ReviveBoss(u8),
    /// Travel to a discovered checkpoint: you respawn there rested, the world is reset.
    Travel { player: u8, checkpoint: u8 },
}

#[derive(Resource, Default, Debug)]
pub struct SimCommands(pub Vec<SimCommand>);

pub fn apply_commands(
    tuning: Res<Tuning>,
    mut queue: ResMut<SimCommands>,
    mut enc: ResMut<Encounter>,
    mut events: ResMut<SimEvents>,
    mut reset: ResMut<ResetFight>,
    mut players: Query<(&mut Player, &Body, &Health)>,
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
                if enc.active
                    || p.zone != Zone::Level
                    || p.found & (1u32 << checkpoint) == 0
                    || checkpoint as usize >= tuning.level.checkpoints.len()
                {
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
            SimCommand::ReviveBoss(i) => {
                if enc.is_defeated(i) && enc.arena != Some(i) {
                    enc.defeated &= !(1u32 << i);
                    events.push(SimEvent::BossRevived { arena: i });
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
    }

    #[test]
    fn fogs_torches_and_sign_are_on_the_ground_of_their_zone() {
        let t = Tuning::builtin();
        for i in 0..t.arenas.len() {
            let (g, _) = gate(&t, i);
            assert_eq!(world::zone_at(&t, g), Zone::Level, "gate {i}");
            let (out, _) = gate_outside(&t, i);
            assert!(world::floor_below(&t, out.x, out.z, out.y, Some(world::Mover::Player { zone: Zone::Level, door_open: false })).is_some(), "gate {i}");
            assert_eq!(near_gate(&t, out), Some(i as u8));
            let (entry, _) = door_entry(&t, i);
            assert_eq!(world::zone_at(&t, entry), Zone::Arena(i as u8), "arena {i}");
            let (d, dir) = world::portal(&t, &t.arenas[i].door);
            assert!(near_door(&t, i, d + dir * 0.8) && !near_door(&t, i, entry), "arena {i}");
            let torch = torch_pos(&t, i);
            assert!(world::floor_at(&t, torch.x, torch.z, 0.0).is_some(), "torch {i}");
            assert_eq!(near_torch(&t, torch + Vec3::X * 0.8), Some(i as u8));
            let [x, z] = t.arenas[i].boss_spawn;
            assert_eq!(world::zone_at(&t, Vec3::new(x, 0.0, z)), Zone::Arena(i as u8), "boss {i}");
        }
        let s = sign_pos(&t);
        assert!(world::floor_at(&t, s.x, s.z, 0.0).is_some());
        assert!(!door_open(&t, 0) && door_open(&t, u32::MAX));
    }
}
