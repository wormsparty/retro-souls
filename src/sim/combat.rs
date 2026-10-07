//! Collisions entre corps, détection des coups et résolution (garde, garde parfaite, dégâts…).

use bevy::prelude::*;

use super::boss::Boss;
use super::data::{HitWindow, MoveDef, MoveRef, PlayerMove, Reaction, Tuning, WeaponMove};
use super::fighter::{Action, Body, Health, Hitstop};
use super::encounter::{CHECKPOINT_RADIUS, Encounter, checkpoint_pos, clamp_walkable};
use super::player::{PState, Player, force_move};
use super::{SimEvent, SimEvents, SimTick, math};

/// Sépare les corps qui se chevauchent et les garde dans la zone praticable.
pub fn separate_bodies(tuning: Res<Tuning>, enc: Res<Encounter>, mut q: Query<(&mut Body, Has<Player>)>) {
    let mut combos = q.iter_combinations_mut();
    while let Some([(mut a, _), (mut b, _)]) = combos.fetch_next() {
        let d = Vec3::new(b.pos.x - a.pos.x, 0.0, b.pos.z - a.pos.z);
        let dist = math::flat_len(d);
        let min = a.radius + b.radius;
        if dist < min {
            let n = if dist > 1e-4 { d / dist } else { Vec3::X };
            let overlap = min - dist;
            let total = a.mass + b.mass;
            let (wa, wb) = (b.mass / total, a.mass / total);
            a.pos -= n * overlap * wa;
            b.pos += n * overlap * wb;
        }
    }
    let arena = &tuning.arena;
    let cp = checkpoint_pos(arena);
    let obstacles = arena.pillars.iter().copied().chain([[cp.x, cp.z, CHECKPOINT_RADIUS]]);
    for (mut b, is_player) in &mut q {
        let r = b.radius;
        for [px, pz, pr] in obstacles.clone() {
            let d = Vec3::new(b.pos.x - px, 0.0, b.pos.z - pz);
            let dist = math::flat_len(d);
            if dist < pr + r && dist > 1e-4 {
                b.pos += d / dist * (pr + r - dist);
            }
        }
        // Le couloir est fermé par la brume pendant le combat, et toujours interdit au boss.
        b.pos = clamp_walkable(arena, b.pos, r, is_player && !enc.active);
        b.pos.y = 0.0;
    }
}

/// Capsule monde d'une fenêtre de frappe à un instant donné (`sub` ∈ [0,1] entre deux ticks).
pub fn hit_capsule(body: &Body, h: &HitWindow, tick: f32) -> (Vec3, Vec3, f32) {
    let (mut a, mut b) = (h.capsule.a, h.capsule.b);
    if let Some([s, e]) = h.arc {
        let span = (h.end - h.start).saturating_sub(1).max(1) as f32;
        let f = ((tick - h.start as f32) / span).clamp(0.0, 1.0);
        let deg = s + (e - s) * f;
        a = math::rotate_local(a, deg);
        b = math::rotate_local(b, deg);
    }
    (
        math::local_to_world(body.pos, body.yaw, a),
        math::local_to_world(body.pos, body.yaw, b),
        h.capsule.r,
    )
}

/// Test d'une fenêtre active contre la hurtbox (capsule verticale) d'un corps.
/// Les balayages en arc sont sous-échantillonnés pour ne pas « sauter » par-dessus la cible.
pub fn hit_test(attacker: &Body, h: &HitWindow, tick: u32, victim: &Body) -> bool {
    let h1 = victim.pos + Vec3::Y * victim.radius;
    let h2 = victim.pos + Vec3::Y * (victim.height - victim.radius).max(victim.radius);
    let samples = if h.arc.is_some() && tick > h.start { 4 } else { 1 };
    (0..samples).any(|i| {
        let ft = tick as f32 - (samples - 1 - i) as f32 / samples as f32;
        let (a, b, r) = hit_capsule(attacker, h, ft);
        math::segment_distance(a, b, h1, h2) <= r + victim.radius
    })
}

fn active_hits(def: &MoveDef, tick: u32) -> impl Iterator<Item = (u8, &HitWindow)> {
    def.hits
        .iter()
        .enumerate()
        .filter(move |(_, h)| tick >= h.start && tick < h.end)
        .map(|(i, h)| (i as u8, h))
}

struct Pending {
    attacker: Entity,
    victim: Entity,
    mv: MoveRef,
    window: u8,
}

/// Point d'impact approximatif pour les effets visuels.
fn impact_point(attacker: &Body, victim: &Body) -> Vec3 {
    let dir = (attacker.pos - victim.pos).normalize_or_zero();
    victim.pos + Vec3::Y * (victim.height * 0.55).min(1.4) + dir * victim.radius
}

#[allow(clippy::type_complexity)]
pub fn resolve_hits(
    tuning: Res<Tuning>,
    tick: Res<SimTick>,
    mut events: ResMut<SimEvents>,
    mut players: Query<
        (Entity, &mut Player, &Body, &mut Health, &mut Action, &mut Hitstop),
        Without<Boss>,
    >,
    mut bosses: Query<
        (Entity, &mut Boss, &Body, &mut Health, &mut Action, &mut Hitstop),
        Without<Player>,
    >,
) {
    let t = &*tuning;
    let now = tick.0;
    let mut pending = Vec::new();

    // 1) Détection (lecture seule), dans un ordre déterministe : joueurs puis boss.
    for (pe, _, pbody, phealth, pact, _) in &players {
        let (Some(mv), true) = (pact.mv, pact.executed) else { continue };
        if phealth.dead() {
            continue;
        }
        for (wi, h) in active_hits(t.get(mv), pact.tick) {
            for (be, _, bbody, bhealth, _, _) in &bosses {
                if !bhealth.dead()
                    && !pact.hits.contains(&(wi, be))
                    && hit_test(pbody, h, pact.tick, bbody)
                {
                    pending.push(Pending { attacker: pe, victim: be, mv, window: wi });
                }
            }
        }
    }
    for (be, _, bbody, bhealth, bact, _) in &bosses {
        let (Some(mv), true) = (bact.mv, bact.executed) else { continue };
        if bhealth.dead() {
            continue;
        }
        for (wi, h) in active_hits(t.get(mv), bact.tick) {
            for (pe, _, pbody, phealth, _, _) in &players {
                if !phealth.dead()
                    && !bact.hits.contains(&(wi, pe))
                    && hit_test(bbody, h, bact.tick, pbody)
                {
                    pending.push(Pending { attacker: be, victim: pe, mv, window: wi });
                }
            }
        }
    }

    // 2) Résolution.
    for hit in pending {
        let h = &t.get(hit.mv).hits[hit.window as usize];
        if let Ok((pe, mut p, pbody, mut php, mut pact, mut pstop)) = players.get_mut(hit.attacker) {
            // Joueur → boss.
            let Ok((_, mut boss, bbody, mut bhp, mut bact, mut bstop)) = bosses.get_mut(hit.victim)
            else {
                continue;
            };
            if bhp.dead() {
                continue;
            }
            pact.hits.push((hit.window, hit.victim));
            let dmg = h.damage;
            bhp.cur = (bhp.cur - dmg).max(0.0);
            boss.last_attacker = Some((pe, now));
            // Regain : frapper rend une partie des PV perdus en garde.
            let heal = (dmg * t.player.regain_ratio).min(p.regain);
            if heal > 0.0 {
                php.cur = (php.cur + heal).min(php.max);
                p.regain -= heal;
            }
            p.special += dmg * t.player.special_per_damage;
            pstop.0 = pstop.0.max(h.hitstop);
            bstop.0 = bstop.0.max(h.hitstop);
            let pos = impact_point(pbody, bbody);
            let heavy = matches!(
                hit.mv,
                MoveRef::Weapon(_, WeaponMove::HeavyCharged | WeaponMove::Fatal)
            );
            events.push(SimEvent::Hit { pos, heavy, on_player: false });
            if bhp.dead() {
                bact.start(MoveRef::Boss(super::data::BossMove::Death), 0.0);
                bact.executed = false;
                events.push(SimEvent::BossDied);
            } else if boss.add_stagger(h.stagger, &mut bact, t) {
                events.push(SimEvent::Groggy { entity: hit.victim });
            }
        } else if let Ok((be, mut boss, bbody, _, mut bact, mut bstop)) = bosses.get_mut(hit.attacker) {
            // Boss → joueur.
            let Ok((_, mut p, pbody, mut php, mut pact, mut pstop)) = players.get_mut(hit.victim)
            else {
                continue;
            };
            if php.dead() || pact.iframes(t) {
                // Les i-frames ne consomment pas le coup : il peut toucher plus tard dans la fenêtre.
                continue;
            }
            bact.hits.push((hit.window, hit.victim));
            let pos = impact_point(bbody, pbody);
            let to_attacker = math::yaw_of(bbody.pos - pbody.pos);
            let facing = math::wrap(to_attacker - pbody.yaw).abs()
                <= t.player.guard.arc.to_radians();
            let g = &t.player.guard;

            // Contre de la posture (spéciale de l'épée longue).
            if let Some(MoveRef::Weapon(w, WeaponMove::Special)) = pact.mv {
                let def = t.get(MoveRef::Weapon(w, WeaponMove::Special));
                if facing
                    && MoveDef::in_window(def.counter, pact.tick)
                    && t.weapons[w as usize].special_counter.is_some()
                {
                    force_move(&mut p, &mut pact, MoveRef::Weapon(w, WeaponMove::SpecialCounter));
                    pstop.0 = g.perfect_hitstop;
                    bstop.0 = g.perfect_hitstop;
                    events.push(SimEvent::Counter { pos });
                    if boss.add_stagger(g.perfect_stagger, &mut bact, t) {
                        events.push(SimEvent::Groggy { entity: be });
                    }
                    continue;
                }
            }

            let in_guard = p.state == PState::Guard
                || pact.is(MoveRef::Player(PlayerMove::GuardHit))
                || pact.is(MoveRef::Player(PlayerMove::PerfectGuard));
            if p.guard_held && facing && in_guard {
                if now.saturating_sub(p.guard_start) <= p.perfect_window(t) {
                    // Garde parfaite : aucun dégât, stagger pour le boss.
                    force_move(&mut p, &mut pact, MoveRef::Player(PlayerMove::PerfectGuard));
                    p.special += g.perfect_special;
                    pstop.0 = g.perfect_hitstop;
                    bstop.0 = g.perfect_hitstop;
                    events.push(SimEvent::PerfectGuard { pos });
                    if boss.add_stagger(g.perfect_stagger, &mut bact, t) {
                        events.push(SimEvent::Groggy { entity: be });
                    }
                    continue;
                }
                if !h.fury {
                    // Garde normale : dégâts réduits, convertis en regain.
                    let dmg = h.damage * g.damage_ratio;
                    php.cur = (php.cur - dmg).max(0.0);
                    p.regain += dmg;
                    p.regain_timer = t.player.regain_ticks;
                    p.spend_stamina(h.damage * g.stamina_ratio, t);
                    pstop.0 = h.hitstop;
                    bstop.0 = h.hitstop;
                    if php.dead() {
                        force_move(&mut p, &mut pact, MoveRef::Player(PlayerMove::Death));
                        events.push(SimEvent::PlayerDied);
                    } else if p.stamina <= 0.0 {
                        force_move(&mut p, &mut pact, MoveRef::Player(PlayerMove::GuardBreak));
                        events.push(SimEvent::GuardBreak { pos });
                    } else {
                        force_move(&mut p, &mut pact, MoveRef::Player(PlayerMove::GuardHit));
                        events.push(SimEvent::Guard { pos });
                    }
                    continue;
                }
            }

            // Coup encaissé.
            php.cur = (php.cur - h.damage).max(0.0);
            p.regain = 0.0;
            p.regain_timer = 0;
            pstop.0 = h.hitstop;
            bstop.0 = h.hitstop;
            events.push(SimEvent::Hit { pos, heavy: h.reaction == Reaction::Heavy, on_player: true });
            if php.dead() {
                force_move(&mut p, &mut pact, MoveRef::Player(PlayerMove::Death));
                events.push(SimEvent::PlayerDied);
            } else if !pact.hyperarmor(t) {
                let mv = match h.reaction {
                    Reaction::Light => PlayerMove::HitLight,
                    Reaction::Heavy => PlayerMove::HitHeavy,
                };
                force_move(&mut p, &mut pact, MoveRef::Player(mv));
            }
        }
    }
}
