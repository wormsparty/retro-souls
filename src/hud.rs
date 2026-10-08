//! Interface : barres du joueur et du boss, objet rapide, braises, réticule de verrouillage,
//! invite d'interaction, bannières et fondu à la mort.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;

use crate::fx::FxState;
use crate::input::Device;
use crate::lang::{Localized, tr};
use crate::menu::MenuState;
use crate::render::ps1::{LowResTarget, WorldCamera};
use crate::render::{AppState, Interp, LocalPlayer};
use std::collections::{HashMap, VecDeque};

use crate::sim::SimEvent;
use crate::sim::boss::Boss;
use crate::sim::data::Tuning;
use crate::sim::encounter::{Encounter, RESPAWN_TICKS, near_checkpoint, near_dropped, near_pickup};
use crate::sim::enemy::Enemy;
use crate::sim::fighter::{Body, Health};
use crate::sim::items::{Item, Kind, QUICK_SLOTS};
use crate::sim::player::{PState, Player};
use crate::ui::{Glyph, Hint, PixelSize, UiFont, hint_node, i, image_bundle, set_hint, t};

const HP_PX: f32 = 0.6; // px par PV
const ST_PX: f32 = 2.0; // px par point d'endurance
const SP_PX: f32 = 0.4; // px par point de jauge spéciale
const TEXT: Color = Color::srgb(0.85, 0.82, 0.75);
const GOLD: Color = Color::srgb(0.9, 0.75, 0.4);
/// Durée d'affichage d'un gain de braises (le compteur l'absorbe après la première seconde).
const GAIN_SHOW: f32 = 4.0;

#[derive(Component)]
enum Bar {
    Hp,
    Regain,
    Stamina,
    HpFrame,
    StaminaFrame,
    Special,
    SpecialFrame,
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
struct EmbersText;
#[derive(Component)]
struct EmbersGain;
#[derive(Component)]
struct Prompt;
#[derive(Component)]
struct BossPanel;
/// Arme équipée (icône, nom, touche pour changer) et ses repères.
#[derive(Component)]
struct WeaponIcon;
#[derive(Component)]
struct WeaponName;
#[derive(Component)]
struct WeaponHint;
#[derive(Component)]
struct WeaponPip(u8);
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
/// Compteur d'images par seconde (option « Afficher les FPS »).
#[derive(Component)]
struct FpsText;
/// Fenêtre « objet obtenu ».
#[derive(Component)]
struct Popup;
#[derive(Component)]
struct PopupIcon;
#[derive(Component)]
struct PopupName;
#[derive(Component)]
struct PopupInfo;
/// Effets en cours (résine, mousse).
#[derive(Component)]
struct Effects;
/// Barre de vie flottante d'un ennemi (et l'ennemi qu'elle suit).
#[derive(Component)]
struct FoeBar(Entity);
#[derive(Component)]
struct FoeBarFill;

/// Icônes des objets (pixel art généré au démarrage).
#[derive(Resource)]
struct ItemIcons(HashMap<Item, Handle<Image>>);

impl ItemIcons {
    fn get(&self, item: Item) -> Handle<Image> {
        self.0.get(&item).cloned().unwrap_or_default()
    }
}

/// Icônes des armes, dans l'ordre de `weapons.ron`.
#[derive(Resource)]
struct WeaponIcons(Vec<Handle<Image>>);

/// Objets ramassés à annoncer, l'un après l'autre.
#[derive(Resource, Default)]
struct PickupPopup {
    queue: VecDeque<(Item, u8)>,
    current: Option<(Item, u8)>,
    timer: f32,
}

const POPUP_SHOW: f32 = 3.2;

/// Bannière de mort / victoire.
#[derive(Resource, Default)]
struct Outcome {
    /// Texte (anglais, français) et couleur.
    text: Option<((&'static str, &'static str), Color)>,
    timer: f32,
    /// Durée d'affichage (`None` : jusqu'à la réapparition).
    duration: Option<f32>,
}

/// Valeurs animées : compteur de braises, fondu au noir.
#[derive(Resource, Default)]
struct HudAnim {
    embers_shown: f32,
    /// Braises du joueur à la frame précédente (détection des gains).
    embers_known: u32,
    gain: u32,
    gain_timer: f32,
    fade: f32,
    dying: bool,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Outcome>()
            .init_resource::<HudAnim>()
            .init_resource::<PickupPopup>()
            .add_systems(Startup, setup)
            .add_systems(OnExit(AppState::Loading), |mut q: Query<&mut Visibility, With<LoadingText>>| {
                for mut v in &mut q {
                    *v = Visibility::Hidden;
                }
            })
            .add_systems(
                OnEnter(AppState::Playing),
                |mut out: ResMut<Outcome>, mut anim: ResMut<HudAnim>, mut pop: ResMut<PickupPopup>| {
                    *out = Outcome::default();
                    *anim = HudAnim { fade: 1.0, embers_shown: -1.0, ..default() };
                    *pop = PickupPopup::default();
                },
            )
            .add_systems(Update, (show_root, fps))
            .add_systems(
                Update,
                (
                    update_bars,
                    update_boss,
                    update_item,
                    embers,
                    prompt,
                    outcome,
                    overlay,
                    reticle,
                    update_weapon,
                    popup,
                    effects,
                    foe_bars,
                )
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

/// Icône façon PS1 de `n`×`n` pixels : `shade(x, y)` donne la couleur de chaque pixel (centre
/// du pixel, en pixels depuis le coin haut gauche) ou `None` s'il est transparent. Les couleurs
/// sont ramenées à 15 bits par canal avec le même tramage 4×4 que le jeu ; avec `outline`,
/// l'icône reçoit un contour sombre d'un pixel pour se détacher du fond.
fn ps1_icon(n: usize, outline: bool, shade: impl Fn(f32, f32) -> Option<Vec3>) -> Image {
    const BAYER: [f32; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];
    let mut data = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let px = match shade(x as f32 + 0.5, y as f32 + 0.5) {
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
    if outline {
        let alpha = |x: i32, y: i32| (0..n as i32).contains(&x) && (0..n as i32).contains(&y) && data[(y as usize * n + x as usize) * 4 + 3] == 255;
        let edge: Vec<usize> = (0..n * n)
            .filter(|&i| {
                let (x, y) = ((i % n) as i32, (i / n) as i32);
                !alpha(x, y) && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| alpha(x + dx, y + dy))
            })
            .collect();
        for i in edge {
            data[i * 4..i * 4 + 4].copy_from_slice(&[16, 12, 12, 255]);
        }
    }
    Image::new(
        Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Fiole de soin : petit rendu « 3D » (panse sphérique éclairée, reflet, liquide vert lumineux).
fn flask_icon() -> Image {
    let light = Vec3::new(-0.55, 0.6, 0.6).normalize();
    let (cx, cy, r) = (12.0, 15.0, 8.2);
    ps1_icon(ITEM_ICON, true, |fx, fy| {
        let (dx, dy) = ((fx - cx) / r, (fy - cy) / r);
        let d2 = dx * dx + dy * dy;
        let neck = (fx - cx).abs() <= 2.6 && (3.0..8.5).contains(&fy);
        let lip = (fx - cx).abs() <= 3.4 && (5.0..6.5).contains(&fy);
        let cork = (fx - cx).abs() <= 2.2 && (0.5..4.0).contains(&fy);
        if cork {
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
        }
    })
}

/// Petite sphère éclairée (forme de base de plusieurs icônes) : normale et facteur diffus.
fn lit(dx: f32, dy: f32) -> Option<(Vec3, f32)> {
    let d2 = dx * dx + dy * dy;
    (d2 <= 1.0).then(|| {
        let n = Vec3::new(dx, -dy, (1.0 - d2).sqrt());
        (n, n.dot(Vec3::new(-0.55, 0.6, 0.6).normalize()).max(0.0))
    })
}

/// Braise à écraser : un caillou de charbon fendu de lueurs (ternie : grise ; vive : ardente).
fn ember_icon(lively: bool) -> Image {
    ps1_icon(ITEM_ICON, true, |fx, fy| {
        let (dx, dy) = ((fx - 12.0) / 8.5, (fy - 13.5) / 7.5);
        let wobble = 0.08 * ((fx * 1.7).sin() + (fy * 2.3).cos());
        let (n, diffuse) = lit(dx * (1.0 + wobble), dy)?;
        let crack = ((fx * 0.9 + fy * 0.4).sin() * (fy * 1.1 - fx * 0.3).cos()).abs() < 0.18;
        let core = (1.0 - (dx * dx + dy * dy)).max(0.0);
        if crack || (lively && n.z > 0.75) {
            let glow = if lively { Vec3::new(1.0, 0.75, 0.3) } else { Vec3::new(0.85, 0.38, 0.12) };
            return Some(glow * (0.6 + 0.6 * core));
        }
        let rock = if lively { Vec3::new(0.32, 0.12, 0.06) } else { Vec3::new(0.26, 0.24, 0.24) };
        Some(rock * (0.45 + 0.9 * diffuse))
    })
}

/// Mousse dorée : touffe de brins verts aux pointes dorées.
fn moss_icon() -> Image {
    const BLADES: [(f32, f32, f32); 7] = [(5.0, 2.0, 7.0), (8.0, 6.5, 3.0), (11.0, 11.5, 1.0), (14.0, 16.0, 2.5), (17.0, 20.5, 5.0), (9.5, 4.0, 5.0), (15.0, 18.0, 4.0)];
    ps1_icon(ITEM_ICON, true, |fx, fy| {
        if fy > 15.0 {
            let (dx, dy) = ((fx - 12.0) / 9.0, (fy - 19.0) / 4.5);
            let (_, diffuse) = lit(dx, dy)?;
            return Some(Vec3::new(0.18, 0.32, 0.1) * (0.5 + 0.8 * diffuse));
        }
        for (bx, tx, ty) in BLADES {
            let k = ((fy - ty) / (17.0 - ty)).clamp(0.0, 1.0);
            if fy < ty {
                continue;
            }
            let x = tx + (bx - tx) * k;
            if (fx - x).abs() <= 0.6 + 0.9 * k {
                let tip = 1.0 - k;
                return Some(Vec3::new(0.25, 0.55, 0.15).lerp(Vec3::new(0.95, 0.8, 0.3), tip * tip));
            }
        }
        None
    })
}

/// Résine ardente : petit pot de terre, résine ambrée qui déborde.
fn resin_icon() -> Image {
    ps1_icon(ITEM_ICON, true, |fx, fy| {
        let (cx, w) = (12.0, 7.5 - (fy - 14.0).abs() * 0.12);
        if (4.5..8.0).contains(&fy) && (fx - cx).abs() <= 6.0 - (fy - 4.5) * 0.3 {
            let k = 0.7 + 0.3 * (1.0 - (fx - cx + 2.0).abs() / 6.0);
            return Some(Vec3::new(1.0, 0.62, 0.15) * k);
        }
        if (8.0..21.0).contains(&fy) && (fx - cx).abs() <= w {
            let shade = 0.55 + 0.45 * (1.0 - ((fx - cx + 2.5) / w).abs()).max(0.0);
            let drip = (fx - 9.0).abs() < 1.0 && fy < 12.0 + (fx * 3.0).sin().abs() * 3.0;
            let base = if drip { Vec3::new(0.95, 0.55, 0.12) } else { Vec3::new(0.48, 0.3, 0.2) };
            return Some(base * shade);
        }
        None
    })
}

/// Éclat de fiole : morceau de verre vert, tranchant.
fn shard_icon() -> Image {
    ps1_icon(ITEM_ICON, true, |fx, fy| {
        let (u, v) = (fx - 12.0, fy - 12.0);
        let inside = v > -10.0 && u.abs() * 1.6 + (v + 2.0).abs() * 0.55 < 7.0 && v < 9.0 - u * 0.5;
        inside.then(|| {
            let facet = if u + v * 0.3 > 0.0 { 0.65 } else { 1.0 };
            let edge = ((u * 1.6).abs() + (v + 2.0).abs() * 0.55 > 5.8) as i32 as f32;
            Vec3::new(0.35, 0.8, 0.5) * facet + Vec3::splat(0.35 * edge)
        })
    })
}

/// Broche de fer : disque de métal martelé, cabochon sombre et épingle.
fn brooch_icon() -> Image {
    ps1_icon(ITEM_ICON, true, |fx, fy| {
        if (fx - fy - 1.0).abs() < 0.8 && (3.0..21.0).contains(&fx) && !(6.0..18.0).contains(&fx) {
            return Some(Vec3::splat(0.75));
        }
        let (dx, dy) = ((fx - 12.0) / 8.0, (fy - 12.0) / 8.0);
        let (n, diffuse) = lit(dx, dy)?;
        let spec = n.dot((Vec3::new(-0.55, 0.6, 0.6).normalize() + Vec3::Z).normalize()).max(0.0).powf(16.0);
        if dx * dx + dy * dy < 0.18 {
            return Some(Vec3::new(0.45, 0.08, 0.08) * (0.5 + diffuse) + Vec3::splat(spec));
        }
        let ring = (dx * dx + dy * dy - 0.55).abs() < 0.06;
        Some(Vec3::splat(if ring { 0.25 } else { 0.42 }) * (0.5 + 0.8 * diffuse) + Vec3::splat(spec * 0.8))
    })
}

/// Plume de manège : plume en diagonale, rayée rouge et blanc comme les chapiteaux.
fn feather_icon() -> Image {
    ps1_icon(ITEM_ICON, true, |fx, fy| {
        // Axe de la plume : du coin bas gauche vers le haut droit.
        let k = std::f32::consts::FRAC_1_SQRT_2;
        let (u, v) = ((fx - fy) * k, (fx + fy - 24.0) * k);
        let half = (1.0 - (v / 10.0) * (v / 10.0)).max(0.0) * 4.2 - if v < -6.0 { 4.0 } else { 0.0 };
        if u.abs() < 0.6 && (-11.0..10.0).contains(&v) {
            return Some(Vec3::new(0.9, 0.82, 0.6));
        }
        (u.abs() < half && v > -6.0).then(|| {
            let stripe = ((v + u * 0.6) / 2.6).floor() as i32 % 2 == 0;
            let c = if stripe { Vec3::new(0.75, 0.14, 0.12) } else { Vec3::new(0.92, 0.86, 0.72) };
            c * (0.75 + 0.25 * (u / half.max(0.1)))
        })
    })
}

/// Arme en diagonale, garde en bas à gauche, pointe en haut à droite. `t` : distance le long de
/// l'arme depuis le pommeau, `v` : écart à l'axe (positif : côté éclairé). La rapière a une
/// lame fine et une garde dorée à coquille, l'espadon une lame large à gorge et une longue
/// garde droite de fer.
fn weapon_icon(index: usize) -> Image {
    let k = std::f32::consts::FRAC_1_SQRT_2;
    let rapier = index == 0;
    let steel = |v: f32, w: f32| {
        let lit = (0.5 + 0.5 * v / w.max(0.1)).clamp(0.0, 1.0);
        Vec3::new(0.38, 0.4, 0.46).lerp(Vec3::new(0.9, 0.92, 0.95), lit)
    };
    ps1_icon(ITEM_ICON, true, move |fx, fy| {
        let (t, v) = ((fx - 2.0 + (22.0 - fy)) * k, (fx - 2.0 - (22.0 - fy)) * -k);
        let leather = |t: f32| {
            let wrap = (t * 2.2).fract() < 0.5;
            Vec3::new(0.36, 0.2, 0.12) * if wrap { 1.0 } else { 0.7 }
        };
        let pommel = (t - 1.3).powi(2) + v * v;
        if rapier {
            let gold = Vec3::new(0.82, 0.62, 0.26);
            if pommel < 1.6 {
                return Some(gold * 1.1);
            }
            if (2.2..5.0).contains(&t) && v.abs() < 0.8 {
                return Some(leather(t));
            }
            // Coquille et quillons.
            if (5.0..6.2).contains(&t) && v.abs() < 3.4 {
                return Some(gold * (0.8 + 0.1 * v));
            }
            // Pas-d'âne : un arc qui revient vers le pommeau.
            let bow = ((t - 4.2).powi(2) + (v - 2.2).powi(2)).sqrt();
            if (bow - 2.0).abs() < 0.45 && v > 1.0 {
                return Some(gold * 0.85);
            }
            let w = 0.85 * (1.0 - ((t - 6.2) / 21.0).max(0.0)).max(0.25);
            if (6.2..27.5).contains(&t) && v.abs() < w {
                return Some(steel(v, w));
            }
        } else {
            let iron = Vec3::new(0.34, 0.32, 0.32);
            if pommel < 2.0 {
                return Some(iron * 1.3);
            }
            if (2.3..6.2).contains(&t) && v.abs() < 0.95 {
                return Some(leather(t));
            }
            if (6.2..7.6).contains(&t) && v.abs() < 5.0 {
                return Some(iron * (1.0 + 0.15 * v.signum()));
            }
            let w = if t < 24.0 { 2.1 } else { 2.1 * (27.5 - t) / 3.5 };
            if (7.6..27.5).contains(&t) && v.abs() < w {
                // Gorge sombre au milieu de la lame.
                let fuller = v.abs() < 0.5 && t < 22.0;
                return Some(steel(v, w) * if fuller { 0.6 } else { 1.0 });
            }
        }
        None
    })
}

/// Taille de l'icône des braises, en pixels (= points) : dessinée à sa taille d'affichage.
const EMBER_ICON: usize = 16;

/// Flamme (braises) : trois langues déchiquetées sur un foyer arrondi, rouge sombre sur les
/// bords, cœur ocre pâle, et deux escarbilles qui s'en échappent. Pas de contour : la
/// silhouette reste irrégulière, comme une texture basse résolution.
fn flame_icon() -> Image {
    // Langues : (x de la base, x de la pointe, y de la pointe, demi-largeur à la base).
    const TONGUES: [(f32, f32, f32, f32); 3] = [(8.0, 8.4, 0.5, 4.2), (6.2, 2.6, 4.5, 2.4), (10.0, 13.0, 3.0, 2.2)];
    // Hauteur où les langues atteignent leur pleine largeur ; le foyer s'arrondit en dessous.
    const BASE: f32 = 12.0;
    const FOOT: f32 = 3.6;
    let hash = |x: i32, y: i32| {
        let n = (x as u32).wrapping_mul(374_761_393).wrapping_add((y as u32).wrapping_mul(668_265_263));
        let n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
        ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
    };
    let tongue = |fx: f32, fy: f32, (bx, tx, ty, w): (f32, f32, f32, f32)| -> f32 {
        if fy < ty || fy > BASE + FOOT {
            return 0.0;
        }
        let k = ((fy - ty) / (BASE - ty)).min(1.0);
        let axis = tx + (bx - tx) * k.powf(0.7);
        let half = if fy <= BASE { w * k.powf(1.1) } else { w * (1.0 - ((fy - BASE) / FOOT).powi(2)).max(0.0).sqrt() };
        // Bords rongés : la largeur varie d'un pixel à l'autre.
        let half = half + (hash(fx as i32, fy as i32) - 0.5) * 0.9 * k;
        (1.0 - (fx - axis).abs() / half.max(1e-3)).max(0.0)
    };
    ps1_icon(EMBER_ICON, false, |fx, fy| {
        match (fx as i32, fy as i32) {
            (3, 2) => return Some(Vec3::new(0.95, 0.55, 0.15)),
            (13, 0) => return Some(Vec3::new(0.8, 0.3, 0.08)),
            _ => {}
        }
        let v = TONGUES.iter().enumerate().map(|(i, &t)| tongue(fx, fy, t) * if i == 0 { 1.0 } else { 0.75 }).fold(0.0, f32::max);
        if v <= 0.0 {
            return None;
        }
        let core = ((v - 0.35) * 1.6).clamp(0.0, 1.0) * ((fy - 6.0) / 7.0).clamp(0.0, 1.0);
        let heat = (v * 1.5).min(1.0) * (0.45 + 0.55 * (fy / BASE).min(1.0));
        let outer = Vec3::new(0.38, 0.05, 0.04);
        let mid = Vec3::new(0.85, 0.32, 0.06);
        let hot = Vec3::new(1.0, 0.82, 0.45);
        Some(outer.lerp(mid, heat.clamp(0.0, 1.0)).lerp(hot, core))
    })
}

fn setup(mut commands: Commands, tuning: Res<Tuning>, font: Res<UiFont>, mut images: ResMut<Assets<Image>>) {
    let icons = ItemIcons(
        Item::ALL
            .into_iter()
            .map(|it| {
                let img = match it {
                    Item::HealFlask => flask_icon(),
                    Item::FadedEmber => ember_icon(false),
                    Item::LivelyEmber => ember_icon(true),
                    Item::GoldenMoss => moss_icon(),
                    Item::EmberResin => resin_icon(),
                    Item::FlaskShard => shard_icon(),
                    Item::IronBrooch => brooch_icon(),
                    Item::CarouselFeather => feather_icon(),
                };
                (it, images.add(img))
            })
            .collect(),
    );
    let flame = images.add(flame_icon());
    let weapon_icons = WeaponIcons((0..tuning.weapons.len()).map(|i| images.add(weapon_icon(i))).collect());
    let p = &tuning.player;
    commands.spawn((
        font.text("", 1, TEXT),
        Localized::tr("Loading…", "Chargement…"),
        Node { position_type: PositionType::Absolute, left: percent(45), top: percent(45), ..default() },
        LoadingText,
    ));
    // FPS, en haut à droite, visible partout (menus compris) quand l'option est active.
    commands.spawn((
        font.text("", 1, TEXT),
        Node { position_type: PositionType::Absolute, right: px(28), top: px(24), ..default() },
        GlobalZIndex(100),
        Visibility::Hidden,
        FpsText,
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
            c.spawn((
                Node {
                    width: px(p.special_segments as f32 * p.special_per_segment * SP_PX + 4.0),
                    height: px(9),
                    padding: UiRect::all(px(2)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                Bar::SpecialFrame,
            ))
            .with_children(|c| {
                c.spawn(bar(0.0, 5.0, Color::srgb(0.9, 0.7, 0.25), Bar::Special));
            });
            c.spawn((font.text("", 1, GOLD), Effects));
        });

    // Arme équipée et objet rapide, en bas à gauche : une case avec l'icône, et à côté le nom,
    // les repères des emplacements et la touche pour changer.
    let slot_box = |c: &mut ChildSpawnerCommands, icon: Handle<Image>, marker: Option<ItemIcon>| {
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
        .with_children(|c| match marker {
            Some(m) => {
                c.spawn((image_bundle(icon, UVec2::splat(ITEM_ICON as u32)), m));
                c.spawn((
                    font.text("", 1, Color::srgb(0.95, 0.92, 0.85)),
                    Node { position_type: PositionType::Absolute, right: px(3), bottom: px(0), ..default() },
                    ItemCount,
                ));
            }
            None => {
                c.spawn((image_bundle(icon, UVec2::splat(ITEM_ICON as u32)), WeaponIcon));
            }
        });
    };
    let row = || Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, column_gap: px(10), ..default() };
    let info = || Node { flex_direction: FlexDirection::Column, row_gap: px(3), ..default() };
    let pips = || Node { flex_direction: FlexDirection::Row, column_gap: px(3), ..default() };
    commands
        .spawn((
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                left: px(28),
                bottom: px(26),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
        ))
        .with_children(|c| {
            c.spawn(row()).with_children(|c| {
                slot_box(c, weapon_icons.0.first().cloned().unwrap_or_default(), None);
                c.spawn(info()).with_children(|c| {
                    c.spawn(pips()).with_children(|c| {
                        for i in 0..tuning.weapons.len() as u8 {
                            c.spawn((Node { width: px(10), height: px(4), ..default() }, BackgroundColor(Color::NONE), WeaponPip(i)));
                        }
                    });
                    c.spawn((font.text("", 1, TEXT), WeaponName));
                    c.spawn((Hint::new(1, Color::srgba(0.85, 0.82, 0.75, 0.8)), hint_node(), WeaponHint));
                });
            });
            c.spawn(row()).with_children(|c| {
                slot_box(c, icons.get(Item::HealFlask), Some(ItemIcon));
                c.spawn(info()).with_children(|c| {
                    c.spawn(pips()).with_children(|c| {
                        for i in 0..QUICK_SLOTS as u8 {
                            c.spawn((Node { width: px(10), height: px(4), ..default() }, BackgroundColor(Color::NONE), SlotPip(i)));
                        }
                    });
                    c.spawn((font.text("", 1, TEXT), ItemName));
                    c.spawn((Hint::new(1, Color::srgba(0.85, 0.82, 0.75, 0.8)), hint_node(), ItemHint));
                });
            });
        });
    // Objet obtenu : icône, nom et description, au-dessus de l'invite d'interaction.
    commands
        .spawn((
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(190),
                width: percent(100),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            Popup,
        ))
        .with_children(|c| {
            c.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(10),
                    padding: UiRect::axes(px(16), px(8)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.42, 0.37, 0.3)),
                BackgroundColor(Color::srgba(0.03, 0.025, 0.02, 0.82)),
            ))
            .with_children(|c| {
                c.spawn((image_bundle(icons.get(Item::FadedEmber), UVec2::splat(ITEM_ICON as u32)), PopupIcon));
                c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(3), ..default() }).with_children(|c| {
                    c.spawn((font.text("", 1, Color::srgb(0.95, 0.92, 0.85)), PopupName));
                    c.spawn((font.text("", 1, Color::srgb(0.7, 0.67, 0.6)), PopupInfo));
                });
            });
        });
    commands.insert_resource(icons);
    commands.insert_resource(weapon_icons);

    // Braises, en bas à droite.
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
            c.spawn((font.text("", 1, GOLD), EmbersGain));
            c.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(6),
                    padding: UiRect { left: px(10), right: px(4), top: px(2), bottom: px(2) },
                    border: UiRect::all(px(1)),
                    ..default()
                },
                // Biseau d'un pixel : arête claire en haut à gauche, sombre en bas à droite.
                BorderColor {
                    top: Color::srgb(0.42, 0.37, 0.3),
                    left: Color::srgb(0.42, 0.37, 0.3),
                    bottom: Color::srgb(0.08, 0.06, 0.05),
                    right: Color::srgb(0.08, 0.06, 0.05),
                },
                BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.7)),
            ))
            .with_children(|c| {
                c.spawn((font.text("0", 1, Color::srgb(0.95, 0.92, 0.85)), EmbersText));
                c.spawn(image_bundle(flame, UVec2::splat(EMBER_ICON as u32)));
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

/// Moyenne sur une demi-seconde, pour que le chiffre reste lisible.
fn fps(
    time: Res<Time<Real>>,
    settings: Res<crate::settings::Settings>,
    mut q: Query<(&mut Text, &mut Visibility), With<FpsText>>,
    mut acc: Local<(f32, u32)>,
) {
    let Ok((mut text, mut vis)) = q.single_mut() else { return };
    let want = if settings.show_fps { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    acc.0 += time.delta_secs();
    acc.1 += 1;
    if acc.0 >= 0.5 {
        set_text(&mut text, format!("{:.0} FPS", acc.1 as f32 / acc.0));
        *acc = (0.0, 0);
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
fn embers(
    time: Res<Time>,
    players: Query<&Player, With<LocalPlayer>>,
    mut anim: ResMut<HudAnim>,
    mut total: Query<&mut Text, (With<EmbersText>, Without<EmbersGain>)>,
    mut gain: Query<(&mut Text, &mut TextColor), (With<EmbersGain>, Without<EmbersText>)>,
) {
    let Ok(p) = players.single() else { return };
    let dt = time.delta_secs();
    if anim.embers_shown < 0.0 {
        anim.embers_shown = p.embers as f32;
        anim.embers_known = p.embers;
    }
    // Braises gagnées : « +N » au-dessus du compteur, qui les absorbe après un instant.
    if p.embers > anim.embers_known {
        anim.gain = if anim.gain_timer > 0.0 { anim.gain + p.embers - anim.embers_known } else { p.embers - anim.embers_known };
        anim.gain_timer = GAIN_SHOW;
    }
    anim.embers_known = p.embers;
    let target = p.embers as f32;
    let absorb = anim.gain_timer < GAIN_SHOW - 1.0;
    if absorb || anim.embers_shown > target {
        let diff = target - anim.embers_shown;
        anim.embers_shown += diff.signum() * (diff.abs() * 4.0 * dt).max(60.0 * dt).min(diff.abs());
    }
    anim.gain_timer = (anim.gain_timer - dt).max(0.0);
    for mut t in &mut total {
        set_text(&mut t, (anim.embers_shown.round() as u32).to_string());
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
    // Ramasser passe avant le repos (même bouton). (texte, avec la touche ?)
    let label = players.single().ok().filter(|(p, _, h)| !h.dead() && matches!(p.state, PState::Free | PState::Guard)).and_then(
        |(p, b, _)| {
            if p.dropped.is_some_and(|d| near_dropped(&d, b.pos)) {
                return Some((tr("Recover your embers", "Récupérer vos braises"), true));
            }
            if near_pickup(&tuning, p.picked, b.pos).is_some() {
                return Some((tr("Pick up", "Ramasser"), true));
            }
            let cp = near_checkpoint(&tuning, b.pos).filter(|_| !enc.active)?;
            Some(if enc.hunted {
                (tr("Enemies nearby: cannot rest", "Des ennemis rôdent : impossible de se reposer"), false)
            } else if p.found & (1 << cp) == 0 {
                (tr("Kindle the brazier", "Ranimer le brasier"), true)
            } else {
                (tr("Rest", "Se reposer"), true)
            })
        },
    );
    let show = !menu.open && label.is_some();
    for (mut vis, children) in &mut root {
        *vis = if show { Visibility::Inherited } else { Visibility::Hidden };
        for c in children {
            if let (Ok(mut h), Some((l, key))) = (hints.get_mut(*c), label) {
                let glyph = if *device == Device::Gamepad { Glyph::PadA } else { Glyph::Key("G") };
                set_hint(&mut h, if key { vec![i(glyph), t(l)] } else { vec![t(l)] });
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
            Bar::Special => px(p.special.max(0.0) * SP_PX),
            Bar::SpecialFrame => px(p.special_max(&tuning) * SP_PX + 4.0),
            _ => continue,
        };
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_weapon(
    tuning: Res<Tuning>,
    device: Res<Device>,
    icons: Res<WeaponIcons>,
    players: Query<&Player, With<LocalPlayer>>,
    mut icon: Query<&mut ImageNode, With<WeaponIcon>>,
    mut name: Query<&mut Text, With<WeaponName>>,
    mut hint: Query<&mut Hint, With<WeaponHint>>,
    mut pips: Query<(&WeaponPip, &mut BackgroundColor)>,
) {
    let Ok(p) = players.single() else { return };
    let w = p.weapon as usize;
    for mut img in &mut icon {
        let h = icons.0.get(w).cloned().unwrap_or_default();
        if img.image != h {
            img.image = h;
        }
    }
    for mut t in &mut name {
        set_text(&mut t, tuning.weapons[w].name.get());
    }
    let key = if *device == Device::Gamepad { Glyph::DpadRight } else { Glyph::Key("R") };
    for mut h in &mut hint {
        set_hint(&mut h, if tuning.weapons.len() > 1 { vec![i(key), t(tr("switch", "changer"))] } else { vec![] });
    }
    for (pip, mut bg) in &mut pips {
        let c = if pip.0 as usize == w { Color::srgb(0.95, 0.85, 0.55) } else { Color::srgba(0.85, 0.82, 0.75, 0.6) };
        if bg.0 != c {
            bg.0 = c;
        }
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
            // Les braises gagnées s'affichent en bas à droite (voir `embers`).
            SimEvent::BossDefeated { .. } => {
                *out = Outcome { text: Some((("AUTOMATON DESTROYED", "AUTOMATE DÉTRUIT"), Color::srgb(0.9, 0.75, 0.35))), timer: 0.0, duration: Some(5.0) };
            }
            SimEvent::Kindled { .. } => {
                *out = Outcome { text: Some((("BRAZIER KINDLED", "BRASIER RANIMÉ"), Color::srgb(0.95, 0.78, 0.4))), timer: 0.0, duration: Some(3.5) };
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

/// « Objet obtenu » : chaque objet ramassé s'affiche quelques secondes, l'un après l'autre.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn popup(
    time: Res<Time>,
    fx: Res<FxState>,
    tuning: Res<Tuning>,
    icons: Res<ItemIcons>,
    mut pop: ResMut<PickupPopup>,
    mut root: Query<&mut Visibility, With<Popup>>,
    mut icon: Query<&mut ImageNode, With<PopupIcon>>,
    mut name: Query<&mut Text, (With<PopupName>, Without<PopupInfo>)>,
    mut info: Query<&mut Text, (With<PopupInfo>, Without<PopupName>)>,
    players: Query<&Player, With<LocalPlayer>>,
) {
    for e in &fx.last {
        if let SimEvent::PickedUp { pickup, .. } = e
            && let Some(p) = tuning.level.pickups.get(*pickup as usize)
        {
            pop.queue.extend(p.items.iter().copied());
        }
    }
    pop.timer -= time.delta_secs();
    if pop.timer <= 0.0 {
        pop.current = pop.queue.pop_front();
        pop.timer = if pop.current.is_some() { POPUP_SHOW } else { 0.0 };
    }
    let Some((item, n)) = pop.current else {
        for mut v in &mut root {
            *v = Visibility::Hidden;
        }
        return;
    };
    for mut v in &mut root {
        *v = Visibility::Inherited;
    }
    for mut img in &mut icon {
        let h = icons.get(item);
        if img.image != h {
            img.image = h;
        }
    }
    for mut t in &mut name {
        set_text(&mut t, if n > 1 { format!("{} ×{n}", item.name()) } else { item.name().to_string() });
    }
    // Talisman : dire où le changer (il est porté d'office si on n'en avait pas).
    let worn = players.single().is_ok_and(|p| p.inventory.wears(item));
    let extra = match item.kind() {
        Kind::Talisman if worn => tr("\nWorn. Change talismans in Equipment (pause menu).", "\nPorté. Changer de talisman : Équipement (menu pause)."),
        Kind::Talisman => tr("\nWear it from Equipment (pause menu).", "\nÀ porter depuis Équipement (menu pause)."),
        _ => "",
    };
    for mut t in &mut info {
        set_text(&mut t, format!("{}{extra}", item.description()));
    }
}

/// Effets en cours, sous les barres : résine (arme enflammée), mousse (régénération).
fn effects(players: Query<&Player, With<LocalPlayer>>, mut q: Query<&mut Text, With<Effects>>) {
    let Ok(p) = players.single() else { return };
    let mut parts = Vec::new();
    if p.resin_ticks > 0 {
        parts.push(format!("{} {} s", Item::EmberResin.name(), p.resin_ticks.div_ceil(60)));
    }
    if p.regen_ticks > 0 {
        parts.push(format!("{} {} s", Item::GoldenMoss.name(), p.regen_ticks.div_ceil(60)));
    }
    for mut t in &mut q {
        set_text(&mut t, parts.join("   "));
    }
}

/// Barres de vie flottantes au-dessus des ennemis blessés (ou verrouillés), façon Dark Souls.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn foe_bars(
    mut commands: Commands,
    root: Single<Entity, With<HudRoot>>,
    players: Query<(&Player, &Interp), With<LocalPlayer>>,
    enemies: Query<(Entity, &Interp, &Body, &Health), With<Enemy>>,
    camera: Single<(&Camera, &GlobalTransform), With<WorldCamera>>,
    target: Res<LowResTarget>,
    window: Single<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    mut bars: Query<(Entity, &FoeBar, &mut Node, &mut Visibility, &Children)>,
    mut fills: Query<&mut Node, (With<FoeBarFill>, Without<FoeBar>)>,
) {
    const W: f32 = 44.0;
    let (cam, gt) = *camera;
    let player = players.single().ok();
    let lock = player.and_then(|(p, _)| p.lock);
    let sx = window.width() / target.size.x as f32;
    let sy = window.height() / target.size.y as f32;
    let mut has_bar: Vec<Entity> = Vec::new();
    for (e, bar, mut node, mut vis, children) in &mut bars {
        let Ok((_, i, b, h)) = enemies.get(bar.0) else {
            commands.entity(e).despawn();
            continue;
        };
        has_bar.push(bar.0);
        let near = player.is_some_and(|(_, pi)| pi.pos.distance(i.pos) < 30.0);
        let show = !h.dead() && near && (h.cur < h.max || lock == Some(bar.0));
        let vp = cam.world_to_viewport(gt, i.pos + Vec3::Y * (b.height + 0.35)).ok();
        let Some(vp) = vp.filter(|_| show) else {
            *vis = Visibility::Hidden;
            continue;
        };
        *vis = Visibility::Inherited;
        node.left = px(vp.x * sx / ui_scale.0 - W / 2.0);
        node.top = px(vp.y * sy / ui_scale.0);
        for c in children {
            if let Ok(mut f) = fills.get_mut(*c) {
                f.width = px((W - 2.0) * (h.cur / h.max).clamp(0.0, 1.0));
            }
        }
    }
    for (e, ..) in &enemies {
        if !has_bar.contains(&e) {
            commands
                .spawn((
                    ChildOf(*root),
                    FoeBar(e),
                    Node { position_type: PositionType::Absolute, width: px(W), height: px(4), padding: UiRect::all(px(1)), ..default() },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                    Visibility::Hidden,
                ))
                .with_children(|c| {
                    c.spawn((Node { width: px(W - 2.0), height: px(2), ..default() }, BackgroundColor(Color::srgb(0.68, 0.08, 0.06)), FoeBarFill));
                });
        }
    }
}
