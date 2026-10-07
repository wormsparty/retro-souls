//! Retours sensoriels déclenchés par les événements de la simulation : sons, étincelles,
//! tremblement de caméra, flash du boss.

use bevy::prelude::*;

use crate::render::AppState;
use crate::render::camera::CameraRig;
use crate::render::models::TintFlash;
use crate::render::ps1::Ps1Material;
use crate::sim::{SimEvent, SimEvents};

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
}

#[derive(Component)]
struct Particle {
    vel: Vec3,
    life: f32,
    max: f32,
    gravity: f32,
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
            .add_systems(Update, (consume_events, update_particles).run_if(in_state(AppState::Playing)));
    }
}

const SOUNDS: [&str; 14] = [
    "heal",
    "perfect_guard", "guard", "guard_break", "hit", "hit_heavy", "slam", "swing", "swing_heavy",
    "dodge", "fury", "fatal", "roar", "switch",
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
    commands.insert_resource(SparkAssets {
        mesh: meshes.add(Cuboid::new(0.05, 0.05, 0.05)),
        perfect: mats.add(Ps1Material::unlit(Color::srgb(1.0, 0.95, 0.6))),
        guard: mats.add(Ps1Material::unlit(Color::srgb(1.0, 0.6, 0.2))),
        hit: mats.add(Ps1Material::unlit(Color::srgb(0.45, 0.05, 0.04))),
        fury: mats.add(Ps1Material::unlit(Color::srgb(1.0, 0.1, 0.05))),
        heal: mats.add(Ps1Material::unlit(Color::srgb(0.45, 1.0, 0.55))),
    });
}

fn play(commands: &mut Commands, sounds: &Sounds, name: &str, volume: f32) {
    let volume = volume * sounds.1;
    if let Some(h) = sounds.0.get(name) {
        commands.spawn((
            AudioPlayer::new(h.clone()),
            PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(volume)),
        ));
    }
}

fn burst(commands: &mut Commands, sp: &SparkAssets, mat: &Handle<Ps1Material>, pos: Vec3, n: usize, speed: f32, seed: u32) {
    for i in 0..n {
        // Pseudo-aléatoire local (purement visuel, hors simulation).
        let h = |k: u32| {
            let x = seed.wrapping_mul(747796405).wrapping_add((i as u32).wrapping_mul(2891336453)).wrapping_add(k.wrapping_mul(1013904223)) >> 9;
            (x & 0xffff) as f32 / 65535.0 * 2.0 - 1.0
        };
        let dir = Vec3::new(h(1), h(2).abs() * 0.8 + 0.2, h(3)).normalize_or_zero();
        let life = 0.25 + h(4).abs() * 0.3;
        commands.spawn((
            Mesh3d(sp.mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(1.0 + h(5).abs())),
            Particle { vel: dir * speed * (0.5 + h(6).abs()), life, max: life, gravity: 9.0 },
        ));
    }
}

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
    time: Res<Time>,
) {
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
                burst(&mut commands, &sparks, &sparks.perfect, pos, 22, 6.0, seed);
                fx.perfect_flash = 1.0;
                rig.shake = rig.shake.max(0.5);
            }
            SimEvent::Guard { pos } => {
                play(&mut commands, &sounds, "guard", 0.8);
                burst(&mut commands, &sparks, &sparks.guard, pos, 8, 3.5, seed);
                rig.shake = rig.shake.max(0.35);
            }
            SimEvent::GuardBreak { pos } => {
                play(&mut commands, &sounds, "guard_break", 1.0);
                burst(&mut commands, &sparks, &sparks.guard, pos, 14, 4.0, seed);
                rig.shake = rig.shake.max(0.7);
            }
            SimEvent::Hit { pos, heavy, on_player } => {
                play(&mut commands, &sounds, if heavy || on_player { "hit_heavy" } else { "hit" }, 0.9);
                burst(&mut commands, &sparks, &sparks.hit, pos, if heavy { 16 } else { 8 }, 3.0, seed);
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
                burst(&mut commands, &sparks, &sparks.fury, pos, 30, 5.0, seed);
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
                    burst(&mut commands, &sparks, &sparks.heal, t.translation() + Vec3::Y * 1.2, 14, 1.6, seed);
                }
            }
            SimEvent::NoStamina { .. } => fx.no_stamina = 1.0,
        }
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
        if t.translation.y < 0.02 {
            t.translation.y = 0.02;
            p.vel *= 0.4;
        }
        let k = p.life / p.max;
        t.scale = Vec3::splat(k.max(0.2));
    }
}
