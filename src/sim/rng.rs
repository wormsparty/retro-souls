//! RNG déterministe (xorshift32) dont l'état fait partie de la simulation.

use bevy::prelude::*;

#[derive(Resource, Clone, Copy, Debug, Hash)]
pub struct SimRng(pub u32);

impl Default for SimRng {
    fn default() -> Self {
        Self(0x9E37_79B9)
    }
}

impl SimRng {
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Flottant uniforme dans [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    /// Entier uniforme dans [lo, hi].
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + self.next_u32() % (hi - lo + 1)
    }
}
