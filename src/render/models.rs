//! Chargement des modèles, création des visuels des combattants et pilotage des animations
//! à partir de l'état de la simulation.

use std::collections::HashMap;

use bevy::animation::AnimationPlayer;
use bevy::camera::visibility::RenderLayers;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use serde::Deserialize;

use super::ps1::{Ps1Lighting, Ps1Material, PointLightPs1, TintMeshes, set_tint};
use super::{AppState, Interp, LocalPlayer};
use crate::sim::boss::{Boss, unblockable_pending};
use crate::sim::data::{BossMove, MoveDef, MoveRef, Tuning};
use crate::sim::enemy::{EState, Enemy};
use crate::sim::fighter::{Action, Body, Health, Hitstop, PrevBody};
use crate::sim::player::{PState, Player};
use crate::sim::{SimEntity, math, world};

/// Noms des armes, dans l'ordre de `weapons.ron` (suffixe des clips et nom des modèles).
pub const WEAPON_MODELS: [&str; 2] = ["rapier", "greatsword"];

/// Modèles des ennemis (`EnemyDef::model`) et leurs marqueurs d'animation.
pub const ENEMY_MODELS: [(&str, &str); 2] = [
    ("hound", include_str!("../../assets/models/hound.anim.json")),
    ("puppet", include_str!("../../assets/models/puppet.anim.json")),
];

/// Modèles des autres boss (`bosses.ron`), en plus de l'Automate (`boss`) et des modèles
/// d'ennemis (qu'un boss peut réutiliser : les chiens du boucher).
pub const BOSS_MODELS: [(&str, &str); 7] = [
    ("dragon", include_str!("../../assets/models/dragon.anim.json")),
    ("horned_butcher", include_str!("../../assets/models/horned_butcher.anim.json")),
    ("lamplighter", include_str!("../../assets/models/lamplighter.anim.json")),
    ("anvil", include_str!("../../assets/models/anvil.anim.json")),
    ("spine_beast", include_str!("../../assets/models/spine_beast.anim.json")),
    ("marionette", include_str!("../../assets/models/marionette.anim.json")),
    ("giant", include_str!("../../assets/models/giant.anim.json")),
];

#[derive(Deserialize, Clone, Debug)]
pub struct Marker {
    pub frames: f32,
    pub marks: Vec<f32>,
    #[serde(rename = "loop")]
    pub looping: bool,
}

#[derive(Resource)]
pub struct GameAssets {
    pub player: Handle<Gltf>,
    pub boss: Handle<Gltf>,
    pub arena: Handle<Gltf>,
    pub weapons: Vec<Handle<Gltf>>,
    pub enemies: HashMap<String, Handle<Gltf>>,
    pub bosses: HashMap<String, Handle<Gltf>>,
}

impl GameAssets {
    /// Modèle d'un boss (l'Automate, un modèle propre, ou un modèle d'ennemi réutilisé).
    fn boss_model(&self, model: &str) -> Option<&Handle<Gltf>> {
        if model == "boss" {
            return Some(&self.boss);
        }
        self.bosses.get(model).or_else(|| self.enemies.get(model))
    }
}

/// Graphe d'animation d'un modèle + correspondance nom de clip → nœud.
pub struct ModelAnims {
    pub graph: Handle<AnimationGraph>,
    pub nodes: HashMap<String, AnimationNodeIndex>,
    pub markers: HashMap<String, Marker>,
}

#[derive(Resource)]
pub struct Models {
    pub player: ModelAnims,
    pub boss: ModelAnims,
    pub enemies: HashMap<String, ModelAnims>,
    pub bosses: HashMap<String, ModelAnims>,
}

impl Models {
    pub fn boss_anims(&self, model: &str) -> Option<&ModelAnims> {
        if model == "boss" {
            return Some(&self.boss);
        }
        self.bosses.get(model).or_else(|| self.enemies.get(model))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualKind {
    Player,
    Boss,
    Enemy,
    Weapon,
    Arena,
    /// Cadavre du joueur, là où il a laissé ses braises.
    Corpse,
}

/// Racine d'une scène glTF instanciée, et l'entité de simulation qu'elle représente.
#[derive(Component, Clone, Copy)]
pub struct VisualScene {
    pub owner: Entity,
    pub kind: VisualKind,
}

/// État du pilotage d'animation, posé sur l'entité de simulation.
#[derive(Component, Default)]
pub struct AnimDriver {
    pub player: Option<Entity>,
    pub current: Option<String>,
    pub phase: f32,
}

#[derive(Component)]
pub struct WeaponVisual {
    pub owner: Entity,
    pub index: u8,
}

/// Flash de teinte (impact) sur le boss et les ennemis.
#[derive(Component, Default)]
pub struct TintFlash {
    pub white: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    Brazier,
    /// Brasier du checkpoint (index) : une braise rougeoyante tant qu'il n'est pas ranimé.
    Checkpoint(u8),
    /// Réverbère.
    Lamp,
}

/// Lumières de la scène (repérées par les empties `light_*` du décor).
#[derive(Resource, Default)]
pub struct SceneLights(pub Vec<(Vec3, LightKind)>);

/// Lueur d'un objet à ramasser (`level.pickups[i]`).
#[derive(Component)]
struct PickupGlow(usize);

/// Braises d'un brasier de checkpoint (index) : éteintes tant qu'il n'est pas ranimé.
#[derive(Component)]
struct CheckpointCoals(u8);

/// Cadavre du joueur local, posé sur ses braises perdues (position et orientation).
#[derive(Component)]
struct Corpse(Vec3);

/// Brume qui ferme l'arène pendant le combat.
#[derive(Component)]
struct FogGate;

pub struct ModelsPlugin;

impl Plugin for ModelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneLights>()
            .add_systems(Startup, load_assets)
            .add_systems(Update, wait_for_assets.run_if(in_state(AppState::Loading)))
            .add_systems(OnExit(AppState::Loading), (spawn_arena, spawn_fog_gate, spawn_pickups))
            .add_systems(
                Update,
                (
                    attach_visuals,
                    drive_player_anims,
                    drive_boss_anims,
                    drive_enemy_anims,
                    weapon_visibility,
                    foe_tint,
                    weapon_glow,
                    fog_gate,
                    pickup_glows,
                    checkpoint_coals,
                    corpse,
                )
                    .run_if(in_state(AppState::Playing))
                    .after(super::interpolate),
            )
            .add_systems(Update, flicker.run_if(not(in_state(AppState::Loading))))
            .add_observer(on_scene_ready);
    }
}

fn load_assets(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(GameAssets {
        player: server.load("models/player.glb"),
        boss: server.load("models/boss.glb"),
        arena: server.load("models/arena.glb"),
        weapons: WEAPON_MODELS.iter().map(|w| server.load(format!("models/{w}.glb"))).collect(),
        enemies: ENEMY_MODELS.iter().map(|(m, _)| (m.to_string(), server.load(format!("models/{m}.glb")))).collect(),
        bosses: BOSS_MODELS.iter().map(|(m, _)| (m.to_string(), server.load(format!("models/{m}.glb")))).collect(),
    });
}

fn build_anims(gltf: &Gltf, markers_json: &str, graphs: &mut Assets<AnimationGraph>) -> ModelAnims {
    let names: Vec<String> = gltf.named_animations.keys().map(|k| k.to_string()).collect();
    let (graph, idx) =
        AnimationGraph::from_clips(names.iter().map(|n| gltf.named_animations[n.as_str()].clone()));
    let markers: HashMap<String, Marker> =
        serde_json::from_str(markers_json).expect("marqueurs d'animation invalides");
    ModelAnims { graph: graphs.add(graph), nodes: names.into_iter().zip(idx).collect(), markers }
}

fn wait_for_assets(
    mut commands: Commands,
    assets: Res<GameAssets>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let all = [&assets.player, &assets.boss, &assets.arena]
        .into_iter()
        .chain(assets.weapons.iter())
        .chain(assets.enemies.values())
        .chain(assets.bosses.values());
    for h in all {
        if !server.is_loaded_with_dependencies(h) {
            return;
        }
    }
    let (Some(p), Some(b)) = (gltfs.get(&assets.player), gltfs.get(&assets.boss)) else { return };
    let mut enemies = HashMap::new();
    for (m, markers) in ENEMY_MODELS {
        let Some(g) = gltfs.get(&assets.enemies[m]) else { return };
        enemies.insert(m.to_string(), build_anims(g, markers, &mut graphs));
    }
    let mut bosses = HashMap::new();
    for (m, markers) in BOSS_MODELS {
        let Some(g) = gltfs.get(&assets.bosses[m]) else { return };
        bosses.insert(m.to_string(), build_anims(g, markers, &mut graphs));
    }
    commands.insert_resource(Models {
        player: build_anims(p, include_str!("../../assets/models/player.anim.json"), &mut graphs),
        boss: build_anims(b, include_str!("../../assets/models/boss.anim.json"), &mut graphs),
        enemies,
        bosses,
    });
    next.set(AppState::Title);
}

fn scene_of(gltfs: &Assets<Gltf>, h: &Handle<Gltf>) -> Handle<bevy::world_serialization::WorldAsset> {
    gltfs.get(h).and_then(|g| g.default_scene.clone()).expect("scène glTF manquante")
}

fn spawn_arena(mut commands: Commands, assets: Res<GameAssets>, gltfs: Res<Assets<Gltf>>) {
    let e = commands.spawn_empty().id();
    commands.entity(e).insert((
        WorldAssetRoot(scene_of(&gltfs, &assets.arena)),
        VisualScene { owner: e, kind: VisualKind::Arena },
        Transform::default(),
    ));
}

/// Ajoute les visuels aux entités de simulation nouvellement créées.
#[allow(clippy::type_complexity)]
fn attach_visuals(
    mut commands: Commands,
    assets: Res<GameAssets>,
    tuning: Res<Tuning>,
    gltfs: Res<Assets<Gltf>>,
    new: Query<(Entity, &Body, Option<&Player>, Option<&Boss>, Option<&Enemy>), (With<SimEntity>, Without<Interp>)>,
) {
    for (e, body, player, boss, enemy) in &new {
        let (kind, handle, scale) = if let Some(b) = boss {
            let def = b.def(&tuning);
            let Some(h) = assets.boss_model(&def.model) else { continue };
            (VisualKind::Boss, h, def.scale)
        } else if let Some(en) = enemy {
            let def = &tuning.enemies[en.kind as usize];
            let Some(h) = assets.enemies.get(&def.model) else { continue };
            (VisualKind::Enemy, h, def.scale)
        } else {
            (VisualKind::Player, &assets.player, 1.0)
        };
        let mut ec = commands.entity(e);
        ec.insert((
            Interp { pos: body.pos, yaw: body.yaw },
            Transform::from_translation(body.pos).with_rotation(Quat::from_rotation_y(body.yaw)),
            Visibility::default(),
            AnimDriver::default(),
        ));
        if boss.is_some() || enemy.is_some() {
            ec.insert(TintFlash::default());
        }
        if player.is_some_and(|p| p.id == 0) {
            ec.insert(LocalPlayer);
        }
        ec.with_child((
            WorldAssetRoot(scene_of(&gltfs, handle)),
            VisualScene { owner: e, kind },
            Transform::from_scale(Vec3::splat(scale)),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn on_scene_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    scenes: Query<&VisualScene>,
    children: Query<&Children>,
    names: Query<&Name>,
    transforms: Query<&Transform>,
    anim_players: Query<(), With<AnimationPlayer>>,
    meshes: Query<(), With<Mesh3d>>,
    mut drivers: Query<&mut AnimDriver>,
    models: Option<Res<Models>>,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
    tuning: Res<Tuning>,
    enemies: Query<&Enemy>,
    bosses: Query<&Boss>,
    mut lights: ResMut<SceneLights>,
) {
    let root = ready.entity;
    let Ok(vs) = scenes.get(root) else { return };
    let Some(models) = models else { return };
    for d in children.iter_descendants(root) {
        // Le décor est aussi filmé par la caméra d'aperçu du menu de voyage.
        if vs.kind == VisualKind::Arena && meshes.contains(d) {
            commands.entity(d).insert(RenderLayers::from_layers(&[0, super::preview::PREVIEW_LAYER]));
        }
        if anim_players.contains(d) {
            let graph = match vs.kind {
                VisualKind::Player | VisualKind::Corpse => models.player.graph.clone(),
                VisualKind::Boss => {
                    let Some(m) = bosses.get(vs.owner).ok().and_then(|b| models.boss_anims(&b.def(&tuning).model)) else { continue };
                    m.graph.clone()
                }
                VisualKind::Enemy => {
                    let Ok(en) = enemies.get(vs.owner) else { continue };
                    let Some(m) = models.enemies.get(&tuning.enemies[en.kind as usize].model) else { continue };
                    m.graph.clone()
                }
                _ => continue,
            };
            commands.entity(d).insert(AnimationGraphHandle(graph));
            if let Ok(mut drv) = drivers.get_mut(vs.owner) {
                drv.player = Some(d);
                drv.current = None;
            }
        }
        let Ok(name) = names.get(d) else { continue };
        // Points verrouillables d'un grand boss qui suivent l'animation (tête, pattes…).
        if vs.kind == VisualKind::Boss
            && let Ok(b) = bosses.get(vs.owner)
            && let Some(i) = b.def(&tuning).parts.iter().filter(|p| p.lock).position(|p| p.bone.as_deref() == Some(name.as_str()))
        {
            commands.entity(d).insert(super::camera::PartBone { owner: vs.owner, part: i as u8 });
        }
        if vs.kind == VisualKind::Arena
            && let Some(i) = name.as_str().strip_prefix("checkpoint_").and_then(|r| r.strip_suffix("_coals"))
        {
            commands.entity(d).insert(CheckpointCoals(i.parse().unwrap_or(0)));
        }
        if vs.kind == VisualKind::Player && name.as_str() == "grip_R" {
            for (i, w) in assets.weapons.iter().enumerate() {
                commands.entity(d).with_child((
                    WorldAssetRoot(scene_of(&gltfs, w)),
                    VisualScene { owner: vs.owner, kind: VisualKind::Weapon },
                    WeaponVisual { owner: vs.owner, index: i as u8 },
                    Visibility::Hidden,
                ));
            }
        }
        if vs.kind == VisualKind::Arena
            && let Some(rest) = name.as_str().strip_prefix("light_")
            && let Ok(t) = transforms.get(d)
        {
            let kind = if let Some(i) = rest.strip_prefix("checkpoint_") {
                LightKind::Checkpoint(i.parse().unwrap_or(0))
            } else if rest.starts_with("lamp") {
                LightKind::Lamp
            } else {
                LightKind::Brazier
            };
            lights.0.push((t.translation, kind));
        }
    }
}

/// Convertit un tick de simulation en temps d'animation, en recalant les marqueurs
/// (début/fin des coups) si le tuning a changé depuis la génération des animations.
fn action_time(def: &MoveDef, tick: f32, m: &Marker) -> f32 {
    let mut sim = vec![0.0];
    for h in &def.hits {
        sim.push(h.start as f32);
        sim.push(h.end as f32);
    }
    sim.push(def.total as f32);
    let monotonic = |v: &[f32]| v.windows(2).all(|w| w[1] >= w[0]);
    let frame = if sim.len() == m.marks.len() && monotonic(&sim) && monotonic(&m.marks) {
        let mut f = m.marks[m.marks.len() - 1];
        for i in 0..sim.len() - 1 {
            if tick <= sim[i + 1] {
                let span = (sim[i + 1] - sim[i]).max(1e-3);
                let k = ((tick - sim[i]) / span).clamp(0.0, 1.0);
                f = m.marks[i] + (m.marks[i + 1] - m.marks[i]) * k;
                break;
            }
        }
        f
    } else {
        tick / def.total.max(1) as f32 * m.frames
    };
    frame.clamp(0.0, m.frames) / 60.0
}

fn set_clip(
    drv: &mut AnimDriver,
    anims: &ModelAnims,
    players: &mut Query<&mut AnimationPlayer>,
    name: &str,
    time: f32,
) {
    let Some(pe) = drv.player else { return };
    let Ok(mut ap) = players.get_mut(pe) else { return };
    let Some(&node) = anims.nodes.get(name) else {
        warn_once!("clip d'animation manquant : {name}");
        return;
    };
    if drv.current.as_deref() != Some(name) {
        ap.stop_all();
        ap.play(node).pause();
        drv.current = Some(name.to_string());
    }
    if let Some(a) = ap.animation_mut(node) {
        a.seek_to(time);
    }
}

fn loop_time(drv: &mut AnimDriver, anims: &ModelAnims, name: &str, dt: f32, rate: f32) -> f32 {
    if drv.current.as_deref() != Some(name) {
        drv.phase = 0.0;
    }
    let dur = anims.markers.get(name).map(|m| m.frames / 60.0).unwrap_or(1.0).max(0.01);
    drv.phase = (drv.phase + dt * rate) % dur;
    drv.phase
}

#[allow(clippy::type_complexity)]
fn drive_player_anims(
    clock: Res<super::AnimClock>,
    tuning: Res<Tuning>,
    models: Res<Models>,
    mut q: Query<(&Player, &Action, &Body, &PrevBody, &Hitstop, &mut AnimDriver)>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let t = &*tuning;
    let anims = &models.player;
    let dt = clock.dt;
    for (p, action, body, prev, hitstop, mut drv) in &mut q {
        let weapon = WEAPON_MODELS[p.weapon as usize % WEAPON_MODELS.len()];
        let over = if hitstop.0 > 0 { 0.0 } else { clock.over };
        let speed = math::flat_len(body.pos - prev.pos) * 60.0;
        let (name, tm): (String, f32) = match p.state {
            // Chute : bras écartés, figé au début de la réaction aux gros coups.
            PState::Falling => ("hit_heavy".into(), 0.12),
            PState::Acting | PState::Dead => {
                let Some(def) = action.def(t) else {
                    // Mort : rester sur la dernière frame.
                    let m = &anims.markers["death"];
                    set_clip(&mut drv, anims, &mut players, "death", m.frames / 60.0);
                    continue;
                };
                let tick = (action.tick as f32 + over - 1.0).max(0.0);
                let m = anims.markers.get(&def.anim);
                (def.anim.clone(), m.map(|m| action_time(def, tick, m)).unwrap_or(tick / 60.0))
            }
            PState::Charging => {
                let w = &t.weapons[p.weapon as usize];
                let frames = anims.markers.get(&w.charge_anim).map(|m| m.frames).unwrap_or(1.0);
                let k = (p.charge as f32 / w.charge_ticks.max(1) as f32).min(1.0);
                (w.charge_anim.clone(), k * frames / 60.0)
            }
            PState::Guard => {
                if speed > 0.3 {
                    let n = "guard_walk";
                    (n.into(), loop_time(&mut drv, anims, n, dt, speed / 1.8))
                } else {
                    (("guard").into(), loop_time(&mut drv, anims, "guard", dt, 1.0))
                }
            }
            PState::Free if p.airborne => {
                let frames = anims.markers.get("jump").map_or(1.0, |m| m.frames);
                ("jump".into(), ((p.air_ticks as f32 + over) / 60.0).min(frames / 60.0))
            }
            PState::Free => {
                let (n, rate) = if speed < 0.25 {
                    (format!("idle_{weapon}"), 1.0)
                } else if p.sprinting {
                    ("sprint".into(), speed / 6.4)
                } else if speed < 2.4 {
                    ("walk".into(), speed / 1.6)
                } else {
                    ("run".into(), speed / 4.2)
                };
                let tm = loop_time(&mut drv, anims, &n, dt, rate);
                (n, tm)
            }
        };
        set_clip(&mut drv, anims, &mut players, &name, tm);
    }
}

fn drive_boss_anims(
    clock: Res<super::AnimClock>,
    tuning: Res<Tuning>,
    models: Res<Models>,
    mut q: Query<(&Boss, &Action, &Body, &PrevBody, &Hitstop, &mut AnimDriver)>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let t = &*tuning;
    let dt = clock.dt;
    for (boss, action, body, prev, hitstop, mut drv) in &mut q {
        let bd = boss.def(t);
        let Some(anims) = models.boss_anims(&bd.model) else { continue };
        let over = if hitstop.0 > 0 { 0.0 } else { clock.over };
        let (name, tm) = if let Some(def) = action.def(t) {
            let tick = (action.tick as f32 + over - 1.0).max(0.0);
            let m = anims.markers.get(&def.anim);
            (def.anim.clone(), m.map(|m| action_time(def, tick, m)).unwrap_or(tick / 60.0))
        } else {
            // Vitesse ramenée à l'échelle du modèle (les pas sont animés à l'échelle 1).
            let speed = math::flat_len(body.pos - prev.pos) * 60.0 / bd.scale;
            let walk = bd.walk_speed / bd.scale;
            // Pivot sur place (radians/s) : il piétine en se tournant.
            let turn = math::wrap(body.yaw - prev.yaw).abs() * 60.0;
            if speed > 2.5 && anims.nodes.contains_key("run") {
                // Au galop (les chiens) : le cycle de course est animé pour ~4,5 m/s.
                ("run".to_string(), loop_time(&mut drv, anims, "run", dt, speed / 4.5))
            } else if speed > 0.2 || turn > 0.5 {
                // Les pas reculés (lanceur de sorts) jouent la marche à l'envers ; de côté (ou en
                // pivotant sur place), à l'endroit : sinon le sens bascule d'une frame à l'autre.
                let step = body.pos - prev.pos;
                let back = math::forward(body.yaw).dot(step) < -0.7 * math::flat_len(step);
                let rate = (speed / walk.max(0.1)).max(turn * 0.6);
                let tm = loop_time(&mut drv, anims, "walk", dt, rate);
                let dur = anims.markers.get("walk").map_or(1.0, |m| m.frames / 60.0);
                ("walk".to_string(), if back { dur - tm } else { tm })
            } else {
                ("idle".to_string(), loop_time(&mut drv, anims, "idle", dt, 1.0))
            }
        };
        set_clip(&mut drv, anims, &mut players, &name, tm);
    }
}

#[allow(clippy::type_complexity)]
fn drive_enemy_anims(
    clock: Res<super::AnimClock>,
    tuning: Res<Tuning>,
    models: Res<Models>,
    mut q: Query<(&Enemy, &Action, &Body, &PrevBody, &Hitstop, &mut AnimDriver)>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let t = &*tuning;
    let dt = clock.dt;
    for (e, action, body, prev, hitstop, mut drv) in &mut q {
        let def = &t.enemies[e.kind as usize];
        let Some(anims) = models.enemies.get(&def.model) else { continue };
        let over = if hitstop.0 > 0 { 0.0 } else { clock.over };
        let (name, tm) = if let Some(mv) = action.def(t) {
            let tick = (action.tick as f32 + over - 1.0).max(0.0);
            let m = anims.markers.get(&mv.anim);
            (mv.anim.clone(), m.map(|m| action_time(mv, tick, m)).unwrap_or(tick / 60.0))
        } else {
            let speed = math::flat_len(body.pos - prev.pos) * 60.0;
            let (n, rate) = if speed > def.walk_speed * 1.6 && anims.nodes.contains_key("run") {
                ("run", speed / def.run_speed)
            } else if speed > 0.2 {
                ("walk", speed / def.walk_speed)
            } else if e.state == EState::Asleep && anims.nodes.contains_key("sleep") {
                ("sleep", 1.0)
            } else {
                ("idle", 1.0)
            };
            (n.to_string(), loop_time(&mut drv, anims, n, dt, rate))
        };
        set_clip(&mut drv, anims, &mut players, &name, tm);
    }
}

fn weapon_visibility(owners: Query<&Player>, mut q: Query<(&WeaponVisual, &mut Visibility)>) {
    for (w, mut v) in &mut q {
        let show = owners.get(w.owner).is_ok_and(|p| p.weapon == w.index);
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

/// Teinte des adversaires : lueur rouge pendant l'anticipation d'un coup imparable qu'aucun
/// cercle n'annonce (attaque furie, jet de feu), teinte propre à certains ennemis, flash blanc
/// à l'impact.
#[allow(clippy::type_complexity)]
fn foe_tint(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<Tuning>,
    mut foes: Query<(Entity, &Action, &Health, &mut TintFlash, Option<&Enemy>, Option<&Boss>)>,
    children: Query<&Children>,
    mut meshes: TintMeshes,
    mut materials: ResMut<Assets<Ps1Material>>,
) {
    let t = time.elapsed_secs();
    for (e, action, health, mut flash, enemy, boss) in &mut foes {
        flash.white = (flash.white - time.delta_secs() * 6.0).max(0.0);
        let own = enemy.and_then(|en| tuning.enemies[en.kind as usize].tint).or(boss.and_then(|b| b.def(&tuning).tint));
        let mut tint = own.map_or(Vec4::ZERO, Vec4::from);
        if unblockable_pending(action, &tuning, boss.map(|b| b.def(&tuning))) {
            let k = 0.35 + 0.25 * (t * 18.0).sin();
            tint = Vec4::new(1.0, 0.05, 0.02, k);
        }
        if matches!(action.mv, Some(MoveRef::Boss(_, BossMove::Groggy))) {
            tint = Vec4::new(1.0, 0.85, 0.4, 0.12 + 0.08 * (t * 6.0).sin());
        }
        if health.dead() {
            tint = Vec4::new(0.0, 0.0, 0.0, 0.35);
        }
        if flash.white > 0.0 {
            tint = Vec4::new(1.0, 1.0, 1.0, flash.white * 0.6);
        }
        set_tint(&mut commands, e, tint, &children, &mut meshes, &mut materials);
    }
}

/// Résine ardente : la lame rougeoie tant que l'effet dure.
fn weapon_glow(
    mut commands: Commands,
    time: Res<Time>,
    owners: Query<&Player>,
    weapons: Query<(Entity, &WeaponVisual)>,
    children: Query<&Children>,
    mut meshes: TintMeshes,
    mut materials: ResMut<Assets<Ps1Material>>,
) {
    let t = time.elapsed_secs();
    for (e, w) in &weapons {
        let resin = owners.get(w.owner).map_or(0, |p| p.resin_ticks);
        let tint = if resin > 0 {
            // Les dernières secondes, la lueur vacille.
            let fading = resin < 300 && (t * 8.0).sin() < 0.0;
            Vec4::new(1.0, 0.45, 0.1, if fading { 0.15 } else { 0.4 + 0.1 * (t * 11.0).sin() })
        } else {
            Vec4::ZERO
        };
        set_tint(&mut commands, e, tint, &children, &mut meshes, &mut materials);
    }
}

/// Lumières ponctuelles : le shader n'en gère que 4, on garde les plus proches de la caméra.
/// Le vacillement est calculé par le shader : la liste ne change que si les lumières retenues
/// changent (et seulement alors, tous les matériaux sont mis à jour).
fn flicker(
    scene: Res<SceneLights>,
    rig: Res<super::camera::CameraRig>,
    players: Query<&Player, With<LocalPlayer>>,
    mut lighting: ResMut<Ps1Lighting>,
) {
    let found = players.single().map_or(u32::MAX, |p| p.found);
    let mut lights: Vec<PointLightPs1> = scene
        .0
        .iter()
        .map(|(p, kind)| {
            let (radius, color, intensity, flicker) = match kind {
                // Brasier ranimé : lumière dorée qui « respire » lentement. Sinon, une
                // braise rouge à peine visible.
                LightKind::Checkpoint(c) if found & (1 << c) != 0 => {
                    (8.0, Vec3::new(1.0, 0.82, 0.45), 1.2, Vec4::new(0.15, 1.6, 0.0, 0.0))
                }
                LightKind::Checkpoint(_) => (3.0, Vec3::new(1.0, 0.3, 0.1), 0.5, Vec4::new(0.2, 0.9, 0.0, 0.0)),
                // Réverbère : lumière de gaz pâle, presque fixe.
                LightKind::Lamp => (7.5, Vec3::new(0.95, 0.88, 0.62), 0.95, Vec4::new(0.04, 3.0, 0.0, 0.0)),
                LightKind::Brazier => (9.0, Vec3::new(1.0, 0.55, 0.22), 1.1, Vec4::new(0.15, 9.0, 0.1, 23.0)),
            };
            PointLightPs1 { pos: *p, radius, color, intensity, flicker }
        })
        .collect();
    lights.sort_by(|a, b| a.pos.distance_squared(rig.focus).total_cmp(&b.pos.distance_squared(rig.focus)));
    lights.truncate(4);
    // Ordre stable : la liste ne change que si les lumières retenues changent.
    lights.sort_by(|a, b| (a.pos.x, a.pos.z).partial_cmp(&(b.pos.x, b.pos.z)).unwrap_or(std::cmp::Ordering::Equal));
    if lighting.lights != lights {
        lighting.lights = lights;
    }
}

fn spawn_fog_gate(
    mut commands: Commands,
    tuning: Res<Tuning>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<Ps1Material>>,
) {
    let a = &tuning.arena;
    let mut m = Ps1Material::unlit(Color::srgba(0.55, 0.58, 0.68, 0.3));
    m.alpha_mode = AlphaMode::Blend;
    // Deux voiles légèrement décalés, pour un peu d'épaisseur.
    let mesh = meshes.add(Rectangle::new(a.gate_half_width * 2.0 + 0.6, 4.2));
    let center = world::fog_gate(a) + Vec3::Y * 2.1;
    for (dz, flip) in [(0.0, false), (-0.25, true)] {
        let rot = if flip { Quat::from_rotation_y(std::f32::consts::PI) } else { Quat::IDENTITY };
        commands.spawn((
            FogGate,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mats.add(m.clone())),
            Transform::from_translation(center + Vec3::Z * dz).with_rotation(rot),
            Visibility::Hidden,
        ));
    }
}

/// La brume n'apparaît que pendant le combat, et ondule doucement.
fn fog_gate(
    time: Res<Time>,
    enc: Res<crate::sim::encounter::Encounter>,
    mut q: Query<(&mut Visibility, &MeshMaterial3d<Ps1Material>), With<FogGate>>,
    mut mats: ResMut<Assets<Ps1Material>>,
) {
    let t = time.elapsed_secs();
    for (i, (mut vis, h)) in q.iter_mut().enumerate() {
        let want = if enc.active { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if enc.active {
            if let Some(mut m) = mats.get_mut(&h.0) {
                m.params.base_color.w = 0.26 + 0.08 * (t * 1.3 + i as f32 * 2.0).sin();
            }
        }
    }
}

/// Objets à ramasser : une lueur blanche qui palpite au ras du sol, comme dans les souls-like
/// (on ne sait pas ce que c'est avant de l'avoir ramassé) : un cœur brillant dans un halo, et
/// des étincelles qui tournoient en montant (`fx`). Tout brille à travers le brouillard : on
/// les repère de loin.
fn spawn_pickups(
    mut commands: Commands,
    tuning: Res<Tuning>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<Ps1Material>>,
) {
    let core = meshes.add(Cuboid::new(0.11, 0.11, 0.11));
    let halo = meshes.add(Cuboid::new(0.28, 0.28, 0.28));
    let mut c = Ps1Material::unlit(Color::srgb(1.0, 0.97, 0.85));
    c.params.emissive = Vec4::new(0.8, 0.75, 0.6, 0.0);
    let core_mat = mats.add(c);
    let mut h = Ps1Material::unlit(Color::srgba(0.95, 0.85, 0.55, 0.3));
    h.alpha_mode = AlphaMode::Add;
    let halo_mat = mats.add(h);
    for i in 0..tuning.level.pickups.len() {
        let pos = crate::sim::encounter::pickup_pos(&tuning, i) + Vec3::Y * 0.3;
        commands
            .spawn((PickupGlow(i), Transform::from_translation(pos), Visibility::Hidden))
            .with_children(|c| {
                c.spawn((Mesh3d(core.clone()), MeshMaterial3d(core_mat.clone()), Transform::default()));
                c.spawn((Mesh3d(halo.clone()), MeshMaterial3d(halo_mat.clone()), Transform::default()));
            });
    }
}

fn pickup_glows(
    time: Res<Time>,
    players: Query<&Player, With<LocalPlayer>>,
    mut q: Query<(&PickupGlow, &mut Visibility, &mut Transform, &Children)>,
    mut parts: Query<&mut Transform, Without<PickupGlow>>,
) {
    let picked = players.single().map_or(u64::MAX, |p| p.picked);
    let t = time.elapsed_secs();
    for (g, mut vis, mut tf, children) in &mut q {
        let want = if picked & (1u64 << g.0) == 0 { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let ph = t * 2.2 + g.0 as f32 * 1.3;
        tf.rotation = Quat::from_rotation_y(t * 1.5) * Quat::from_rotation_x(0.6);
        for (k, c) in children.iter().enumerate() {
            if let Ok(mut ct) = parts.get_mut(c) {
                let s = if k == 0 { 0.9 + 0.2 * ph.sin() } else { 0.7 + 0.5 * (0.5 + 0.5 * (ph * 0.7).sin()) };
                ct.scale = Vec3::splat(s);
                ct.translation.y = 0.05 * (ph * 0.5).sin();
            }
        }
    }
}

/// Les braises d'un brasier pas encore ranimé ne sont qu'un tas de charbon à peine rougeoyant.
fn checkpoint_coals(
    mut commands: Commands,
    players: Query<&Player, With<LocalPlayer>>,
    coals: Query<(Entity, &CheckpointCoals)>,
    children: Query<&Children>,
    mut meshes: TintMeshes,
    mut materials: ResMut<Assets<Ps1Material>>,
) {
    let found = players.single().map_or(u32::MAX, |p| p.found);
    for (e, c) in &coals {
        let tint = if found & (1 << c.0) != 0 { Vec4::ZERO } else { Vec4::new(0.12, 0.03, 0.02, 0.8) };
        set_tint(&mut commands, e, tint, &children, &mut meshes, &mut materials);
    }
}

/// Le cadavre du joueur : son modèle figé à la fin de l'animation de mort, nimbé de vert, là où il a
/// laissé ses braises (les braises qui en montent sont dans `fx`). Il disparaît quand on les
/// récupère, ou quand on meurt avant (elles sont alors perdues).
#[allow(clippy::too_many_arguments)]
fn corpse(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
    models: Res<Models>,
    players: Query<&Player, With<LocalPlayer>>,
    mut corpses: Query<(Entity, &Corpse, &mut AnimDriver)>,
    mut anim_players: Query<&mut AnimationPlayer>,
    children: Query<&Children>,
    mut meshes: TintMeshes,
    mut materials: ResMut<Assets<Ps1Material>>,
) {
    let want = players.single().ok().and_then(|p| p.dropped).map(|d| d.pos());
    let mut have = false;
    for (e, c, mut drv) in &mut corpses {
        if want != Some(c.0) {
            commands.entity(e).despawn();
            continue;
        }
        have = true;
        let anims = &models.player;
        if let Some(m) = anims.markers.get("death") {
            set_clip(&mut drv, anims, &mut anim_players, "death", m.frames / 60.0);
        }
        // Une lueur verte qui palpite, comme les taches de sang des souls-like : on le repère
        // de loin (les lueurs qui en montent sont dans `fx`).
        let glow = 0.5 + 0.5 * (time.elapsed_secs() * 2.2).sin();
        let tint = Vec4::new(0.1, 0.55, 0.18, 0.7).lerp(Vec4::new(0.4, 1.0, 0.45, 0.8), glow);
        set_tint(&mut commands, e, tint, &children, &mut meshes, &mut materials);
    }
    if let (Some(pos), false) = (want, have) {
        // Orientation pseudo-aléatoire mais stable (dépend de l'endroit).
        let yaw = (pos.x * 12.9898 + pos.z * 78.233).sin() * std::f32::consts::PI;
        let e = commands.spawn_empty().id();
        commands
            .entity(e)
            .insert((Corpse(pos), AnimDriver::default(), Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default()))
            .with_child((WorldAssetRoot(scene_of(&gltfs, &assets.player)), VisualScene { owner: e, kind: VisualKind::Corpse }, Transform::default()));
    }
}
