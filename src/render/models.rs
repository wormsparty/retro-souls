//! Chargement des modèles, création des visuels des combattants et pilotage des animations
//! à partir de l'état de la simulation.

use std::collections::HashMap;

use bevy::animation::AnimationPlayer;
use bevy::camera::visibility::RenderLayers;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use serde::Deserialize;

use super::ps1::{Ps1Lighting, Ps1Material, PointLightPs1};
use super::{AppState, Interp, LocalPlayer};
use crate::sim::boss::{Boss, fury_pending};
use crate::sim::data::{BossMove, MoveDef, MoveRef, Tuning};
use crate::sim::fighter::{Action, Body, Health, Hitstop, PrevBody};
use crate::sim::player::{PState, Player};
use crate::sim::{SimEntity, math};

/// Noms des armes, dans l'ordre de `weapons.ron` (suffixe des clips et nom des modèles).
pub const WEAPON_MODELS: [&str; 2] = ["rapier", "greatsword"];

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualKind {
    Player,
    Boss,
    Weapon,
    Arena,
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

/// Flash de teinte (impact) sur le boss.
#[derive(Component, Default)]
pub struct TintFlash {
    pub white: f32,
}

/// Lumières de la scène : braseros, et la lanterne du checkpoint (`true`).
#[derive(Resource, Default)]
pub struct BrazierLights(pub Vec<(Vec3, bool)>);

/// Brume qui ferme l'arène pendant le combat.
#[derive(Component)]
struct FogGate;

pub struct ModelsPlugin;

impl Plugin for ModelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BrazierLights>()
            .add_systems(Startup, load_assets)
            .add_systems(Update, wait_for_assets.run_if(in_state(AppState::Loading)))
            .add_systems(OnExit(AppState::Loading), (spawn_arena, spawn_fog_gate))
            .add_systems(
                Update,
                (attach_visuals, drive_player_anims, drive_boss_anims, weapon_visibility, boss_tint, fog_gate)
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
    let all = [&assets.player, &assets.boss, &assets.arena].into_iter().chain(assets.weapons.iter());
    for h in all {
        if !server.is_loaded_with_dependencies(h) {
            return;
        }
    }
    let (Some(p), Some(b)) = (gltfs.get(&assets.player), gltfs.get(&assets.boss)) else { return };
    commands.insert_resource(Models {
        player: build_anims(p, include_str!("../../assets/models/player.anim.json"), &mut graphs),
        boss: build_anims(b, include_str!("../../assets/models/boss.anim.json"), &mut graphs),
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
fn attach_visuals(
    mut commands: Commands,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
    new: Query<(Entity, &Body, Option<&Player>, Has<Boss>), (With<SimEntity>, Without<Interp>)>,
) {
    for (e, body, player, is_boss) in &new {
        let kind = if is_boss { VisualKind::Boss } else { VisualKind::Player };
        let handle = if is_boss { &assets.boss } else { &assets.player };
        let mut ec = commands.entity(e);
        ec.insert((
            Interp { pos: body.pos, yaw: body.yaw },
            Transform::from_translation(body.pos).with_rotation(Quat::from_rotation_y(body.yaw)),
            Visibility::default(),
            AnimDriver::default(),
        ));
        if is_boss {
            ec.insert(TintFlash::default());
        }
        if player.is_some_and(|p| p.id == 0) {
            ec.insert(LocalPlayer);
        }
        ec.with_child((WorldAssetRoot(scene_of(&gltfs, handle)), VisualScene { owner: e, kind }));
    }
}

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
    mut braziers: ResMut<BrazierLights>,
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
                VisualKind::Player => models.player.graph.clone(),
                VisualKind::Boss => models.boss.graph.clone(),
                _ => continue,
            };
            commands.entity(d).insert(AnimationGraphHandle(graph));
            if let Ok(mut drv) = drivers.get_mut(vs.owner) {
                drv.player = Some(d);
                drv.current = None;
            }
        }
        let Ok(name) = names.get(d) else { continue };
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
        if vs.kind == VisualKind::Arena && name.as_str().starts_with("light_") {
            if let Ok(t) = transforms.get(d) {
                braziers.0.push((t.translation, name.as_str() == "light_checkpoint"));
            }
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
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    tuning: Res<Tuning>,
    models: Res<Models>,
    mut q: Query<(&Player, &Action, &Body, &PrevBody, &Hitstop, &mut AnimDriver)>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let t = &*tuning;
    let anims = &models.player;
    let dt = time.delta_secs();
    for (p, action, body, prev, hitstop, mut drv) in &mut q {
        let weapon = WEAPON_MODELS[p.weapon as usize % WEAPON_MODELS.len()];
        let over = if hitstop.0 > 0 { 0.0 } else { fixed.overstep_fraction() };
        let speed = math::flat_len(body.pos - prev.pos) * 60.0;
        let (name, tm): (String, f32) = match p.state {
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
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    tuning: Res<Tuning>,
    models: Res<Models>,
    mut q: Query<(&Action, &Body, &PrevBody, &Hitstop, &mut AnimDriver), With<Boss>>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let t = &*tuning;
    let anims = &models.boss;
    let dt = time.delta_secs();
    for (action, body, prev, hitstop, mut drv) in &mut q {
        let over = if hitstop.0 > 0 { 0.0 } else { fixed.overstep_fraction() };
        let (name, tm) = if let Some(def) = action.def(t) {
            let tick = (action.tick as f32 + over - 1.0).max(0.0);
            let m = anims.markers.get(&def.anim);
            (def.anim.clone(), m.map(|m| action_time(def, tick, m)).unwrap_or(tick / 60.0))
        } else {
            let speed = math::flat_len(body.pos - prev.pos) * 60.0;
            if speed > 0.2 {
                ("walk".to_string(), loop_time(&mut drv, anims, "walk", dt, speed / 1.4))
            } else {
                ("idle".to_string(), loop_time(&mut drv, anims, "idle", dt, 1.0))
            }
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

/// Teinte du boss : lueur rouge pendant l'anticipation d'une attaque furie, flash blanc à l'impact.
fn boss_tint(
    time: Res<Time>,
    tuning: Res<Tuning>,
    mut bosses: Query<(Entity, &Action, &Health, &mut TintFlash), With<Boss>>,
    children: Query<&Children>,
    mats: Query<&MeshMaterial3d<Ps1Material>>,
    mut materials: ResMut<Assets<Ps1Material>>,
) {
    let t = time.elapsed_secs();
    for (e, action, health, mut flash) in &mut bosses {
        flash.white = (flash.white - time.delta_secs() * 6.0).max(0.0);
        let mut tint = Vec4::ZERO;
        if fury_pending(action, &tuning) {
            let k = 0.35 + 0.25 * (t * 18.0).sin();
            tint = Vec4::new(1.0, 0.05, 0.02, k);
        }
        if action.is(MoveRef::Boss(BossMove::Groggy)) {
            tint = Vec4::new(1.0, 0.85, 0.4, 0.12 + 0.08 * (t * 6.0).sin());
        }
        if health.dead() {
            tint = Vec4::new(0.0, 0.0, 0.0, 0.35);
        }
        if flash.white > 0.0 {
            tint = Vec4::new(1.0, 1.0, 1.0, flash.white * 0.6);
        }
        for d in children.iter_descendants(e) {
            if let Ok(h) = mats.get(d) {
                if let Some(mut m) = materials.get_mut(&h.0) {
                    m.params.tint = tint;
                }
            }
        }
    }
}

/// Lumières ponctuelles : le shader n'en gère que 4, on garde les plus proches de la caméra.
fn flicker(
    time: Res<Time>,
    braziers: Res<BrazierLights>,
    rig: Res<super::camera::CameraRig>,
    mut lighting: ResMut<Ps1Lighting>,
) {
    let t = time.elapsed_secs();
    let mut lights: Vec<PointLightPs1> = braziers
        .0
        .iter()
        .enumerate()
        .map(|(i, (p, checkpoint))| {
            let f = i as f32 * 1.7;
            if *checkpoint {
                // Lanterne du checkpoint : lumière dorée, qui « respire » lentement.
                PointLightPs1 {
                    pos: *p,
                    radius: 8.0,
                    color: Vec3::new(1.0, 0.82, 0.45),
                    intensity: 1.2 + 0.15 * (t * 1.6).sin(),
                }
            } else {
                PointLightPs1 {
                    pos: *p,
                    radius: 9.0,
                    color: Vec3::new(1.0, 0.55, 0.22),
                    intensity: 1.1 + 0.15 * (t * 9.0 + f).sin() + 0.1 * (t * 23.0 + f * 2.0).sin(),
                }
            }
        })
        .collect();
    lights.sort_by(|a, b| a.pos.distance_squared(rig.focus).total_cmp(&b.pos.distance_squared(rig.focus)));
    lights.truncate(4);
    lighting.lights = lights;
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
    let mesh = meshes.add(Rectangle::new(a.corridor_half_width * 2.0 + 0.6, 4.2));
    let center = crate::sim::encounter::fog_gate(a) + Vec3::Y * 2.1;
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
