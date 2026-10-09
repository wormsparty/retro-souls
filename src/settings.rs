//! Player settings (display, sound, camera), saved and reloaded on startup.
//!
//! See `storage` for the file location (`settings.ron`).

use bevy::audio::{GlobalVolume, Volume};
use bevy::prelude::*;
use bevy::window::{
    Monitor, MonitorSelection, PresentMode, PrimaryMonitor, PrimaryWindow, VideoMode,
    VideoModeSelection, WindowMode,
};
use serde::{Deserialize, Serialize};

use crate::lang::Lang;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    /// Windowed full screen (borderless), at native resolution.
    Fullscreen,
    /// Exclusive full screen: resolution and refresh rate of your choice.
    Exclusive,
    Windowed,
}

#[derive(Resource, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub display: DisplayMode,
    /// Size in windowed or exclusive mode (`None` = automatic).
    pub resolution: Option<(u32, u32)>,
    /// Refresh rate in exclusive mode, in millihertz (`None` = automatic).
    pub refresh_mhz: Option<u32>,
    pub vsync: bool,
    /// Height of the internal resolution (240 = PS1).
    pub internal_height: u32,
    /// False until the player has chosen the graphics style (question asked on first
    /// launch: PS1 or modern, i.e. the lowest or highest internal resolution).
    pub style_chosen: bool,
    pub master_volume: f32,
    pub effects_volume: f32,
    pub sensitivity: f32,
    pub invert_y: bool,
    pub camera_shake: bool,
    /// Frames-per-second counter in the top right.
    pub show_fps: bool,
    /// `None` until the player has chosen (the question is asked on first launch).
    pub language: Option<Lang>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            display: DisplayMode::Fullscreen,
            resolution: None,
            refresh_mhz: None,
            vsync: true,
            internal_height: 240,
            style_chosen: false,
            master_volume: 0.8,
            effects_volume: 1.0,
            sensitivity: 1.0,
            invert_y: false,
            camera_shake: true,
            show_fps: false,
            language: None,
        }
    }
}

const FILE: &str = "settings";

/// Internal resolutions of the two styles: PS1 and modern.
pub const PS1_HEIGHT: u32 = 240;
pub const MODERN_HEIGHT: u32 = 480;
pub const INTERNAL_HEIGHTS: [u32; 2] = [PS1_HEIGHT, MODERN_HEIGHT];
/// Sizes offered in windowed mode (filtered according to the screen).
pub const WINDOWED_SIZES: [(u32, u32); 6] =
    [(960, 540), (1280, 720), (1600, 900), (1920, 1080), (2560, 1440), (3840, 2160)];

impl Settings {
    pub fn load() -> Self {
        crate::storage::read(FILE)
            .and_then(|s| ron::from_str::<Settings>(&s).ok())
            .map(Settings::sanitized)
            .unwrap_or_default()
    }

    pub fn save(&self) {
        match ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()) {
            Ok(s) => crate::storage::write(FILE, &s),
            Err(e) => warn!("cannot save the settings: {e}"),
        }
    }

    fn sanitized(mut self) -> Self {
        self.master_volume = self.master_volume.clamp(0.0, 1.0);
        self.effects_volume = self.effects_volume.clamp(0.0, 1.0);
        self.sensitivity = self.sensitivity.clamp(0.1, 5.0);
        // Old values (360p): the closest style.
        if !INTERNAL_HEIGHTS.contains(&self.internal_height) {
            self.internal_height = if self.internal_height > PS1_HEIGHT { MODERN_HEIGHT } else { PS1_HEIGHT };
        }
        if !exclusive_supported() && self.display == DisplayMode::Exclusive {
            self.display = DisplayMode::Fullscreen;
        }
        self
    }

    /// Matching Bevy window mode (the web goes through the browser's Fullscreen API).
    ///
    /// Exclusive targets a specific monitor: until the monitors are known (window
    /// creation), stay in borderless full screen — winit doesn't know
    /// "the current monitor" yet at that point and Bevy panics.
    pub fn window_mode(&self, monitor: Option<(Entity, &Monitor)>) -> WindowMode {
        if cfg!(target_arch = "wasm32") {
            return WindowMode::Windowed;
        }
        match (self.display, monitor) {
            (DisplayMode::Windowed, _) => WindowMode::Windowed,
            (DisplayMode::Exclusive, Some((e, m))) if exclusive_supported() => WindowMode::Fullscreen(
                MonitorSelection::Entity(e),
                self.pick_video_mode(m).map(VideoModeSelection::Specific).unwrap_or(VideoModeSelection::Current),
            ),
            _ => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        }
    }

    /// Exclusive video mode closest to the choices (resolution, then refresh rate).
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

/// Exclusive full screen exists neither in the browser nor under Wayland (the protocol doesn't
/// allow changing video modes: winit ignores it). Launching with `WAYLAND_DISPLAY=`
/// makes the game go through XWayland, where the compositor emulates it.
pub fn exclusive_supported() -> bool {
    if cfg!(target_arch = "wasm32") {
        return false;
    }
    // Same rule as winit for choosing Wayland over X11.
    let set = |v: &str| std::env::var(v).is_ok_and(|x| !x.is_empty());
    !(cfg!(target_os = "linux") && (set("WAYLAND_DISPLAY") || set("WAYLAND_SOCKET")))
}

/// Reference monitor: the primary one if known (not always the case under Wayland), otherwise the first.
pub fn pick_monitor<'a>(monitors: impl Iterator<Item = (&'a Monitor, bool)>) -> Option<&'a Monitor> {
    let all: Vec<_> = monitors.collect();
    all.iter().find(|(_, primary)| *primary).or(all.first()).map(|(m, _)| *m)
}

/// Available exclusive resolutions (unique, from largest to smallest).
pub fn exclusive_sizes(m: &Monitor) -> Vec<(u32, u32)> {
    let mut v: Vec<(u32, u32)> = m.video_modes.iter().map(|v| (v.physical_size.x, v.physical_size.y)).collect();
    v.sort_by(|a, b| (b.0 * b.1).cmp(&(a.0 * a.1)).then(b.0.cmp(&a.0)));
    v.dedup();
    v
}

/// Available refresh rates for an exclusive resolution (mHz, ascending).
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
        crate::lang::set(app.world().resource::<Settings>().language.unwrap_or_default());
        app.add_systems(PostUpdate, apply_settings);
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, web_fullscreen);
    }
}

/// Applies the changed settings (window, audio) and saves them.
fn apply_settings(
    settings: Res<Settings>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    monitors: Query<(Entity, &Monitor, Has<PrimaryMonitor>)>,
    mut volume: ResMut<GlobalVolume>,
    mut ui_scale: ResMut<UiScale>,
    mut seen_monitors: Local<usize>,
    mut applied: Local<Option<Settings>>,
) {
    // UI scaled to the window (layout designed for 720p).
    let scale = (window.height() / 720.0).clamp(0.75, 4.0);
    if (ui_scale.0 - scale).abs() > 0.01 {
        ui_scale.0 = scale;
    }
    let s = &*settings;
    crate::lang::set(s.language.unwrap_or_default());
    // Exclusive mode depends on the monitor, detected after startup: reapply then.
    let monitor = monitors
        .iter()
        .find(|(.., primary)| *primary)
        .or(monitors.iter().next())
        .map(|(e, m, _)| (e, m));
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
    // Web: on startup, full screen waits for the player's first action (`web_fullscreen`).
    #[cfg(target_arch = "wasm32")]
    if !first {
        web::sync_fullscreen(s.display != DisplayMode::Windowed);
    }
    if !first {
        s.save();
    }
    *applied = Some(s.clone());
}

/// Web: full screen is only allowed in response to a user action.
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
    // Not while in the menu: Esc there exits the browser's full screen.
    if gesture && (!menu.open || menu.on_title()) && settings.display != DisplayMode::Windowed {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_and_defaults() {
        let s = Settings { master_volume: 0.3, invert_y: true, ..default() };
        let txt = ron::ser::to_string(&s).unwrap();
        let back: Settings = ron::from_str(&txt).unwrap();
        assert_eq!(back, s);
        // A partial file (older version) keeps the default values for the rest.
        let partial: Settings = ron::from_str("(master_volume: 0.5)").unwrap();
        assert_eq!(partial.master_volume, 0.5);
        assert_eq!(partial.display, DisplayMode::Fullscreen);
    }
}
