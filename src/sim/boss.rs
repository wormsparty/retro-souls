//! IA du boss : lente, télégraphiée, avec des pauses où on peut punir.

use bevy::prelude::*;

use super::data::{BossMove, MoveDef, MoveRef, Tuning};
use super::encounter::Encounter;
use super::fighter::{Action, Body, Health, Hitstop};
use super::player::{Player, motion_speed, run_frame};
use super::rng::SimRng;
use super::{DT, SimDebug, SimEvent, SimEvents, SimTick, math};

#[derive(Component, Clone, Debug)]
pub struct Boss {
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
}

impl Boss {
    pub fn new(t: &Tuning) -> Self {
        Self {
            phase: 1,
            target: None,
            last_attacker: None,
            stagger: 0.0,
            stagger_delay: 0,
            cooldowns: vec![0; t.boss.attacks.len()],
            idle: 90,
            strafe: 1.0,
            strafe_timer: 0,
        }
    }

    /// Ajoute du stagger ; retourne vrai si le boss devient groggy.
    pub fn add_stagger(&mut self, amount: f32, action: &mut Action, t: &Tuning) -> bool {
        let immune = matches!(
            action.mv,
            Some(MoveRef::Boss(BossMove::Groggy | BossMove::FatalReceived | BossMove::Death))
        );
        if immune || amount <= 0.0 {
            return false;
        }
        self.stagger += amount;
        self.stagger_delay = t.boss.stagger_delay;
        if self.stagger >= t.boss.stagger_max {
            self.stagger = t.boss.stagger_max;
            action.start(MoveRef::Boss(BossMove::Groggy), 0.0);
            action.executed = false;
            return true;
        }
        false
    }
}

pub fn boss_act(
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
    let bd = &t.boss;
    let now = tick.0;

    for (entity, mut boss, mut body, mut action, mut hitstop, health) in &mut bosses {
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
                    events.push(SimEvent::Shockwave { pos: Vec3::new(a.x, 0.0, a.z), radius: r });
                }
                continue;
            }
            match mv {
                MoveRef::Boss(BossMove::Death) => {
                    // Reste sur la dernière frame.
                    action.tick = def.total - 1;
                    action.executed = false;
                    continue;
                }
                MoveRef::Boss(BossMove::Groggy | BossMove::FatalReceived) => {
                    boss.stagger = 0.0;
                    action.stop();
                    boss.idle = rng.range(bd.idle_ticks[0], bd.idle_ticks[1]) / 2;
                }
                MoveRef::BossAttack(i) => {
                    action.stop();
                    let chained = bd.attacks[i as usize].next.as_ref().and_then(|(name, chance)| {
                        (rng.next_f32() < *chance)
                            .then(|| bd.attacks.iter().position(|a| &a.name == name))
                            .flatten()
                    });
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
        if boss.phase == 1 && health.cur <= health.max * bd.phase2_at {
            boss.phase = 2;
            action.start(MoveRef::Boss(BossMove::Roar), 0.0);
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
        body.yaw = math::turn_towards(body.yaw, want, bd.turn_rate.to_radians() * DT);
        if dist > bd.preferred_range + 0.75 {
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

        if boss.idle > 0 {
            boss.idle -= 1;
            continue;
        }
        if debug.boss_passive {
            continue;
        }
        let angle = math::wrap(want - body.yaw).abs().to_degrees();
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
    let a = &t.boss.attacks[idx];
    boss.cooldowns[idx] = now + a.cooldown;
    action.start(MoveRef::BossAttack(idx as u16), math::flat_len(target_pos - body.pos));
    if a.mv.hits.iter().any(|h| h.fury) {
        events.push(SimEvent::FuryWarn { entity });
    }
    run_frame(body, action, &a.mv, Some(target_pos), None);
}

pub fn boss_end_tick(tuning: Res<Tuning>, mut q: Query<(&mut Boss, &Action)>) {
    let bd = &tuning.boss;
    for (mut boss, action) in &mut q {
        if action.is(MoveRef::Boss(BossMove::Groggy)) {
            continue;
        }
        if boss.stagger_delay > 0 {
            boss.stagger_delay -= 1;
        } else {
            boss.stagger = (boss.stagger - bd.stagger_decay * DT).max(0.0);
        }
    }
}

/// Zone d'effet annoncée par l'action en cours : centre au sol à l'impact (en extrapolant le
/// déplacement restant), rayon, et avancement de l'anticipation (0 → 1 à l'impact).
/// Sert à dessiner l'alerte au sol ; `None` hors anticipation ou pendant l'impact passé.
pub fn aoe_telegraph(body: &Body, action: &Action, t: &Tuning) -> Option<(Vec3, f32, f32)> {
    let def = action.def(t)?;
    let h = def.hits.iter().find(|h| h.aoe && action.tick < h.end)?;
    let mut pos = body.pos;
    let f = math::forward(body.yaw);
    for tick in action.tick..h.start {
        for m in def.motion.iter().filter(|m| tick >= m.start && tick < m.end) {
            pos += f * motion_speed(m, action.target_dist) * DT;
        }
    }
    let at = Body { pos, ..*body };
    let (a, _, r) = super::combat::hit_capsule(&at, h, h.start as f32);
    let progress = (action.tick as f32 / h.start.max(1) as f32).min(1.0);
    Some((Vec3::new(a.x, 0.0, a.z), r, progress))
}

/// Vrai si l'action courante du boss contient une attaque furie pas encore déclenchée.
pub fn fury_pending(action: &Action, t: &Tuning) -> bool {
    action.def(t).is_some_and(|d: &MoveDef| {
        d.hits.iter().any(|h| h.fury && action.tick < h.end)
    })
}
