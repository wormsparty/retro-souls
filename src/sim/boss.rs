//! IA des boss : lente, télégraphiée, avec des pauses où on peut punir. Chaque boss a sa
//! définition (`boss.ron`, `bosses.ron`) : attaques, sorts, parties à verrouiller. Une rencontre
//! peut en réunir plusieurs (un duo, un boucher et ses chiens).

use bevy::prelude::*;

use super::data::{BossDef, BossMove, MoveDef, MoveRef, Side, Tuning};
use super::encounter::Encounter;
use super::fighter::{Action, Body, Health, Hitstop};
use super::player::{Player, motion_speed, run_frame};
use super::rng::SimRng;
use super::{DT, SimDebug, SimEvent, SimEvents, SimTick, math, spell};

#[derive(Component, Clone, Debug)]
pub struct Boss {
    /// Définition (index dans `Tuning::bosses`).
    pub def: u8,
    pub phase: u8,
    pub target: Option<Entity>,
    /// Dernier joueur à l'avoir frappé, et quand (aggro en coop).
    pub last_attacker: Option<(Entity, u32)>,
    pub stagger: f32,
    pub stagger_delay: u32,
    /// Tick à partir duquel chaque attaque redevient disponible.
    pub cooldowns: Vec<u32>,
    /// Ticks restants avant de pouvoir attaquer.
    pub idle: u32,
    pub strafe: f32,
    pub strafe_timer: u32,
    /// Grande bête (`heading_slack`) : elle est en train de se tourner vers sa cible.
    pub turning: bool,
    /// Écart (radians) entre la direction visée et la cible, renouvelé de temps en temps.
    pub aim_offset: f32,
    pub aim_timer: u32,
}

impl Boss {
    pub fn new(t: &Tuning, def: u8) -> Self {
        Self {
            def,
            phase: 1,
            target: None,
            last_attacker: None,
            stagger: 0.0,
            stagger_delay: 0,
            cooldowns: vec![0; t.bosses[def as usize].attacks.len()],
            idle: 90,
            strafe: 1.0,
            strafe_timer: 0,
            turning: false,
            aim_offset: 0.0,
            aim_timer: 0,
        }
    }

    pub fn def<'a>(&self, t: &'a Tuning) -> &'a BossDef {
        &t.bosses[self.def as usize]
    }

    /// Ajoute du stagger ; retourne vrai si le boss devient groggy.
    pub fn add_stagger(&mut self, amount: f32, action: &mut Action, t: &Tuning) -> bool {
        let immune = matches!(
            action.mv,
            Some(MoveRef::Boss(_, BossMove::Groggy | BossMove::FatalReceived | BossMove::Death))
        );
        if immune || amount <= 0.0 {
            return false;
        }
        let bd = self.def(t);
        self.stagger += amount;
        self.stagger_delay = bd.stagger_delay;
        if self.stagger >= bd.stagger_max {
            self.stagger = bd.stagger_max;
            action.start(MoveRef::Boss(self.def, BossMove::Groggy), 0.0);
            action.executed = false;
            return true;
        }
        false
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn boss_act(
    mut commands: Commands,
    tuning: Res<Tuning>,
    tick: Res<SimTick>,
    debug: Res<SimDebug>,
    encounter: Res<Encounter>,
    mut rng: ResMut<SimRng>,
    mut events: ResMut<SimEvents>,
    mut bosses: Query<
        (Entity, &mut Boss, &mut Body, &mut Action, &mut Hitstop, &Health),
        Without<Player>,
    >,
    players: Query<(Entity, &Body, &Health), (With<Player>, Without<Boss>)>,
) {
    let t = &*tuning;
    let now = tick.0;
    // Un des premiers rôles est tombé : son partenaire passe en phase 2.
    let partner_fallen = bosses.iter().any(|(_, b, .., h)| h.dead() && !b.def(t).minor);

    for (entity, mut boss, mut body, mut action, mut hitstop, health) in &mut bosses {
        let def = boss.def;
        let bd = &t.bosses[def as usize];
        if hitstop.0 > 0 {
            hitstop.0 -= 1;
            continue;
        }
        action.executed = true;

        // Choix de la cible : dernier attaquant récent, sinon le joueur vivant le plus proche.
        let alive = |e: Entity| players.get(e).is_ok_and(|(_, _, h)| !h.dead());
        let recent = boss
            .last_attacker
            .filter(|&(e, at)| now.saturating_sub(at) <= bd.aggro_ticks && alive(e))
            .map(|(e, _)| e);
        boss.target = recent.or_else(|| {
            players
                .iter()
                .filter(|(_, _, h)| !h.dead())
                .min_by(|a, b| a.1.pos.distance(body.pos).total_cmp(&b.1.pos.distance(body.pos)))
                .map(|(e, ..)| e)
        });
        let target_pos = boss.target.and_then(|e| players.get(e).ok()).map(|(_, b, _)| b.pos);

        if let Some(mv) = action.mv {
            let def = t.get(mv);
            if action.tick < def.total {
                // Saut qui retombe sur la cible : distance suivie jusqu'au décollage.
                if let Some(tp) = target_pos
                    && def.motion.iter().any(|m| m.to_target && m.retarget && action.tick <= m.start)
                {
                    action.target_dist = math::flat_len(tp - body.pos);
                }
                run_frame(&mut body, &action, def, target_pos, None);
                for h in def.hits.iter().filter(|h| h.aoe && h.start == action.tick) {
                    let (a, _, r) = super::combat::hit_capsule(&body, h, action.tick as f32);
                    events.push(SimEvent::Shockwave { pos: Vec3::new(a.x, body.pos.y, a.z), radius: r, boss: boss.def });
                }
                // Une même attaque ne touche qu'une fois, quel que soit son nombre de sorts.
                let volley = (entity.to_bits() as u32).rotate_left(16) ^ action.seq;
                for c in def.casts.iter().filter(|c| c.at == action.tick) {
                    let aim = boss.target.zip(target_pos);
                    spell::cast(&mut commands, t, boss.def, (entity, action.seq), c, &body, aim, volley, &mut rng, &mut events);
                }
                continue;
            }
            match mv {
                MoveRef::Boss(_, BossMove::Death) => {
                    // Reste sur la dernière frame.
                    action.tick = def.total - 1;
                    action.executed = false;
                    continue;
                }
                MoveRef::Boss(_, BossMove::Groggy | BossMove::FatalReceived) => {
                    boss.stagger = 0.0;
                    action.stop();
                    boss.idle = rng.range(bd.idle_ticks[0], bd.idle_ticks[1]) / 2;
                }
                MoveRef::BossAttack(_, i) => {
                    action.stop();
                    let a = &bd.attacks[i as usize];
                    let chained = (!a.next.is_empty()).then(|| a.chained(&bd.attacks, rng.next_f32())).flatten();
                    if let (Some(j), Some(tp)) = (chained, target_pos) {
                        start_attack(&mut boss, &mut body, &mut action, j, tp, t, now, &mut events, entity);
                        continue;
                    }
                    boss.idle = rng.range(bd.idle_ticks[0], bd.idle_ticks[1]);
                }
                _ => {
                    action.stop();
                    boss.idle = bd.idle_ticks[0];
                }
            }
        }

        // Endormi tant que personne n'est entré dans l'arène.
        if health.dead() || !encounter.active {
            continue;
        }
        if boss.phase == 1 && (health.cur <= health.max * bd.phase2_at || (partner_fallen && !bd.minor)) {
            boss.phase = 2;
            action.start(MoveRef::Boss(def, BossMove::Roar), 0.0);
            events.push(SimEvent::BossPhase2);
            continue;
        }
        let Some(tp) = target_pos else {
            continue;
        };

        // Approche : se tourner lentement vers la cible, marcher ou tourner autour.
        let to = tp - body.pos;
        let dist = math::flat_len(to);
        let want = math::yaw_of(to);
        if bd.heading_slack > 0.0 {
            lumber(&mut boss, &mut body, bd, want, dist, &mut rng);
        } else {
            body.yaw = math::turn_towards(body.yaw, want, bd.turn_rate.to_radians() * DT);
            if bd.keep_away > 0.0 && dist < bd.keep_away {
                // Lanceur de sorts : il recule face à la cible.
                let back = math::forward(body.yaw) * bd.walk_speed * DT;
                body.pos -= back;
            } else if dist > bd.preferred_range + 0.75 {
                let f = math::forward(body.yaw);
                body.pos += f * bd.walk_speed * DT;
            } else {
                if boss.strafe_timer == 0 {
                    boss.strafe = if rng.next_f32() < 0.5 { -1.0 } else { 1.0 };
                    boss.strafe_timer = rng.range(90, 180);
                }
                boss.strafe_timer -= 1;
                let r = math::right(body.yaw);
                body.pos += r * boss.strafe * bd.strafe_speed * DT;
            }
        }

        if boss.idle > 0 {
            boss.idle -= 1;
            continue;
        }
        if debug.boss_passive {
            continue;
        }
        let signed = math::wrap(want - body.yaw);
        let angle = signed.abs().to_degrees();
        let side = if signed > 0.0 { Side::Left } else { Side::Right };
        let phase = boss.phase;
        let candidates: Vec<(usize, f32)> = bd
            .attacks
            .iter()
            .enumerate()
            .filter(|(i, a)| {
                a.phases.contains(&phase)
                    && boss.cooldowns[*i] <= now
                    && dist >= a.min_range
                    && dist <= a.max_range
                    && angle <= a.max_angle
                    && angle >= a.min_angle
                    && (a.side == Side::Any || a.side == side)
            })
            .map(|(i, a)| (i, a.weight))
            .collect();
        let total: f32 = candidates.iter().map(|c| c.1).sum();
        if total <= 0.0 {
            continue;
        }
        let mut roll = rng.next_f32() * total;
        let mut chosen = candidates[candidates.len() - 1].0;
        for (i, w) in &candidates {
            if roll < *w {
                chosen = *i;
                break;
            }
            roll -= w;
        }
        start_attack(&mut boss, &mut body, &mut action, chosen, tp, t, now, &mut events, entity);
    }
}

/// Déplacement d'une grande bête : elle ne se tourne que lorsque sa cible sort du cône
/// `heading_slack`, vise à peu près sa direction, et n'avance que face à elle. Tout près, elle
/// pivote plus lentement (ses attaques de côté la font aussi tourner).
fn lumber(boss: &mut Boss, body: &mut Body, bd: &BossDef, want: f32, dist: f32, rng: &mut SimRng) {
    let slack = bd.heading_slack.to_radians();
    if boss.aim_timer == 0 {
        boss.aim_offset = (rng.next_f32() - 0.5) * slack * 0.8;
        boss.aim_timer = rng.range(120, 260);
    }
    boss.aim_timer -= 1;
    let err = math::wrap(want - body.yaw);
    boss.turning |= err.abs() > slack;
    if boss.turning {
        let goal = want + boss.aim_offset;
        let near = dist < bd.radius + 2.5;
        // Tout près, elle pivote plus lentement : la cible sur son flanc a le temps d'en profiter
        // (ou d'y prendre un coup de queue), mais elle finit par lui faire face.
        let rate = bd.turn_rate.to_radians() * DT * if near { 0.7 } else { 1.0 };
        body.yaw = math::turn_towards(body.yaw, goal, rate);
        boss.turning = math::wrap(goal - body.yaw).abs() > 0.05;
    }
    if dist > bd.preferred_range + 0.75 && err.abs() < 1.0 {
        body.pos += math::forward(body.yaw) * bd.walk_speed * DT;
    }
}

#[allow(clippy::too_many_arguments)]
fn start_attack(
    boss: &mut Boss,
    body: &mut Body,
    action: &mut Action,
    idx: usize,
    target_pos: Vec3,
    t: &Tuning,
    now: u32,
    events: &mut SimEvents,
    entity: Entity,
) {
    let a = &boss.def(t).attacks[idx];
    boss.cooldowns[idx] = now + a.cooldown;
    action.start(MoveRef::BossAttack(boss.def, idx as u16), math::flat_len(target_pos - body.pos));
    if a.mv.hits.iter().any(|h| h.fury) {
        events.push(SimEvent::FuryWarn { entity });
    }
    run_frame(body, action, &a.mv, Some(target_pos), None);
}

pub fn boss_end_tick(tuning: Res<Tuning>, mut q: Query<(&mut Boss, &Action)>) {
    for (mut boss, action) in &mut q {
        let bd = &tuning.bosses[boss.def as usize];
        if action.is(MoveRef::Boss(boss.def, BossMove::Groggy)) {
            continue;
        }
        if boss.stagger_delay > 0 {
            boss.stagger_delay -= 1;
        } else {
            boss.stagger = (boss.stagger - bd.stagger_decay * DT).max(0.0);
        }
    }
}

/// Avertissement minimal d'une zone d'effet (ticks) : le cercle reste fixe tout ce temps, et le
/// boss cesse de suivre sa cible dès qu'il apparaît (`Tuning::parse`).
pub const MIN_WARNING: u32 = 40;

/// Tick à partir duquel l'endroit visé par la zone d'effet `h` ne bouge plus : fin du suivi de
/// la cible, derniers sorts lancés, fin d'un pivot — au plus tard `MIN_WARNING` ticks avant
/// l'impact. Un saut qui retombe sur sa cible la suit jusqu'au décollage : son vol sert d'alerte.
pub fn aoe_lock_tick(def: &MoveDef, h: &super::data::HitWindow) -> u32 {
    let casts = def.casts.iter().map(|c| c.at + 1).filter(|&at| at < h.start).max().unwrap_or(0);
    let jumps = def.motion.iter().filter(|m| m.to_target && m.retarget && m.start < h.start).map(|m| m.start).max();
    // Un pivot en cours déplace aussi la zone : elle n'est fixée qu'après.
    let turns = def.motion.iter().filter(|m| m.turn != 0.0 && m.start < h.start).map(|m| m.end).max().unwrap_or(0);
    match jumps {
        Some(j) => j.max(casts).max(turns).min(h.start),
        None => casts.max(turns).max(def.track_until).min(h.start.saturating_sub(MIN_WARNING)),
    }
}

/// Zone d'effet annoncée par l'action en cours : centre au sol à l'impact (en extrapolant le
/// déplacement restant), rayon, et avancement de l'anticipation (0 → 1 à l'impact).
/// Sert à dessiner l'alerte au sol ; `None` hors anticipation ou pendant l'impact passé, et
/// tant que l'endroit n'est pas fixé (le cercle ne suit jamais le joueur).
pub fn aoe_telegraph(body: &Body, action: &Action, t: &Tuning) -> Option<(Vec3, f32, f32)> {
    let def = action.def(t)?;
    let h = def.hits.iter().find(|h| h.aoe && action.tick < h.end)?;
    let from = aoe_lock_tick(def, h);
    if action.tick < from {
        return None;
    }
    let mut pos = body.pos;
    let f = math::forward(body.yaw);
    for tick in action.tick..h.start {
        for m in def.motion.iter().filter(|m| tick >= m.start && tick < m.end) {
            pos += (f * motion_speed(m, action.target_dist) + math::right(body.yaw) * m.side) * DT;
        }
    }
    let at = Body { pos, ..*body };
    let (a, _, r) = super::combat::hit_capsule(&at, h, h.start as f32);
    let progress = ((action.tick - from) as f32 / (h.start - from).max(1) as f32).min(1.0);
    Some((Vec3::new(a.x, 0.0, a.z), r, progress))
}

/// Vrai si l'action courante du boss contient une attaque furie pas encore déclenchée.
pub fn fury_pending(action: &Action, t: &Tuning) -> bool {
    action.def(t).is_some_and(|d: &MoveDef| {
        d.hits.iter().any(|h| h.fury && action.tick < h.end)
    })
}

/// Vrai si l'action courante prépare un coup qu'on ne peut pas bloquer et qu'aucun cercle au sol
/// n'annonce (attaque furie, jet de feu) : le modèle rougeoie, il faut fuir.
pub fn unblockable_pending(action: &Action, t: &Tuning, boss: Option<&BossDef>) -> bool {
    if fury_pending(action, t) {
        return true;
    }
    let (Some(d), Some(bd)) = (action.def(t), boss) else { return false };
    d.casts.iter().any(|c| {
        bd.spell(&c.spell).map(|i| &bd.spells[i as usize]).is_some_and(|sd| {
            let open = sd.kind == super::data::SpellKind::Beam || (sd.kind == super::data::SpellKind::Bolt && sd.aoe);
            open && action.tick < c.at + sd.life
        })
    })
}

/// Points verrouillables d'un adversaire (repère monde, position de simulation) : les parties
/// d'un grand boss, sinon le milieu du corps.
pub fn lock_points(t: &Tuning, body: &Body, boss: Option<&Boss>) -> Vec<Vec3> {
    let parts: Vec<Vec3> = boss
        .map(|b| &b.def(t).parts)
        .into_iter()
        .flatten()
        .filter(|p| p.lock)
        .map(|p| math::local_to_world(body.pos, body.yaw, p.at))
        .collect();
    if parts.is_empty() { vec![body.pos + Vec3::Y * body.height * 0.55] } else { parts }
}

/// Index de la `n`-ième partie verrouillable dans `parts` (pour retrouver sa pièce du modèle).
pub fn lock_part(def: &BossDef, n: u8) -> Option<&super::data::PartDef> {
    def.parts.iter().filter(|p| p.lock).nth(n as usize)
}
