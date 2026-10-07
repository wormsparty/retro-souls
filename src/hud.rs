//! Interface : barres du joueur et du boss, objet rapide, âmes, réticule de verrouillage,
//! invite d'interaction, bannières et fondu à la mort.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;

use crate::fx::{FxState, Sounds, play};
use crate::input::Device;
use crate::lang::{Localized, tr};
use crate::menu::MenuState;
use crate::render::ps1::{LowResTarget, WorldCamera};
use crate::render::{AppState, Interp, LocalPlayer};
use crate::sim::SimEvent;
use crate::sim::boss::Boss;
use crate::sim::data::Tuning;
use crate::sim::encounter::{Encounter, RESPAWN_TICKS, near_checkpoint};
use crate::sim::fighter::{Body, Health};
use crate::sim::items::{Item, QUICK_SLOTS};
use crate::sim::player::{PState, Player};
use crate::ui::{Glyph, Hint, PixelSize, UiFont, hint_node, i, image_bundle, set_hint, t};

const HP_PX: f32 = 0.6; // px par PV
const ST_PX: f32 = 2.0; // px par point d'endurance
const TEXT: Color = Color::srgb(0.85, 0.82, 0.75);
const GOLD: Color = Color::srgb(0.9, 0.75, 0.4);
/// Durée d'affichage d'un gain d'âmes (le compteur l'absorbe après la première seconde).
const GAIN_SHOW: f32 = 4.0;

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
struct HudRoot;
#[derive(Component)]
struct ItemIcon;
#[derive(Component)]
struct ItemCount;
#[derive(Component)]
struct ItemName;
#[derive(Component)]
struct ItemHint;
#[derive(Component)]
struct SlotPip(u8);
#[derive(Component)]
struct SoulsText;
#[derive(Component)]
struct SoulsGain;
#[derive(Component)]
struct Prompt;
#[derive(Component)]
struct BossPanel;
#[derive(Component)]
struct WeaponLabel;
#[derive(Component)]
struct Banner;
#[derive(Component)]
struct BannerText;
#[derive(Component)]
struct Overlay;
#[derive(Component)]
struct Fade;
#[derive(Component)]
struct Reticle;
#[derive(Component)]
struct LoadingText;

/// Icônes des objets (pixel art généré au démarrage).
#[derive(Resource)]
struct ItemIcons {
    flask: Handle<Image>,
}

impl ItemIcons {
    fn get(&self, item: Item) -> Handle<Image> {
        match item {
            Item::HealFlask => self.flask.clone(),
        }
    }
}

/// Bannière de mort / victoire.
#[derive(Resource, Default)]
struct Outcome {
    /// Texte (anglais, français) et couleur.
    text: Option<((&'static str, &'static str), Color)>,
    timer: f32,
    /// Durée d'affichage (`None` : jusqu'à la réapparition).
    duration: Option<f32>,
}

/// Valeurs animées : compteur d'âmes, fondu au noir.
#[derive(Resource, Default)]
struct HudAnim {
    souls_shown: f32,
    /// Âmes du joueur à la frame précédente (détection des gains).
    souls_known: u32,
    gain: u32,
    gain_timer: f32,
    /// Le compteur a commencé à absorber le gain (son joué).
    absorbing: bool,
    fade: f32,
    dying: bool,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Outcome>()
            .init_resource::<HudAnim>()
            .add_systems(Startup, setup)
            .add_systems(OnExit(AppState::Loading), |mut q: Query<&mut Visibility, With<LoadingText>>| {
                for mut v in &mut q {
                    *v = Visibility::Hidden;
                }
            })
            .add_systems(OnEnter(AppState::Playing), |mut out: ResMut<Outcome>, mut anim: ResMut<HudAnim>| {
                *out = Outcome::default();
                *anim = HudAnim { fade: 1.0, souls_shown: -1.0, ..default() };
            })
            .add_systems(Update, show_root)
            .add_systems(
                Update,
                (update_bars, update_boss, update_item, souls, prompt, outcome, overlay, reticle, weapon_label)
                    .run_if(in_state(AppState::Playing))
                    .after(crate::fx::consume_events),
            );
    }
}

fn bar(width: f32, height: f32, color: Color, kind: Bar) -> impl Bundle {
    (Node { width: px(width), height: px(height), ..default() }, BackgroundColor(color), kind)
}

/// Taille des icônes d'objets, en pixels (= points).
const ITEM_ICON: usize = 24;

/// Fiole de soin façon icône PS1 : petit rendu « 3D » (panse sphérique éclairée, reflet,
/// liquide vert lumineux) ramené à 15 bits par canal avec le même tramage 4×4 que le jeu.
fn flask_icon() -> Image {
    const N: usize = ITEM_ICON;
    const BAYER: [f32; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];
    let light = Vec3::new(-0.55, 0.6, 0.6).normalize();
    let (cx, cy, r) = (12.0, 15.0, 8.2);
    let mut data = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (dx, dy) = ((fx - cx) / r, (fy - cy) / r);
            let d2 = dx * dx + dy * dy;
            let neck = (fx - cx).abs() <= 2.6 && (3.0..8.5).contains(&fy);
            let lip = (fx - cx).abs() <= 3.4 && (5.0..6.5).contains(&fy);
            let cork = (fx - cx).abs() <= 2.2 && (0.5..4.0).contains(&fy);
            let rgb: Option<Vec3> = if cork {
                let k = 0.75 + 0.35 * (1.0 - (fx - cx + 1.0).abs() / 3.0) - (fy - 0.5) * 0.06;
                Some(Vec3::new(0.55, 0.36, 0.2) * k)
            } else if lip || (neck && d2 > 1.0) {
                let k = 0.5 + 0.5 * (1.0 - ((fx - cx + 0.8) / 3.4).abs()).max(0.0);
                Some(Vec3::new(0.55, 0.62, 0.66) * k + Vec3::splat(0.08))
            } else if d2 <= 1.0 {
                let n = Vec3::new(dx, -dy, (1.0 - d2).sqrt());
                let diffuse = n.dot(light).max(0.0);
                let spec = n.dot((light + Vec3::Z).normalize()).max(0.0).powf(24.0);
                let rim = (1.0 - n.z).powf(2.0);
                // Liquide sous la ligne de niveau (légère ondulation), verre vide au-dessus.
                let level = cy - 2.0 + 0.6 * ((fx - cx) * 0.9).sin();
                let base = if fy > level {
                    let glow = 0.35 + 0.65 * (1.0 - d2);
                    Vec3::new(0.12, 0.62, 0.3) * (0.35 + 0.75 * diffuse) + Vec3::new(0.1, 0.35, 0.12) * glow
                } else {
                    Vec3::new(0.28, 0.33, 0.38) * (0.4 + 0.6 * diffuse)
                };
                Some(base + Vec3::new(0.55, 0.65, 0.7) * rim * 0.45 + Vec3::splat(spec * 1.1))
            } else {
                None
            };
            let px = match rgb {
                Some(c) => {
                    let d = BAYER[(y % 4) * 4 + x % 4] / 16.0 - 0.5;
                    let q = |v: f32| ((v.clamp(0.0, 1.0) * 31.0 + d).round().clamp(0.0, 31.0) / 31.0 * 255.0) as u8;
                    [q(c.x), q(c.y), q(c.z), 255]
                }
                None => [0, 0, 0, 0],
            };
            data.extend(px);
        }
    }
    // Contour sombre d'un pixel, pour détacher l'icône du fond.
    let alpha = |x: i32, y: i32| (0..N as i32).contains(&x) && (0..N as i32).contains(&y) && data[(y as usize * N + x as usize) * 4 + 3] == 255;
    let edge: Vec<usize> = (0..N * N)
        .filter(|&i| {
            let (x, y) = ((i % N) as i32, (i / N) as i32);
            !alpha(x, y) && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| alpha(x + dx, y + dy))
        })
        .collect();
    for i in edge {
        data[i * 4..i * 4 + 4].copy_from_slice(&[16, 12, 12, 255]);
    }
    Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn setup(mut commands: Commands, tuning: Res<Tuning>, font: Res<UiFont>, mut images: ResMut<Assets<Image>>) {
    let icons = ItemIcons { flask: images.add(flask_icon()) };
    let p = &tuning.player;
    commands.spawn((
        font.text("", 1, TEXT),
        Localized::tr("Loading…", "Chargement…"),
        Node { position_type: PositionType::Absolute, left: percent(45), top: percent(45), ..default() },
        LoadingText,
    ));
    let root = commands
        .spawn((
            HudRoot,
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
            Visibility::Hidden,
        ))
        .id();

    // Barres du joueur, en haut à gauche.
    commands
        .spawn((
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                left: px(28),
                top: px(24),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                ..default()
            },
        ))
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
                    c.spawn((font.text("", 1, GOLD), Localized::tr("Special", "Spéciale"), Node { margin: UiRect::right(px(4)), ..default() }));
                    for i in 0..p.special_segments as u8 {
                        c.spawn((
                            Node { width: px(26), height: px(8), padding: UiRect::all(px(1)), ..default() },
                            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                        ))
                        .with_children(|c| {
                            c.spawn(bar(0.0, 6.0, Color::srgb(0.9, 0.7, 0.25), Bar::Special(i)));
                        });
                    }
                    c.spawn((font.text("", 1, TEXT), WeaponLabel, Node { margin: UiRect::left(px(10)), ..default() }));
                });
        });

    // Objet rapide, en bas à gauche.
    commands
        .spawn((
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                left: px(28),
                bottom: px(26),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::FlexEnd,
                column_gap: px(10),
                ..default()
            },
        ))
        .with_children(|c| {
            c.spawn((
                Node {
                    border: UiRect::all(px(2)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                PixelSize(UVec2::splat(32)),
                BorderColor::all(Color::srgb(0.62, 0.55, 0.42)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            ))
            .with_children(|c| {
                c.spawn((image_bundle(icons.flask.clone(), UVec2::splat(ITEM_ICON as u32)), ItemIcon));
                c.spawn((
                    font.text("", 1, Color::srgb(0.95, 0.92, 0.85)),
                    Node { position_type: PositionType::Absolute, right: px(3), bottom: px(0), ..default() },
                    ItemCount,
                ));
            });
            c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(3), ..default() }).with_children(|c| {
                c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(3), ..default() }).with_children(|c| {
                    for i in 0..QUICK_SLOTS as u8 {
                        c.spawn((Node { width: px(10), height: px(4), ..default() }, BackgroundColor(Color::NONE), SlotPip(i)));
                    }
                });
                c.spawn((font.text("", 1, TEXT), ItemName));
                c.spawn((Hint::new(1, Color::srgba(0.85, 0.82, 0.75, 0.8)), hint_node(), ItemHint));
            });
        });
    commands.insert_resource(icons);

    // Âmes, en bas à droite.
    commands
        .spawn((
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                right: px(28),
                bottom: px(26),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: px(2),
                ..default()
            },
        ))
        .with_children(|c| {
            c.spawn((font.text("", 1, GOLD), SoulsGain));
            c.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Baseline,
                    column_gap: px(10),
                    padding: UiRect::axes(px(12), px(4)),
                    border: UiRect::bottom(px(1)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.9, 0.75, 0.4, 0.5)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            ))
            .with_children(|c| {
                c.spawn((font.text("", 1, Color::srgba(0.9, 0.75, 0.4, 0.8)), Localized::tr("Souls", "Âmes")));
                c.spawn((font.text("0", 1, Color::srgb(0.95, 0.92, 0.85)), SoulsText));
            });
        });

    // Invite d'interaction, au-dessus du panneau du boss.
    commands
        .spawn((
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(150),
                width: percent(100),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            Prompt,
        ))
        .with_children(|c| {
            c.spawn((
                Hint::new(1, Color::srgb(0.95, 0.92, 0.85)),
                Node { padding: UiRect::axes(px(18), px(6)), ..hint_node() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            ));
        });

    // Boss, en bas au centre.
    commands
        .spawn((
            ChildOf(root),
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
                    c.spawn((font.text("", 1, Color::srgb(0.92, 0.88, 0.8)), Localized(tuning.boss.name.clone())));
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

    // Flash plein écran (garde parfaite / furie), puis fondu au noir.
    commands.spawn((
        ChildOf(root),
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
        BackgroundColor(Color::NONE),
        Overlay,
    ));
    commands.spawn((
        ChildOf(root),
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
        BackgroundColor(Color::NONE),
        Fade,
    ));

    // Bannière de mort / victoire.
    commands
        .spawn((
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                top: percent(40),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(6),
                padding: UiRect::axes(px(0), px(22)),
                ..default()
            },
            // Bandeau sombre sur toute la largeur, façon Dark Souls.
            BackgroundColor(Color::NONE),
            Visibility::Hidden,
            Banner,
        ))
        .with_children(|c| {
            c.spawn((font.text("", 3, Color::srgb(0.75, 0.12, 0.08)), BannerText));
        });

    commands.spawn((
        ChildOf(root),
        Node { position_type: PositionType::Absolute, width: px(6), height: px(6), ..default() },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.9)),
        Visibility::Hidden,
        Reticle,
    ));
}

/// Le HUD n'est visible qu'en jeu (pas sur l'écran titre).
fn show_root(state: Res<State<AppState>>, mut q: Query<&mut Visibility, With<HudRoot>>) {
    let want = if *state.get() == AppState::Playing { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut q {
        if *v != want {
            *v = want;
        }
    }
}

fn set_text(t: &mut Text, s: impl AsRef<str>) {
    if t.0 != s.as_ref() {
        t.0 = s.as_ref().into();
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_item(
    players: Query<&Player, With<LocalPlayer>>,
    device: Res<Device>,
    icons: Res<ItemIcons>,
    mut icon: Query<(&mut ImageNode, &mut Visibility), With<ItemIcon>>,
    mut count: Query<&mut Text, (With<ItemCount>, Without<ItemName>, Without<ItemHint>)>,
    mut name: Query<&mut Text, (With<ItemName>, Without<ItemCount>, Without<ItemHint>)>,
    mut hint: Query<&mut Hint, With<ItemHint>>,
    mut pips: Query<(&SlotPip, &mut BackgroundColor)>,
) {
    let Ok(p) = players.single() else { return };
    let inv = &p.inventory;
    let item = inv.active_item();
    let n = item.map_or(0, |i| inv.count(i));
    for (mut img, mut vis) in &mut icon {
        *vis = if item.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if let Some(i) = item {
            let h = icons.get(i);
            if img.image != h {
                img.image = h;
            }
            img.color = if n > 0 { Color::WHITE } else { Color::srgb(0.35, 0.35, 0.35) };
        }
    }
    for mut t in &mut count {
        set_text(&mut t, if item.is_some() { n.to_string() } else { String::new() });
    }
    for mut t in &mut name {
        set_text(&mut t, item.map_or("—", Item::name));
    }
    let (use_key, next_key) =
        if *device == Device::Gamepad { (Glyph::PadX, Glyph::DpadDown) } else { (Glyph::Key("F"), Glyph::Key("C")) };
    let equipped = inv.slots.iter().filter(|s| s.is_some()).count();
    for mut h in &mut hint {
        let mut segs = vec![i(use_key), t(tr("use", "utiliser"))];
        if equipped > 1 {
            segs.extend([i(next_key), t(tr("next", "suivant"))]);
        }
        set_hint(&mut h, segs);
    }
    for (pip, mut bg) in &mut pips {
        let i = pip.0 as usize;
        bg.0 = if i == inv.active as usize {
            Color::srgb(0.95, 0.85, 0.55)
        } else if inv.slots[i].is_some() {
            Color::srgba(0.85, 0.82, 0.75, 0.6)
        } else {
            Color::srgba(0.85, 0.82, 0.75, 0.15)
        };
    }
}

#[allow(clippy::type_complexity)]
fn souls(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Res<Sounds>,
    players: Query<&Player, With<LocalPlayer>>,
    mut anim: ResMut<HudAnim>,
    mut total: Query<&mut Text, (With<SoulsText>, Without<SoulsGain>)>,
    mut gain: Query<(&mut Text, &mut TextColor), (With<SoulsGain>, Without<SoulsText>)>,
) {
    let Ok(p) = players.single() else { return };
    let dt = time.delta_secs();
    if anim.souls_shown < 0.0 {
        anim.souls_shown = p.souls as f32;
        anim.souls_known = p.souls;
    }
    // Âmes gagnées : « +N » au-dessus du compteur, qui les absorbe après un instant.
    if p.souls > anim.souls_known {
        anim.gain = if anim.gain_timer > 0.0 { anim.gain + p.souls - anim.souls_known } else { p.souls - anim.souls_known };
        anim.gain_timer = GAIN_SHOW;
        anim.absorbing = false;
    }
    anim.souls_known = p.souls;
    let target = p.souls as f32;
    let absorb = anim.gain_timer < GAIN_SHOW - 1.0;
    if absorb && !anim.absorbing && anim.gain > 0 {
        anim.absorbing = true;
        play(&mut commands, &sounds, "souls", 0.8);
    }
    if absorb || anim.souls_shown > target {
        let diff = target - anim.souls_shown;
        anim.souls_shown += diff.signum() * (diff.abs() * 4.0 * dt).max(60.0 * dt).min(diff.abs());
    }
    anim.gain_timer = (anim.gain_timer - dt).max(0.0);
    for mut t in &mut total {
        set_text(&mut t, (anim.souls_shown.round() as u32).to_string());
    }
    for (mut t, mut c) in &mut gain {
        set_text(&mut t, if anim.gain_timer > 0.0 { format!("+{}", anim.gain) } else { String::new() });
        c.0 = GOLD.with_alpha(anim.gain_timer.min(1.0));
    }
}

#[allow(clippy::too_many_arguments)]
fn prompt(
    tuning: Res<Tuning>,
    device: Res<Device>,
    enc: Res<Encounter>,
    menu: Res<MenuState>,
    players: Query<(&Player, &Body, &Health), With<LocalPlayer>>,
    mut root: Query<(&mut Visibility, &Children), With<Prompt>>,
    mut hints: Query<&mut Hint>,
) {
    let show = !menu.open
        && !enc.active
        && players.single().is_ok_and(|(p, b, h)| {
            !h.dead() && matches!(p.state, PState::Free | PState::Guard) && near_checkpoint(&tuning.arena, b.pos)
        });
    for (mut vis, children) in &mut root {
        *vis = if show { Visibility::Inherited } else { Visibility::Hidden };
        for c in children {
            if let Ok(mut h) = hints.get_mut(*c) {
                let key = if *device == Device::Gamepad { Glyph::PadA } else { Glyph::Key("G") };
                set_hint(&mut h, vec![i(key), t(tr("Rest", "Se reposer"))]);
            }
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
    for mut t in &mut q {
        set_text(&mut t, tuning.weapons[p.weapon as usize].name.get());
    }
}

fn update_boss(
    enc: Res<Encounter>,
    bosses: Query<(&Boss, &Health)>,
    tuning: Res<Tuning>,
    mut panel: Query<&mut Visibility, With<BossPanel>>,
    mut bars: Query<(&Bar, &mut Node)>,
) {
    // Le panneau n'apparaît que pendant le combat.
    for mut v in &mut panel {
        *v = if enc.active { Visibility::Inherited } else { Visibility::Hidden };
    }
    let Ok((boss, hp)) = bosses.single() else { return };
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
    mut banner: Query<(&mut Visibility, &mut BackgroundColor), With<Banner>>,
    mut texts: Query<(&mut Text, &mut TextColor), With<BannerText>>,
) {
    for e in &fx.last {
        match e {
            SimEvent::PlayerDied => {
                *out = Outcome { text: Some((("YOU DIED", "VOUS ÊTES MORT"), Color::srgb(0.75, 0.12, 0.08))), ..default() };
            }
            // Les âmes gagnées s'affichent en bas à droite (voir `souls`).
            SimEvent::BossDefeated { .. } => {
                *out = Outcome { text: Some((("AUTOMATON DESTROYED", "AUTOMATE DÉTRUIT"), Color::srgb(0.9, 0.75, 0.35))), timer: 0.0, duration: Some(5.0) };
            }
            SimEvent::Respawned => *out = Outcome::default(),
            _ => {}
        }
    }
    let Some(((en, fr), color)) = out.text else {
        for (mut v, _) in &mut banner {
            *v = Visibility::Hidden;
        }
        return;
    };
    out.timer += time.delta_secs();
    if out.duration.is_some_and(|d| out.timer > d) {
        *out = Outcome::default();
        return;
    }
    let fade_out = out.duration.map_or(1.0, |d| (d - out.timer).min(1.0));
    let a = (out.timer / 0.9).min(1.0) * fade_out;
    for (mut v, mut bg) in &mut banner {
        *v = Visibility::Inherited;
        bg.0 = Color::srgba(0.0, 0.0, 0.0, 0.7 * a);
    }
    for (mut tx, mut c) in &mut texts {
        set_text(&mut tx, tr(en, fr));
        c.0 = color.with_alpha(a);
    }
}

#[allow(clippy::type_complexity)]
fn overlay(
    time: Res<Time>,
    fx: Res<FxState>,
    mut anim: ResMut<HudAnim>,
    players: Query<&Player, With<LocalPlayer>>,
    mut flash: Query<&mut BackgroundColor, (With<Overlay>, Without<Fade>)>,
    mut fade: Query<&mut BackgroundColor, (With<Fade>, Without<Overlay>)>,
) {
    for mut bg in &mut flash {
        bg.0 = if fx.perfect_flash > 0.0 {
            Color::srgba(1.0, 0.95, 0.8, fx.perfect_flash * 0.18)
        } else if fx.fury_flash > 0.0 {
            Color::srgba(0.8, 0.0, 0.0, fx.fury_flash * 0.22)
        } else {
            Color::NONE
        };
    }
    // Mort : la scène s'assombrit pendant « VOUS ÊTES MORT », noir complet juste avant la
    // réapparition au checkpoint, puis retour progressif.
    if fx.last.contains(&SimEvent::Respawned) {
        anim.fade = 1.0;
    }
    let dying = fx_dying(&fx, &mut anim);
    let target = match players.single() {
        Ok(p) if p.state == PState::Dead && p.dead_ticks + 45 >= RESPAWN_TICKS => 1.0,
        Ok(p) if p.state == PState::Dead || dying => 0.4,
        _ => 0.0,
    };
    let dt = time.delta_secs();
    anim.fade = if target > anim.fade {
        (anim.fade + dt * 1.6).min(target)
    } else {
        (anim.fade - dt * 1.2).max(target)
    };
    for mut bg in &mut fade {
        bg.0 = Color::srgba(0.0, 0.0, 0.0, anim.fade);
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

/// Vrai entre le coup fatal et la fin de l'animation de mort.
fn fx_dying(fx: &FxState, anim: &mut HudAnim) -> bool {
    if fx.last.contains(&SimEvent::PlayerDied) {
        anim.dying = true;
    }
    if fx.last.contains(&SimEvent::Respawned) {
        anim.dying = false;
    }
    anim.dying
}
