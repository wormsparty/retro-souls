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

/// Horloge de l'animation : pas de temps et avancement entre deux ticks de simulation. Figée
/// tant que le jeu est en pause (menu ouvert en jeu : la simulation ne tourne plus) ; les
/// particules, elles, continuent avec le temps réel.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct AnimClock {
    pub dt: f32,
    /// Fraction du tick en cours (0 → 1) ; 1 en pause (dernier état de la simulation).
    pub over: f32,
    pub paused: bool,
}

/// Joueur contrôlé sur cette machine (la caméra et le HUD le suivent).
#[derive(Component)]
pub struct LocalPlayer;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((ps1::Ps1Plugin, camera::CameraPlugin, models::ModelsPlugin, preview::PreviewPlugin))
            .init_resource::<AnimClock>()
            // Après la boucle à pas fixe : la fraction de tick n'est juste qu'une fois les ticks
            // de la frame joués (en PreUpdate, elle a un tick de retard une frame sur deux et
            // le joueur tremble en courant).
            .add_systems(RunFixedMainLoop, tick_anim_clock.in_set(RunFixedMainLoopSystems::AfterFixedMainLoop))
            .add_systems(Update, interpolate.run_if(in_state(AppState::Playing)));
    }
}

fn tick_anim_clock(
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    state: Res<State<AppState>>,
    menu: Res<crate::menu::MenuState>,
    mut clock: ResMut<AnimClock>,
) {
    let paused = *state.get() == AppState::Playing && menu.open;
    *clock = if paused {
        AnimClock { dt: 0.0, over: 1.0, paused }
    } else {
        AnimClock { dt: time.delta_secs(), over: fixed.overstep_fraction().clamp(0.0, 1.0), paused }
    };
}

fn interpolate(clock: Res<AnimClock>, mut q: Query<(&Body, &PrevBody, &mut Interp, &mut Transform)>) {
    let a = clock.over;
    for (b, p, mut i, mut t) in &mut q {
        i.pos = p.pos.lerp(b.pos, a);
        i.yaw = p.yaw + math::wrap(b.yaw - p.yaw) * a;
        t.translation = i.pos;
        t.rotation = Quat::from_rotation_y(i.yaw);
    }
}
