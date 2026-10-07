//! Menu pause / options (Échap au clavier, Start à la manette).
//!
//! Navigation : haut/bas pour choisir, gauche/droite pour changer une valeur,
//! Entrée / (A) pour valider, Échap / (B) pour revenir au jeu. La souris marche aussi :
//! survoler une ligne la sélectionne, clic sur ‹ / › pour changer la valeur.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, Monitor, PrimaryMonitor, PrimaryWindow};

use crate::render::AppState;
use crate::settings::{
    DisplayMode, INTERNAL_HEIGHTS, Settings, exclusive_sizes, refresh_rates, windowed_sizes,
};
use crate::sim::ResetFight;

#[derive(Resource, Default)]
pub struct MenuState {
    pub open: bool,
    selected: usize,
    /// Répétition de navigation au stick/D-pad maintenu.
    repeat: f32,
}

pub fn menu_closed(menu: Res<MenuState>) -> bool {
    !menu.open
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Resume,
    Display,
    Resolution,
    Refresh,
    VSync,
    Internal,
    Master,
    Effects,
    Sensitivity,
    InvertY,
    Shake,
    Restart,
    Quit,
}

fn items() -> Vec<Item> {
    use Item::*;
    if cfg!(target_arch = "wasm32") {
        // Dans le navigateur, résolution, fréquence et VSync sont gérées par le navigateur.
        vec![Resume, Display, Internal, Master, Effects, Sensitivity, InvertY, Shake, Restart]
    } else {
        vec![Resume, Display, Resolution, Refresh, VSync, Internal, Master, Effects, Sensitivity, InvertY, Shake, Restart, Quit]
    }
}

#[derive(Component)]
struct MenuRoot;
#[derive(Component)]
struct Row(usize);
#[derive(Component)]
struct RowLabel(usize);
#[derive(Component)]
struct RowValue(usize);
#[derive(Component)]
struct Arrow(usize, i32);

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .add_systems(Startup, spawn_menu)
            .add_systems(
                Update,
                (toggle_menu, navigate, mouse_input, refresh_menu)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

fn spawn_menu(mut commands: Commands, server: Res<AssetServer>) {
    let serif: Handle<Font> = server.load("fonts/DejaVuSerif.ttf");
    let font = |size: f32| TextFont { font: serif.clone().into(), font_size: FontSize::Px(size), ..default() };
    commands
        .spawn((
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
            Visibility::Hidden,
            ZIndex(50),
        ))
        .with_children(|c| {
            c.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                padding: UiRect::all(px(28)),
                width: px(660),
                ..default()
            })
            .with_children(|c| {
                c.spawn((
                    Text::new("PAUSE"),
                    font(34.0),
                    TextColor(Color::srgb(0.9, 0.82, 0.62)),
                    Node { margin: UiRect::bottom(px(14)), ..default() },
                ));
                for (i, _) in items().iter().enumerate() {
                    c.spawn((
                        Row(i),
                        Button,
                        Node {
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(px(12), px(5)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ))
                    .with_children(|c| {
                        c.spawn((Text::new(""), font(19.0), TextColor(Color::srgb(0.85, 0.82, 0.75)), RowLabel(i)));
                        c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(10), align_items: AlignItems::Center, ..default() })
                            .with_children(|c| {
                                c.spawn((Button, Text::new("‹"), font(21.0), TextColor(Color::srgb(0.9, 0.82, 0.62)), Arrow(i, -1)));
                                c.spawn((
                                    Text::new(""),
                                    font(19.0),
                                    TextColor(Color::srgb(0.95, 0.92, 0.85)),
                                    Node { min_width: px(230), justify_content: JustifyContent::Center, ..default() },
                                    RowValue(i),
                                ));
                                c.spawn((Button, Text::new("›"), font(21.0), TextColor(Color::srgb(0.9, 0.82, 0.62)), Arrow(i, 1)));
                            });
                    });
                }
                c.spawn((
                    Text::new("↑↓ choisir · ←→ modifier · Entrée/(A) valider · Échap/(B) reprendre"),
                    font(13.0),
                    TextColor(Color::srgba(0.85, 0.82, 0.75, 0.7)),
                    Node { margin: UiRect::top(px(16)), ..default() },
                ));
            });
        });
}

fn set_open(menu: &mut MenuState, cursor: &mut CursorOptions, open: bool) {
    menu.open = open;
    if open {
        menu.selected = 0;
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else if !cfg!(target_arch = "wasm32") {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

fn toggle_menu(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut menu: ResMut<MenuState>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let start = keys.just_pressed(KeyCode::Escape)
        || gamepads.iter().any(|g| g.just_pressed(GamepadButton::Start) || g.just_pressed(GamepadButton::Select));
    let back = menu.open && gamepads.iter().any(|g| g.just_pressed(GamepadButton::East));
    if start || back {
        let open = !menu.open;
        set_open(&mut menu, &mut cursor, open);
    }
}

/// Valeurs possibles d'une option, et index de la valeur actuelle.
struct Choices {
    labels: Vec<String>,
    current: usize,
    /// Faux si l'option ne s'applique pas dans le mode actuel (affichée grisée).
    enabled: bool,
}

fn pct(v: f32) -> String {
    format!("{} %", (v * 100.0).round() as i32)
}

fn steps01() -> Vec<f32> {
    (0..=10).map(|i| i as f32 / 10.0).collect()
}

fn nearest(values: &[f32], v: f32) -> usize {
    values
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - v).abs().total_cmp(&(b.1 - v).abs()))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

const SENSITIVITIES: [f32; 12] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0];

fn display_modes() -> Vec<DisplayMode> {
    if cfg!(target_arch = "wasm32") {
        vec![DisplayMode::Fullscreen, DisplayMode::Windowed]
    } else {
        vec![DisplayMode::Fullscreen, DisplayMode::Exclusive, DisplayMode::Windowed]
    }
}

fn resolutions(s: &Settings, m: Option<&Monitor>) -> Vec<(u32, u32)> {
    match s.display {
        DisplayMode::Exclusive => m.map(exclusive_sizes).unwrap_or_default(),
        DisplayMode::Windowed => windowed_sizes(m),
        DisplayMode::Fullscreen => Vec::new(),
    }
}

fn current_resolution(s: &Settings, m: Option<&Monitor>) -> (u32, u32) {
    s.resolution.unwrap_or_else(|| match (s.display, m) {
        (DisplayMode::Windowed, _) => (1280, 720),
        (_, Some(m)) => (m.physical_width, m.physical_height),
        _ => (1920, 1080),
    })
}

fn choices(item: Item, s: &Settings, m: Option<&Monitor>) -> Option<Choices> {
    let yes_no = |b: bool, on: &str, off: &str| Choices {
        labels: vec![off.into(), on.into()],
        current: b as usize,
        enabled: true,
    };
    Some(match item {
        Item::Resume | Item::Restart | Item::Quit => return None,
        Item::Display => {
            let modes = display_modes();
            Choices {
                labels: modes
                    .iter()
                    .map(|d| match d {
                        DisplayMode::Fullscreen => "Plein écran",
                        DisplayMode::Exclusive => "Plein écran exclusif",
                        DisplayMode::Windowed => "Fenêtré",
                    }.into())
                    .collect(),
                current: modes.iter().position(|d| *d == s.display).unwrap_or(0),
                enabled: true,
            }
        }
        Item::Resolution => {
            let list = resolutions(s, m);
            if list.is_empty() {
                let (w, h) = current_resolution(s, m);
                return Some(Choices { labels: vec![format!("Native ({w}×{h})")], current: 0, enabled: false });
            }
            let cur = current_resolution(s, m);
            Choices {
                labels: list.iter().map(|(w, h)| format!("{w}×{h}")).collect(),
                current: list.iter().position(|r| *r == cur).unwrap_or(0),
                enabled: true,
            }
        }
        Item::Refresh => {
            let auto = m.and_then(|m| m.refresh_rate_millihertz).map(|r| format!("Auto ({} Hz)", (r as f32 / 1000.0).round()));
            if s.display != DisplayMode::Exclusive {
                return Some(Choices { labels: vec![auto.unwrap_or("Auto".into())], current: 0, enabled: false });
            }
            let rates = m.map(|m| refresh_rates(m, current_resolution(s, Some(m)))).unwrap_or_default();
            if rates.is_empty() {
                return Some(Choices { labels: vec!["Auto".into()], current: 0, enabled: false });
            }
            let cur = s.refresh_mhz.or(m.and_then(|m| m.refresh_rate_millihertz)).unwrap_or(rates[rates.len() - 1]);
            Choices {
                labels: rates.iter().map(|r| format!("{} Hz", (*r as f32 / 1000.0 * 100.0).round() / 100.0)).collect(),
                current: rates.iter().enumerate().min_by_key(|(_, r)| r.abs_diff(cur)).map(|(i, _)| i).unwrap_or(0),
                enabled: true,
            }
        }
        Item::VSync => yes_no(s.vsync, "Activée", "Désactivée"),
        Item::Internal => Choices {
            labels: INTERNAL_HEIGHTS.iter().map(|h| if *h == 240 { "240p (PS1)".into() } else { format!("{h}p") }).collect(),
            current: INTERNAL_HEIGHTS.iter().position(|h| *h == s.internal_height).unwrap_or(0),
            enabled: true,
        },
        Item::Master => Choices { labels: steps01().iter().map(|v| pct(*v)).collect(), current: nearest(&steps01(), s.master_volume), enabled: true },
        Item::Effects => Choices { labels: steps01().iter().map(|v| pct(*v)).collect(), current: nearest(&steps01(), s.effects_volume), enabled: true },
        Item::Sensitivity => Choices {
            labels: SENSITIVITIES.iter().map(|v| format!("{v:.2}")).collect(),
            current: nearest(&SENSITIVITIES, s.sensitivity),
            enabled: true,
        },
        Item::InvertY => yes_no(s.invert_y, "Oui", "Non"),
        Item::Shake => yes_no(s.camera_shake, "Oui", "Non"),
    })
}

/// Change la valeur d'une option de `delta` crans (sans boucler).
fn change(item: Item, delta: i32, s: &mut Settings, m: Option<&Monitor>) {
    let Some(c) = choices(item, s, m) else { return };
    if !c.enabled || c.labels.len() < 2 {
        return;
    }
    let i = (c.current as i32 + delta).clamp(0, c.labels.len() as i32 - 1) as usize;
    if i == c.current {
        return;
    }
    match item {
        Item::Display => {
            s.display = display_modes()[i];
            // La résolution choisie dépend du mode : on repart sur l'automatique.
            s.resolution = None;
            s.refresh_mhz = None;
        }
        Item::Resolution => {
            s.resolution = Some(resolutions(s, m)[i]);
            s.refresh_mhz = None;
        }
        Item::Refresh => {
            if let Some(m) = m {
                s.refresh_mhz = Some(refresh_rates(m, current_resolution(s, Some(m)))[i]);
            }
        }
        Item::VSync => s.vsync = i == 1,
        Item::Internal => s.internal_height = INTERNAL_HEIGHTS[i],
        Item::Master => s.master_volume = steps01()[i],
        Item::Effects => s.effects_volume = steps01()[i],
        Item::Sensitivity => s.sensitivity = SENSITIVITIES[i],
        Item::InvertY => s.invert_y = i == 1,
        Item::Shake => s.camera_shake = i == 1,
        Item::Resume | Item::Restart | Item::Quit => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn activate(
    item: Item,
    menu: &mut MenuState,
    cursor: &mut CursorOptions,
    settings: &mut Settings,
    m: Option<&Monitor>,
    reset: &mut ResetFight,
    exit: &mut MessageWriter<AppExit>,
) {
    match item {
        Item::Resume => set_open(menu, cursor, false),
        Item::Restart => {
            reset.requested = true;
            set_open(menu, cursor, false);
        }
        Item::Quit => {
            exit.write(AppExit::Success);
        }
        // Valider une option la fait avancer d'un cran (en bouclant pour les choix binaires).
        other => {
            let before = settings.clone();
            change(other, 1, settings, m);
            if *settings == before {
                if let Some(c) = choices(other, settings, m) {
                    change(other, -(c.labels.len() as i32), settings, m);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn navigate(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    monitors: Query<(&Monitor, Has<PrimaryMonitor>)>,
    mut menu: ResMut<MenuState>,
    mut settings: ResMut<Settings>,
    mut reset: ResMut<ResetFight>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !menu.open {
        return;
    }
    let list = items();
    let m = crate::settings::pick_monitor(monitors.iter());
    let k = |codes: &[KeyCode]| codes.iter().any(|c| keys.just_pressed(*c));
    let mut dir = IVec2::ZERO;
    if k(&[KeyCode::ArrowUp, KeyCode::KeyW]) {
        dir.y -= 1;
    }
    if k(&[KeyCode::ArrowDown, KeyCode::KeyS]) {
        dir.y += 1;
    }
    if k(&[KeyCode::ArrowLeft, KeyCode::KeyA]) {
        dir.x -= 1;
    }
    if k(&[KeyCode::ArrowRight, KeyCode::KeyD]) {
        dir.x += 1;
    }
    let mut confirm = k(&[KeyCode::Enter, KeyCode::Space]);
    // Manette : D-pad ou stick, avec répétition quand on maintient.
    let mut held = Vec2::ZERO;
    for g in &gamepads {
        confirm |= g.just_pressed(GamepadButton::South);
        let mut v = g.left_stick();
        if g.pressed(GamepadButton::DPadUp) {
            v.y = 1.0;
        }
        if g.pressed(GamepadButton::DPadDown) {
            v.y = -1.0;
        }
        if g.pressed(GamepadButton::DPadLeft) {
            v.x = -1.0;
        }
        if g.pressed(GamepadButton::DPadRight) {
            v.x = 1.0;
        }
        if v.length() > 0.6 {
            held = v;
        }
    }
    if held != Vec2::ZERO {
        menu.repeat -= time.delta_secs();
        if menu.repeat <= 0.0 {
            if held.y.abs() > held.x.abs() {
                dir.y += if held.y > 0.0 { -1 } else { 1 };
            } else {
                dir.x += if held.x > 0.0 { 1 } else { -1 };
            }
            menu.repeat = if menu.repeat < -1.0 { 0.35 } else { 0.16 };
        }
    } else {
        menu.repeat = -2.0;
    }

    if dir.y != 0 {
        menu.selected = (menu.selected as i32 + dir.y).rem_euclid(list.len() as i32) as usize;
    }
    let item = list[menu.selected];
    if dir.x != 0 {
        change(item, dir.x, &mut settings, m);
    }
    if confirm {
        activate(item, &mut menu, &mut cursor, &mut settings, m, &mut reset, &mut exit);
    }
}

#[allow(clippy::too_many_arguments)]
fn mouse_input(
    rows: Query<(&Row, &Interaction), Changed<Interaction>>,
    arrows: Query<(&Arrow, &Interaction), Changed<Interaction>>,
    monitors: Query<(&Monitor, Has<PrimaryMonitor>)>,
    mut menu: ResMut<MenuState>,
    mut settings: ResMut<Settings>,
    mut reset: ResMut<ResetFight>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !menu.open {
        return;
    }
    let list = items();
    let m = crate::settings::pick_monitor(monitors.iter());
    for (a, i) in &arrows {
        if *i == Interaction::Pressed {
            menu.selected = a.0;
            change(list[a.0], a.1, &mut settings, m);
            return;
        }
    }
    for (r, i) in &rows {
        match i {
            Interaction::Hovered => menu.selected = r.0,
            Interaction::Pressed => {
                menu.selected = r.0;
                if matches!(list[r.0], Item::Resume | Item::Restart | Item::Quit) {
                    activate(list[r.0], &mut menu, &mut cursor, &mut settings, m, &mut reset, &mut exit);
                    return;
                }
            }
            Interaction::None => {}
        }
    }
}

fn label(item: Item) -> &'static str {
    match item {
        Item::Resume => "Reprendre",
        Item::Display => "Affichage",
        Item::Resolution => "Résolution",
        Item::Refresh => "Fréquence",
        Item::VSync => "Synchronisation verticale",
        Item::Internal => "Résolution interne",
        Item::Master => "Volume général",
        Item::Effects => "Volume des effets",
        Item::Sensitivity => "Sensibilité de la caméra",
        Item::InvertY => "Inverser l'axe vertical",
        Item::Shake => "Tremblements de caméra",
        Item::Restart => "Recommencer le combat",
        Item::Quit => "Quitter le jeu",
    }
}

#[allow(clippy::type_complexity)]
fn refresh_menu(
    menu: Res<MenuState>,
    settings: Res<Settings>,
    monitors: Query<(&Monitor, Has<PrimaryMonitor>)>,
    mut root: Single<&mut Visibility, With<MenuRoot>>,
    mut rows: Query<(&Row, &mut BackgroundColor)>,
    mut labels: Query<(&RowLabel, &mut Text, &mut TextColor), Without<RowValue>>,
    mut values: Query<(&RowValue, &mut Text, &mut TextColor), Without<RowLabel>>,
    mut arrows: Query<(&Arrow, &mut Visibility), Without<MenuRoot>>,
) {
    let want = if menu.open { Visibility::Inherited } else { Visibility::Hidden };
    if **root != want {
        **root = want;
    }
    if !menu.open {
        return;
    }
    let list = items();
    let m = crate::settings::pick_monitor(monitors.iter());
    for (r, mut bg) in &mut rows {
        bg.0 = if r.0 == menu.selected { Color::srgba(0.9, 0.82, 0.62, 0.16) } else { Color::NONE };
    }
    for (l, mut t, mut c) in &mut labels {
        let s = label(list[l.0]);
        if t.0 != s {
            t.0 = s.into();
        }
        c.0 = if l.0 == menu.selected { Color::srgb(1.0, 0.95, 0.85) } else { Color::srgb(0.78, 0.75, 0.68) };
    }
    for (v, mut t, mut c) in &mut values {
        let ch = choices(list[v.0], &settings, m);
        let s = ch.as_ref().map(|c| c.labels[c.current].clone()).unwrap_or_default();
        if t.0 != s {
            t.0 = s;
        }
        let enabled = ch.as_ref().is_some_and(|c| c.enabled);
        c.0 = if enabled { Color::srgb(0.95, 0.92, 0.85) } else { Color::srgba(0.6, 0.58, 0.55, 0.6) };
    }
    for (a, mut vis) in &mut arrows {
        let show = choices(list[a.0], &settings, m).is_some_and(|c| c.enabled && c.labels.len() > 1);
        let w = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != w {
            *vis = w;
        }
    }
}
