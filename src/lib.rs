//! Prototype de combat souls-like style PS1.
//!
//! - `sim` : simulation déterministe à 60 ticks/s (aucune dépendance au rendu) ;
//! - `render`, `fx`, `hud` : présentation, qui ne fait que lire la simulation ;
//! - `input` : périphériques → `PlayerInput`.

pub mod config;
pub mod debug;
pub mod fx;
pub mod hud;
pub mod input;
pub mod menu;
pub mod render;
pub mod settings;
pub mod sim;

use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;
use bevy::window::WindowResolution;

use render::AppState;

pub fn run() {
    // Options chargées avant de créer la fenêtre : on démarre directement dans le bon mode.
    let settings = settings::Settings::load();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Souls PS1".into(),
                    canvas: Some("#bevy".into()),
                    fit_canvas_to_parent: true,
                    prevent_default_event_handling: true,
                    resolution: {
                        let (w, h) = settings.resolution.unwrap_or((1280, 720));
                        WindowResolution::new(w, h)
                    },
                    // Le mode exclusif précis est appliqué une fois l'écran connu.
                    mode: settings.window_mode(None),
                    present_mode: settings.present_mode(),
                    ..default()
                }),
                ..default()
            })
            .set(ImagePlugin::default_nearest())
            .set(AssetPlugin {
                // Pas de fichiers .meta : évite des requêtes 404 sur le web.
                meta_check: AssetMetaCheck::Never,
                ..default()
            }),
    )
    .insert_resource(settings)
    .insert_resource(Time::<Fixed>::from_hz(sim::TICK_HZ))
    .init_state::<AppState>()
    .add_plugins((
        settings::SettingsPlugin,
        menu::MenuPlugin,
        sim::SimPlugin,
        config::ConfigPlugin,
        input::InputPlugin,
        render::RenderPlugin,
        fx::FxPlugin,
        hud::HudPlugin,
        debug::DebugPlugin,
        debug::AutoShotPlugin,
    ))
    .add_systems(
        FixedUpdate,
        (input::collect_local_input, sim::run_sim_tick)
            .chain()
            .run_if(in_state(AppState::Playing).and_then(menu::menu_closed)),
    );
    app.run();
}
