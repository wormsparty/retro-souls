//! Simulation de combat déterministe à 60 ticks/s.
//!
//! Règles pour rester compatible avec un futur rollback réseau :
//! - aucune lecture de `Time` ni d'input matériel ici : seulement `PlayerInputs` et `SimTick` ;
//! - maths via `math` (libm), aléatoire via `SimRng` ;
//! - tout l'état est dans des composants/ressources `Clone`.

pub mod boss;
pub mod combat;
pub mod data;
pub mod encounter;
pub mod enemy;
pub mod fighter;
pub mod input;
pub mod items;
pub mod math;
pub mod player;
pub mod rng;
pub mod world;

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;

use data::Tuning;
use encounter::{Encounter, Progress, SimCommands};
use fighter::{Action, Body, Foe, Health, Hitstop, PrevBody, Team};
use input::PlayerInputs;

pub const TICK_HZ: f64 = 60.0;
pub const DT: f32 = 1.0 / 60.0;

/// Numéro du tick de simulation courant.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SimTick(pub u32);

/// Schedule exécuté une fois par tick de simulation.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct SimSchedule;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum SimSet {
    Begin,
    Act,
    Physics,
    Combat,
    End,
}

/// Événements produits par la sim à destination de la présentation (sons, VFX, HUD).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SimEvent {
    Hit { pos: Vec3, heavy: bool, on_player: bool },
    Guard { pos: Vec3 },
    PerfectGuard { pos: Vec3 },
    GuardBreak { pos: Vec3 },
    Counter { pos: Vec3 },
    Swing { entity: Entity, heavy: bool },
    Dodge { entity: Entity },
    FuryWarn { entity: Entity },
    /// Impact d'une attaque de zone (`aoe`) : centre au sol et rayon.
    Shockwave { pos: Vec3, radius: f32 },
    Groggy { entity: Entity },
    Fatal { pos: Vec3 },
    BossPhase2,
    BossDied,
    PlayerDied,
    WeaponSwitched { entity: Entity },
    Heal { entity: Entity },
    /// Action refusée faute d'endurance (retour visuel sur la barre).
    NoStamina { entity: Entity },
    /// Emplacement rapide suivant sélectionné.
    ItemCycled { entity: Entity },
    /// Le joueur est entré dans l'arène : le boss se réveille, la brume se ferme.
    BossAwake,
    BossDefeated { embers: u32 },
    BossRevived,
    /// Repos au checkpoint (PV, objets et endurance restaurés).
    Rested { entity: Entity },
    /// Les combattants viennent d'être (re)créés : chargement, réapparition, voyage.
    Respawned,
    /// Un ennemi repère un joueur et donne l'alerte.
    EnemyAlert { entity: Entity },
    /// Un ennemi est vaincu (braises données au joueur qui l'a achevé).
    EnemyDied { pos: Vec3, embers: u32 },
    /// Son corps disparaît, quelques instants plus tard.
    EnemyVanished { pos: Vec3 },
    /// Objet ramassé (`level.pickups[pickup]`).
    PickedUp { entity: Entity, pickup: u16 },
    /// Consommable utilisé (autre que la fiole, qui donne `Heal`).
    ItemUsed { entity: Entity, item: items::Item },
    /// Checkpoint découvert (premier repos).
    Kindled { checkpoint: u8 },
    /// Le joueur est passé par-dessus bord.
    Fell { entity: Entity },
    /// Saut, et retour au sol.
    Jumped { entity: Entity },
    Landed { entity: Entity },
    /// Braises laissées à la mort récupérées.
    EmbersRecovered { entity: Entity, pos: Vec3, embers: u32 },
}

#[derive(Resource, Default, Debug)]
pub struct SimEvents(pub Vec<SimEvent>);

impl SimEvents {
    pub fn push(&mut self, e: SimEvent) {
        // Évite une croissance infinie si personne ne lit (tests, onglet en arrière-plan).
        if self.0.len() < 256 {
            self.0.push(e);
        }
    }
}

/// Options de debug qui influencent la sim (identiques chez tous les pairs).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SimDebug {
    pub boss_passive: bool,
}

/// Demande de (re)création des combattants, traitée au début du tick suivant.
#[derive(Resource, Clone, Debug)]
pub struct ResetFight {
    pub requested: bool,
    pub players: u8,
    /// Progression de départ. `None` : celle du joueur actuel (ou une nouvelle partie).
    pub progress: Option<Progress>,
}

impl Default for ResetFight {
    fn default() -> Self {
        Self { requested: true, players: 1, progress: None }
    }
}

/// Marqueur de toutes les entités de simulation (pour le reset).
#[derive(Component, Clone, Copy, Debug)]
pub struct SimEntity;

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<Tuning>() {
            app.insert_resource(Tuning::builtin());
        }
        app.init_resource::<SimTick>()
            .init_resource::<SimEvents>()
            .init_resource::<PlayerInputs>()
            .init_resource::<rng::SimRng>()
            .init_resource::<SimDebug>()
            .init_resource::<ResetFight>()
            .init_resource::<Encounter>()
            .init_resource::<SimCommands>();

        let mut schedule = Schedule::new(SimSchedule);
        schedule.configure_sets(
            (SimSet::Begin, SimSet::Act, SimSet::Physics, SimSet::Combat, SimSet::End).chain(),
        );
        schedule.add_systems((
            (reset_fight, encounter::apply_commands, begin_tick).chain().in_set(SimSet::Begin),
            (player::player_act, boss::boss_act, enemy::enemy_act).chain().in_set(SimSet::Act),
            combat::separate_bodies.in_set(SimSet::Physics),
            combat::resolve_hits.in_set(SimSet::Combat),
            (
                player::player_end_tick,
                boss::boss_end_tick,
                encounter::encounter_tick,
                enemy::respawn_on_request,
                advance_actions,
                end_tick,
            )
                .chain()
                .in_set(SimSet::End),
        ));
        app.add_schedule(schedule);
    }
}

/// Exécute un tick de simulation. À appeler depuis `FixedUpdate` (ou par le rollback plus tard).
pub fn run_sim_tick(world: &mut World) {
    world.run_schedule(SimSchedule);
}

#[allow(clippy::too_many_arguments)]
fn reset_fight(
    mut commands: Commands,
    mut reset: ResMut<ResetFight>,
    tuning: Res<Tuning>,
    existing: Query<Entity, With<SimEntity>>,
    players: Query<(&player::Player, &Body, &Health)>,
    mut enc: ResMut<Encounter>,
    mut events: ResMut<SimEvents>,
    mut rng: ResMut<rng::SimRng>,
) {
    if !reset.requested {
        return;
    }
    reset.requested = false;
    let progress = reset.progress.take().unwrap_or_else(|| {
        players
            .iter()
            .min_by_key(|(p, ..)| p.id)
            .map(|(p, b, h)| Progress::of_player(p, b, h, &enc, &tuning))
            .unwrap_or_else(|| Progress::new_game(&tuning))
    });
    for e in &existing {
        commands.entity(e).despawn();
    }
    events.0.clear();
    *rng = rng::SimRng::default();
    *enc = Encounter { boss_defeated: progress.boss_defeated, ..default() };
    spawn_fight(&mut commands, &tuning, reset.players, &progress);
    events.push(SimEvent::Respawned);
}

/// Crée les joueurs, le boss (s'il n'a pas été vaincu) et les ennemis du chemin. Les entités
/// sont créées dans un ordre fixe (déterminisme).
pub fn spawn_fight(commands: &mut Commands, t: &Tuning, players: u8, progress: &Progress) {
    let checkpoint = (progress.checkpoint as usize).min(t.level.checkpoints.len() - 1);
    for id in 0..players.max(1) {
        let (spawn, spawn_yaw) = encounter::checkpoint_spawn(t, checkpoint);
        // Une position sauvegardée hors du sol (ancienne sauvegarde, niveau modifié) : au checkpoint.
        // Au pied d'un brasier (on avait quitté en s'y reposant) : à la place habituelle, tourné
        // vers la suite du chemin plutôt que vers le feu.
        let saved = progress.pos.and_then(|[x, z, yaw]| {
            let y = world::floor_at(t, x, z, 0.0)?;
            let pos = Vec3::new(x, y, z);
            match encounter::near_checkpoint(t, pos) {
                Some(cp) => Some(encounter::checkpoint_spawn(t, cp as usize)),
                None => Some((pos, yaw)),
            }
        });
        let (pos, yaw) = saved.unwrap_or((spawn, spawn_yaw));
        let pos = match world::step(t, pos + math::right(yaw) * id as f32, t.player.radius, world::Mover::Player) {
            world::Step::Ground(p) => p,
            world::Step::Fall => pos,
        };
        let mut p = player::Player::new(id, t);
        p.embers = progress.embers;
        p.weapon = progress.weapon.min(t.weapons.len().saturating_sub(1) as u8);
        p.inventory = progress.inventory.clone();
        p.checkpoint = checkpoint as u8;
        p.found = progress.found | (1 << checkpoint);
        p.picked = progress.picked;
        p.slain = progress.slain;
        p.dropped = progress.dropped;
        let mut hp = Health::new(t.player.max_hp);
        if let Some(cur) = progress.hp {
            hp.cur = cur.clamp(1.0, hp.max);
        }
        commands.spawn((
            SimEntity,
            Team::Players,
            Body { pos, yaw, radius: t.player.radius, height: t.player.height, mass: 1.0 },
            PrevBody { pos, yaw },
            hp,
            Hitstop::default(),
            Action::default(),
            p,
        ));
    }
    if !progress.boss_defeated {
        spawn_boss(commands, t);
    }
    enemy::spawn_all(commands, t, progress.slain);
}

/// Boss endormi à son point d'apparition, tourné vers l'ouverture de l'arène.
pub fn spawn_boss(commands: &mut Commands, t: &Tuning) {
    let [bx, bz] = t.arena.boss_spawn;
    let pos = Vec3::new(bx, 0.0, bz);
    let yaw = math::yaw_of(world::fog_gate(&t.arena) - pos);
    commands.spawn((
        SimEntity,
        Team::Enemies,
        Foe,
        Body { pos, yaw, radius: t.boss.radius, height: t.boss.height, mass: 8.0 },
        PrevBody { pos, yaw },
        Health::new(t.boss.max_hp),
        Hitstop::default(),
        Action::default(),
        boss::Boss::new(t),
    ));
}

fn begin_tick(mut q: Query<(&Body, &mut PrevBody, &mut Action)>) {
    for (b, mut p, mut a) in &mut q {
        p.pos = b.pos;
        p.yaw = b.yaw;
        a.executed = false;
    }
}

/// Avance le compteur des actions qui ont exécuté une frame ce tick.
fn advance_actions(mut q: Query<&mut Action>) {
    for mut a in &mut q {
        if a.executed && a.mv.is_some() {
            a.tick += 1;
        }
    }
}

fn end_tick(mut tick: ResMut<SimTick>) {
    tick.0 = tick.0.wrapping_add(1);
}

/// Hash de l'état de simulation : sert au test de déterminisme (et plus tard à la détection
/// de désynchronisation en réseau).
pub fn state_hash(world: &mut World) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    world.resource::<SimTick>().0.hash(&mut h);
    world.resource::<rng::SimRng>().hash(&mut h);
    world.resource::<Encounter>().hash(&mut h);
    let mut q = world.query::<(&Body, &Health, &Action)>();
    for (b, hp, a) in q.iter(world) {
        for f in [b.pos.x, b.pos.y, b.pos.z, b.yaw, hp.cur] {
            f.to_bits().hash(&mut h);
        }
        a.mv.hash(&mut h);
        a.tick.hash(&mut h);
    }
    let mut qp = world.query::<&player::Player>();
    for p in qp.iter(world) {
        p.stamina.to_bits().hash(&mut h);
        p.regain.to_bits().hash(&mut h);
        p.special.to_bits().hash(&mut h);
        p.inventory.hash(&mut h);
        p.embers.hash(&mut h);
        (p.picked, p.slain, p.found, p.checkpoint).hash(&mut h);
        p.dropped.map(|d| (d.at.map(f32::to_bits), d.embers)).hash(&mut h);
        (p.airborne, p.air_vy.to_bits()).hash(&mut h);
    }
    let mut qb = world.query::<&boss::Boss>();
    for b in qb.iter(world) {
        b.stagger.to_bits().hash(&mut h);
        b.phase.hash(&mut h);
    }
    let mut qe = world.query::<&enemy::Enemy>();
    for e in qe.iter(world) {
        (e.spawn, e.state, e.target, e.idle).hash(&mut h);
        e.poise.to_bits().hash(&mut h);
    }
    h.finish()
}
