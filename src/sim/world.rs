//! Walkable geometry: the (walled) arena, and around it floors hanging above the
//! void (stairs, square, paths, bridges). Each piece of floor has a height (ramps and
//! stairs on a continuous slope). Past a walled edge you bump; past an open edge,
//! you fall.
//!
//! Everything uses deterministic maths (`math`): this is simulation.

use bevy::prelude::*;

use super::data::{ArenaDef, FloorDef, Shape, Tuning};
use super::math;

/// Who is moving: determines the reachable floors and what open edges mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mover {
    /// Player: everywhere; an open edge makes them fall.
    Player,
    /// Player during the boss fight: the arena only (the fog closes the exit).
    PlayerInFight,
    /// The boss never leaves the arena.
    Boss,
    /// Path enemies: not in the arena, and they never jump into the void.
    Enemy,
}

/// Piece of floor (the arena is one: a walled disc at height 0).
#[derive(Clone, Copy, Debug)]
struct Piece {
    shape: Shape,
    walled: bool,
    arena: bool,
    /// The arena disc itself.
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

/// Margin to keep from the edge: a body of radius `r` bumps against a wall, but
/// can step out over the void up to its centre. Enemies stay well on the floor.
fn margin(p: &Piece, m: Mover, r: f32) -> f32 {
    if p.walled || m != Mover::Player { r } else { 0.0 }
}

/// Wall thickness, outside the walled floor (stair balustrades, tools/blender/arena.py).
const WALL_THICKNESS: f32 = 0.4;

/// Result of an "inside the piece" test: point brought back onto the piece (with the margin),
/// distance to that point (0 if inside), floor height at that spot.
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
            // Radial projection (approximate, but continuous) onto the edge of the ellipse.
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
            // No margin at the ends: they connect to other pieces.
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

/// Where a body that wants to go to `pos` ends up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// On the floor (adjusted position, including height).
    Ground(Vec3),
    /// Above the void: it falls.
    Fall,
}

/// Brings `pos` (body of radius `r`) back onto a floor reachable by `m`. Among the pieces that
/// contain the point, keep the one whose height is closest to `pos.y`.
/// A body touching the wall of a walled floor bumps, even if the nearest floor is open
/// (at the foot of the stairs, along the balustrades, next to the landing).
pub fn step(t: &Tuning, pos: Vec3, r: f32, m: Mover) -> Step {
    let mut inside: Option<f32> = None;
    let mut nearest: Option<(Fit, bool)> = None;
    let mut against_wall = false;
    for p in pieces(t).filter(|p| allowed(p, m)) {
        if p.walled && fit(&p.shape, pos, -(WALL_THICKNESS + r)).dist <= 1e-5 {
            against_wall = true;
        }
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
        // Open edge, for the player: the void.
        Some((_, false)) if m == Mover::Player && !against_wall => Step::Fall,
        Some((f, _)) => Step::Ground(Vec3::new(f.point.x, f.y, f.point.z)),
        None => Step::Ground(pos),
    }
}

/// Brings `pos` back onto the nearest floor, at least `m` from its edges (except at the ends
/// of strips, which connect to other floors).
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

/// Floor height under (x, z), if any (the one closest to `near_y`).
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

/// First wall crossed by the segment `from` → `to` (seen from above): fraction of the segment where it
/// is hit (`None`: nothing in the way). Walls: the arena's perimeter (except its opening)
/// and the sides of walled floors (stairs). Used by the camera, which must not go behind them.
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
    // Arena circle: |a + s·d| = r.
    let (qa, qb, qc) = (d.dot(d), 2.0 * a.dot(d), a.dot(a) - r * r);
    let disc = qb * qb - 4.0 * qa * qc;
    if qa > 1e-8 && disc >= 0.0 {
        let sq = math::sqrt(disc);
        for s in [(-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa)] {
            let p = a + d * s;
            // The opening, to the south, doesn't block.
            if !(p.x.abs() < hw && p.y < 0.0) {
                hit(s);
            }
        }
    }
    // Sides of walled strips (the part inside the arena doesn't count: it's just the threshold).
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

/// Circular obstacles (x, z, radius): arena pillars, braziers, decor.
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

/// Centre of the fog, in the opening of the arena wall.
pub fn fog_gate(a: &ArenaDef) -> Vec3 {
    let hw = a.gate_half_width;
    Vec3::new(0.0, 0.0, -math::sqrt(a.radius * a.radius - hw * hw) - 0.3)
}

/// Point just in front of the fog, on the stairs side.
pub fn gate_outside(a: &ArenaDef) -> Vec3 {
    Vec3::new(0.0, 0.0, -(a.radius + 1.5))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground(s: Step) -> Vec3 {
        match s {
            Step::Ground(p) => p,
            Step::Fall => panic!("unexpected fall"),
        }
    }

    #[test]
    fn arena_is_walled_and_stairs_lead_down_to_the_plaza() {
        let t = Tuning::builtin();
        let r = 0.4;
        // Against the arena wall: bump.
        let p = ground(step(&t, Vec3::new(20.0, 0.0, 0.0), r, Mover::Player));
        assert!((math::flat_len(p) - (t.arena.radius - r)).abs() < 1e-3);
        // The boss and the player in a fight don't go out through the opening.
        let g = gate_outside(&t.arena);
        for m in [Mover::Boss, Mover::PlayerInFight] {
            let q = ground(step(&t, g, r, m));
            assert!(math::flat_len(q) <= t.arena.radius - r + 1e-3);
        }
        // The player, though, goes onto the landing then down the stairs to the square.
        assert_eq!(ground(step(&t, g, r, Mover::Player)), g);
        let mut pos = g;
        let mut low = 0.0f32;
        for _ in 0..200 {
            pos = ground(step(&t, pos - Vec3::Z * 0.1, r, Mover::Player));
            low = low.min(pos.y);
        }
        assert!(low < -1.0, "going down ({low})");
        // Enemies don't enter the arena.
        let q = ground(step(&t, Vec3::new(0.0, 0.0, -5.0), r, Mover::Enemy));
        assert!(q.z < -t.arena.radius);
    }

    #[test]
    fn no_fall_along_the_stair_rails_from_the_landing() {
        let t = Tuning::builtin();
        let r = 0.4;
        // From the lower landing, going back up towards the arena along the balustrades, on each
        // side and at every distance from the axis: you bump, you never fall.
        for i in 0..=48 {
            let x = -2.4 + 0.1 * i as f32;
            let mut pos = Vec3::new(x, -2.4, -24.5);
            for _ in 0..40 {
                pos = ground(step(&t, pos + Vec3::Z * 0.1, r, Mover::Player));
            }
        }
        // The landing's open edge, though, always makes you fall.
        assert_eq!(step(&t, Vec3::new(3.0, -2.4, -23.5), r, Mover::Player), Step::Fall);
    }

    #[test]
    fn walls_block_the_camera_but_not_the_open_end_of_the_stairs() {
        let t = Tuning::builtin();
        // In the arena, towards the outside: the wall.
        let s = wall_hit(&t, Vec3::new(10.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0)).expect("wall");
        assert!((s - 0.6).abs() < 1e-3);
        // Through the opening, along the stairs: nothing.
        assert_eq!(wall_hit(&t, Vec3::new(0.0, 0.0, -10.0), Vec3::new(0.0, 0.0, -22.0)), None);
        // At the foot of the stairs, camera behind you on the square: nothing either.
        assert_eq!(wall_hit(&t, Vec3::new(0.0, -2.0, -23.0), Vec3::new(0.5, -1.0, -28.5)), None);
        // On the stairs, camera from the side: the stair wall.
        assert!(wall_hit(&t, Vec3::new(0.0, -1.0, -20.0), Vec3::new(5.0, 0.0, -21.0)).is_some());
    }

    #[test]
    fn open_edges_make_the_player_fall_but_not_enemies() {
        let t = Tuning::builtin();
        let cp = t.level.checkpoints[0].pos;
        let y = floor_at(&t, cp[0], cp[1], 0.0).expect("checkpoint on the ground");
        // Very far from everything, at the height of the square.
        let far = Vec3::new(cp[0] + 60.0, y, cp[1]);
        assert_eq!(step(&t, far, 0.4, Mover::Player), Step::Fall);
        assert!(matches!(step(&t, far, 0.4, Mover::Enemy), Step::Ground(_)));
        assert!(floor_at(&t, far.x, far.z, y).is_none());
    }

    #[test]
    fn level_content_is_on_the_ground() {
        let t = Tuning::builtin();
        for c in &t.level.checkpoints {
            assert!(floor_at(&t, c.pos[0], c.pos[1], 0.0).is_some(), "checkpoint in the void: {c:?}");
        }
        for e in &t.level.enemies {
            assert!(floor_at(&t, e.pos[0], e.pos[1], 0.0).is_some(), "enemy in the void: {e:?}");
        }
        for p in &t.level.pickups {
            assert!(floor_at(&t, p.pos[0], p.pos[1], 0.0).is_some(), "item in the void: {p:?}");
        }
        for p in &t.level.props {
            assert!(floor_at(&t, p.pos[0], p.pos[1], 0.0).is_some(), "decor in the void: {p:?}");
        }
    }
}
