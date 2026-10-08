//! Composants communs à tous les combattants (joueurs et boss).

use bevy::prelude::*;

use super::data::{MoveDef, MoveRef, Tuning};

/// Position et orientation de simulation. Le `Transform` de rendu est interpolé à partir de ça.
#[derive(Component, Clone, Copy, Debug)]
pub struct Body {
    pub pos: Vec3,
    pub yaw: f32,
    pub radius: f32,
    pub height: f32,
    /// Poids relatif pour la séparation des corps (le boss pousse plus qu'il n'est poussé).
    pub mass: f32,
}

/// État du tick précédent, pour l'interpolation de rendu.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PrevBody {
    pub pos: Vec3,
    pub yaw: f32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Health {
    pub cur: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { cur: max, max }
    }
    pub fn dead(&self) -> bool {
        self.cur <= 0.0
    }
}

/// Gel de l'entité pendant N ticks après un impact (« hitstop »).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Hitstop(pub u8);

/// Adversaire des joueurs (boss ou ennemi du chemin) : on peut le verrouiller et le frapper.
#[derive(Component, Clone, Copy, Debug)]
pub struct Foe;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Team {
    Players,
    Enemies,
}

/// Action en cours (attaque, esquive, réaction…), commune aux joueurs et au boss.
#[derive(Component, Clone, Debug, Default)]
pub struct Action {
    pub mv: Option<MoveRef>,
    /// Index du tick en cours d'exécution dans l'action.
    pub tick: u32,
    /// Compteur incrémenté à chaque nouvelle action (identifie l'instance d'attaque).
    pub seq: u32,
    /// Coups déjà portés pendant cette action : (index de la fenêtre, victime).
    pub hits: Vec<(u8, Entity)>,
    /// Distance à la cible figée au démarrage (pour les mouvements `to_target`).
    pub target_dist: f32,
    /// Vrai si l'entité a exécuté une frame ce tick (faux pendant le hitstop).
    pub executed: bool,
}

impl Action {
    pub fn start(&mut self, mv: MoveRef, target_dist: f32) {
        self.mv = Some(mv);
        self.tick = 0;
        self.seq = self.seq.wrapping_add(1);
        self.hits.clear();
        self.target_dist = target_dist;
    }
    pub fn stop(&mut self) {
        self.mv = None;
        self.tick = 0;
        self.hits.clear();
    }
    pub fn def<'a>(&self, t: &'a Tuning) -> Option<&'a MoveDef> {
        self.mv.map(|m| t.get(m))
    }
    pub fn finished(&self, t: &Tuning) -> bool {
        self.def(t).is_some_and(|d| self.tick >= d.total)
    }
    pub fn is(&self, mv: MoveRef) -> bool {
        self.mv == Some(mv)
    }
    pub fn iframes(&self, t: &Tuning) -> bool {
        self.def(t).is_some_and(|d| MoveDef::in_window(d.iframes, self.tick))
    }
    pub fn hyperarmor(&self, t: &Tuning) -> bool {
        self.def(t).is_some_and(|d| MoveDef::in_window(d.hyperarmor, self.tick))
    }
}
