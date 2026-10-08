//! Géométrie praticable : l'arène (murée), et autour d'elle des sols suspendus au-dessus du
//! vide (escalier, place, chemins, ponts). Chaque morceau de sol a une hauteur (rampes et
//! escaliers en pente continue). Au-delà d'un bord muré on bute ; au-delà d'un bord ouvert,
//! c'est la chute.
//!
//! Tout est en maths déterministes (`math`) : c'est de la simulation.

use bevy::prelude::*;

use super::data::{ArenaDef, FloorDef, Shape, Tuning};
use super::math;

/// Qui se déplace : détermine les sols accessibles et ce que valent les bords ouverts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mover {
    /// Joueur : partout ; un bord ouvert fait tomber.
    Player,
    /// Joueur pendant le combat de boss : l'arène seule (la brume ferme la sortie).
    PlayerInFight,
    /// Le boss ne quitte jamais l'arène.
    Boss,
    /// Ennemis du chemin : pas dans l'arène, et ils ne sautent jamais dans le vide.
    Enemy,
}

/// Morceau de sol (l'arène en est un : un disque muré à la hauteur 0).
#[derive(Clone, Copy, Debug)]
struct Piece {
    shape: Shape,
    walled: bool,
    arena: bool,
    /// Le disque de l'arène elle-même.
    arena_disc: bool,
}

fn pieces(t: &Tuning) -> impl Iterator<Item = Piece> + '_ {
    let disc = Piece {
        shape: Shape::Ellipse { center: [0.0, 0.0], radii: [t.arena.radius; 2], y: 0.0 },
        walled: true,
        arena: true,
        arena_disc: true,
    };
    std::iter::once(disc).chain(t.level.floors.iter().map(|f: &FloorDef| Piece {
        shape: f.shape,
        walled: f.walled,
        arena: f.arena,
        arena_disc: false,
    }))
}

fn allowed(p: &Piece, m: Mover) -> bool {
    match m {
        Mover::Player => true,
        Mover::PlayerInFight | Mover::Boss => p.arena_disc,
        Mover::Enemy => !p.arena,
    }
}

/// Marge à respecter par rapport au bord : un corps de rayon `r` bute contre un mur, mais
/// peut s'avancer au-dessus du vide jusqu'à son centre. Les ennemis restent bien sur le sol.
fn margin(p: &Piece, m: Mover, r: f32) -> f32 {
    if p.walled || m != Mover::Player { r } else { 0.0 }
}

/// Résultat d'un test « dans le morceau » : point ramené sur le morceau (avec la marge),
/// distance à ce point (0 si dedans), hauteur du sol à cet endroit.
struct Fit {
    point: Vec3,
    dist: f32,
    y: f32,
}

fn fit(shape: &Shape, pos: Vec3, m: f32) -> Fit {
    match *shape {
        Shape::Ellipse { center: [cx, cz], radii: [rx, rz], y } => {
            let (rx, rz) = ((rx - m).max(0.05), (rz - m).max(0.05));
            let (dx, dz) = (pos.x - cx, pos.z - cz);
            let k = math::sqrt((dx / rx) * (dx / rx) + (dz / rz) * (dz / rz));
            if k <= 1.0 {
                return Fit { point: pos, dist: 0.0, y };
            }
            // Projection radiale (approchée, mais continue) sur le bord de l'ellipse.
            let p = Vec3::new(cx + dx / k, pos.y, cz + dz / k);
            Fit { point: p, dist: math::flat_len(pos - p), y }
        }
        Shape::Strip { from: [x0, z0, y0], to: [x1, z1, y1], half_width } => {
            let a = Vec3::new(x0, 0.0, z0);
            let ab = Vec3::new(x1 - x0, 0.0, z1 - z0);
            let len = math::flat_len(ab).max(1e-4);
            let dir = ab / len;
            let side = Vec3::new(dir.z, 0.0, -dir.x);
            let rel = Vec3::new(pos.x, 0.0, pos.z) - a;
            // Pas de marge aux extrémités : elles se raccordent à d'autres morceaux.
            let along = rel.dot(dir).clamp(0.0, len);
            let hw = (half_width - m).max(0.0);
            let across = rel.dot(side).clamp(-hw, hw);
            let p = a + dir * along + side * across;
            let y = y0 + (y1 - y0) * (along / len);
            let p = Vec3::new(p.x, pos.y, p.z);
            Fit { point: p, dist: math::flat_len(pos - p), y }
        }
    }
}

/// Où se retrouve un corps qui veut aller en `pos`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// Sur le sol (position ajustée, y compris la hauteur).
    Ground(Vec3),
    /// Au-dessus du vide : il tombe.
    Fall,
}

/// Ramène `pos` (corps de rayon `r`) sur un sol accessible à `m`. Parmi les morceaux qui
/// contiennent le point, on garde celui dont la hauteur est la plus proche de `pos.y`.
pub fn step(t: &Tuning, pos: Vec3, r: f32, m: Mover) -> Step {
    let mut inside: Option<f32> = None;
    let mut nearest: Option<(Fit, bool)> = None;
    for p in pieces(t).filter(|p| allowed(p, m)) {
        let f = fit(&p.shape, pos, margin(&p, m, r));
        if f.dist <= 1e-5 {
            if inside.is_none_or(|y| (f.y - pos.y).abs() < (y - pos.y).abs()) {
                inside = Some(f.y);
            }
        } else if nearest.as_ref().is_none_or(|(n, _)| f.dist < n.dist) {
            nearest = Some((f, p.walled));
        }
    }
    if let Some(y) = inside {
        return Step::Ground(Vec3::new(pos.x, y, pos.z));
    }
    match nearest {
        // Bord ouvert, pour le joueur : le vide.
        Some((_, false)) if m == Mover::Player => Step::Fall,
        Some((f, _)) => Step::Ground(Vec3::new(f.point.x, f.y, f.point.z)),
        None => Step::Ground(pos),
    }
}

/// Ramène `pos` sur le sol le plus proche, à au moins `m` de ses bords (sauf aux extrémités
/// des bandes, qui se raccordent à d'autres sols).
pub fn settle(t: &Tuning, pos: Vec3, m: f32) -> Vec3 {
    let score = |f: &Fit| f.dist + (f.y - pos.y).abs();
    pieces(t)
        .map(|p| (fit(&p.shape, pos, 0.0), p))
        .min_by(|a, b| score(&a.0).total_cmp(&score(&b.0)))
        .map_or(pos, |(_, p)| {
            let f = fit(&p.shape, pos, m);
            Vec3::new(f.point.x, f.y, f.point.z)
        })
}

/// Hauteur du sol sous (x, z), s'il y en a (le plus proche de `near_y`).
pub fn floor_at(t: &Tuning, x: f32, z: f32, near_y: f32) -> Option<f32> {
    let pos = Vec3::new(x, near_y, z);
    let mut best: Option<f32> = None;
    for p in pieces(t) {
        let f = fit(&p.shape, pos, 0.0);
        if f.dist <= 1e-5 && best.is_none_or(|y| (f.y - near_y).abs() < (y - near_y).abs()) {
            best = Some(f.y);
        }
    }
    best
}

/// Premier mur traversé par le segment `from` → `to` (vu de dessus) : fraction du segment où il
/// est touché (`None` : rien ne s'interpose). Murs : le pourtour de l'arène (sauf son ouverture)
/// et les côtés des sols murés (escalier). Sert à la caméra, qui ne doit pas passer derrière.
pub fn wall_hit(t: &Tuning, from: Vec3, to: Vec3) -> Option<f32> {
    let (a, d) = (Vec2::new(from.x, from.z), Vec2::new(to.x - from.x, to.z - from.z));
    let r = t.arena.radius;
    let hw = t.arena.gate_half_width;
    let mut best: Option<f32> = None;
    let mut hit = |s: f32| {
        if (0.0..=1.0).contains(&s) && best.is_none_or(|b| s < b) {
            best = Some(s);
        }
    };
    // Cercle de l'arène : |a + s·d| = r.
    let (qa, qb, qc) = (d.dot(d), 2.0 * a.dot(d), a.dot(a) - r * r);
    let disc = qb * qb - 4.0 * qa * qc;
    if qa > 1e-8 && disc >= 0.0 {
        let sq = math::sqrt(disc);
        for s in [(-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa)] {
            let p = a + d * s;
            // L'ouverture, au sud, ne bloque pas.
            if !(p.x.abs() < hw && p.y < 0.0) {
                hit(s);
            }
        }
    }
    // Côtés des bandes murées (la partie dans l'arène ne compte pas : ce n'est que le seuil).
    for f in t.level.floors.iter().filter(|f| f.walled) {
        let Shape::Strip { from: [x0, z0, _], to: [x1, z1, _], half_width } = f.shape else { continue };
        let (p0, p1) = (Vec2::new(x0, z0), Vec2::new(x1, z1));
        let side = (p1 - p0).perp().normalize_or_zero() * half_width;
        for o in [side, -side] {
            let (w0, w) = (p0 + o, p1 - p0);
            let den = d.perp_dot(w);
            if den.abs() < 1e-8 {
                continue;
            }
            let s = (w0 - a).perp_dot(w) / den;
            let u = (w0 - a).perp_dot(d) / den;
            if (0.0..=1.0).contains(&u) && (a + d * s).length() >= r {
                hit(s);
            }
        }
    }
    best
}

/// Obstacles circulaires (x, z, rayon) : piliers de l'arène, brasiers, décor.
pub fn obstacles(t: &Tuning, checkpoint_radius: f32) -> Vec<[f32; 3]> {
    let mut v: Vec<[f32; 3]> = t.arena.pillars.clone();
    for c in &t.level.checkpoints {
        v.push([c.pos[0], c.pos[1], checkpoint_radius]);
    }
    for p in &t.level.props {
        let yaw = p.yaw.to_radians();
        for [dx, dz, r] in p.kind.colliders() {
            let w = math::local_to_world(Vec3::new(p.pos[0], 0.0, p.pos[1]), yaw, [*dx, 0.0, *dz]);
            v.push([w.x, w.z, *r]);
        }
    }
    v
}

/// Centre de la brume, dans l'ouverture du mur de l'arène.
pub fn fog_gate(a: &ArenaDef) -> Vec3 {
    let hw = a.gate_half_width;
    Vec3::new(0.0, 0.0, -math::sqrt(a.radius * a.radius - hw * hw) - 0.3)
}

/// Point juste devant la brume, côté escalier.
pub fn gate_outside(a: &ArenaDef) -> Vec3 {
    Vec3::new(0.0, 0.0, -(a.radius + 1.5))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground(s: Step) -> Vec3 {
        match s {
            Step::Ground(p) => p,
            Step::Fall => panic!("chute inattendue"),
        }
    }

    #[test]
    fn arena_is_walled_and_stairs_lead_down_to_the_plaza() {
        let t = Tuning::builtin();
        let r = 0.4;
        // Contre le mur de l'arène : on bute.
        let p = ground(step(&t, Vec3::new(20.0, 0.0, 0.0), r, Mover::Player));
        assert!((math::flat_len(p) - (t.arena.radius - r)).abs() < 1e-3);
        // Le boss et le joueur en combat ne sortent pas par l'ouverture.
        let g = gate_outside(&t.arena);
        for m in [Mover::Boss, Mover::PlayerInFight] {
            let q = ground(step(&t, g, r, m));
            assert!(math::flat_len(q) <= t.arena.radius - r + 1e-3);
        }
        // Le joueur, lui, passe sur le palier puis descend l'escalier jusqu'à la place.
        assert_eq!(ground(step(&t, g, r, Mover::Player)), g);
        let mut pos = g;
        let mut low = 0.0f32;
        for _ in 0..200 {
            pos = ground(step(&t, pos - Vec3::Z * 0.1, r, Mover::Player));
            low = low.min(pos.y);
        }
        assert!(low < -1.0, "on descend ({low})");
        // Les ennemis n'entrent pas dans l'arène.
        let q = ground(step(&t, Vec3::new(0.0, 0.0, -5.0), r, Mover::Enemy));
        assert!(q.z < -t.arena.radius);
    }

    #[test]
    fn walls_block_the_camera_but_not_the_open_end_of_the_stairs() {
        let t = Tuning::builtin();
        // Dans l'arène, vers l'extérieur : le mur.
        let s = wall_hit(&t, Vec3::new(10.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0)).expect("mur");
        assert!((s - 0.6).abs() < 1e-3);
        // Par l'ouverture, le long de l'escalier : rien.
        assert_eq!(wall_hit(&t, Vec3::new(0.0, 0.0, -10.0), Vec3::new(0.0, 0.0, -22.0)), None);
        // Au pied de l'escalier, caméra derrière soi sur la place : rien non plus.
        assert_eq!(wall_hit(&t, Vec3::new(0.0, -2.0, -23.0), Vec3::new(0.5, -1.0, -28.5)), None);
        // Sur l'escalier, caméra de côté : le mur de l'escalier.
        assert!(wall_hit(&t, Vec3::new(0.0, -1.0, -20.0), Vec3::new(5.0, 0.0, -21.0)).is_some());
    }

    #[test]
    fn open_edges_make_the_player_fall_but_not_enemies() {
        let t = Tuning::builtin();
        let cp = t.level.checkpoints[0].pos;
        let y = floor_at(&t, cp[0], cp[1], 0.0).expect("checkpoint sur le sol");
        // Très loin de tout, à la hauteur de la place.
        let far = Vec3::new(cp[0] + 60.0, y, cp[1]);
        assert_eq!(step(&t, far, 0.4, Mover::Player), Step::Fall);
        assert!(matches!(step(&t, far, 0.4, Mover::Enemy), Step::Ground(_)));
        assert!(floor_at(&t, far.x, far.z, y).is_none());
    }

    #[test]
    fn level_content_is_on_the_ground() {
        let t = Tuning::builtin();
        for c in &t.level.checkpoints {
            assert!(floor_at(&t, c.pos[0], c.pos[1], 0.0).is_some(), "checkpoint dans le vide : {c:?}");
        }
        for e in &t.level.enemies {
            assert!(floor_at(&t, e.pos[0], e.pos[1], 0.0).is_some(), "ennemi dans le vide : {e:?}");
        }
        for p in &t.level.pickups {
            assert!(floor_at(&t, p.pos[0], p.pos[1], 0.0).is_some(), "objet dans le vide : {p:?}");
        }
        for p in &t.level.props {
            assert!(floor_at(&t, p.pos[0], p.pos[1], 0.0).is_some(), "décor dans le vide : {p:?}");
        }
    }
}
