//! Collisions entre corps, détection des coups et résolution (garde, garde parfaite, dégâts…).

use bevy::prelude::*;

use super::boss::Boss;
use super::data::{EnemyMove, HitWindow, MoveDef, MoveRef, PlayerMove, Reaction, Tuning, WeaponMove};
use super::encounter::{CHECKPOINT_RADIUS, Encounter};
use super::enemy::Enemy;
use super::fighter::{Action, Body, Foe, Health, Hitstop, PrevBody};
use super::player::{PState, Player, force_move};
use super::world::{self, Mover, Step};
use super::{DT, SimEvent, SimEvents, SimTick, math};

/// Sépare les corps qui se chevauchent et les garde sur un sol praticable. Un joueur poussé
/// (ou qui marche, roule…) au-delà d'un bord ouvert tombe.
#[allow(clippy::type_complexity)]
pub fn separate_bodies(
    tuning: Res<Tuning>,
    enc: Res<Encounter>,
    mut events: ResMut<SimEvents>,
    mut q: Query<(Entity, &mut Body, &PrevBody, &Health, Option<&mut Player>, &mut Action, Has<Boss>)>,
) {
    let solid = |h: &Health, p: Option<&Mut<Player>>| !h.dead() && !p.is_some_and(|p| p.falling);
    let mut combos = q.iter_combinations_mut();
    while let Some([(_, mut a, _, ha, pa, ..), (_, mut b, _, hb, pb, ..)]) = combos.fetch_next() {
        if !solid(ha, pa.as_ref()) || !solid(hb, pb.as_ref()) {
            continue;
        }
        let d = Vec3::new(b.pos.x - a.pos.x, 0.0, b.pos.z - a.pos.z);
        let dist = math::flat_len(d);
        let min = a.radius + b.radius;
        if dist < min && (a.pos.y - b.pos.y).abs() < 1.5 {
            let n = if dist > 1e-4 { d / dist } else { Vec3::X };
            let overlap = min - dist;
            let total = a.mass + b.mass;
            let (wa, wb) = (b.mass / total, a.mass / total);
            a.pos -= n * overlap * wa;
            b.pos += n * overlap * wb;
        }
    }
    let t = &*tuning;
    let obstacles = world::obstacles(t, CHECKPOINT_RADIUS);
    for (entity, mut b, prev, hp, mut player, mut action, is_boss) in &mut q {
        if player.as_ref().is_some_and(|p| p.falling) {
            continue;
        }
        let r = b.radius;
        for [px, pz, pr] in obstacles.iter().copied() {
            let d = Vec3::new(b.pos.x - px, 0.0, b.pos.z - pz);
            let dist = math::flat_len(d);
            if dist < pr + r && dist > 1e-4 {
                b.pos += d / dist * (pr + r - dist);
            }
        }
        // La brume ferme l'arène pendant le combat ; le boss n'en sort jamais, les ennemis du
        // chemin n'y entrent pas.
        let mover = match (&player, is_boss) {
            (Some(_), _) if enc.active => Mover::PlayerInFight,
            (Some(_), _) => Mover::Player,
            (None, true) => Mover::Boss,
            (None, false) => Mover::Enemy,
        };
        if let Some(p) = player.as_mut().filter(|p| p.airborne) {
            air_step(t, &mut b, prev, p, &mut action, mover, hp.dead(), entity, &mut events);
            continue;
        }
        match world::step(t, b.pos, r, mover) {
            Step::Ground(p) => b.pos = p,
            Step::Fall => {
                if let Some(mut p) = player {
                    let momentum = (b.pos - prev.pos) / DT;
                    p.start_fall(&mut action, momentum, prev.pos);
                    events.push(SimEvent::Fell { entity });
                }
            }
        }
    }
}

/// Un sol plus haut que ça au-dessus des pieds arrête un sauteur (il en heurte le bord) ;
/// en dessous, il s'y hisse en retombant.
const LEDGE: f32 = 0.35;
/// Au-dessus du vide, passé d'autant sous le sol quitté, le saut devient une chute.
const AIR_FALL: f32 = 1.0;

/// Joueur en l'air : les murs l'arrêtent, le vide non. Il atterrit sur le premier sol qu'il
/// rencontre en retombant ; s'il passe trop bas au-dessus du vide, c'est la chute.
#[allow(clippy::too_many_arguments)]
fn air_step(t: &Tuning, b: &mut Body, prev: &PrevBody, p: &mut Player, action: &mut Action, mover: Mover, dead: bool, entity: Entity, events: &mut SimEvents) {
    if let Step::Ground(g) = world::step(t, b.pos, b.radius, mover) {
        b.pos.x = g.x;
        b.pos.z = g.z;
    }
    let mut floor = world::floor_at(t, b.pos.x, b.pos.z, b.pos.y);
    if floor.is_some_and(|y| y > b.pos.y + LEDGE) {
        // Contre le flanc d'une plate-forme plus haute : on y reste collé, on retombe.
        b.pos.x = prev.pos.x;
        b.pos.z = prev.pos.z;
        p.vel = Vec3::ZERO;
        floor = world::floor_at(t, b.pos.x, b.pos.z, b.pos.y).filter(|y| *y <= b.pos.y + LEDGE);
    }
    match floor {
        Some(y) if p.air_vy <= 0.0 && b.pos.y <= y => {
            b.pos.y = y;
            p.airborne = false;
            p.air_vy = 0.0;
            events.push(SimEvent::Landed { entity });
        }
        None if b.pos.y < p.air_from - AIR_FALL => {
            let vy = p.air_vy;
            let momentum = p.vel;
            p.start_fall(action, momentum, Vec3::new(prev.pos.x, p.air_from, prev.pos.z));
            p.fall_vy = vy;
            if dead {
                p.state = PState::Dead;
            }
            events.push(SimEvent::Fell { entity });
        }
        _ => {}
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
///
/// `reach_down` : la frappe s'abaisse jusqu'aux adversaires plus petits qu'elle (un chien
/// sous un estoc porté à hauteur de poitrine), comme le ferait l'animation.
pub fn hit_test(attacker: &Body, h: &HitWindow, tick: u32, victim: &Body, reach_down: bool) -> bool {
    let h1 = victim.pos + Vec3::Y * victim.radius;
    let h2 = victim.pos + Vec3::Y * (victim.height - victim.radius).max(victim.radius);
    let top = victim.pos.y + victim.height;
    let samples = if h.arc.is_some() && tick > h.start { 4 } else { 1 };
    (0..samples).any(|i| {
        let ft = tick as f32 - (samples - 1 - i) as f32 / samples as f32;
        let (mut a, mut b, r) = hit_capsule(attacker, h, ft);
        if reach_down {
            a.y = a.y.min(top);
            b.y = b.y.min(top);
        }
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

/// Coup encaissé par un adversaire (boss ou ennemi) qui ne le tue pas : stagger du boss,
/// interruption de l'ennemi. Vrai si le boss devient groggy.
fn foe_stagger(boss: Option<&mut Boss>, enemy: Option<&mut Enemy>, attacker: Entity, amount: f32, poise: f32, action: &mut Action, t: &Tuning) -> bool {
    if let Some(b) = boss {
        return b.add_stagger(amount, action, t);
    }
    if let Some(e) = enemy {
        e.take_hit(attacker, poise, action, t);
    }
    false
}

#[allow(clippy::type_complexity)]
pub fn resolve_hits(
    tuning: Res<Tuning>,
    tick: Res<SimTick>,
    mut events: ResMut<SimEvents>,
    mut players: Query<
        (Entity, &mut Player, &Body, &mut Health, &mut Action, &mut Hitstop),
        Without<Foe>,
    >,
    mut foes: Query<
        (Entity, Option<&mut Boss>, Option<&mut Enemy>, &Body, &mut Health, &mut Action, &mut Hitstop),
        (With<Foe>, Without<Player>),
    >,
) {
    let t = &*tuning;
    let now = tick.0;
    let mut pending = Vec::new();
    let targetable = |p: &Player, h: &Health| !h.dead() && !matches!(p.state, PState::Falling | PState::Dead);

    // 1) Détection (lecture seule), dans un ordre déterministe : joueurs puis adversaires.
    for (pe, p, pbody, phealth, pact, _) in &players {
        let (Some(mv), true) = (pact.mv, pact.executed) else { continue };
        if !targetable(p, phealth) {
            continue;
        }
        for (wi, h) in active_hits(t.get(mv), pact.tick) {
            for (fe, _, _, fbody, fhealth, _, _) in &foes {
                if !fhealth.dead() && !pact.hits.contains(&(wi, fe)) && hit_test(pbody, h, pact.tick, fbody, true) {
                    pending.push(Pending { attacker: pe, victim: fe, mv, window: wi });
                }
            }
        }
    }
    for (fe, _, _, fbody, fhealth, fact, _) in &foes {
        let (Some(mv), true) = (fact.mv, fact.executed) else { continue };
        if fhealth.dead() || matches!(mv, MoveRef::Enemy(_, EnemyMove::Death)) {
            continue;
        }
        for (wi, h) in active_hits(t.get(mv), fact.tick) {
            for (pe, p, pbody, phealth, _, _) in &players {
                if targetable(p, phealth) && !fact.hits.contains(&(wi, pe)) && hit_test(fbody, h, fact.tick, pbody, false) {
                    pending.push(Pending { attacker: fe, victim: pe, mv, window: wi });
                }
            }
        }
    }

    // 2) Résolution.
    for hit in pending {
        let h = &t.get(hit.mv).hits[hit.window as usize];
        if let Ok((pe, mut p, pbody, mut php, mut pact, mut pstop)) = players.get_mut(hit.attacker) {
            // Joueur → adversaire.
            let Ok((fe, mut boss, mut enemy, fbody, mut fhp, mut fact, mut fstop)) = foes.get_mut(hit.victim) else {
                continue;
            };
            if fhp.dead() {
                continue;
            }
            pact.hits.push((hit.window, hit.victim));
            let dmg = h.damage * p.damage_mult();
            fhp.cur = (fhp.cur - dmg).max(0.0);
            if let Some(b) = boss.as_mut() {
                b.last_attacker = Some((pe, now));
            }
            // Regain : frapper rend une partie des PV perdus en garde.
            let heal = (dmg * t.player.regain_ratio).min(p.regain);
            if heal > 0.0 {
                php.cur = (php.cur + heal).min(php.max);
                p.regain -= heal;
            }
            // Les coups de la spéciale ne rechargent pas la jauge (sinon elle se rembourse).
            if !matches!(hit.mv, MoveRef::Weapon(_, WeaponMove::Special | WeaponMove::SpecialCounter)) {
                p.special += dmg * t.player.special_per_damage;
            }
            pstop.0 = pstop.0.max(h.hitstop);
            fstop.0 = fstop.0.max(h.hitstop);
            let pos = impact_point(pbody, fbody);
            let heavy = matches!(hit.mv, MoveRef::Weapon(_, WeaponMove::HeavyCharged | WeaponMove::Fatal));
            events.push(SimEvent::Hit { pos, heavy, on_player: false });
            if fhp.dead() {
                if boss.is_some() {
                    fact.start(MoveRef::Boss(super::data::BossMove::Death), 0.0);
                    fact.executed = false;
                    events.push(SimEvent::BossDied);
                } else if let Some(e) = enemy.as_mut() {
                    fact.start(MoveRef::Enemy(e.kind, EnemyMove::Death), 0.0);
                    fact.executed = false;
                    let embers = t.enemies[e.kind as usize].embers;
                    p.embers = p.embers.saturating_add(embers);
                    if e.unique {
                        p.slain |= 1u64 << e.spawn;
                    }
                    events.push(SimEvent::EnemyDied { pos: fbody.pos, embers });
                }
            } else if foe_stagger(boss.as_deref_mut(), enemy.as_deref_mut(), pe, h.stagger, dmg, &mut fact, t) {
                events.push(SimEvent::Groggy { entity: fe });
            }
        } else if let Ok((fe, mut boss, mut enemy, fbody, _, mut fact, mut fstop)) = foes.get_mut(hit.attacker) {
            // Adversaire → joueur.
            let Ok((pe, mut p, pbody, mut php, mut pact, mut pstop)) = players.get_mut(hit.victim) else {
                continue;
            };
            if php.dead() || pact.iframes(t) {
                // Les i-frames ne consomment pas le coup : il peut toucher plus tard dans la fenêtre.
                continue;
            }
            fact.hits.push((hit.window, hit.victim));
            let pos = impact_point(fbody, pbody);
            let to_attacker = math::yaw_of(fbody.pos - pbody.pos);
            let facing = math::wrap(to_attacker - pbody.yaw).abs() <= t.player.guard.arc.to_radians();
            let g = &t.player.guard;
            let damage = h.damage * p.defense_mult();
            // Garde parfaite ou contre : stagger du boss ; un ennemi est interrompu (sauf s'il
            // a assez d'équilibre : il en perd beaucoup).
            let parry = g.perfect_stagger;
            let parry_poise = g.perfect_stagger * 5.0;

            // Contre de la posture (spéciale de l'épée longue). Une onde de choc ne se contre pas.
            if let (Some(MoveRef::Weapon(w, WeaponMove::Special)), false) = (pact.mv, h.aoe) {
                let def = t.get(MoveRef::Weapon(w, WeaponMove::Special));
                if facing && MoveDef::in_window(def.counter, pact.tick) && t.weapons[w as usize].special_counter.is_some() {
                    force_move(&mut p, &mut pact, MoveRef::Weapon(w, WeaponMove::SpecialCounter));
                    pstop.0 = g.perfect_hitstop;
                    fstop.0 = g.perfect_hitstop;
                    events.push(SimEvent::Counter { pos });
                    if foe_stagger(boss.as_deref_mut(), enemy.as_deref_mut(), pe, parry, parry_poise, &mut fact, t) {
                        events.push(SimEvent::Groggy { entity: fe });
                    }
                    continue;
                }
            }

            let in_guard = p.state == PState::Guard
                || pact.is(MoveRef::Player(PlayerMove::GuardHit))
                || pact.is(MoveRef::Player(PlayerMove::PerfectGuard));
            if p.guard_held && facing && in_guard && !h.aoe {
                if now.saturating_sub(p.guard_start) <= p.perfect_window(t) {
                    // Garde parfaite : aucun dégât, stagger pour l'attaquant.
                    force_move(&mut p, &mut pact, MoveRef::Player(PlayerMove::PerfectGuard));
                    p.special += g.perfect_special;
                    pstop.0 = g.perfect_hitstop;
                    fstop.0 = g.perfect_hitstop;
                    events.push(SimEvent::PerfectGuard { pos });
                    if foe_stagger(boss.as_deref_mut(), enemy.as_deref_mut(), pe, parry, parry_poise, &mut fact, t) {
                        events.push(SimEvent::Groggy { entity: fe });
                    }
                    continue;
                }
                if !h.fury {
                    // Garde normale : dégâts réduits, convertis en regain.
                    let dmg = damage * g.damage_ratio;
                    php.cur = (php.cur - dmg).max(0.0);
                    p.regain += dmg;
                    p.regain_timer = t.player.regain_ticks;
                    p.spend_stamina(h.damage * g.stamina_ratio, t);
                    pstop.0 = h.hitstop;
                    fstop.0 = h.hitstop;
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
            php.cur = (php.cur - damage).max(0.0);
            p.regain = 0.0;
            p.regain_timer = 0;
            pstop.0 = h.hitstop;
            fstop.0 = h.hitstop;
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
