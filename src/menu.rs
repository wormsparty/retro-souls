//! Menus : écran titre, pause, checkpoint (voyage, niveau, équipement), options, aide.
//!
//! Navigation : haut/bas pour choisir, gauche/droite pour changer une valeur, Entrée / (A)
//! pour valider, Échap / (B) pour revenir. Échap / Start ouvre le menu pause, Start le referme.
//! La souris marche aussi : survoler une ligne la sélectionne (seulement si la souris bouge,
//! pour ne pas voler la sélection quand une page s'ouvre sous le curseur), clic pour valider,
//! clic sur ‹ / › pour changer la valeur.
//!
//! Les actions s'exécutent avec un accès complet au monde (`handle`), ce qui permet de lancer
//! une partie, de sauvegarder ou d'appliquer un équipement directement.

use bevy::prelude::*;
use bevy::text::LineBreak;
use bevy::window::{CursorGrabMode, CursorOptions, Monitor, PrimaryMonitor, PrimaryWindow};

use crate::fx::FxState;
use crate::input::Device;
use crate::lang::{Lang, tr};
use crate::render::camera::CameraRig;
use crate::render::{AppState, LocalPlayer};
use crate::save::{SaveData, SaveSlot};
use crate::settings::{
    DisplayMode, INTERNAL_HEIGHTS, MODERN_HEIGHT, PS1_HEIGHT, Settings, exclusive_sizes, refresh_rates,
    windowed_sizes,
};
use crate::sim::data::Tuning;
use crate::sim::encounter::{Encounter, Progress, SimCommand, SimCommands, apply_commands, near_checkpoint};
use crate::sim::fighter::Body;
use crate::sim::items::{Item, Kind, QUICK_SLOTS};
use crate::sim::player::Player;
use crate::sim::{ResetFight, SimEntity, SimEvent, SimEvents};
use crate::render::preview::{CheckpointPreview, PREVIEW_SIZE};
use crate::ui::{Glyph, Hint, Icons, Seg, UiFont, hint_node, i, icon_bundle, image_bundle, set_hint, t};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Title,
    ConfirmNew,
    Pause,
    Checkpoint,
    /// Voyage rapide entre checkpoints (avec un aperçu du lieu).
    Travel,
    Equipment,
    Options,
    Help,
    /// Choix de la langue au premier lancement.
    Language,
    /// Choix du style graphique (PS1 ou moderne) au premier lancement.
    Style,
}

#[derive(Resource)]
pub struct MenuState {
    pub open: bool,
    page: Page,
    /// Pages précédentes (et ligne sélectionnée), pour revenir en arrière.
    stack: Vec<(Page, usize)>,
    selected: usize,
    /// Répétition de navigation au stick/D-pad maintenu.
    repeat: f32,
    /// Ouvert à cette frame : on ignore les entrées (le bouton qui l'a ouvert, par exemple (A)
    /// pour se reposer, ne doit pas aussi valider la première ligne).
    fresh: bool,
    /// Dernière position connue du curseur (le survol ne compte que s'il a bougé).
    cursor: Option<Vec2>,
    /// Temps pendant lequel on ignore les mouvements du curseur après l'ouverture (il peut
    /// sauter quand on le libère).
    settle: f32,
}

impl Default for MenuState {
    fn default() -> Self {
        Self { open: false, page: Page::Pause, stack: Vec::new(), selected: 0, repeat: 0.0, fresh: false, cursor: None, settle: 0.0 }
    }
}

impl MenuState {
    pub fn on_title(&self) -> bool {
        self.open && matches!(self.page, Page::Title | Page::Language | Page::Style)
    }

    /// Ouvre le menu sur une page (sans toucher au curseur).
    pub fn open(&mut self, page: Page) {
        self.open = true;
        self.fresh = true;
        self.settle = 0.25;
        self.page = page;
        self.stack.clear();
        self.selected = 0;
    }

    fn push(&mut self, page: Page) {
        self.stack.push((self.page, self.selected));
        self.page = page;
        self.selected = 0;
    }

    /// Revient à la page précédente ; faux s'il n'y en a pas.
    fn back(&mut self) -> bool {
        match self.stack.pop() {
            Some((p, s)) => {
                self.page = p;
                self.selected = s;
                true
            }
            None => false,
        }
    }
}

pub fn menu_closed(menu: Res<MenuState>) -> bool {
    !menu.open
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Resume,
    Leave,
    NewGame,
    ConfirmNew,
    Load,
    Open(Page),
    Back,
    ToTitle,
    Quit,
    ReviveBoss,
    /// Monter de niveau (pas encore disponible : affiché grisé).
    LevelUp,
}

/// Option de réglage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Opt {
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
    ShowFps,
    Language,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Entry {
    Act(Act),
    Opt(Opt),
    /// Emplacement rapide d'objet.
    Slot(u8),
    /// Talisman porté.
    Talisman,
    /// Destination de voyage (index du checkpoint).
    Place(u8),
    /// Ligne d'aide (index dans `help_lines`), non sélectionnable.
    Line(u8),
    /// Langue proposée (page de choix de la langue).
    Lang(Lang),
    /// Style graphique proposé : sa résolution interne.
    Style(u32),
}

/// Ce dont dépend le contenu des pages.
#[derive(Clone, Copy, PartialEq)]
struct PageCtx {
    has_save: bool,
    boss_defeated: bool,
    device: Device,
    /// Checkpoints découverts par le joueur local (bits), et celui où il se trouve.
    found: u32,
    here: Option<u8>,
}

const NATIVE: bool = !cfg!(target_arch = "wasm32");

/// Lignes de la page d'aide : action, touches.
fn help_lines(device: Device) -> Vec<(&'static str, Vec<Seg>)> {
    let or = || t(tr("or", "ou"));
    if device == Device::Gamepad {
        vec![
            (tr("Move / camera", "Déplacement / caméra"), vec![i(Glyph::StickL), t("/"), i(Glyph::StickR)]),
            (tr("Light attack", "Attaque légère"), vec![i(Glyph::PadRB)]),
            (tr("Heavy attack (hold: charge)", "Attaque lourde (maintenir : charge)"), vec![i(Glyph::PadRT)]),
            (tr("Guard (well-timed: perfect)", "Garde (au bon moment : parfaite)"), vec![i(Glyph::PadLB)]),
            (tr("Special attack", "Attaque spéciale"), vec![i(Glyph::PadLT)]),
            (tr("Dodge", "Esquive"), vec![i(Glyph::PadB)]),
            (tr("Sprint", "Course"), vec![t(tr("hold", "maintenir")), i(Glyph::PadB), or(), i(Glyph::StickL3)]),
            (tr("Use item", "Utiliser l'objet"), vec![i(Glyph::PadX)]),
            (tr("Next item", "Objet suivant"), vec![i(Glyph::DpadDown)]),
            (tr("Switch weapon", "Changer d'arme"), vec![i(Glyph::DpadRight)]),
            (tr("Lock on", "Verrouillage"), vec![i(Glyph::StickR3)]),
            (tr("Rest (checkpoint)", "Se reposer (checkpoint)"), vec![i(Glyph::PadA)]),
            (tr("Menu", "Menu"), vec![i(Glyph::PadMenu)]),
            (tr("Back (menus)", "Retour (menus)"), vec![i(Glyph::PadB)]),
        ]
    } else {
        vec![
            (
                tr("Move", "Déplacement"),
                vec![i(Glyph::Key("W")), i(Glyph::Key("A")), i(Glyph::Key("S")), i(Glyph::Key("D")), t("AZERTY"), i(Glyph::Key("Z")), i(Glyph::Key("Q"))],
            ),
            (tr("Camera", "Caméra"), vec![i(Glyph::MouseMove), t(tr("(click to capture)", "(clic pour capturer)"))]),
            (tr("Light attack", "Attaque légère"), vec![i(Glyph::MouseLeft)]),
            (tr("Heavy attack (hold: charge)", "Attaque lourde (maintenir : charge)"), vec![i(Glyph::MouseRight)]),
            (
                tr("Guard (well-timed: perfect)", "Garde (au bon moment : parfaite)"),
                vec![i(Glyph::Key("Q")), or(), i(Glyph::Key(tr("SHIFT", "MAJ"))), t("AZERTY"), i(Glyph::Key("A"))],
            ),
            (tr("Special attack", "Attaque spéciale"), vec![i(Glyph::Key("E"))]),
            (tr("Dodge (hold: sprint)", "Esquive (maintenir : course)"), vec![i(Glyph::Key(tr("SPACE", "ESPACE")))]),
            (tr("Use item", "Utiliser l'objet"), vec![i(Glyph::Key("F"))]),
            (tr("Next item", "Objet suivant"), vec![i(Glyph::Key("C"))]),
            (tr("Switch weapon", "Changer d'arme"), vec![i(Glyph::Key("R"))]),
            (tr("Lock on", "Verrouillage"), vec![i(Glyph::Key("TAB")), or(), i(Glyph::MouseMiddle)]),
            (tr("Rest (checkpoint)", "Se reposer (checkpoint)"), vec![i(Glyph::Key("G"))]),
            (tr("Menu", "Menu"), vec![i(Glyph::Key("ESC"))]),
            (tr("Tuning (debug)", "Réglages (debug)"), vec![i(Glyph::Key("F1")), t(tr("to", "à")), i(Glyph::Key("F5"))]),
        ]
    }
}

fn entries(page: Page, c: &PageCtx) -> Vec<Entry> {
    use Act::*;
    let mut v = match page {
        // Avec une sauvegarde, « Continuer » vient en premier (choix par défaut).
        Page::Title if c.has_save => vec![Entry::Act(Load), Entry::Act(NewGame), Entry::Act(Open(Page::Options))],
        Page::Title => vec![Entry::Act(NewGame), Entry::Act(Load), Entry::Act(Open(Page::Options))],
        Page::ConfirmNew => vec![Entry::Act(Back), Entry::Act(ConfirmNew)],
        Page::Pause => vec![
            Entry::Act(Resume),
            Entry::Act(Open(Page::Equipment)),
            Entry::Act(Open(Page::Options)),
            Entry::Act(Open(Page::Help)),
            Entry::Act(ToTitle),
        ],
        Page::Checkpoint => {
            // « Partir » d'abord : c'est la ligne sélectionnée à l'ouverture.
            let mut v = vec![Entry::Act(Leave), Entry::Act(Open(Page::Travel)), Entry::Act(LevelUp), Entry::Act(Open(Page::Equipment))];
            if c.boss_defeated {
                v.push(Entry::Act(ReviveBoss));
            }
            v
        }
        Page::Travel => (0..32u8).filter(|i| c.found & (1 << i) != 0).map(Entry::Place).chain([Entry::Act(Back)]).collect(),
        Page::Equipment => {
            (0..QUICK_SLOTS as u8).map(Entry::Slot).chain([Entry::Talisman, Entry::Act(Back)]).collect()
        }
        Page::Options => {
            use Opt::*;
            let opts: &[Opt] = if NATIVE {
                &[Language, Display, Resolution, Refresh, VSync, Internal, Master, Effects, Sensitivity, InvertY, Shake, ShowFps]
            } else {
                // Dans le navigateur, résolution, fréquence et VSync sont gérées par le navigateur.
                &[Language, Display, Internal, Master, Effects, Sensitivity, InvertY, Shake, ShowFps]
            };
            opts.iter().map(|o| Entry::Opt(*o)).chain([Entry::Act(Back)]).collect()
        }
        Page::Help => (0..help_lines(c.device).len() as u8).map(Entry::Line).chain([Entry::Act(Back)]).collect(),
        Page::Language => Lang::ALL.into_iter().map(Entry::Lang).collect(),
        Page::Style => vec![Entry::Style(PS1_HEIGHT), Entry::Style(MODERN_HEIGHT)],
    };
    if NATIVE && matches!(page, Page::Title | Page::Pause) {
        v.push(Entry::Act(Quit));
    }
    v
}

fn selectable(e: Entry, c: &PageCtx) -> bool {
    match e {
        Entry::Line(..) | Entry::Act(Act::LevelUp) => false,
        Entry::Place(i) => c.here != Some(i),
        Entry::Act(Act::Load) => c.has_save,
        _ => true,
    }
}

fn act_label(a: Act) -> &'static str {
    match a {
        Act::Resume => tr("Resume", "Reprendre"),
        Act::Leave => tr("Leave", "Partir"),
        Act::NewGame => tr("New game", "Nouvelle partie"),
        Act::ConfirmNew => tr("Start a new game", "Commencer une nouvelle partie"),
        Act::Load => tr("Continue", "Continuer"),
        Act::Open(Page::Equipment) => tr("Equipment", "Équipement"),
        Act::Open(Page::Options) => tr("Options", "Options"),
        Act::Open(Page::Help) => tr("Help", "Aide"),
        Act::Open(Page::Travel) => tr("Travel", "Voyager"),
        Act::Open(_) => "…",
        Act::Back => tr("Back", "Retour"),
        Act::ToTitle => tr("Return to title screen", "Retour à l'écran titre"),
        Act::Quit => tr("Quit game", "Quitter le jeu"),
        Act::ReviveBoss => tr("Revive the Automaton", "Ranimer l'Automate"),
        Act::LevelUp => tr("Level up", "Monter de niveau"),
    }
}

fn opt_label(o: Opt) -> &'static str {
    match o {
        Opt::Display => tr("Display", "Affichage"),
        Opt::Resolution => tr("Resolution", "Résolution"),
        Opt::Refresh => tr("Refresh rate", "Fréquence"),
        Opt::VSync => tr("Vertical sync", "Synchronisation verticale"),
        Opt::Internal => tr("Internal resolution", "Résolution interne"),
        Opt::Master => tr("Master volume", "Volume général"),
        Opt::Effects => tr("Effects volume", "Volume des effets"),
        Opt::Sensitivity => tr("Camera sensitivity", "Sensibilité de la caméra"),
        Opt::InvertY => tr("Invert vertical axis", "Inverser l'axe vertical"),
        Opt::Shake => tr("Camera shake", "Tremblements de caméra"),
        Opt::ShowFps => tr("Show FPS", "Afficher les FPS"),
        Opt::Language => "Language / Langue",
    }
}

fn page_title(p: Page, device: Device) -> &'static str {
    match p {
        Page::Title => "GIANT'S FLAME",
        Page::ConfirmNew => tr("NEW GAME", "NOUVELLE PARTIE"),
        Page::Pause => "PAUSE",
        Page::Checkpoint => "CHECKPOINT",
        Page::Travel => tr("TRAVEL", "VOYAGER"),
        Page::Equipment => tr("EQUIPMENT", "ÉQUIPEMENT"),
        Page::Options => "OPTIONS",
        Page::Help if device == Device::Gamepad => tr("HELP — GAMEPAD", "AIDE — MANETTE"),
        Page::Help => tr("HELP — KEYBOARD AND MOUSE", "AIDE — CLAVIER ET SOURIS"),
        Page::Language => "LANGUAGE / LANGUE",
        Page::Style => tr("GRAPHICS STYLE", "STYLE GRAPHIQUE"),
    }
}

fn page_info(p: Page) -> &'static str {
    match p {
        Page::Title => tr("The Carousel Automaton", "L'Automate du Carrousel"),
        Page::ConfirmNew => tr("The current save will be overwritten.", "La sauvegarde actuelle sera remplacée."),
        Page::Checkpoint => tr("You rest. HP, stamina and items restored.", "Vous vous reposez. PV, endurance et objets restaurés."),
        Page::Travel => tr("Travel to a brazier you have already kindled.", "Rejoindre un brasier déjà ranimé."),
        Page::Style => tr(
            "Can be changed at any time in Options (Internal resolution).",
            "Modifiable à tout moment dans les Options (Résolution interne).",
        ),
        Page::Equipment => tr("Quick slot items (“Next item” cycles through them in game), and your talisman.", "Objets des emplacements rapides (« Objet suivant » passe de l'un à l'autre en jeu), et talisman."),
        _ => "",
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
    if crate::settings::exclusive_supported() {
        vec![DisplayMode::Fullscreen, DisplayMode::Exclusive, DisplayMode::Windowed]
    } else {
        vec![DisplayMode::Fullscreen, DisplayMode::Windowed]
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

fn choices(item: Opt, s: &Settings, m: Option<&Monitor>) -> Option<Choices> {
    let yes_no = |b: bool, on: &str, off: &str| Choices {
        labels: vec![off.into(), on.into()],
        current: b as usize,
        enabled: true,
    };
    Some(match item {
                Opt::Display => {
            let modes = display_modes();
            Choices {
                labels: modes
                    .iter()
                    .map(|d| match d {
                        DisplayMode::Fullscreen => tr("Fullscreen", "Plein écran"),
                        DisplayMode::Exclusive => tr("Exclusive fullscreen", "Plein écran exclusif"),
                        DisplayMode::Windowed => tr("Windowed", "Fenêtré"),
                    }.into())
                    .collect(),
                current: modes.iter().position(|d| *d == s.display).unwrap_or(0),
                enabled: true,
            }
        }
        Opt::Resolution => {
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
        Opt::Refresh => {
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
        Opt::VSync => yes_no(s.vsync, tr("On", "Activée"), tr("Off", "Désactivée")),
        Opt::Internal => Choices {
            labels: INTERNAL_HEIGHTS
                .iter()
                .map(|h| if *h == PS1_HEIGHT { format!("{h}p (PS1)") } else { format!("{h}p ({})", tr("Modern", "Moderne")) })
                .collect(),
            current: INTERNAL_HEIGHTS.iter().position(|h| *h == s.internal_height).unwrap_or(0),
            enabled: true,
        },
        Opt::Master => Choices { labels: steps01().iter().map(|v| pct(*v)).collect(), current: nearest(&steps01(), s.master_volume), enabled: true },
        Opt::Effects => Choices { labels: steps01().iter().map(|v| pct(*v)).collect(), current: nearest(&steps01(), s.effects_volume), enabled: true },
        Opt::Sensitivity => Choices {
            labels: SENSITIVITIES.iter().map(|v| format!("{v:.2}")).collect(),
            current: nearest(&SENSITIVITIES, s.sensitivity),
            enabled: true,
        },
        Opt::InvertY => yes_no(s.invert_y, tr("Yes", "Oui"), tr("No", "Non")),
        Opt::Shake => yes_no(s.camera_shake, tr("Yes", "Oui"), tr("No", "Non")),
        Opt::ShowFps => yes_no(s.show_fps, tr("Yes", "Oui"), tr("No", "Non")),
        Opt::Language => Choices {
            labels: Lang::ALL.iter().map(|l| l.native_name().into()).collect(),
            current: Lang::ALL.iter().position(|l| *l == s.language.unwrap_or_default()).unwrap_or(0),
            enabled: true,
        },
    })
}

/// Change la valeur d'une option de `delta` crans (sans boucler).
fn change(item: Opt, delta: i32, s: &mut Settings, m: Option<&Monitor>) {
    let Some(c) = choices(item, s, m) else { return };
    if !c.enabled || c.labels.len() < 2 {
        return;
    }
    let i = (c.current as i32 + delta).clamp(0, c.labels.len() as i32 - 1) as usize;
    if i == c.current {
        return;
    }
    match item {
        Opt::Display => {
            s.display = display_modes()[i];
            // La résolution choisie dépend du mode : on repart sur l'automatique.
            s.resolution = None;
            s.refresh_mhz = None;
        }
        Opt::Resolution => {
            s.resolution = Some(resolutions(s, m)[i]);
            s.refresh_mhz = None;
        }
        Opt::Refresh => {
            if let Some(m) = m {
                s.refresh_mhz = Some(refresh_rates(m, current_resolution(s, Some(m)))[i]);
            }
        }
        Opt::VSync => s.vsync = i == 1,
        Opt::Internal => s.internal_height = INTERNAL_HEIGHTS[i],
        Opt::Master => s.master_volume = steps01()[i],
        Opt::Effects => s.effects_volume = steps01()[i],
        Opt::Sensitivity => s.sensitivity = SENSITIVITIES[i],
        Opt::InvertY => s.invert_y = i == 1,
        Opt::Shake => s.camera_shake = i == 1,
        Opt::ShowFps => s.show_fps = i == 1,
        Opt::Language => s.language = Some(Lang::ALL[i]),
    }
}


#[derive(Component)]
struct MenuRoot;
#[derive(Component)]
struct MenuTitle;
#[derive(Component)]
struct MenuInfo;
#[derive(Component)]
struct MenuList;
/// Aperçu du lieu (page de voyage).
#[derive(Component)]
struct MenuPreview;
#[derive(Component)]
struct MenuPreviewCaption;
/// Description de l'objet sélectionné (page d'équipement).
#[derive(Component)]
struct MenuDetail;
#[derive(Component)]
struct MenuFooter;
#[derive(Component)]
struct Row(usize);
#[derive(Component)]
struct RowLabel(usize);
#[derive(Component)]
struct RowValue(usize);
#[derive(Component)]
struct Arrow(usize, i32);

/// Illustrations des deux styles graphiques (captures du jeu en 240p et en 480p).
#[derive(Resource)]
struct StyleImages {
    ps1: Handle<Image>,
    modern: Handle<Image>,
}

/// Taille des illustrations, en points.
const STYLE_IMAGE_SIZE: UVec2 = UVec2::new(192, 108);

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .add_systems(Startup, spawn_menu)
            .add_systems(OnEnter(AppState::Title), enter_title)
            .add_systems(
                Update,
                (open_on_rest.run_if(in_state(AppState::Playing)), menu_input, refresh_menu)
                    .chain()
                    .after(crate::fx::consume_events)
                    .run_if(not(in_state(AppState::Loading))),
            );
    }
}

fn spawn_menu(mut commands: Commands, ui_font: Res<UiFont>, preview: Res<CheckpointPreview>, assets: Res<AssetServer>) {
    commands.insert_resource(StyleImages { ps1: assets.load("ui/style_ps1.png"), modern: assets.load("ui/style_modern.png") });
    let text = Color::srgba(0.85, 0.82, 0.75, 0.75);
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
            c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(6), padding: UiRect::all(px(16)), width: px(960), ..default() })
                .with_children(|c| {
                    c.spawn((ui_font.text("", 2, Color::srgb(0.9, 0.82, 0.62)), MenuTitle));
                    c.spawn((ui_font.text("", 1, text), Node { margin: UiRect::bottom(px(10)), ..default() }, MenuInfo));
                    c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(24), align_items: AlignItems::FlexStart, ..default() })
                        .with_children(|c| {
                            c.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(2), flex_grow: 1.0, ..default() }, MenuList));
                            c.spawn((
                                Node {
                                    display: Display::None,
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(6),
                                    padding: UiRect::all(px(4)),
                                    border: UiRect::all(px(2)),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                                BorderColor::all(Color::srgb(0.62, 0.55, 0.42)),
                                BackgroundColor(Color::BLACK),
                                MenuPreview,
                            ))
                            .with_children(|c| {
                                c.spawn(image_bundle(preview.image.clone(), PREVIEW_SIZE));
                                c.spawn((ui_font.text("", 1, Color::srgb(0.9, 0.82, 0.62)), MenuPreviewCaption));
                            });
                        });
                    c.spawn((
                        ui_font.text("", 1, Color::srgb(0.9, 0.82, 0.62)),
                        Node { display: Display::None, margin: UiRect::top(px(10)), ..default() },
                        MenuDetail,
                    ));
                    c.spawn((Hint::new(1, text), Node { margin: UiRect::top(px(10)), ..hint_node() }, MenuFooter));
                });
        });
}

fn cursor_free(w: &mut World, free: bool) {
    let mut q = w.query_filtered::<&mut CursorOptions, With<PrimaryWindow>>();
    let Ok(mut c) = q.single_mut(w) else { return };
    if free {
        c.grab_mode = CursorGrabMode::None;
        c.visible = true;
    } else if NATIVE {
        c.grab_mode = CursorGrabMode::Locked;
        c.visible = false;
    }
}

fn open_page(w: &mut World, page: Page) {
    w.resource_mut::<MenuState>().open(page);
    cursor_free(w, true);
}

fn close(w: &mut World) {
    w.resource_mut::<MenuState>().open = false;
    cursor_free(w, false);
}

/// Écran titre : plus aucun combattant dans le monde.
pub fn enter_title(mut commands: Commands) {
    commands.queue(|w: &mut World| {
        let sim: Vec<Entity> = w.query_filtered::<Entity, With<SimEntity>>().iter(w).collect();
        for e in sim {
            w.despawn(e);
        }
        *w.resource_mut::<Encounter>() = Encounter::default();
        w.resource_mut::<SimEvents>().0.clear();
        w.resource_mut::<SimCommands>().0.clear();
        w.resource_mut::<CameraRig>().initialized = false;
        let page = first_launch_page(w.resource::<Settings>());
        open_page(w, page);
    });
}

/// Premier lancement : on demande d'abord la langue, puis le style graphique.
fn first_launch_page(s: &Settings) -> Page {
    if s.language.is_none() {
        Page::Language
    } else if !s.style_chosen {
        Page::Style
    } else {
        Page::Title
    }
}

pub enum Launch {
    New,
    Load,
}

/// Lance une partie (nouvelle, ou depuis la sauvegarde).
pub fn launch(w: &mut World, how: Launch) {
    let t = w.resource::<Tuning>().clone();
    let (progress, play_time) = match how {
        Launch::New => (Progress::new_game(&t), 0.0),
        Launch::Load => match w.resource::<SaveSlot>().data.clone() {
            Some(d) => (d.progress, d.play_time),
            None => return,
        },
    };
    {
        let mut slot = w.resource_mut::<SaveSlot>();
        slot.play_time = play_time;
        if matches!(how, Launch::New) {
            slot.store(SaveData { version: crate::save::VERSION, play_time, progress: progress.clone() });
        }
    }
    *w.resource_mut::<ResetFight>() = ResetFight { requested: true, players: 1, progress: Some(progress) };
    w.resource_mut::<NextState<AppState>>().set(AppState::Playing);
    close(w);
}

/// Repos au checkpoint : ouvre le menu du checkpoint.
fn open_on_rest(mut commands: Commands, fx: Res<FxState>, local: Query<(), With<LocalPlayer>>) {
    let rested = fx.last.iter().any(|e| matches!(e, SimEvent::Rested { entity } if local.contains(*entity)));
    if rested {
        commands.queue(|w: &mut World| open_page(w, Page::Checkpoint));
    }
}

#[derive(Clone, Copy, Debug)]
enum Intent {
    /// Ouvrir le menu pause.
    Pause,
    /// Page précédente (ou fermer).
    Back,
    /// Fermer le menu (Start).
    Close,
    Move(i32),
    Change(i32),
    Confirm,
    Hover(usize),
    Click(usize),
    Arrow(usize, i32),
}

#[allow(clippy::too_many_arguments)]
fn menu_input(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<State<AppState>>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    window: Single<&Window, With<PrimaryWindow>>,
    rows: Query<(&Row, &Interaction)>,
    pressed: Query<(&Row, &Interaction), Changed<Interaction>>,
    arrows: Query<(&Arrow, &Interaction), Changed<Interaction>>,
    mut menu: ResMut<MenuState>,
) {
    let cursor = window.cursor_position();
    let moved = menu.settle <= 0.0 && matches!((menu.cursor, cursor), (Some(a), Some(b)) if a.distance(b) > 0.5);
    menu.settle -= time.delta_secs();
    if cursor.is_some() {
        menu.cursor = cursor;
    }
    if menu.fresh {
        menu.fresh = false;
        return;
    }
    let mut out = Vec::new();
    let k = |codes: &[KeyCode]| codes.iter().any(|c| keys.just_pressed(*c));
    let pad = |b: GamepadButton| gamepads.iter().any(|g| g.just_pressed(b));
    let esc = keys.just_pressed(KeyCode::Escape);
    let start = pad(GamepadButton::Start) || pad(GamepadButton::Select);
    if !menu.open {
        if (esc || start) && *state.get() == AppState::Playing {
            out.push(Intent::Pause);
        }
    } else {
        if esc || pad(GamepadButton::East) {
            out.push(Intent::Back);
        } else if start {
            out.push(Intent::Close);
        }
        if k(&[KeyCode::ArrowUp, KeyCode::KeyW]) {
            out.push(Intent::Move(-1));
        }
        if k(&[KeyCode::ArrowDown, KeyCode::KeyS]) {
            out.push(Intent::Move(1));
        }
        if k(&[KeyCode::ArrowLeft, KeyCode::KeyA]) {
            out.push(Intent::Change(-1));
        }
        if k(&[KeyCode::ArrowRight, KeyCode::KeyD]) {
            out.push(Intent::Change(1));
        }
        if k(&[KeyCode::Enter, KeyCode::Space]) || pad(GamepadButton::South) {
            out.push(Intent::Confirm);
        }
        // Manette : D-pad ou stick, avec répétition quand on maintient.
        let mut held = Vec2::ZERO;
        for g in &gamepads {
            let mut v = g.left_stick();
            for (b, d) in [
                (GamepadButton::DPadUp, Vec2::Y),
                (GamepadButton::DPadDown, -Vec2::Y),
                (GamepadButton::DPadLeft, -Vec2::X),
                (GamepadButton::DPadRight, Vec2::X),
            ] {
                if g.pressed(b) {
                    v = d;
                }
            }
            if v.length() > 0.6 {
                held = v;
            }
        }
        if held != Vec2::ZERO {
            menu.repeat -= time.delta_secs();
            if menu.repeat <= 0.0 {
                out.push(if held.y.abs() > held.x.abs() {
                    Intent::Move(if held.y > 0.0 { -1 } else { 1 })
                } else {
                    Intent::Change(if held.x > 0.0 { 1 } else { -1 })
                });
                menu.repeat = if menu.repeat < -1.0 { 0.35 } else { 0.16 };
            }
        } else {
            menu.repeat = -2.0;
        }
        let mut arrow_clicked = false;
        for (a, i) in &arrows {
            if *i == Interaction::Pressed {
                out.push(Intent::Arrow(a.0, a.1));
                arrow_clicked = true;
            }
        }
        // Survol : seulement quand la souris bouge (une page qui s'ouvre sous le curseur, au
        // centre de l'écran, ne doit pas changer la ligne sélectionnée par défaut).
        if moved {
            for (r, i) in &rows {
                if *i == Interaction::Hovered {
                    out.push(Intent::Hover(r.0));
                }
            }
        }
        for (r, i) in &pressed {
            if *i == Interaction::Pressed && !arrow_clicked {
                out.push(Intent::Click(r.0));
            }
        }
    }
    if !out.is_empty() {
        commands.queue(move |w: &mut World| {
            for i in out {
                handle(w, i);
            }
        });
    }
}

fn page_ctx(w: &mut World) -> PageCtx {
    let (found, here) = local_place(w);
    PageCtx {
        has_save: w.resource::<SaveSlot>().data.is_some(),
        boss_defeated: w.resource::<Encounter>().boss_defeated,
        device: *w.resource::<Device>(),
        found,
        here,
    }
}

/// Checkpoints découverts par le joueur local, et celui près duquel il se trouve.
fn local_place(w: &mut World) -> (u32, Option<u8>) {
    let t = w.resource::<Tuning>().clone();
    let mut q = w.query::<(&Player, &Body)>();
    q.iter(w).min_by_key(|(p, _)| p.id).map_or((0, None), |(p, b)| (p.found, near_checkpoint(&t, b.pos)))
}

fn local_player(w: &mut World) -> Option<Player> {
    let mut q = w.query::<&Player>();
    q.iter(w).min_by_key(|p| p.id).cloned()
}

fn handle(w: &mut World, intent: Intent) {
    let ctx = page_ctx(w);
    let (open, page, selected) = {
        let m = w.resource::<MenuState>();
        (m.open, m.page, m.selected)
    };
    if !open {
        if matches!(intent, Intent::Pause) {
            open_page(w, Page::Pause);
        }
        return;
    }
    let list = entries(page, &ctx);
    match intent {
        Intent::Pause => {}
        Intent::Back => {
            if !w.resource_mut::<MenuState>().back() && !w.resource::<MenuState>().on_title() {
                close(w);
            }
        }
        Intent::Close => {
            if !w.resource::<MenuState>().on_title() {
                close(w);
            }
        }
        Intent::Move(d) => {
            let n = list.len() as i32;
            let mut i = selected as i32;
            for _ in 0..n {
                i = (i + d).rem_euclid(n);
                if selectable(list[i as usize], &ctx) {
                    break;
                }
            }
            w.resource_mut::<MenuState>().selected = i as usize;
        }
        // Styles côte à côte : gauche/droite passe de l'un à l'autre.
        Intent::Change(d) if page == Page::Style => {
            w.resource_mut::<MenuState>().selected = (selected as i32 + d).clamp(0, list.len() as i32 - 1) as usize;
        }
        Intent::Change(d) => {
            if let Some(e) = list.get(selected) {
                change_entry(w, *e, d);
            }
        }
        Intent::Confirm => {
            if let Some(e) = list.get(selected).filter(|e| selectable(**e, &ctx)) {
                confirm(w, *e);
            }
        }
        Intent::Hover(i) => {
            if list.get(i).is_some_and(|e| selectable(*e, &ctx)) {
                w.resource_mut::<MenuState>().selected = i;
            }
        }
        Intent::Click(i) => {
            if let Some(e) = list.get(i).filter(|e| selectable(**e, &ctx)) {
                w.resource_mut::<MenuState>().selected = i;
                if matches!(e, Entry::Act(_)) {
                    confirm(w, *e);
                }
            }
        }
        Intent::Arrow(i, d) => {
            if let Some(e) = list.get(i) {
                w.resource_mut::<MenuState>().selected = i;
                change_entry(w, *e, d);
            }
        }
    }
}

fn change_entry(w: &mut World, e: Entry, d: i32) {
    match e {
        Entry::Opt(o) => {
            let mon = monitor(w);
            let mut s = w.resource::<Settings>().clone();
            change(o, d, &mut s, mon.as_ref());
            if *w.resource::<Settings>() != s {
                *w.resource_mut::<Settings>() = s;
            }
        }
        Entry::Slot(slot) => {
            let Some(p) = local_player(w) else { return };
            let choices = slot_choices(&p, Kind::Consumable);
            let cur = choices.iter().position(|c| *c == p.inventory.slots[slot as usize]).unwrap_or(0);
            let i = (cur as i32 + d).rem_euclid(choices.len() as i32) as usize;
            if i != cur {
                w.resource_mut::<SimCommands>().0.push(SimCommand::Equip { player: p.id, slot, item: choices[i] });
                // La sim est en pause pendant le menu : la commande est appliquée tout de suite.
                let _ = w.run_system_cached(apply_commands);
            }
        }
        Entry::Talisman => {
            let Some(p) = local_player(w) else { return };
            let choices = slot_choices(&p, Kind::Talisman);
            let cur = choices.iter().position(|c| *c == p.inventory.talisman).unwrap_or(0);
            let i = (cur as i32 + d).rem_euclid(choices.len() as i32) as usize;
            if i != cur {
                w.resource_mut::<SimCommands>().0.push(SimCommand::EquipTalisman { player: p.id, item: choices[i] });
                let _ = w.run_system_cached(apply_commands);
            }
        }
        _ => {}
    }
}

fn confirm(w: &mut World, e: Entry) {
    match e {
        Entry::Act(a) => act(w, a),
        // Valider une option la fait avancer d'un cran (en bouclant pour les choix binaires).
        Entry::Opt(o) => {
            let mon = monitor(w);
            let mut s = w.resource::<Settings>().clone();
            let before = s.clone();
            change(o, 1, &mut s, mon.as_ref());
            if s == before
                && let Some(c) = choices(o, &s, mon.as_ref())
            {
                change(o, -(c.labels.len() as i32), &mut s, mon.as_ref());
            }
            if s != before {
                *w.resource_mut::<Settings>() = s;
            }
        }
        Entry::Slot(_) | Entry::Talisman => change_entry(w, e, 1),
        // Voyage : on réapparaît reposé devant l'autre brasier (ennemis revenus à leur poste).
        Entry::Place(i) => {
            let Some(p) = local_player(w) else { return };
            w.resource_mut::<SimCommands>().0.push(SimCommand::Travel { player: p.id, checkpoint: i });
            let _ = w.run_system_cached(apply_commands);
            crate::save::save_now(w);
            close(w);
        }
        Entry::Line(..) => {}
        Entry::Lang(l) => {
            w.resource_mut::<Settings>().language = Some(l);
            crate::lang::set(l);
            let page = first_launch_page(w.resource::<Settings>());
            w.resource_mut::<MenuState>().open(page);
        }
        Entry::Style(h) => {
            let mut s = w.resource_mut::<Settings>();
            s.internal_height = h;
            s.style_chosen = true;
            w.resource_mut::<MenuState>().open(Page::Title);
        }
    }
}

fn act(w: &mut World, a: Act) {
    match a {
        Act::Resume | Act::Leave => close(w),
        Act::NewGame => {
            if w.resource::<SaveSlot>().data.is_some() {
                w.resource_mut::<MenuState>().push(Page::ConfirmNew);
            } else {
                launch(w, Launch::New);
            }
        }
        Act::ConfirmNew => launch(w, Launch::New),
        Act::Load => launch(w, Launch::Load),
        Act::Open(p) => w.resource_mut::<MenuState>().push(p),
        Act::Back => {
            w.resource_mut::<MenuState>().back();
        }
        Act::ToTitle => {
            crate::save::save_now(w);
            w.resource_mut::<NextState<AppState>>().set(AppState::Title);
        }
        Act::Quit => {
            // La sauvegarde est écrite en fin de frame (voir `save`).
            w.write_message(AppExit::Success);
        }
        Act::ReviveBoss => {
            w.resource_mut::<SimCommands>().0.push(SimCommand::ReviveBoss);
            let _ = w.run_system_cached(apply_commands);
            crate::save::save_now(w);
            w.resource_mut::<MenuState>().selected = 0;
        }
        Act::LevelUp => {}
    }
}

fn monitor(w: &mut World) -> Option<Monitor> {
    let mut q = w.query::<(&Monitor, Has<PrimaryMonitor>)>();
    crate::settings::pick_monitor(q.iter(w)).cloned()
}

/// Choix possibles pour un emplacement (rapide ou talisman) : vide, ou un des objets possédés.
fn slot_choices(p: &Player, kind: Kind) -> Vec<Option<Item>> {
    std::iter::once(None)
        .chain(Item::ALL.into_iter().filter(|i| i.kind() == kind && p.inventory.owns(*i)).map(Some))
        .collect()
}

fn entry_value(e: Entry, c: &PageCtx, s: &Settings, m: Option<&Monitor>, player: Option<&Player>, save: Option<&SaveData>) -> (String, bool) {
    match e {
        Entry::Opt(o) => choices(o, s, m).map(|c| (c.labels[c.current].clone(), c.enabled)).unwrap_or_default(),
        Entry::Slot(i) => {
            let item = player.and_then(|p| p.inventory.slots[i as usize].map(|it| (it, p.inventory.count(it))));
            (item.map_or("—".into(), |(it, n)| format!("{} ×{n}", it.name())), true)
        }
        Entry::Talisman => (player.and_then(|p| p.inventory.talisman).map_or("—", Item::name).into(), true),
        Entry::Act(Act::Load) => match save {
            Some(_) => (String::new(), true),
            None => (tr("No save", "Aucune sauvegarde").into(), false),
        },
        Entry::Place(i) if c.here == Some(i) => (tr("You are here", "Vous êtes ici").into(), false),
        Entry::Place(_) => (String::new(), true),
        Entry::Act(Act::LevelUp) => (tr("Coming soon", "Bientôt").into(), false),
        Entry::Line(..) | Entry::Lang(_) | Entry::Style(_) | Entry::Act(_) => (String::new(), true),
    }
}

fn entry_label(e: Entry, t: &Tuning, device: Device) -> String {
    match e {
        Entry::Place(i) => t.level.checkpoints.get(i as usize).map_or("", |c| c.name.get()).into(),
        Entry::Talisman => tr("Talisman", "Talisman").into(),
        Entry::Act(a) => act_label(a).into(),
        Entry::Opt(o) => opt_label(o).into(),
        Entry::Slot(i) => format!("{} {}", tr("Slot", "Emplacement"), i + 1),
        Entry::Line(l) => help_lines(device).get(l as usize).map_or("", |h| h.0).into(),
        Entry::Lang(l) => l.native_name().into(),
        Entry::Style(h) if h == PS1_HEIGHT => "PS1".into(),
        Entry::Style(_) => tr("Modern", "Moderne").into(),
    }
}

/// Recrée les lignes de la liste.
fn build_rows(w: &mut World, list: Entity, entries: &[Entry]) {
    let ui_font = w.resource::<UiFont>().clone();
    let help = help_lines(*w.resource::<Device>());
    let (left, right) = w.resource_scope(|w, mut icons: Mut<Icons>| {
        let mut images = w.resource_mut::<Assets<Image>>();
        (icon_bundle(&mut icons, &mut images, Glyph::ValueLeft, 1), icon_bundle(&mut icons, &mut images, Glyph::ValueRight, 1))
    });
    let style = (w.resource::<StyleImages>().ps1.clone(), w.resource::<StyleImages>().modern.clone());
    let side_by_side = entries.iter().any(|e| matches!(e, Entry::Style(_)));
    let mut commands = w.commands();
    commands.entity(list).entry::<Node>().and_modify(move |mut n| {
        n.flex_direction = if side_by_side { FlexDirection::Row } else { FlexDirection::Column };
        n.justify_content = if side_by_side { JustifyContent::SpaceEvenly } else { JustifyContent::Default };
    });
    commands.entity(list).despawn_children().with_children(|c| {
        for (i, e) in entries.iter().enumerate() {
            if let Entry::Style(h) = *e {
                // Carte : illustration, nom du style, description.
                let (image, desc) = if h == PS1_HEIGHT {
                    (style.0.clone(), tr("240p: big pixels, the 1997 look", "240p : gros pixels, l'image de 1997"))
                } else {
                    (style.1.clone(), tr("480p: a sharper image", "480p : une image plus nette"))
                };
                c.spawn((
                    Row(i),
                    Button,
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(8),
                        padding: UiRect::all(px(10)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|c| {
                    c.spawn((Node { border: UiRect::all(px(2)), ..default() }, BorderColor::all(Color::srgb(0.62, 0.55, 0.42))))
                        .with_child(image_bundle(image, STYLE_IMAGE_SIZE));
                    c.spawn((ui_font.text("", 2, Color::srgb(0.85, 0.82, 0.75)), RowLabel(i)));
                    c.spawn(ui_font.text(desc, 1, Color::srgb(0.78, 0.75, 0.68)));
                });
                continue;
            }
            let line = matches!(e, Entry::Line(..));
            c.spawn((
                Row(i),
                Button,
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(px(12), px(if line { 0 } else { 4 })),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|c| {
                c.spawn((
                    ui_font.text("", 1, Color::srgb(0.85, 0.82, 0.75)),
                    TextLayout::linebreak(LineBreak::NoWrap),
                    Node { flex_shrink: 0.0, margin: UiRect::right(px(16)), ..default() },
                    RowLabel(i),
                ));
                if let Entry::Line(l) = e {
                    // Aide : icônes des touches, alignées à gauche.
                    let mut h = Hint::new(1, Color::srgb(0.85, 0.82, 0.75));
                    h.segs = help.get(*l as usize).map(|h| h.1.clone()).unwrap_or_default();
                    c.spawn(Node { width: px(340), flex_shrink: 0.0, ..default() }).with_children(|c| {
                        c.spawn((h, hint_node()));
                    });
                    return;
                }
                if matches!(e, Entry::Lang(_)) {
                    return;
                }
                c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(10), align_items: AlignItems::Center, ..default() })
                    .with_children(|c| {
                        c.spawn((Button, left.clone(), Arrow(i, -1)));
                        c.spawn((
                            ui_font.text("", 1, Color::srgb(0.95, 0.92, 0.85)),
                            Node { min_width: px(240), justify_content: JustifyContent::Center, ..default() },
                            RowValue(i),
                        ));
                        c.spawn((Button, right.clone(), Arrow(i, 1)));
                    });
            });
        }
    });
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn refresh_menu(
    mut commands: Commands,
    mut menu: ResMut<MenuState>,
    (settings, save, enc, device, tuning): (Res<Settings>, Res<SaveSlot>, Res<Encounter>, Res<Device>, Res<Tuning>),
    mut preview: ResMut<CheckpointPreview>,
    mut preview_node: Query<&mut Node, With<MenuPreview>>,
    monitors: Query<(&Monitor, Has<PrimaryMonitor>)>,
    players: Query<(&Player, &Body), With<LocalPlayer>>,
    mut root: Single<(&mut Visibility, &mut BackgroundColor), With<MenuRoot>>,
    list: Single<Entity, With<MenuList>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<MenuTitle>>,
        Query<(&mut Text, &mut Node), (With<MenuInfo>, Without<MenuPreview>)>,
        Query<&mut Hint, With<MenuFooter>>,
        Query<(&RowLabel, &mut Text, &mut TextColor)>,
        Query<(&RowValue, &mut Text, &mut TextColor)>,
        Query<&mut Text, With<MenuPreviewCaption>>,
        Query<(&mut Text, &mut Node), (With<MenuDetail>, Without<MenuPreview>)>,
    )>,
    mut rows: Query<(&Row, &mut BackgroundColor), Without<MenuRoot>>,
    mut arrows: Query<(&Arrow, &mut Visibility), Without<MenuRoot>>,
    mut built: Local<(Vec<Entry>, Option<Lang>)>,
) {
    let want = if menu.open { Visibility::Inherited } else { Visibility::Hidden };
    if *root.0 != want {
        *root.0 = want;
    }
    // L'aperçu du lieu (et sa caméra) ne vit que sur la page de voyage.
    let travel = menu.open && menu.page == Page::Travel;
    if preview.shown != travel {
        preview.shown = travel;
    }
    for mut n in &mut preview_node {
        let want = if travel { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
    }
    if !menu.open {
        return;
    }
    // L'écran titre masque complètement le monde.
    root.1.0 = if menu.on_title() { Color::srgb(0.03, 0.025, 0.035) } else { Color::srgba(0.0, 0.0, 0.0, 0.72) };
    let (found, here) = players
        .iter()
        .next()
        .map_or((0, None), |(p, b)| (p.found, near_checkpoint(&tuning, b.pos)));
    let ctx = PageCtx { has_save: save.data.is_some(), boss_defeated: enc.boss_defeated, device: *device, found, here };
    let list_entries = entries(menu.page, &ctx);
    // Les lignes d'aide (icônes) dépendent aussi de la langue.
    let key = (list_entries.clone(), Some(crate::lang::current()));
    if *built != key {
        let (list, entries) = (*list, list_entries.clone());
        commands.queue(move |w: &mut World| build_rows(w, list, &entries));
        *built = key;
        // Les nouvelles lignes n'existent qu'à la frame suivante.
        return;
    }
    if !list_entries.get(menu.selected).is_some_and(|e| selectable(*e, &ctx)) {
        menu.selected = list_entries.iter().position(|e| selectable(*e, &ctx)).unwrap_or(0);
    }
    let set = |t: &mut Text, s: &str| {
        if t.0 != s {
            t.0 = s.into();
        }
    };
    for mut t in &mut texts.p0() {
        set(&mut t, page_title(menu.page, *device));
    }
    for (mut t, mut n) in &mut texts.p1() {
        let info = page_info(menu.page);
        set(&mut t, info);
        let want = if info.is_empty() { Display::None } else { Display::Flex };
        if n.display != want {
            n.display = want;
        }
    }
    let mut footer = if matches!(menu.page, Page::Language | Page::Style) {
        // Rien à modifier sur ces pages : choisir et valider. La langue n'est pas encore choisie
        // sur la première.
        let keys = if menu.page == Page::Style { ["←", "→"] } else { ["↑", "↓"] };
        let (nav, ok) = if *device == Device::Gamepad { (vec![i(Glyph::Dpad)], Glyph::PadA) } else { (keys.map(|k| i(Glyph::Key(k))).to_vec(), Glyph::Key("↵")) };
        let select = if menu.page == Page::Style { tr("select", "choisir") } else { "select / choisir" };
        nav.into_iter().chain([t(select), i(ok), t("OK")]).collect()
    } else if *device == Device::Gamepad {
        vec![i(Glyph::Dpad), t(tr("select / change", "choisir / modifier")), i(Glyph::PadA), t(tr("confirm", "valider"))]
    } else {
        vec![
            i(Glyph::Key("↑")),
            i(Glyph::Key("↓")),
            t(tr("select", "choisir")),
            i(Glyph::Key("←")),
            i(Glyph::Key("→")),
            t(tr("change", "modifier")),
            i(Glyph::Key("↵")),
            t(tr("confirm", "valider")),
        ]
    };
    if !menu.on_title() {
        footer.extend([i(if *device == Device::Gamepad { Glyph::PadB } else { Glyph::Key("ESC") }), t(tr("back", "retour"))]);
    }
    for mut h in &mut texts.p2() {
        set_hint(&mut h, footer.clone());
    }
    let m = crate::settings::pick_monitor(monitors.iter());
    let player = players.iter().next().map(|(p, _)| p);
    for (r, mut bg) in &mut rows {
        bg.0 = if r.0 == menu.selected { Color::srgba(0.9, 0.82, 0.62, 0.16) } else { Color::NONE };
    }
    for (l, mut t, mut c) in &mut texts.p3() {
        let Some(e) = list_entries.get(l.0) else { continue };
        set(&mut t, &entry_label(*e, &tuning, *device));
        c.0 = if matches!(e, Entry::Line(..)) {
            Color::srgb(0.78, 0.75, 0.68)
        } else if !selectable(*e, &ctx) {
            Color::srgba(0.6, 0.58, 0.55, 0.6)
        } else if l.0 == menu.selected {
            Color::srgb(1.0, 0.95, 0.85)
        } else {
            Color::srgb(0.78, 0.75, 0.68)
        };
    }
    for (v, mut t, mut c) in &mut texts.p4() {
        let Some(e) = list_entries.get(v.0) else { continue };
        let (s, enabled) = entry_value(*e, &ctx, &settings, m, player, save.data.as_ref());
        set(&mut t, &s);
        c.0 = if enabled { Color::srgb(0.95, 0.92, 0.85) } else { Color::srgba(0.6, 0.58, 0.55, 0.6) };
    }
    // Aperçu : le lieu de la ligne sélectionnée (ou celui où l'on se trouve).
    let shown = match list_entries.get(menu.selected) {
        Some(Entry::Place(i)) => *i as usize,
        _ => here.unwrap_or(0) as usize,
    };
    if preview.index != shown {
        preview.index = shown;
    }
    for mut t in &mut texts.p5() {
        set(&mut t, tuning.level.checkpoints.get(shown).map_or("", |c| c.name.get()));
    }
    // Équipement : ce que fait l'objet de la ligne sélectionnée.
    let item = match list_entries.get(menu.selected) {
        Some(Entry::Slot(i)) => player.and_then(|p| p.inventory.slots[*i as usize]),
        Some(Entry::Talisman) => player.and_then(|p| p.inventory.talisman),
        _ => None,
    };
    let detail = item.map_or(String::new(), |it| format!("{} — {}", it.name(), it.description()));
    for (mut t, mut n) in &mut texts.p6() {
        set(&mut t, &detail);
        let want = if detail.is_empty() { Display::None } else { Display::Flex };
        if n.display != want {
            n.display = want;
        }
    }
    for (a, mut vis) in &mut arrows {
        let show = match list_entries.get(a.0) {
            Some(Entry::Opt(o)) => choices(*o, &settings, m).is_some_and(|c| c.enabled && c.labels.len() > 1),
            Some(Entry::Slot(_)) => player.is_some_and(|p| slot_choices(p, Kind::Consumable).len() > 1),
            Some(Entry::Talisman) => player.is_some_and(|p| slot_choices(p, Kind::Talisman).len() > 1),
            _ => false,
        };
        let w = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != w {
            *vis = w;
        }
    }
}
