//! Options du joueur (affichage, son, caméra), sauvegardées et rechargées au démarrage.
//!
//! - natif : `settings.ron` dans le dossier de config de l'OS
//!   (`~/.config/souls-ps1/`, `%APPDATA%\souls-ps1\`, `~/Library/Application Support/souls-ps1/`) ;
//! - web : `localStorage` du navigateur.

use bevy::audio::{GlobalVolume, Volume};
use bevy::prelude::*;
use bevy::window::{
    Monitor, MonitorSelection, PresentMode, PrimaryMonitor, PrimaryWindow, VideoMode,
    VideoModeSelection, WindowMode,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    /// Plein écran fenêtré (sans bordure), à la résolution native.
    Fullscreen,
    /// Plein écran exclusif : résolution et fréquence au choix.
    Exclusive,
    Windowed,
}

#[derive(Resource, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub display: DisplayMode,
    /// Taille en mode fenêtré ou exclusif (`None` = automatique).
    pub resolution: Option<(u32, u32)>,
    /// Fréquence en mode exclusif, en millihertz (`None` = automatique).
    pub refresh_mhz: Option<u32>,
    pub vsync: bool,
    /// Hauteur de la résolution interne (240 = PS1).
    pub internal_height: u32,
    pub master_volume: f32,
    pub effects_volume: f32,
    pub sensitivity: f32,
    pub invert_y: bool,
    pub camera_shake: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            display: DisplayMode::Fullscreen,
            resolution: None,
            refresh_mhz: None,
            vsync: true,
            internal_height: 240,
            master_volume: 0.8,
            effects_volume: 1.0,
            sensitivity: 1.0,
            invert_y: false,
            camera_shake: true,
        }
    }
}

pub const INTERNAL_HEIGHTS: [u32; 3] = [240, 360, 480];
/// Tailles proposées en mode fenêtré (filtrées selon l'écran).
pub const WINDOWED_SIZES: [(u32, u32); 6] =
    [(960, 540), (1280, 720), (1600, 900), (1920, 1080), (2560, 1440), (3840, 2160)];

impl Settings {
    pub fn load() -> Self {
        storage::read()
            .and_then(|s| ron::from_str::<Settings>(&s).ok())
            .map(Settings::sanitized)
            .unwrap_or_default()
    }

    pub fn save(&self) {
        match ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()) {
            Ok(s) => storage::write(&s),
            Err(e) => warn!("impossible d'enregistrer les options : {e}"),
        }
    }

    fn sanitized(mut self) -> Self {
        self.master_volume = self.master_volume.clamp(0.0, 1.0);
        self.effects_volume = self.effects_volume.clamp(0.0, 1.0);
        self.sensitivity = self.sensitivity.clamp(0.1, 5.0);
        if !INTERNAL_HEIGHTS.contains(&self.internal_height) {
            self.internal_height = 240;
        }
        if cfg!(target_arch = "wasm32") && self.display == DisplayMode::Exclusive {
            self.display = DisplayMode::Fullscreen;
        }
        self
    }

    /// Mode de fenêtre Bevy correspondant (le web passe par l'API Fullscreen du navigateur).
    pub fn window_mode(&self, monitor: Option<&Monitor>) -> WindowMode {
        if cfg!(target_arch = "wasm32") {
            return WindowMode::Windowed;
        }
        match self.display {
            DisplayMode::Fullscreen => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            DisplayMode::Windowed => WindowMode::Windowed,
            DisplayMode::Exclusive => {
                let mode = monitor.and_then(|m| self.pick_video_mode(m));
                WindowMode::Fullscreen(
                    MonitorSelection::Current,
                    mode.map(VideoModeSelection::Specific).unwrap_or(VideoModeSelection::Current),
                )
            }
        }
    }

    /// Mode vidéo exclusif le plus proche des choix (résolution, puis fréquence).
    pub fn pick_video_mode(&self, m: &Monitor) -> Option<VideoMode> {
        let size = self.resolution.map(|(w, h)| UVec2::new(w, h)).unwrap_or(m.physical_size());
        let mut modes: Vec<&VideoMode> = m.video_modes.iter().filter(|v| v.physical_size == size).collect();
        if modes.is_empty() {
            return None;
        }
        modes.sort_by_key(|v| (v.refresh_rate_millihertz, v.bit_depth));
        let want = self.refresh_mhz.or(m.refresh_rate_millihertz);
        let best = match want {
            Some(r) => modes.iter().min_by_key(|v| v.refresh_rate_millihertz.abs_diff(r)),
            None => modes.last(),
        };
        best.map(|v| **v)
    }

    pub fn present_mode(&self) -> PresentMode {
        if self.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync }
    }
}

/// Écran de référence : le principal s'il est connu (pas toujours le cas sous Wayland), sinon le premier.
pub fn pick_monitor<'a>(monitors: impl Iterator<Item = (&'a Monitor, bool)>) -> Option<&'a Monitor> {
    let all: Vec<_> = monitors.collect();
    all.iter().find(|(_, primary)| *primary).or(all.first()).map(|(m, _)| *m)
}

/// Résolutions exclusives disponibles (uniques, de la plus grande à la plus petite).
pub fn exclusive_sizes(m: &Monitor) -> Vec<(u32, u32)> {
    let mut v: Vec<(u32, u32)> = m.video_modes.iter().map(|v| (v.physical_size.x, v.physical_size.y)).collect();
    v.sort_by(|a, b| (b.0 * b.1).cmp(&(a.0 * a.1)).then(b.0.cmp(&a.0)));
    v.dedup();
    v
}

/// Fréquences disponibles pour une résolution exclusive (mHz, croissantes).
pub fn refresh_rates(m: &Monitor, size: (u32, u32)) -> Vec<u32> {
    let mut v: Vec<u32> = m
        .video_modes
        .iter()
        .filter(|v| (v.physical_size.x, v.physical_size.y) == size)
        .map(|v| v.refresh_rate_millihertz)
        .collect();
    v.sort();
    v.dedup();
    v
}

pub fn windowed_sizes(m: Option<&Monitor>) -> Vec<(u32, u32)> {
    let max = m.map(|m| (m.physical_width, m.physical_height)).unwrap_or((u32::MAX, u32::MAX));
    let v: Vec<_> = WINDOWED_SIZES.iter().copied().filter(|&(w, h)| w <= max.0 && h <= max.1).collect();
    if v.is_empty() { vec![WINDOWED_SIZES[0]] } else { v }
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<Settings>() {
            app.insert_resource(Settings::load());
        }
        app.add_systems(PostUpdate, apply_settings);
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, web_fullscreen);
    }
}

/// Applique les options modifiées (fenêtre, audio) et les enregistre.
fn apply_settings(
    settings: Res<Settings>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    monitors: Query<(&Monitor, Has<PrimaryMonitor>)>,
    mut volume: ResMut<GlobalVolume>,
    mut ui_scale: ResMut<UiScale>,
    mut seen_monitors: Local<usize>,
    mut applied: Local<Option<Settings>>,
) {
    // Interface à l'échelle de la fenêtre (maquette pensée pour 720p).
    let scale = (window.height() / 720.0).clamp(0.75, 4.0);
    if (ui_scale.0 - scale).abs() > 0.01 {
        ui_scale.0 = scale;
    }
    let s = &*settings;
    // Le mode exclusif dépend de l'écran, détecté après le démarrage : on réapplique alors.
    let monitor = pick_monitor(monitors.iter());
    let count = monitors.iter().count();
    let monitors_changed = count != *seen_monitors;
    *seen_monitors = count;
    if applied.as_ref() == Some(s) && !monitors_changed {
        return;
    }
    let first = applied.is_none();
    let mode = s.window_mode(monitor);
    if window.mode != mode {
        window.mode = mode;
    }
    if window.present_mode != s.present_mode() {
        window.present_mode = s.present_mode();
    }
    if s.display == DisplayMode::Windowed && !cfg!(target_arch = "wasm32") {
        let (w, h) = s.resolution.unwrap_or((1280, 720));
        if window.physical_width() != w || window.physical_height() != h {
            window.resolution.set_physical_resolution(w, h);
        }
    }
    volume.volume = Volume::Linear(s.master_volume);
    // Web : au démarrage, le plein écran attend la première action du joueur (`web_fullscreen`).
    #[cfg(target_arch = "wasm32")]
    if !first {
        web::sync_fullscreen(s.display != DisplayMode::Windowed);
    }
    if !first {
        s.save();
    }
    *applied = Some(s.clone());
}

/// Web : le plein écran n'est autorisé qu'en réponse à une action de l'utilisateur.
#[cfg(target_arch = "wasm32")]
fn web_fullscreen(
    settings: Res<Settings>,
    menu: Res<crate::menu::MenuState>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
) {
    let gesture = mouse.get_just_pressed().next().is_some()
        || keys.get_just_pressed().any(|k| *k != KeyCode::Escape)
        || gamepads.iter().any(|g| g.get_just_pressed().next().is_some());
    // Pas pendant le menu : Échap y fait sortir du plein écran du navigateur.
    if gesture && !menu.open && settings.display != DisplayMode::Windowed {
        web::request_fullscreen();
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    pub fn request_fullscreen() {
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
        if doc.fullscreen_element().is_none() {
            if let Some(root) = doc.document_element() {
                let _ = root.request_fullscreen();
            }
        }
    }

    pub fn sync_fullscreen(want: bool) {
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
        if want {
            request_fullscreen();
        } else if doc.fullscreen_element().is_some() {
            doc.exit_fullscreen();
        }
    }
}

mod storage {
    #[cfg(target_arch = "wasm32")]
    const KEY: &str = "souls-ps1.settings";

    #[cfg(target_arch = "wasm32")]
    fn local() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    #[cfg(target_arch = "wasm32")]
    pub fn read() -> Option<String> {
        local()?.get_item(KEY).ok()?
    }

    #[cfg(target_arch = "wasm32")]
    pub fn write(s: &str) {
        if let Some(l) = local() {
            let _ = l.set_item(KEY, s);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn path() -> Option<std::path::PathBuf> {
        use std::env::var_os;
        use std::path::PathBuf;
        let base = if cfg!(target_os = "windows") {
            var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
        } else {
            var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        }?;
        Some(base.join("souls-ps1").join("settings.ron"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn read() -> Option<String> {
        std::fs::read_to_string(path()?).ok()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn write(s: &str) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&p, s) {
            bevy::log::warn!("impossible d'écrire {}: {e}", p.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_and_defaults() {
        let s = Settings { master_volume: 0.3, invert_y: true, ..default() };
        let txt = ron::ser::to_string(&s).unwrap();
        let back: Settings = ron::from_str(&txt).unwrap();
        assert_eq!(back, s);
        // Un fichier partiel (ancienne version) garde les valeurs par défaut pour le reste.
        let partial: Settings = ron::from_str("(master_volume: 0.5)").unwrap();
        assert_eq!(partial.master_volume, 0.5);
        assert_eq!(partial.display, DisplayMode::Fullscreen);
    }
}
