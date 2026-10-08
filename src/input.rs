//! Périphériques (clavier/souris, manette) → `PlayerInput` de la simulation.
//!
//! Les appuis sont « verrouillés » entre deux ticks : un appui très bref, relâché entre deux
//! ticks de simulation (écran 144 Hz…), n'est jamais perdu.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::menu::MenuState;
use crate::render::camera::CameraRig;
use crate::settings::Settings;
use crate::sim::input::{PlayerInput, PlayerInputs, btn};

/// Boutons pressés depuis le dernier tick de simulation.
#[derive(Resource, Default)]
pub struct InputLatch {
    pressed: u16,
    /// Boutons tenus à la fermeture d'un menu : ignorés jusqu'à ce qu'on les relâche
    /// (le (A) qui valide le menu ne doit pas aussi agir en jeu).
    suppressed: u16,
}

/// Dernier périphérique utilisé (libellés du HUD, page d'aide).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    #[default]
    Keyboard,
    Gamepad,
}

/// Valeurs de caméra fournies par la souris / le stick droit pendant la frame.
#[derive(Resource, Default)]
pub struct LookInput {
    pub delta: Vec2,
    pub recenter: bool,
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputLatch>()
            .init_resource::<Device>()
            .init_resource::<LookInput>()
            .add_systems(PreUpdate, (track_device, latch_presses).after(bevy::input::InputSystems))
            .add_systems(Update, (grab_cursor, read_look));
    }
}

/// `mouse` vaut `None` quand le curseur n'est pas capturé (le clic sert alors à le capturer).
fn keyboard_buttons(
    keys: &ButtonInput<KeyCode>,
    mouse: Option<&ButtonInput<MouseButton>>,
    pressed: bool,
) -> u16 {
    let check_k = |k: KeyCode| if pressed { keys.just_pressed(k) } else { keys.pressed(k) };
    let check_m = |m: MouseButton| {
        mouse.is_some_and(|mouse| if pressed { mouse.just_pressed(m) } else { mouse.pressed(m) })
    };
    let mut b = 0;
    if check_k(KeyCode::KeyG) {
        b |= btn::INTERACT;
    }
    if check_k(KeyCode::KeyC) {
        b |= btn::NEXT_ITEM;
    }
    if check_m(MouseButton::Left) {
        b |= btn::LIGHT;
    }
    if check_m(MouseButton::Right) {
        b |= btn::HEAVY;
    }
    if check_k(KeyCode::KeyQ) || check_k(KeyCode::ShiftLeft) {
        b |= btn::GUARD;
    }
    if check_k(KeyCode::KeyE) {
        b |= btn::SPECIAL;
    }
    if check_k(KeyCode::Space) {
        b |= btn::DODGE;
    }
    if check_k(KeyCode::Tab) || check_m(MouseButton::Middle) {
        b |= btn::LOCK;
    }
    if check_k(KeyCode::KeyR) {
        b |= btn::SWITCH;
    }
    if check_k(KeyCode::KeyF) {
        b |= btn::ITEM;
    }
    b
}

fn gamepad_buttons(g: &Gamepad, pressed: bool) -> u16 {
    let check = |x: GamepadButton| if pressed { g.just_pressed(x) } else { g.pressed(x) };
    let mut b = 0;
    if check(GamepadButton::RightTrigger) {
        b |= btn::LIGHT;
    }
    if check(GamepadButton::RightTrigger2) {
        b |= btn::HEAVY;
    }
    if check(GamepadButton::LeftTrigger) {
        b |= btn::GUARD;
    }
    if check(GamepadButton::LeftTrigger2) {
        b |= btn::SPECIAL;
    }
    if check(GamepadButton::East) {
        b |= btn::DODGE;
    }
    if check(GamepadButton::RightThumb) {
        b |= btn::LOCK;
    }
    if check(GamepadButton::DPadRight) {
        b |= btn::SWITCH;
    }
    if check(GamepadButton::West) {
        b |= btn::ITEM;
    }
    if check(GamepadButton::DPadDown) {
        b |= btn::NEXT_ITEM;
    }
    if check(GamepadButton::South) {
        b |= btn::INTERACT;
    }
    if check(GamepadButton::LeftThumb) {
        b |= btn::SPRINT;
    }
    b
}

/// Retient le dernier périphérique utilisé, en jeu comme dans les menus.
fn track_device(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    gamepads: Query<&Gamepad>,
    mut device: ResMut<Device>,
) {
    let pad = gamepads.iter().any(|g| {
        g.get_just_pressed().next().is_some() || g.left_stick().length() > 0.5 || g.right_stick().length() > 0.5
    });
    let kb = keys.get_just_pressed().next().is_some()
        || mouse.get_just_pressed().next().is_some()
        || motion.delta.length() > 4.0;
    let new = if pad {
        Device::Gamepad
    } else if kb {
        Device::Keyboard
    } else {
        return;
    };
    if *device != new {
        *device = new;
    }
}

fn latch_presses(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    mut latch: ResMut<InputLatch>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    menu: Res<MenuState>,
) {
    if menu.open {
        // Les touches du menu ne doivent pas se retrouver dans le jeu à la fermeture.
        latch.pressed = 0;
        latch.suppressed = u16::MAX;
        return;
    }
    let grabbed = cursor.grab_mode != CursorGrabMode::None;
    latch.pressed |= keyboard_buttons(&keys, grabbed.then_some(&*mouse), true);
    for g in &gamepads {
        latch.pressed |= gamepad_buttons(g, true);
    }
}

/// Construit l'input du joueur local pour le tick de simulation à venir.
pub fn collect_local_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    rig: Res<CameraRig>,
    mut latch: ResMut<InputLatch>,
    mut inputs: ResMut<PlayerInputs>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
) {
    let grabbed = cursor.grab_mode != CursorGrabMode::None;
    let mut held = keyboard_buttons(&keys, grabbed.then_some(&*mouse), false);
    for g in &gamepads {
        held |= gamepad_buttons(g, false);
    }
    latch.suppressed &= held;
    let buttons = (held & !latch.suppressed) | latch.pressed;
    latch.pressed = 0;

    let mut stick = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        stick.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        stick.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        stick.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        stick.x -= 1.0;
    }
    stick = stick.normalize_or_zero();
    for g in &gamepads {
        let s = g.left_stick();
        if s.length() > 0.15 {
            stick = s.clamp_length_max(1.0);
        }
    }
    inputs.0[0] = PlayerInput {
        buttons,
        move_x: PlayerInput::quantize_axis(stick.x),
        move_y: PlayerInput::quantize_axis(stick.y),
        cam_yaw: PlayerInput::quantize_yaw(rig.yaw),
    };
}

fn read_look(
    motion: Res<AccumulatedMouseMotion>,
    gamepads: Query<&Gamepad>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    time: Res<Time>,
    settings: Res<Settings>,
    menu: Res<MenuState>,
    mut look: ResMut<LookInput>,
) {
    let mut d = Vec2::ZERO;
    if menu.open {
        look.delta = d;
        return;
    }
    if cursor.grab_mode != CursorGrabMode::None {
        d += motion.delta * 0.0025;
    }
    for g in &gamepads {
        let s = g.right_stick();
        if s.length() > 0.15 {
            d += Vec2::new(s.x, -s.y) * 2.6 * time.delta_secs();
        }
    }
    d *= settings.sensitivity;
    if settings.invert_y {
        d.y = -d.y;
    }
    look.delta = d;
}

/// Clic dans le jeu : capture la souris (Échap ouvre le menu, qui la libère).
fn grab_cursor(
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    menu: Res<MenuState>,
) {
    if !menu.open && mouse.just_pressed(MouseButton::Left) && cursor.grab_mode == CursorGrabMode::None {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}
