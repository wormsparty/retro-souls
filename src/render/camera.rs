//! Caméra à la troisième personne, avec verrouillage de cible et tremblement.

use bevy::prelude::*;

use super::ps1::WorldCamera;
use super::{Interp, LocalPlayer};
use crate::input::LookInput;
use crate::sim::data::Tuning;
use crate::sim::fighter::{Body, Foe};
use crate::sim::player::Player;
use crate::sim::{math, world};

#[derive(Resource, Clone, Debug)]
pub struct CameraRig {
    /// Yaw horizontal de la caméra (même convention que la sim : 0 = regarde vers +Z).
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub shake: f32,
    pub focus: Vec3,
    /// Part de la distance laissée par les murs (la caméra se rapproche d'un coup quand un mur
    /// s'interpose, et ne recule que progressivement).
    pub reach: f32,
    pub initialized: bool,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: 0.3, distance: 5.8, shake: 0.0, focus: Vec3::ZERO, reach: 1.0, initialized: false }
    }
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraRig>()
            .add_systems(PostUpdate, update_camera.before(TransformSystems::Propagate));
    }
}

#[allow(clippy::too_many_arguments)]
fn update_camera(
    time: Res<Time>,
    tuning: Res<Tuning>,
    look: Res<LookInput>,
    mut rig: ResMut<CameraRig>,
    players: Query<(&Player, &Interp), With<LocalPlayer>>,
    foes: Query<(&Interp, &Body), With<Foe>>,
    mut cam: Single<&mut Transform, With<WorldCamera>>,
    settings: Res<crate::settings::Settings>,
) {
    let dt = time.delta_secs();
    let Ok((player, pi)) = players.single() else { return };
    let lock = player.lock.and_then(|e| foes.get(e).ok());

    let fresh = !rig.initialized;
    if fresh {
        rig.yaw = pi.yaw;
        rig.focus = pi.pos;
        rig.initialized = true;
    }

    if let Some((bi, bbody)) = lock {
        // Verrouillé : la caméra se place derrière le joueur, orientée vers le boss.
        let want = math::yaw_of(bi.pos - pi.pos);
        let d = math::wrap(want - rig.yaw);
        rig.yaw = math::wrap(rig.yaw + d * (1.0 - (-8.0 * dt).exp()));
        let dist = math::flat_len(bi.pos - pi.pos);
        let target_pitch = (0.22 + (bbody.height - 2.0).max(0.0) * 0.05 - dist * 0.004).clamp(0.12, 0.4);
        rig.pitch += (target_pitch - rig.pitch) * (1.0 - (-4.0 * dt).exp());
        rig.yaw = math::wrap(rig.yaw - look.delta.x * 0.15);
    } else {
        rig.yaw = math::wrap(rig.yaw - look.delta.x);
        rig.pitch = (rig.pitch + look.delta.y).clamp(-0.35, 1.1);
    }

    let mut target = pi.pos + Vec3::Y * 1.7;
    // Chute : la caméra reste au bord et regarde le corps disparaître dans le noir.
    if player.falling {
        target.y = target.y.max(player.fall_from + 0.4);
    }
    // Saut : la caméra ne suit qu'une partie de la hauteur (moins de secousses).
    if player.airborne {
        target.y = player.air_from + 1.7 + (pi.pos.y - player.air_from) * 0.4;
    }
    rig.focus = rig.focus.lerp(target, 1.0 - (-14.0 * dt).exp());
    let back = -math::forward(rig.yaw);
    let horiz = rig.pitch.cos() * rig.distance;
    let mut pos = rig.focus + back * horiz + Vec3::Y * (rig.pitch.sin() * rig.distance + 0.25);
    let ground = if player.falling {
        player.fall_from
    } else if player.airborne {
        player.air_from.min(pi.pos.y)
    } else {
        pi.pos.y
    };
    pos.y = pos.y.max(ground + 0.4);
    // Un mur (arène, escalier) entre le joueur et la caméra la cacherait : elle passe devant.
    // Ailleurs, rien ne l'arrête : autour, c'est le vide.
    let offset = pos - rig.focus;
    let len = math::flat_len(offset).max(0.01);
    let allowed = if player.falling {
        1.0
    } else {
        world::wall_hit(&tuning, rig.focus, pos).map_or(1.0, |s| (s - 0.3 / len).max(0.08))
    };
    rig.reach = if allowed < rig.reach { allowed } else { rig.reach + (allowed - rig.reach) * (1.0 - (-5.0 * dt).exp()) };
    if fresh {
        rig.reach = allowed;
    }
    pos = rig.focus + Vec3::new(offset.x * rig.reach, offset.y, offset.z * rig.reach);
    let mut look_at = rig.focus;
    if let Some((bi, bbody)) = lock {
        let boss_point = bi.pos + Vec3::Y * (bbody.height * 0.55);
        look_at = rig.focus.lerp(boss_point, 0.35);
    }

    rig.shake = (rig.shake - dt * 2.5).max(0.0);
    let t = time.elapsed_secs();
    let s = if settings.camera_shake { rig.shake * rig.shake * 0.25 } else { 0.0 };
    let jitter = Vec3::new((t * 71.0).sin(), (t * 53.0).cos(), (t * 61.0).sin()) * s;

    **cam = Transform::from_translation(pos + jitter).looking_at(look_at, Vec3::Y);
}
