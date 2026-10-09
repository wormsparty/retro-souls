//! Devices (keyboard/mouse, gamepad) → the simulation's `PlayerInput`.
//!
//! Presses are "latched" between two ticks: a very short press, released between two
//! simulation ticks (144 Hz screen…), is never lost.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::menu::MenuState;
use crate::render::LocalPlayer;
use crate::render::camera::CameraRig;
use crate::settings::Settings;
use crate::sim::input::{PlayerInput, PlayerInputs, btn};
use crate::sim::player::Player;

/// Buttons pressed since the last simulation tick.
#[derive(Resource, Default)]
pub struct InputLatch {
    pressed: u16,
    /// Buttons held when a menu closes: ignored until they're released
    /// (the (A) that confirms the menu must not also act in game).
    suppressed: u16,
}

/// Last device used (HUD labels, help page).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    #[default]
    Keyboard,
    Gamepad,
}

/// Camera values provided by the mouse / right stick during the frame.
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

/// `mouse` is `None` when the cursor isn't captured (the click then captures it).
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
    if check(GamepadButton::DPadUp) {
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

/// Remembers the last device used, in game as in the menus.
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

/// Target switching: a flick of the right stick (or mouse) in one of four directions.
#[derive(Default)]
struct Flick {
    /// The stick has returned to the centre since the last switch.
    armed: bool,
    /// Accumulated mouse movement, and time since the last switch.
    mouse: Vec2,
    cooldown: f32,
}

/// Thresholds: stick pushed, stick back at centre, mouse flick (pixels).
const FLICK_ON: f32 = 0.65;
const FLICK_OFF: f32 = 0.3;
const FLICK_MOUSE: f32 = 90.0;

#[allow(clippy::too_many_arguments)]
fn latch_presses(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    time: Res<Time>,
    gamepads: Query<&Gamepad>,
    mut latch: ResMut<InputLatch>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    menu: Res<MenuState>,
    players: Query<&Player, With<LocalPlayer>>,
    mut flick: Local<Flick>,
) {
    if menu.open {
        // Menu keys must not leak into the game when it closes.
        latch.pressed = 0;
        latch.suppressed = u16::MAX;
        return;
    }
    let grabbed = cursor.grab_mode != CursorGrabMode::None;
    latch.pressed |= keyboard_buttons(&keys, grabbed.then_some(&*mouse), true);
    for g in &gamepads {
        latch.pressed |= gamepad_buttons(g, true);
    }

    // Locked on, the right stick and the mouse no longer turn the camera: they switch targets.
    let locked = players.single().is_ok_and(|p| p.lock.is_some());
    flick.cooldown = (flick.cooldown - time.delta_secs()).max(0.0);
    let stick = gamepads.iter().map(|g| g.right_stick()).max_by(|a, b| a.length().total_cmp(&b.length())).unwrap_or(Vec2::ZERO);
    if stick.length() < FLICK_OFF {
        flick.armed = true;
    }
    if !locked {
        flick.mouse = Vec2::ZERO;
        return;
    }
    // Clear direction (y = up): the stronger of the two components.
    let mut dir = Vec2::ZERO;
    if flick.armed && stick.length() > FLICK_ON {
        flick.armed = false;
        dir = stick;
    }
    if grabbed {
        // The mouse flick decays if it isn't decisive enough (mouse upwards: y < 0).
        let delta = Vec2::new(motion.delta.x, -motion.delta.y);
        flick.mouse = flick.mouse * (-6.0 * time.delta_secs()).exp() + delta;
        if flick.mouse.max_element().max(-flick.mouse.min_element()) > FLICK_MOUSE && flick.cooldown <= 0.0 {
            dir = flick.mouse;
            flick.mouse = Vec2::ZERO;
            flick.cooldown = 0.3;
        }
    }
    if dir.x.abs() >= dir.y.abs() {
        if dir.x < 0.0 {
            latch.pressed |= btn::TARGET_LEFT;
        } else if dir.x > 0.0 {
            latch.pressed |= btn::TARGET_RIGHT;
        }
    } else if dir.y > 0.0 {
        latch.pressed |= btn::TARGET_UP;
    } else {
        latch.pressed |= btn::TARGET_DOWN;
    }
}

/// Builds the local player's input for the upcoming simulation tick.
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

/// Click in the game: captures the mouse (Esc opens the menu, which releases it).
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
