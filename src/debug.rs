//! Outils de réglage : overlay d'état, hitboxes, ralenti, boss passif.

use bevy::prelude::*;

use crate::render::{AppState, LocalPlayer};
use crate::sim::boss::Boss;
use crate::sim::combat::hit_capsule;
use crate::sim::data::{MoveDef, Tuning};
use crate::sim::fighter::{Action, Body};
use crate::sim::player::{PState, Player};
use crate::sim::{SimDebug, SimTick, TICK_HZ};

#[derive(Resource, Default)]
struct DebugUi {
    overlay: bool,
    hitboxes: bool,
    slow: bool,
}

#[derive(Component)]
struct DebugText;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugUi>()
            .add_systems(Startup, |mut commands: Commands, server: Res<AssetServer>| {
                commands.spawn((
                    Text::new(""),
                    TextFont { font: server.load("fonts/DejaVuSansMono.ttf").into(), font_size: FontSize::Px(13.0), ..default() },
                    TextColor(Color::srgb(0.6, 1.0, 0.6)),
                    Node { position_type: PositionType::Absolute, left: px(28), top: px(96), ..default() },
                    DebugText,
                ));
            })
            .add_systems(Update, (keys, overlay, hitboxes).run_if(in_state(AppState::Playing)));
    }
}

fn keys(
    input: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<DebugUi>,
    mut sim: ResMut<SimDebug>,
    mut fixed: ResMut<Time<Fixed>>,
) {
    if input.just_pressed(KeyCode::F1) {
        ui.overlay = !ui.overlay;
    }
    if input.just_pressed(KeyCode::F2) {
        ui.hitboxes = !ui.hitboxes;
    }
    if input.just_pressed(KeyCode::F3) {
        ui.slow = !ui.slow;
        // Ralenti : la sim tourne à 15 ticks/s, les timings en ticks restent identiques.
        fixed.set_timestep_hz(if ui.slow { TICK_HZ / 4.0 } else { TICK_HZ });
    }
    if input.just_pressed(KeyCode::F4) {
        sim.boss_passive = !sim.boss_passive;
    }
}

fn overlay(
    ui: Res<DebugUi>,
    sim: Res<SimDebug>,
    tick: Res<SimTick>,
    tuning: Res<Tuning>,
    players: Query<(&Player, &Action), With<LocalPlayer>>,
    bosses: Query<(&Boss, &Action)>,
    mut text: Single<&mut Text, With<DebugText>>,
) {
    if !ui.overlay {
        if !text.0.is_empty() {
            text.0.clear();
        }
        return;
    }
    let mut s = format!("tick {}{}{}\n", tick.0, if ui.slow { "  [RALENTI x0.25]" } else { "" }, if sim.boss_passive { "  [BOSS PASSIF]" } else { "" });
    if let Ok((p, a)) = players.single() {
        let window = p.perfect_window(&tuning);
        let since = tick.0.saturating_sub(p.guard_start);
        let in_perfect = matches!(p.state, PState::Guard) && since <= window;
        s += &format!(
            "joueur  {:?}  {:?} t{}\nendurance {:.0}  regain {:.0}  spéciale {:.0}\ngarde parfaite: fenêtre {} ticks (spam {}) {}\n",
            p.state,
            a.mv,
            a.tick,
            p.stamina,
            p.regain,
            p.special,
            window,
            p.guard_spam,
            if in_perfect { "■ ACTIVE" } else { "" },
        );
    }
    for (b, a) in &bosses {
        let def: Option<&MoveDef> = a.def(&tuning);
        let next_hit = def.and_then(|d| d.hits.iter().find(|h| h.end > a.tick)).map(|h| h.start as i64 - a.tick as i64);
        s += &format!(
            "boss  phase {}  {:?} t{}/{}  coup dans {:?}\nstagger {:.0}/{:.0}  pause {}\n",
            b.phase,
            a.mv,
            a.tick,
            def.map(|d| d.total).unwrap_or(0),
            next_hit,
            b.stagger,
            tuning.boss.stagger_max,
            b.idle,
        );
    }
    text.0 = s;
}

fn hitboxes(ui: Res<DebugUi>, tuning: Res<Tuning>, q: Query<(&Body, &Action)>, mut gizmos: Gizmos) {
    if !ui.hitboxes {
        return;
    }
    for (body, action) in &q {
        // Hurtbox.
        let iso = Isometry3d::from_translation(body.pos + Vec3::Y * body.height * 0.5);
        gizmos.primitive_3d(
            &Capsule3d::new(body.radius, (body.height - 2.0 * body.radius).max(0.01)),
            iso,
            Color::srgb(0.2, 0.8, 1.0),
        );
        let Some(def) = action.def(&tuning) else { continue };
        for h in &def.hits {
            let active = action.tick >= h.start && action.tick < h.end;
            let soon = action.tick < h.start && h.start - action.tick < 20;
            if !active && !soon {
                continue;
            }
            let (a, b, r) = hit_capsule(body, h, action.tick.max(h.start) as f32);
            let color = if !active {
                Color::srgba(1.0, 1.0, 0.2, 0.4)
            } else if h.fury {
                Color::srgb(1.0, 0.0, 0.0)
            } else {
                Color::srgb(1.0, 0.4, 0.1)
            };
            let mid = (a + b) * 0.5;
            let dir = (b - a).normalize_or_zero();
            let rot = Quat::from_rotation_arc(Vec3::Y, if dir == Vec3::ZERO { Vec3::Y } else { dir });
            gizmos.primitive_3d(&Capsule3d::new(r, a.distance(b)), Isometry3d::new(mid, rot), color);
        }
    }
}

/// Captures d'écran automatiques et pilote automatique, pour vérifier le rendu sans jouer :
/// `SOULS_SHOTS=3,6,9 SOULS_SHOT_DIR=/tmp SOULS_AUTOPILOT=1 cargo run`
pub struct AutoShotPlugin;

#[derive(Resource)]
struct AutoShots {
    times: Vec<f32>,
    dir: String,
    done: usize,
}

impl Plugin for AutoShotPlugin {
    fn build(&self, app: &mut App) {
        let Ok(spec) = std::env::var("SOULS_SHOTS") else { return };
        let times = spec.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        let dir = std::env::var("SOULS_SHOT_DIR").unwrap_or_else(|_| ".".into());
        app.insert_resource(AutoShots { times, dir, done: 0 })
            .add_systems(Update, autoshot.run_if(in_state(AppState::Playing)));
        if std::env::var("SOULS_AUTOPILOT").is_ok() {
            app.add_systems(
                FixedUpdate,
                autopilot
                    .after(crate::input::collect_local_input)
                    .before(crate::sim::run_sim_tick)
                    .run_if(in_state(AppState::Playing)),
            );
        }
    }
}

fn autoshot(
    mut menu: ResMut<crate::menu::MenuState>,
    mut commands: Commands,
    time: Res<Time>,
    mut shots: ResMut<AutoShots>,
    mut start: Local<Option<f32>>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    let t0 = *start.get_or_insert(time.elapsed_secs());
    let t = time.elapsed_secs() - t0;
    if std::env::var("SOULS_OPEN_MENU").is_ok() && t > 1.0 && !menu.open {
        menu.open = true;
    }
    if let Some(&at) = shots.times.get(shots.done) {
        if t >= at {
            let path = format!("{}/shot_{}.png", shots.dir, shots.done);
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
            shots.done += 1;
        }
    } else if t >= shots.times.last().copied().unwrap_or(0.0) + 1.0 {
        exit.write(AppExit::Success);
    }
}

fn autopilot(
    tick: Res<SimTick>,
    mut inputs: ResMut<crate::sim::input::PlayerInputs>,
    rig: Res<crate::render::camera::CameraRig>,
    players: Query<(&Player, &Body)>,
    bosses: Query<&Body, With<Boss>>,
) {
    use crate::sim::input::{PlayerInput, btn};
    let (Ok((p, pb)), Ok(bb)) = (players.single(), bosses.single()) else { return };
    let t = tick.0;
    let mut i = PlayerInput { cam_yaw: PlayerInput::quantize_yaw(rig.yaw), ..default() };
    if t == 30 {
        i.buttons |= btn::LOCK;
    }
    if t == 50 {
        i.buttons |= btn::SWITCH;
    }
    if t % 480 == 400 {
        i.buttons |= btn::HEAL;
    }
    let dist = pb.pos.distance(bb.pos);
    if t > 40 && dist > 3.2 {
        i.move_y = 127;
    }
    if dist <= 3.4 && t % 90 < 3 {
        i.buttons |= btn::LIGHT;
    }
    if dist <= 3.4 && (t / 200) % 2 == 1 {
        i.buttons |= btn::GUARD;
    }
    let _ = p;
    inputs.0[0] = i;
}
