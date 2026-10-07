//! Interface : barres du joueur et du boss, réticule de verrouillage, bannières, aide.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::fx::FxState;
use crate::input::Device;
use crate::render::models::WEAPON_MODELS;
use crate::render::ps1::{LowResTarget, WorldCamera};
use crate::render::{AppState, Interp, LocalPlayer};
use crate::sim::boss::Boss;
use crate::sim::data::Tuning;
use crate::sim::fighter::{Body, Health};
use crate::sim::player::{PState, Player};
use crate::sim::{ResetFight, SimEvent};

const HP_PX: f32 = 0.6; // px par PV
const ST_PX: f32 = 2.0; // px par point d'endurance

#[derive(Component)]
enum Bar {
    Hp,
    Regain,
    Stamina,
    HpFrame,
    StaminaFrame,
    Special(u8),
    BossHp,
    BossStagger,
}

#[derive(Component)]
struct Flask(u8);
#[derive(Component)]
struct HealHint;
#[derive(Component)]
struct SpecialHint;
#[derive(Component)]
struct BossPanel;
#[derive(Component)]
struct BossName;
#[derive(Component)]
struct WeaponLabel;
#[derive(Component)]
struct Banner;
#[derive(Component)]
struct BannerSub;
#[derive(Component)]
struct Overlay;
#[derive(Component)]
struct Reticle;
#[derive(Component)]
struct Help;
#[derive(Component)]
struct LoadingText;

/// État de fin de combat affiché par la bannière.
#[derive(Resource, Default)]
struct Outcome {
    text: Option<&'static str>,
    timer: f32,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Outcome>()
            .add_systems(Startup, setup)
            .add_systems(OnEnter(AppState::Playing), |mut q: Query<&mut Visibility, With<LoadingText>>| {
                for mut v in &mut q {
                    *v = Visibility::Hidden;
                }
            })
            .add_systems(
                Update,
                (update_bars, update_boss, outcome, overlay, reticle, help_toggle, weapon_label, update_heals)
                    .run_if(in_state(AppState::Playing))
                    .after(crate::fx::consume_events),
            );
    }
}

fn text(s: impl Into<String>, size: f32, color: Color, font: &Handle<Font>) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font: font.clone().into(), font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

fn bar(width: f32, height: f32, color: Color, kind: Bar) -> impl Bundle {
    (Node { width: px(width), height: px(height), ..default() }, BackgroundColor(color), kind)
}

fn setup(mut commands: Commands, tuning: Res<Tuning>, server: Res<AssetServer>) {
    let serif: Handle<Font> = server.load("fonts/DejaVuSerif.ttf");
    let mono: Handle<Font> = server.load("fonts/DejaVuSansMono.ttf");
    let p = &tuning.player;
    // Barres du joueur, en haut à gauche.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: px(28),
            top: px(24),
            flex_direction: FlexDirection::Column,
            row_gap: px(5),
            ..default()
        })
        .with_children(|c| {
            c.spawn((
                Node { width: px(p.max_hp * HP_PX + 4.0), height: px(14), padding: UiRect::all(px(2)), ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                Bar::HpFrame,
            ))
            .with_children(|c| {
                c.spawn(bar(0.0, 10.0, Color::srgb(0.72, 0.1, 0.08), Bar::Hp));
                c.spawn(bar(0.0, 10.0, Color::srgb(0.55, 0.52, 0.5), Bar::Regain));
            });
            c.spawn((
                Node { width: px(p.max_stamina * ST_PX + 4.0), height: px(9), padding: UiRect::all(px(2)), ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                Bar::StaminaFrame,
            ))
            .with_children(|c| {
                c.spawn(bar(0.0, 5.0, Color::srgb(0.3, 0.62, 0.25), Bar::Stamina));
            });
            c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(4), align_items: AlignItems::Center, ..default() })
                .with_children(|c| {
                    c.spawn((text("Spéciale", 13.0, Color::srgb(0.9, 0.75, 0.4), &serif), SpecialHint, Node { margin: UiRect::right(px(4)), ..default() }));
                    for i in 0..p.special_segments as u8 {
                        c.spawn((
                            Node { width: px(26), height: px(8), padding: UiRect::all(px(1)), ..default() },
                            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                        ))
                        .with_children(|c| {
                            c.spawn(bar(0.0, 6.0, Color::srgb(0.9, 0.7, 0.25), Bar::Special(i)));
                        });
                    }
                    c.spawn((text("", 15.0, Color::srgb(0.85, 0.82, 0.75), &serif), WeaponLabel, Node { margin: UiRect::left(px(10)), ..default() }));
                });
        });

    // Soins, en bas à gauche.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: px(28),
            bottom: px(26),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexEnd,
            column_gap: px(6),
            ..default()
        })
        .with_children(|c| {
            for i in 0..p.heal_charges {
                c.spawn((
                    Node { width: px(16), height: px(26), border: UiRect::all(px(2)), ..default() },
                    BorderColor::all(Color::srgb(0.75, 0.7, 0.6)),
                    BackgroundColor(Color::srgb(0.3, 0.85, 0.45)),
                    Flask(i),
                ));
            }
            c.spawn((text("", 15.0, Color::srgb(0.85, 0.82, 0.75), &serif), HealHint, Node { margin: UiRect::left(px(6)), ..default() }));
        });

    // Boss, en bas au centre.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(48),
                width: percent(100),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BossPanel,
            Visibility::Hidden,
        ))
        .with_children(|c| {
            c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), width: px(620), ..default() })
                .with_children(|c| {
                    c.spawn((text(tuning.boss.name.clone(), 20.0, Color::srgb(0.92, 0.88, 0.8), &serif), BossName));
                    c.spawn((Node { width: px(620), height: px(12), padding: UiRect::all(px(2)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7))))
                        .with_children(|c| {
                            c.spawn((Node { width: percent(100), height: px(8), ..default() }, BackgroundColor(Color::srgb(0.68, 0.08, 0.06)), Bar::BossHp));
                        });
                    c.spawn((Node { width: px(620), height: px(6), padding: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6))))
                        .with_children(|c| {
                            c.spawn((Node { width: percent(0), height: px(4), ..default() }, BackgroundColor(Color::srgb(0.95, 0.9, 0.75)), Bar::BossStagger));
                        });
                });
        });

    // Flash plein écran (garde parfaite / furie).
    commands.spawn((
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
        BackgroundColor(Color::NONE),
        Overlay,
    ));

    // Bannière de fin de combat.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                top: percent(38),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(10),
                ..default()
            },
            Visibility::Hidden,
            Banner,
        ))
        .with_children(|c| {
            c.spawn((text("", 56.0, Color::srgb(0.75, 0.12, 0.08), &serif), BannerSub));
            c.spawn((text("", 18.0, Color::srgb(0.85, 0.82, 0.75), &serif), BannerSub));
        });

    commands.spawn((
        Node { position_type: PositionType::Absolute, width: px(6), height: px(6), ..default() },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.9)),
        Visibility::Hidden,
        Reticle,
    ));

    commands.spawn((
        text(HELP, 12.0, Color::srgba(0.85, 0.82, 0.75, 0.85), &mono),
        Node { position_type: PositionType::Absolute, right: px(16), top: px(14), ..default() },
        Help,
    ));
    commands.spawn((
        text("Chargement…", 24.0, Color::srgb(0.85, 0.82, 0.75), &serif),
        Node { position_type: PositionType::Absolute, left: percent(45), top: percent(45), ..default() },
        LoadingText,
    ));
}

const HELP: &str = "H : afficher/masquer l'aide\n\
Manette : RB légère · RT lourde (maintenir = charge) · LB garde · LT spéciale\n\
          B esquive (maintenir = sprint) · X soin · R3 verrouillage · Y changer d'arme\n\
Clavier : clic G légère · clic D lourde · Q/Maj garde · E spéciale · Espace esquive · F soin\n\
          Tab/clic molette verrouillage · R changer d'arme · WASD déplacement (AZERTY : ZQSD, garde sur A)\n\
Clic pour capturer la souris · Échap / Start : menu et options\n\
F1 debug · F2 hitboxes · F3 ralenti · F4 boss passif · F5 recommencer";

fn help_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    menu: Res<crate::menu::MenuState>,
    mut hidden: Local<bool>,
    mut q: Query<&mut Visibility, With<Help>>,
) {
    if keys.just_pressed(KeyCode::KeyH) && !menu.open {
        *hidden = !*hidden;
    }
    let want = if *hidden || menu.open { Visibility::Hidden } else { Visibility::Inherited };
    for mut v in &mut q {
        if *v != want {
            *v = want;
        }
    }
}

fn update_heals(
    players: Query<&Player, With<LocalPlayer>>,
    device: Res<Device>,
    mut flasks: Query<(&Flask, &mut BackgroundColor)>,
    mut hint: Query<&mut Text, With<HealHint>>,
) {
    let Ok(p) = players.single() else { return };
    for (f, mut bg) in &mut flasks {
        bg.0 = if f.0 < p.heals { Color::srgb(0.3, 0.85, 0.45) } else { Color::srgba(0.1, 0.1, 0.1, 0.6) };
    }
    let key = if *device == Device::Gamepad { "X" } else { "F" };
    for mut t in &mut hint {
        let s = format!("Soins ({key})  {}", p.heals);
        if t.0 != s {
            t.0 = s;
        }
    }
}

fn update_bars(
    tuning: Res<Tuning>,
    fx: Res<FxState>,
    players: Query<(&Player, &Health), With<LocalPlayer>>,
    mut bars: Query<(&Bar, &mut Node, &mut BackgroundColor)>,
) {
    let Ok((p, hp)) = players.single() else { return };
    let seg = tuning.player.special_per_segment;
    let hp_scale = HP_PX;
    for (b, mut n, mut bg) in &mut bars {
        match b {
            // Endurance : orange tant qu'on ne peut pas agir, cadre rouge si une action est refusée.
            Bar::Stamina => {
                bg.0 = if p.can_act() { Color::srgb(0.3, 0.62, 0.25) } else { Color::srgb(0.75, 0.42, 0.12) };
            }
            Bar::StaminaFrame => {
                bg.0 = Color::srgba(0.1 + 0.7 * fx.no_stamina, 0.0, 0.0, 0.7);
            }
            _ => {}
        }
        n.width = match b {
            Bar::Hp => px(hp.cur.max(0.0) * hp_scale),
            Bar::Regain => px(p.regain.min(hp.max - hp.cur).max(0.0) * hp_scale),
            Bar::HpFrame => px(hp.max * hp_scale + 4.0),
            Bar::Stamina => px(p.stamina.max(0.0) * ST_PX),
            Bar::StaminaFrame => px(tuning.player.max_stamina * ST_PX + 4.0),
            Bar::Special(i) => {
                let fill = ((p.special - *i as f32 * seg) / seg).clamp(0.0, 1.0);
                px(24.0 * fill)
            }
            _ => continue,
        };
    }
}

fn weapon_label(tuning: Res<Tuning>, players: Query<&Player, With<LocalPlayer>>, mut q: Query<&mut Text, With<WeaponLabel>>) {
    let Ok(p) = players.single() else { return };
    let name = &tuning.weapons[p.weapon as usize].name;
    for mut t in &mut q {
        if t.0 != *name {
            t.0 = name.clone();
        }
    }
    let _ = WEAPON_MODELS;
}

fn update_boss(
    players: Query<&Player, With<LocalPlayer>>,
    bosses: Query<(Entity, &Boss, &Health)>,
    tuning: Res<Tuning>,
    mut panel: Query<&mut Visibility, With<BossPanel>>,
    mut bars: Query<(&Bar, &mut Node), Without<Flask>>,
    mut seen: Local<bool>,
) {
    let Ok((_, boss, hp)) = bosses.single() else { return };
    // Le panneau apparaît au premier verrouillage ou dès que le combat commence vraiment.
    if players.single().is_ok_and(|p| p.lock.is_some()) || hp.cur < hp.max {
        *seen = true;
    }
    for mut v in &mut panel {
        *v = if *seen { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (b, mut n) in &mut bars {
        match b {
            Bar::BossHp => n.width = percent(hp.cur / hp.max * 100.0),
            Bar::BossStagger => n.width = percent(boss.stagger / tuning.boss.stagger_max * 100.0),
            _ => {}
        }
    }
}

fn outcome(
    time: Res<Time>,
    fx: Res<FxState>,
    mut out: ResMut<Outcome>,
    mut reset: ResMut<ResetFight>,
    device: Res<Device>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    players: Query<(&Player, &Health), With<LocalPlayer>>,
    mut banner: Query<&mut Visibility, With<Banner>>,
    mut texts: Query<(&mut Text, &mut TextColor), With<BannerSub>>,
) {
    for e in &fx.last {
        match e {
            SimEvent::PlayerDied => *out = Outcome { text: Some("VOUS ÊTES MORT"), timer: 0.0 },
            SimEvent::BossDied => *out = Outcome { text: Some("AUTOMATE DÉTRUIT"), timer: 0.0 },
            _ => {}
        }
    }
    let alive = players.single().is_ok_and(|(p, h)| p.state != PState::Dead && !h.dead());
    if out.text == Some("VOUS ÊTES MORT") && alive {
        *out = Outcome::default();
    }
    let confirm = keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::F5)
        || gamepads.iter().any(|g| g.just_pressed(GamepadButton::South));
    if let Some(t) = out.text {
        out.timer += time.delta_secs();
        let ready = out.timer > 2.0;
        for mut v in &mut banner {
            *v = Visibility::Inherited;
        }
        let mut it = texts.iter_mut();
        if let Some((mut tx, mut c)) = it.next() {
            tx.0 = t.into();
            let a = (out.timer / 1.2).min(1.0);
            c.0 = if t.starts_with("VOUS") { Color::srgba(0.75, 0.12, 0.08, a) } else { Color::srgba(0.9, 0.75, 0.35, a) };
        }
        if let Some((mut tx, _)) = it.next() {
            tx.0 = if !ready {
                String::new()
            } else if *device == Device::Gamepad {
                "Appuyez sur (A) pour recommencer".into()
            } else {
                "Appuyez sur Entrée pour recommencer".into()
            };
        }
        if ready && confirm {
            reset.requested = true;
            *out = Outcome::default();
        }
    } else {
        for mut v in &mut banner {
            *v = Visibility::Hidden;
        }
        if keys.just_pressed(KeyCode::F5) {
            reset.requested = true;
        }
    }
}

fn overlay(fx: Res<FxState>, mut q: Query<&mut BackgroundColor, With<Overlay>>) {
    for mut bg in &mut q {
        bg.0 = if fx.perfect_flash > 0.0 {
            Color::srgba(1.0, 0.95, 0.8, fx.perfect_flash * 0.18)
        } else if fx.fury_flash > 0.0 {
            Color::srgba(0.8, 0.0, 0.0, fx.fury_flash * 0.22)
        } else {
            Color::NONE
        };
    }
}

fn reticle(
    players: Query<&Player, With<LocalPlayer>>,
    bosses: Query<(&Interp, &Body)>,
    camera: Single<(&Camera, &GlobalTransform), With<WorldCamera>>,
    target: Res<LowResTarget>,
    window: Single<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    mut q: Query<(&mut Node, &mut Visibility), With<Reticle>>,
) {
    let Ok((mut node, mut vis)) = q.single_mut() else { return };
    let lock = players.single().ok().and_then(|p| p.lock).and_then(|e| bosses.get(e).ok());
    let Some((i, b)) = lock else {
        *vis = Visibility::Hidden;
        return;
    };
    let (cam, gt) = *camera;
    let world = i.pos + Vec3::Y * b.height * 0.6;
    let Ok(vp) = cam.world_to_viewport(gt, world) else {
        *vis = Visibility::Hidden;
        return;
    };
    let sx = window.width() / target.size.x as f32;
    let sy = window.height() / target.size.y as f32;
    // Les longueurs d'UI sont multipliées par UiScale : on convertit en unités d'UI.
    node.left = px(vp.x * sx / ui_scale.0 - 3.0);
    node.top = px(vp.y * sy / ui_scale.0 - 3.0);
    *vis = Visibility::Inherited;
}
