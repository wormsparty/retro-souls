//! Inputs abstraits d'un joueur pour un tick. C'est la seule chose que la simulation lit
//! côté joueur : en réseau, c'est exactement ce qui sera échangé entre les pairs.

use bevy::prelude::*;

pub const MAX_PLAYERS: usize = 2;

/// Boutons logiques (bitfield).
pub mod btn {
    pub const LIGHT: u16 = 1 << 0;
    pub const HEAVY: u16 = 1 << 1;
    pub const GUARD: u16 = 1 << 2;
    pub const SPECIAL: u16 = 1 << 3;
    pub const DODGE: u16 = 1 << 4;
    pub const LOCK: u16 = 1 << 5;
    pub const SWITCH: u16 = 1 << 6;
    /// Utiliser l'objet de l'emplacement rapide sélectionné.
    pub const ITEM: u16 = 1 << 7;
    /// Emplacement rapide suivant.
    pub const NEXT_ITEM: u16 = 1 << 8;
    /// Interagir (se reposer au checkpoint).
    pub const INTERACT: u16 = 1 << 9;
    /// Lancer la course (clic du stick) : elle dure tant que le stick est poussé.
    pub const SPRINT: u16 = 1 << 10;
    /// Cible suivante, à gauche ou à droite (verrouillé : stick droit, souris).
    pub const TARGET_LEFT: u16 = 1 << 11;
    pub const TARGET_RIGHT: u16 = 1 << 12;
    /// Point verrouillable plus haut ou plus bas (la tête d'un grand boss, ses pattes).
    pub const TARGET_UP: u16 = 1 << 13;
    pub const TARGET_DOWN: u16 = 1 << 14;
    pub const COUNT: usize = 15;
}

/// Input compact d'un joueur pour un tick (7 octets, prêt à être sérialisé pour le réseau).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PlayerInput {
    pub buttons: u16,
    /// Stick de déplacement quantifié (-127..=127). y = vers l'avant de la caméra.
    pub move_x: i8,
    pub move_y: i8,
    /// Yaw de la caméra quantifié sur 16 bits (les déplacements sont relatifs à la caméra).
    pub cam_yaw: i16,
}

impl PlayerInput {
    pub fn held(&self, b: u16) -> bool {
        self.buttons & b != 0
    }
    pub fn stick(&self) -> Vec2 {
        Vec2::new(self.move_x as f32 / 127.0, self.move_y as f32 / 127.0)
    }
    pub fn cam_yaw_rad(&self) -> f32 {
        self.cam_yaw as f32 / 32768.0 * std::f32::consts::PI
    }
    pub fn quantize_yaw(yaw: f32) -> i16 {
        let w = crate::sim::math::wrap(yaw);
        (w / std::f32::consts::PI * 32767.0).round() as i16
    }
    pub fn quantize_axis(v: f32) -> i8 {
        (v.clamp(-1.0, 1.0) * 127.0).round() as i8
    }
}

/// Inputs de tous les joueurs pour le tick en cours (rempli avant chaque tick de sim).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct PlayerInputs(pub [PlayerInput; MAX_PLAYERS]);

/// Mémorise les appuis récents pour qu'une action pressée un peu trop tôt parte dès que possible.
#[derive(Clone, Copy, Debug, Default, Hash)]
pub struct InputBuffer {
    /// Tick de l'appui non consommé pour chaque bouton.
    pressed_at: [Option<u32>; btn::COUNT],
    prev: u16,
}

impl InputBuffer {
    /// À appeler une fois par tick avec les boutons courants. Retourne les fronts montants.
    pub fn update(&mut self, buttons: u16, tick: u32) -> u16 {
        let pressed = buttons & !self.prev;
        for i in 0..btn::COUNT {
            if pressed & (1 << i) != 0 {
                self.pressed_at[i] = Some(tick);
            }
        }
        self.prev = buttons;
        pressed
    }

    /// Vrai si `b` a été pressé dans les `window` derniers ticks et pas encore consommé.
    pub fn buffered(&self, b: u16, tick: u32, window: u32) -> bool {
        let i = b.trailing_zeros() as usize;
        self.pressed_at[i].is_some_and(|t| tick.saturating_sub(t) <= window)
    }

    pub fn consume(&mut self, b: u16) {
        self.pressed_at[b.trailing_zeros() as usize] = None;
    }

    pub fn clear(&mut self) {
        self.pressed_at = [None; btn::COUNT];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_expires_and_consumes() {
        let mut b = InputBuffer::default();
        b.update(btn::LIGHT, 10);
        assert!(b.buffered(btn::LIGHT, 15, 8));
        assert!(!b.buffered(btn::LIGHT, 19, 8));
        b.consume(btn::LIGHT);
        assert!(!b.buffered(btn::LIGHT, 11, 8));
        // Maintenir le bouton ne re-déclenche pas.
        b.update(btn::LIGHT, 11);
        assert!(!b.buffered(btn::LIGHT, 11, 8));
    }

    #[test]
    fn yaw_roundtrip() {
        for y in [-3.0f32, -1.0, 0.0, 0.5, 3.1] {
            let q = PlayerInput { cam_yaw: PlayerInput::quantize_yaw(y), ..default() };
            assert!((q.cam_yaw_rad() - y).abs() < 1e-3);
        }
    }
}
