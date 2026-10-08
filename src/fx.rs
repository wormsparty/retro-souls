//! Retours sensoriels déclenchés par les événements de la simulation : sons, étincelles,
//! tremblement de caméra, flash du boss, alertes au sol des attaques de zone. Et des particules
//! d'ambiance : braises et cendres des brasiers, braises laissées à la mort, eau de la fontaine.

use bevy::prelude::*;

use crate::render::AppState;
use crate::render::camera::CameraRig;
use crate::render::models::TintFlash;
use crate::render::ps1::Ps1Material;
use crate::sim::boss::{Boss, aoe_telegraph};
use crate::sim::data::Tuning;
use crate::sim::enemy::Enemy;
use crate::sim::fighter::{Action, Body};
use crate::sim::items::Item;
use crate::sim::{SimEvent, SimEvents, encounter, world};

#[derive(Resource)]
/// Sons chargés, et volume des effets (copié depuis les options à chaque frame).
pub struct Sounds(std::collections::HashMap<&'static str, Handle<AudioSource>>, f32);

#[derive(Resource)]
pub struct SparkAssets {
    mesh: Handle<Mesh>,
    perfect: Handle<Ps1Material>,
    guard: Handle<Ps1Material>,
    hit: Handle<Ps1Material>,
    fury: Handle<Ps1Material>,
    heal: Handle<Ps1Material>,
    /// Braises qui s'envolent (ennemi vaincu, checkpoint découvert).
    ember: Handle<Ps1Material>,
    /// Braises d'ambiance : elles brillent à travers le brouillard.
    glow: Handle<Ps1Material>,
    ash: Handle<Ps1Material>,
    water: Handle<Ps1Material>,
    /// Lueurs vertes du cadavre (braises perdues), lumière pâle des objets au sol : elles
    /// brillent à travers le brouillard.
    soul: Handle<Ps1Material>,
    wisp: Handle<Ps1Material>,
    /// Anneau de rayon 1 couché au sol (onde de choc).
    ring: Handle<Mesh>,
}

/// Alerte au sol d'une attaque de zone : contour du cercle (`fill: false`) et disque qui
/// grandit jusqu'à l'impact (`fill: true`).
#[derive(Component)]
struct AoeMarker {
    fill: bool,
}

/// Anneau de l'onde de choc, qui s'élargit et s'efface après l'impact.
#[derive(Component)]
struct Shock {
    radius: f32,
    life: f32,
}

const SHOCK_LIFE: f32 = 0.35;

#[derive(Component)]
struct Particle {
    vel: Vec3,
    life: f32,
    max: f32,
    gravity: f32,
    /// Hauteur du sol sous le point d'émission (les étincelles y rebondissent).
    floor: f32,
    /// Oscillation horizontale (m/s) : cendres et braises qui dansent en montant.
    sway: f32,
    phase: f32,
    /// Disparaît en touchant le sol (gouttes d'eau) au lieu de rebondir.
    splash: bool,
}

impl Particle {
    fn new(vel: Vec3, life: f32, gravity: f32, floor: f32) -> Self {
        Self { vel, life, max: life, gravity, floor, sway: 0.0, phase: 0.0, splash: false }
    }
}

/// Événements récents, consultables par le HUD (bannière, flash…).
#[derive(Resource, Default)]
pub struct FxState {
    pub perfect_flash: f32,
    pub fury_flash: f32,
    /// Clignotement de la barre d'endurance quand une action est refusée.
    pub no_stamina: f32,
    pub last: Vec<SimEvent>,
}

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FxState>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (consume_events, update_particles, aoe_markers, update_shocks, ambient).run_if(in_state(AppState::Playing)),
            );
    }
}

const SOUNDS: [&str; 19] = [
    "heal",
    "perfect_guard", "guard", "guard_break", "hit", "hit_heavy", "slam", "swing", "swing_heavy",
    "dodge", "fury", "fatal", "roar", "switch",
    "bark", "creak", "pickup", "kindle", "fall",
];

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<Ps1Material>>,
) {
    commands.insert_resource(Sounds(
        SOUNDS.iter().map(|s| (*s, server.load(format!("audio/{s}.wav")))).collect(),
        1.0,
    ));
    // Disques et anneaux de rayon 1, couchés au sol (mis à l'échelle du rayon de la zone).
    let flat = Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
    let ring = meshes.add(Annulus::new(0.93, 1.0).mesh().resolution(40));
    let disc = meshes.add(Circle::new(1.0).mesh().resolution(40));
    commands.insert_resource(SparkAssets {
        mesh: meshes.add(Cuboid::new(0.05, 0.05, 0.05)),
        perfect: mats.add(Ps1Material::unlit(Color::srgb(1.0, 0.95, 0.6))),
        guard: mats.add(Ps1Material::unlit(Color::srgb(1.0, 0.6, 0.2))),
        hit: mats.add(Ps1Material::unlit(Color::srgb(0.45, 0.05, 0.04))),
        fury: mats.add(Ps1Material::unlit(Color::srgb(1.0, 0.1, 0.05))),
        heal: mats.add(Ps1Material::unlit(Color::srgb(0.45, 1.0, 0.55))),
        ember: mats.add(Ps1Material::unlit(Color::srgb(1.0, 0.62, 0.2))),
        glow: mats.add({
            let mut m = Ps1Material::unlit(Color::srgb(1.0, 0.55, 0.16));
            m.params.emissive = Vec4::new(0.6, 0.25, 0.05, 0.0);
            m
        }),
        ash: mats.add(Ps1Material::unlit(Color::srgb(0.5, 0.48, 0.46))),
        water: mats.add(Ps1Material::unlit(Color::srgb(0.55, 0.72, 0.85))),
        soul: mats.add({
            let mut m = Ps1Material::unlit(Color::srgb(0.35, 1.0, 0.45));
            m.params.emissive = Vec4::new(0.15, 0.7, 0.2, 0.0);
            m
        }),
        wisp: mats.add({
            let mut m = Ps1Material::unlit(Color::srgb(1.0, 0.96, 0.8));
            m.params.emissive = Vec4::new(0.6, 0.55, 0.4, 0.0);
            m
        }),
        ring: ring.clone(),
    });
    let blend = |c: Color| {
        let mut m = Ps1Material::unlit(c);
        m.alpha_mode = AlphaMode::Blend;
        m
    };
    for (fill, mesh, color, y) in [
        (false, ring, Color::srgba(1.0, 0.3, 0.08, 0.9), 0.05),
        (true, disc, Color::srgba(0.9, 0.12, 0.04, 0.35), 0.04),
    ] {
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mats.add(blend(color))),
            flat.with_translation(Vec3::Y * y),
            Visibility::Hidden,
            AoeMarker { fill },
        ));
    }
}

pub fn play(commands: &mut Commands, sounds: &Sounds, name: &str, volume: f32) {
    let volume = volume * sounds.1;
    if let Some(h) = sounds.0.get(name) {
        commands.spawn((
            AudioPlayer::new(h.clone()),
            PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(volume)),
        ));
    }
}

/// Gerbe d'étincelles. `rise` : braises qui montent doucement au lieu de retomber.
#[allow(clippy::too_many_arguments)]
fn burst(commands: &mut Commands, sp: &SparkAssets, mat: &Handle<Ps1Material>, pos: Vec3, n: usize, speed: f32, seed: u32, floor: f32) {
    spray(commands, sp, mat, pos, n, speed, seed, floor, false);
}

#[allow(clippy::too_many_arguments)]
fn spray(commands: &mut Commands, sp: &SparkAssets, mat: &Handle<Ps1Material>, pos: Vec3, n: usize, speed: f32, seed: u32, floor: f32, rise: bool) {
    for i in 0..n {
        // Pseudo-aléatoire local (purement visuel, hors simulation).
        let h = |k: u32| {
            let x = seed.wrapping_mul(747796405).wrapping_add((i as u32).wrapping_mul(2891336453)).wrapping_add(k.wrapping_mul(1013904223)) >> 9;
            (x & 0xffff) as f32 / 65535.0 * 2.0 - 1.0
        };
        let dir = Vec3::new(h(1), h(2).abs() * 0.8 + 0.2, h(3)).normalize_or_zero();
        let (life, gravity, start) = if rise {
            (0.9 + h(4).abs() * 1.0, -1.5, pos + Vec3::new(h(7), h(8).abs() * 1.2, h(9)) * 0.5)
        } else {
            (0.25 + h(4).abs() * 0.3, 9.0, pos)
        };
        commands.spawn((
            Mesh3d(sp.mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(start).with_scale(Vec3::splat(1.0 + h(5).abs())),
            Particle::new(dir * speed * (0.5 + h(6).abs()), life, gravity, floor),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn consume_events(
    mut commands: Commands,
    mut events: ResMut<SimEvents>,
    mut sounds: ResMut<Sounds>,
    settings: Res<crate::settings::Settings>,
    sparks: Res<SparkAssets>,
    mut rig: ResMut<CameraRig>,
    mut fx: ResMut<FxState>,
    mut flashes: Query<(&GlobalTransform, &mut TintFlash)>,
    transforms: Query<&GlobalTransform>,
    enemies: Query<&Enemy>,
    tuning: Res<Tuning>,
    time: Res<Time>,
) {
    let floor = |p: Vec3| world::floor_at(&tuning, p.x, p.z, p.y - 1.0).unwrap_or(-1000.0);
    sounds.1 = settings.effects_volume;
    let dt = time.delta_secs();
    fx.perfect_flash = (fx.perfect_flash - dt * 4.0).max(0.0);
    fx.fury_flash = (fx.fury_flash - dt * 2.5).max(0.0);
    fx.no_stamina = (fx.no_stamina - dt * 3.0).max(0.0);
    fx.last.clear();
    let seed = (time.elapsed_secs() * 1000.0) as u32;
    for (k, e) in events.0.drain(..).enumerate() {
        let seed = seed.wrapping_add(k as u32 * 7919);
        fx.last.push(e);
        match e {
            SimEvent::PerfectGuard { pos } | SimEvent::Counter { pos } => {
                play(&mut commands, &sounds, "perfect_guard", 1.0);
                burst(&mut commands, &sparks, &sparks.perfect, pos, 22, 6.0, seed, floor(pos));
                fx.perfect_flash = 1.0;
                rig.shake = rig.shake.max(0.5);
            }
            SimEvent::Guard { pos } => {
                play(&mut commands, &sounds, "guard", 0.8);
                burst(&mut commands, &sparks, &sparks.guard, pos, 8, 3.5, seed, floor(pos));
                rig.shake = rig.shake.max(0.35);
            }
            SimEvent::GuardBreak { pos } => {
                play(&mut commands, &sounds, "guard_break", 1.0);
                burst(&mut commands, &sparks, &sparks.guard, pos, 14, 4.0, seed, floor(pos));
                rig.shake = rig.shake.max(0.7);
            }
            SimEvent::Hit { pos, heavy, on_player } => {
                play(&mut commands, &sounds, if heavy || on_player { "hit_heavy" } else { "hit" }, 0.9);
                burst(&mut commands, &sparks, &sparks.hit, pos, if heavy { 16 } else { 8 }, 3.0, seed, floor(pos));
                rig.shake = rig.shake.max(if on_player { 0.8 } else if heavy { 0.5 } else { 0.2 });
                if !on_player {
                    // Flash blanc sur le boss touché le plus proche du point d'impact.
                    if let Some((_, mut f)) = flashes
                        .iter_mut()
                        .min_by(|a, b| a.0.translation().distance(pos).total_cmp(&b.0.translation().distance(pos)))
                    {
                        f.white = 1.0;
                    }
                }
            }
            SimEvent::Swing { heavy, .. } => {
                play(&mut commands, &sounds, if heavy { "swing_heavy" } else { "swing" }, 0.5);
            }
            SimEvent::Dodge { .. } => play(&mut commands, &sounds, "dodge", 0.5),
            SimEvent::Jumped { .. } => play(&mut commands, &sounds, "dodge", 0.3),
            SimEvent::Landed { entity } => {
                play(&mut commands, &sounds, "slam", 0.12);
                if let Ok(t) = transforms.get(entity) {
                    let at = t.translation() + Vec3::Y * 0.05;
                    burst(&mut commands, &sparks, &sparks.ash, at, 8, 1.2, seed, floor(at + Vec3::Y));
                }
            }
            SimEvent::FuryWarn { .. } => {
                play(&mut commands, &sounds, "fury", 0.9);
                fx.fury_flash = 1.0;
            }
            SimEvent::Groggy { .. } => {
                play(&mut commands, &sounds, "guard_break", 0.8);
                rig.shake = rig.shake.max(0.4);
            }
            SimEvent::Fatal { pos } => {
                play(&mut commands, &sounds, "fatal", 1.0);
                burst(&mut commands, &sparks, &sparks.fury, pos, 30, 5.0, seed, floor(pos));
                rig.shake = rig.shake.max(0.9);
            }
            SimEvent::BossPhase2 => {
                play(&mut commands, &sounds, "roar", 1.0);
                rig.shake = rig.shake.max(1.0);
            }
            SimEvent::BossDied => play(&mut commands, &sounds, "slam", 1.0),
            SimEvent::PlayerDied => play(&mut commands, &sounds, "slam", 0.8),
            SimEvent::WeaponSwitched { .. } => play(&mut commands, &sounds, "switch", 0.6),
            SimEvent::Heal { entity } => {
                play(&mut commands, &sounds, "heal", 0.8);
                if let Ok(t) = transforms.get(entity) {
                    burst(&mut commands, &sparks, &sparks.heal, t.translation() + Vec3::Y * 1.2, 14, 1.6, seed, floor(t.translation() + Vec3::Y * 1.2));
                }
            }
            SimEvent::NoStamina { .. } => fx.no_stamina = 1.0,
            SimEvent::ItemCycled { .. } => play(&mut commands, &sounds, "switch", 0.35),
            SimEvent::BossAwake => {
                play(&mut commands, &sounds, "roar", 1.0);
                rig.shake = rig.shake.max(0.8);
            }
            SimEvent::BossRevived => play(&mut commands, &sounds, "roar", 0.5),
            SimEvent::BossDefeated { .. } => {}
            SimEvent::EnemyAlert { entity } => {
                let hound = enemies.get(entity).is_ok_and(|e| tuning.enemies[e.kind as usize].model == "hound");
                play(&mut commands, &sounds, if hound { "bark" } else { "creak" }, 0.8);
            }
            SimEvent::EnemyDied { pos, .. } => {
                play(&mut commands, &sounds, "slam", 0.45);
                spray(&mut commands, &sparks, &sparks.ember, pos + Vec3::Y * 0.6, 14, 1.2, seed, floor(pos), true);
            }
            SimEvent::EnemyVanished { pos } => {
                spray(&mut commands, &sparks, &sparks.ember, pos + Vec3::Y * 0.3, 20, 0.8, seed, floor(pos), true);
            }
            SimEvent::PickedUp { .. } => play(&mut commands, &sounds, "pickup", 0.8),
            SimEvent::ItemUsed { entity, item } => {
                let at = transforms.get(entity).map_or(Vec3::ZERO, |t| t.translation()) + Vec3::Y * 1.1;
                match item {
                    Item::FadedEmber | Item::LivelyEmber => {
                        spray(&mut commands, &sparks, &sparks.ember, at, 18, 1.0, seed, floor(at), true);
                    }
                    Item::GoldenMoss => {
                        play(&mut commands, &sounds, "heal", 0.6);
                        burst(&mut commands, &sparks, &sparks.heal, at, 10, 1.4, seed, floor(at));
                    }
                    _ => {
                        play(&mut commands, &sounds, "fury", 0.35);
                        burst(&mut commands, &sparks, &sparks.ember, at, 14, 2.0, seed, floor(at));
                    }
                }
            }
            SimEvent::Kindled { checkpoint } => {
                play(&mut commands, &sounds, "kindle", 1.0);
                let at = encounter::checkpoint_pos(&tuning, checkpoint as usize) + Vec3::Y * 1.9;
                spray(&mut commands, &sparks, &sparks.ember, at, 40, 2.5, seed, floor(at), true);
            }
            SimEvent::Fell { .. } => play(&mut commands, &sounds, "fall", 0.9),
            SimEvent::EmbersRecovered { pos, .. } => {
                play(&mut commands, &sounds, "kindle", 0.7);
                spray(&mut commands, &sparks, &sparks.glow, pos + Vec3::Y * 0.4, 30, 1.8, seed, floor(pos), true);
            }
            SimEvent::Rested { entity } => {
                play(&mut commands, &sounds, "heal", 0.9);
                if let Ok(t) = transforms.get(entity) {
                    burst(&mut commands, &sparks, &sparks.heal, t.translation() + Vec3::Y * 1.0, 24, 2.0, seed, floor(t.translation() + Vec3::Y * 1.0));
                }
            }
            SimEvent::Shockwave { pos, radius } => {
                play(&mut commands, &sounds, "slam", 1.0);
                rig.shake = rig.shake.max(1.0);
                commands.spawn((
                    Mesh3d(sparks.ring.clone()),
                    MeshMaterial3d(sparks.fury.clone()),
                    Transform::from_translation(pos + Vec3::Y * 0.08)
                        .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                        .with_scale(Vec3::splat(0.3)),
                    Shock { radius, life: SHOCK_LIFE },
                ));
                // Gerbe de débris tout autour du cercle.
                let n = (radius * 8.0) as usize;
                for i in 0..n {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU;
                    let p = pos + Vec3::new(a.cos(), 0.1, a.sin()) * radius * 0.85;
                    burst(&mut commands, &sparks, &sparks.guard, p, 2, 3.0, seed.wrapping_add(i as u32 * 31), floor(p));
                }
            }
            // Nouveaux combattants : la caméra se recale derrière le joueur.
            SimEvent::Respawned => rig.initialized = false,
        }
    }
}

/// Place l'alerte au sol de la prochaine attaque de zone du boss (contour fixe, disque qui
/// se remplit jusqu'à l'impact, clignotement à l'approche).
fn aoe_markers(
    tuning: Res<Tuning>,
    time: Res<Time>,
    bosses: Query<(&Body, &Action), With<Boss>>,
    mut markers: Query<(&AoeMarker, &mut Transform, &mut Visibility, &MeshMaterial3d<Ps1Material>)>,
    mut mats: ResMut<Assets<Ps1Material>>,
) {
    let tele = bosses.iter().find_map(|(b, a)| aoe_telegraph(b, a, &tuning));
    for (m, mut tf, mut vis, mat) in &mut markers {
        let want = if tele.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let Some((pos, r, k)) = tele else { continue };
        tf.translation.x = pos.x;
        tf.translation.z = pos.z;
        let s = if m.fill { r * k } else { r };
        tf.scale = Vec3::new(s, s, 1.0);
        if let Some(mut mat) = mats.get_mut(&mat.0) {
            let blink = if k > 0.7 { 0.5 + 0.5 * (time.elapsed_secs() * 18.0).sin() } else { 1.0 };
            mat.params.base_color.w = if m.fill { 0.2 + 0.25 * k } else { 0.5 + 0.45 * blink };
        }
    }
}

fn update_shocks(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Shock, &mut Transform)>) {
    for (e, mut s, mut t) in &mut q {
        s.life -= time.delta_secs();
        if s.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let k = 1.0 - s.life / SHOCK_LIFE;
        let r = s.radius * (0.3 + 0.75 * k.sqrt());
        t.scale = Vec3::new(r, r, 1.0);
    }
}

fn update_particles(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Particle, &mut Transform)>) {
    let dt = time.delta_secs();
    for (e, mut p, mut t) in &mut q {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        p.vel.y -= p.gravity * dt;
        t.translation += p.vel * dt;
        if p.sway > 0.0 {
            let a = p.life * 2.3 + p.phase;
            t.translation += Vec3::new(a.sin(), 0.0, (a * 0.8).cos()) * p.sway * dt;
        }
        if t.translation.y < p.floor + 0.02 {
            if p.splash {
                commands.entity(e).despawn();
                continue;
            }
            t.translation.y = p.floor + 0.02;
            p.vel *= 0.4;
        }
        let k = p.life / p.max;
        t.scale = Vec3::splat(k.max(0.2));
    }
}

/// Hauteur des braises d'un brasier de checkpoint au-dessus du sol (`tools/blender/arena.py`).
const COALS: f32 = 1.0;
/// Fontaine (`tools/blender/arena.py`) : surface du bassin, vasque haute (rayon, hauteur de
/// l'eau) et bec.
const FOUNTAIN_WATER: f32 = 0.42;
const FOUNTAIN_BOWL: (f32, f32) = (0.78, 1.93);
const FOUNTAIN_SPOUT: f32 = 2.2;
/// Au-delà, le brouillard cache tout : pas de particules.
const AMBIENT_RANGE: f32 = 50.0;

#[derive(Clone, Copy)]
enum Ambient {
    /// Braises qui montent en dansant.
    Ember,
    /// Flocons de cendre, plus haut et plus lents.
    Ash,
    /// Jet de la fontaine : il monte puis retombe dans la vasque.
    Jet,
    /// Eau qui déborde de la vasque et tombe en pluie dans le bassin.
    Spill,
    /// Lueurs vertes qui montent du cadavre (braises perdues, à récupérer).
    Soul,
    /// Étincelles pâles qui tournoient au-dessus d'un objet à ramasser.
    Wisp,
}

/// Particules d'ambiance, émises en continu (débit par seconde) près de la caméra : brasiers des
/// checkpoints (un filet de braises tant qu'ils ne sont pas ranimés, puis une colonne de braises
/// et de cendres visible de loin), braises laissées à la mort (vertes, comme les taches de sang
/// des souls-like), objets à ramasser, fontaine.
#[allow(clippy::too_many_arguments)]
fn ambient(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<Tuning>,
    sparks: Res<SparkAssets>,
    rig: Res<CameraRig>,
    players: Query<&crate::sim::player::Player, With<crate::render::LocalPlayer>>,
    mut acc: Local<Vec<f32>>,
    mut seed: Local<u32>,
) {
    use crate::sim::data::Prop;
    let t = &*tuning;
    let dt = time.delta_secs().min(0.1);
    let player = players.single().ok();
    let found = player.map_or(0, |p| p.found);
    let mut emitters: Vec<(Vec3, Ambient, f32)> = Vec::new();
    for i in 0..t.level.checkpoints.len() {
        let at = encounter::checkpoint_pos(t, i) + Vec3::Y * COALS;
        let lit = found & (1 << i) != 0;
        emitters.push((at, Ambient::Ember, if lit { 16.0 } else { 2.5 }));
        emitters.push((at, Ambient::Ash, if lit { 7.0 } else { 1.5 }));
    }
    if let Some(d) = player.and_then(|p| p.dropped) {
        emitters.push((d.pos() + Vec3::Y * 0.1, Ambient::Soul, 22.0));
    }
    let picked = player.map_or(u64::MAX, |p| p.picked);
    for i in (0..t.level.pickups.len()).filter(|i| picked & (1u64 << i) == 0) {
        emitters.push((encounter::pickup_pos(t, i) + Vec3::Y * 0.3, Ambient::Wisp, 7.0));
    }
    for p in t.level.props.iter().filter(|p| p.kind == Prop::Fountain) {
        let y = world::floor_at(t, p.pos[0], p.pos[1], 0.0).unwrap_or(0.0);
        let at = Vec3::new(p.pos[0], y, p.pos[1]);
        emitters.push((at, Ambient::Jet, 28.0));
        emitters.push((at, Ambient::Spill, 45.0));
    }
    acc.resize(emitters.len(), 0.0);
    for (k, (pos, kind, rate)) in emitters.into_iter().enumerate() {
        if pos.distance(rig.focus) > AMBIENT_RANGE {
            acc[k] = 0.0;
            continue;
        }
        acc[k] += rate * dt;
        while acc[k] >= 1.0 {
            acc[k] -= 1.0;
            *seed = seed.wrapping_add(1);
            let s = *seed;
            // Pseudo-aléatoire local (purement visuel), dans [-1, 1].
            let h = |n: u32| {
                let x = s.wrapping_mul(747796405).wrapping_add(n.wrapping_mul(2891336453)) ^ (k as u32).wrapping_mul(1013904223);
                let x = (x ^ (x >> 15)).wrapping_mul(2246822519);
                ((x >> 9) & 0xffff) as f32 / 65535.0 * 2.0 - 1.0
            };
            let (mat, start, mut p, scale) = match kind {
                Ambient::Ember => {
                    let start = pos + Vec3::new(h(1) * 0.3, 0.05, h(2) * 0.3);
                    let vel = Vec3::new(h(3) * 0.25, 0.9 + h(4).abs() * 0.9, h(5) * 0.25);
                    (&sparks.glow, start, Particle::new(vel, 1.6 + h(6).abs() * 1.6, -0.35, -1000.0), 0.6 + h(7).abs() * 0.6)
                }
                Ambient::Ash => {
                    let start = pos + Vec3::new(h(1) * 0.4, 0.4 + h(2).abs() * 0.8, h(3) * 0.4);
                    let vel = Vec3::new(h(4) * 0.2 + 0.12, 0.45 + h(5).abs() * 0.4, h(6) * 0.2);
                    (&sparks.ash, start, Particle::new(vel, 3.0 + h(7).abs() * 2.5, -0.05, -1000.0), 0.8 + h(8).abs() * 0.7)
                }
                Ambient::Jet => {
                    let start = pos + Vec3::Y * FOUNTAIN_SPOUT;
                    let vel = Vec3::new(h(1) * 0.35, 2.6 + h(2).abs() * 0.6, h(3) * 0.35);
                    (&sparks.water, start, Particle::new(vel, 1.2, 9.0, pos.y + FOUNTAIN_BOWL.1), 0.8 + h(4).abs() * 0.5)
                }
                Ambient::Soul => {
                    let start = pos + Vec3::new(h(1) * 0.45, h(2).abs() * 0.3, h(3) * 0.45);
                    let vel = Vec3::new(h(4) * 0.15, 0.7 + h(5).abs() * 0.8, h(6) * 0.15);
                    (&sparks.soul, start, Particle::new(vel, 1.4 + h(7).abs() * 1.4, -0.2, -1000.0), 0.9 + h(8).abs() * 0.9)
                }
                Ambient::Wisp => {
                    // Autour de la lueur, sur un petit cercle, elles montent en tournoyant.
                    let a = h(1) * std::f32::consts::PI;
                    let start = pos + Vec3::new(a.cos() * 0.18, h(2) * 0.1, a.sin() * 0.18);
                    let vel = Vec3::new(-a.sin() * 0.25, 0.45 + h(3).abs() * 0.4, a.cos() * 0.25);
                    (&sparks.wisp, start, Particle::new(vel, 1.0 + h(4).abs() * 0.8, -0.1, -1000.0), 0.5 + h(5).abs() * 0.5)
                }
                Ambient::Spill => {
                    let a = h(1) * std::f32::consts::PI;
                    let out = Vec3::new(a.cos(), 0.0, a.sin());
                    let start = pos + out * FOUNTAIN_BOWL.0 + Vec3::Y * (FOUNTAIN_BOWL.1 - 0.02);
                    let vel = out * (0.35 + h(2).abs() * 0.35) + Vec3::Y * h(3).abs() * 0.15;
                    (&sparks.water, start, Particle::new(vel, 1.2, 9.0, pos.y + FOUNTAIN_WATER), 0.6 + h(4).abs() * 0.5)
                }
            };
            match kind {
                Ambient::Ember | Ambient::Soul | Ambient::Wisp => (p.sway, p.phase) = (0.35, h(9) * 3.0),
                Ambient::Ash => (p.sway, p.phase) = (0.5, h(9) * 3.0),
                Ambient::Jet | Ambient::Spill => p.splash = true,
            }
            commands.spawn((
                Mesh3d(sparks.mesh.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(start).with_scale(Vec3::splat(scale)),
                p,
            ));
        }
    }
}
