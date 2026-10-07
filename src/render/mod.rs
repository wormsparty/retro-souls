//! Présentation : tout ce qui lit la simulation pour l'afficher, sans jamais la modifier.

pub mod camera;
pub mod models;
pub mod preview;
pub mod ps1;

use bevy::prelude::*;

use crate::sim::fighter::{Body, PrevBody};
use crate::sim::math;

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Loading,
    /// Écran titre (aucun combattant dans le monde).
    Title,
    Playing,
}

/// Position/orientation interpolées entre les deux derniers ticks de simulation.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Interp {
    pub pos: Vec3,
    pub yaw: f32,
}

/// Joueur contrôlé sur cette machine (la caméra et le HUD le suivent).
#[derive(Component)]
pub struct LocalPlayer;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((ps1::Ps1Plugin, camera::CameraPlugin, models::ModelsPlugin, preview::PreviewPlugin))
            .add_systems(Update, interpolate.run_if(in_state(AppState::Playing)));
    }
}

fn interpolate(
    fixed: Res<Time<Fixed>>,
    mut q: Query<(&Body, &PrevBody, &mut Interp, &mut Transform)>,
) {
    let a = fixed.overstep_fraction().clamp(0.0, 1.0);
    for (b, p, mut i, mut t) in &mut q {
        i.pos = p.pos.lerp(b.pos, a);
        i.yaw = p.yaw + math::wrap(b.yaw - p.yaw) * a;
        t.translation = i.pos;
        t.rotation = Quat::from_rotation_y(i.yaw);
    }
}
