//! Machine à états du joueur.

use bevy::prelude::*;

use super::boss::Boss;
use super::data::{BossMove, MoveRef, PlayerMove, Tuning, WeaponMove};
use super::encounter::{Encounter, near_checkpoint};
use super::fighter::{Action, Body, Health, Hitstop};
use super::input::{InputBuffer, PlayerInputs, btn};
use super::items::{Inventory, Item};
use super::{DT, SimEvent, SimEvents, SimTick, math};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PState {
    /// Déplacement libre.
    Free,
    /// Garde maintenue.
    Guard,
    /// Attaque lourde en cours de charge.
    Charging,
    /// Une action (`Action::mv`) est en cours.
    Acting,
    Dead,
}

#[derive(Component, Clone, Debug)]
pub struct Player {
    pub id: u8,
    pub state: PState,
    pub weapon: u8,
    /// Index de la prochaine attaque légère du combo.
    pub combo: u8,
    /// Jusqu'à ce tick, une attaque légère continue le combo après la fin de la précédente.
    pub combo_until: u32,
    pub stamina: f32,
    pub stamina_delay: u32,
    /// PV récupérables en frappant (« regain »).
    pub regain: f32,
    pub regain_timer: u32,
    /// Jauge d'attaque spéciale.
    pub special: f32,
    pub guard_start: u32,
    pub guard_held: bool,
    pub last_guard_press: u32,
    pub guard_spam: u8,
    pub charge: u32,
    pub lock: Option<Entity>,
    pub vel: Vec3,
    pub sprinting: bool,
    /// Course lancée par `btn::SPRINT`, active jusqu'à ce que le joueur s'arrête.
    pub sprint_latched: bool,
    pub dodge_held: u32,
    pub buffer: InputBuffer,
    /// Le changement d'arme de l'action `Switch` en cours a déjà été appliqué.
    pub switched: bool,
    pub inventory: Inventory,
    /// Le soin de l'action `Heal` en cours a déjà été appliqué.
    pub healed: bool,
    /// Braises (monnaie), gagnées en battant le boss.
    pub embers: u32,
    /// Ticks passés à l'état `Dead` (réapparition au bout de `RESPAWN_TICKS`).
    pub dead_ticks: u32,
}

impl Player {
    pub fn new(id: u8, t: &Tuning) -> Self {
        Self {
            id,
            state: PState::Free,
            weapon: 0,
            combo: 0,
            combo_until: 0,
            stamina: t.player.max_stamina,
            stamina_delay: 0,
            regain: 0.0,
            regain_timer: 0,
            special: t.player.special_per_segment,
            guard_start: 0,
            guard_held: false,
            last_guard_press: 0,
            guard_spam: 0,
            charge: 0,
            lock: None,
            vel: Vec3::ZERO,
            sprinting: false,
            sprint_latched: false,
            dodge_held: 0,
            buffer: InputBuffer::default(),
            switched: false,
            inventory: Inventory::new_game(t),
            healed: false,
            embers: 0,
            dead_ticks: 0,
        }
    }

    /// Il faut au moins 1 point d'endurance pour attaquer, esquiver ou utiliser la spéciale.
    pub fn can_act(&self) -> bool {
        self.stamina >= 1.0
    }

    /// Fenêtre de garde parfaite effective (réduite si on spamme la garde).
    pub fn perfect_window(&self, t: &Tuning) -> u32 {
        let g = &t.player.guard;
        g.perfect_window
            .saturating_sub(self.guard_spam as u32 * g.spam_penalty)
            .max(g.min_window)
    }

    pub fn spend_stamina(&mut self, amount: f32, t: &Tuning) {
        if amount > 0.0 {
            // L'endurance peut passer en négatif : il faudra attendre qu'elle remonte.
            self.stamina = (self.stamina - amount).max(t.player.stamina_floor);
            self.stamina_delay = t.player.stamina_delay;
        }
    }

    pub fn special_max(&self, t: &Tuning) -> f32 {
        t.player.special_segments as f32 * t.player.special_per_segment
    }
}

/// Infos sur la cible verrouillée ou le boss le plus proche, extraites avant de muter le joueur.
#[derive(Clone, Copy)]
struct TargetInfo {
    entity: Entity,
    pos: Vec3,
    yaw: f32,
    radius: f32,
    groggy: bool,
}

pub fn player_act(
    tuning: Res<Tuning>,
    inputs: Res<PlayerInputs>,
    tick: Res<SimTick>,
    encounter: Res<Encounter>,
    mut events: ResMut<SimEvents>,
    mut players: Query<
        (Entity, &mut Player, &mut Body, &mut Action, &mut Hitstop, &mut Health),
        Without<Boss>,
    >,
    mut bosses: Query<(Entity, &Body, &mut Action, &Health), (With<Boss>, Without<Player>)>,
) {
    let t = &*tuning;
    let now = tick.0;
    let pd = &t.player;

    for (entity, mut p, mut body, mut action, mut hitstop, mut health) in &mut players {
        let inp = inputs.0[p.id as usize % inputs.0.len()];
        let pressed = p.buffer.update(inp.buttons, now);
        p.guard_held = inp.held(btn::GUARD);
        p.dodge_held = if inp.held(btn::DODGE) { p.dodge_held + 1 } else { 0 };
        if pressed & btn::SPRINT != 0 {
            p.sprint_latched = true;
        }

        const STAMINA_BTNS: u16 = btn::LIGHT | btn::HEAVY | btn::DODGE | btn::SPECIAL;
        if pressed & STAMINA_BTNS != 0 && !p.can_act() && p.state != PState::Dead {
            events.push(SimEvent::NoStamina { entity });
        }

        if pressed & btn::GUARD != 0 {
            p.guard_spam = if now.saturating_sub(p.last_guard_press) < pd.guard.spam_window {
                (p.guard_spam + 1).min(8)
            } else {
                0
            };
            p.last_guard_press = now;
            // Re-presser la garde pendant un impact en garde relance la fenêtre de garde parfaite.
            let in_guard_move = action.is(MoveRef::Player(PlayerMove::GuardHit))
                || action.is(MoveRef::Player(PlayerMove::PerfectGuard));
            if p.state == PState::Guard || in_guard_move {
                p.guard_start = now;
            }
        }

        // Verrouillage (perdu à la mort du joueur ou de la cible).
        let alive = |e: Entity| bosses.get(e).is_ok_and(|(_, _, _, h)| !h.dead());
        if health.dead() || p.lock.is_some_and(|e| !alive(e)) {
            p.lock = None;
        }
        if pressed & btn::LOCK != 0 && !health.dead() {
            if p.lock.is_some() {
                p.lock = None;
            } else {
                p.lock = bosses
                    .iter()
                    .filter(|(_, b, _, h)| !h.dead() && b.pos.distance(body.pos) <= pd.lock_range)
                    .min_by(|a, b| {
                        a.1.pos.distance(body.pos).total_cmp(&b.1.pos.distance(body.pos))
                    })
                    .map(|(e, ..)| e);
            }
        }

        let target = {
            let pick = p.lock.or_else(|| {
                bosses
                    .iter()
                    .filter(|(_, _, _, h)| !h.dead())
                    .min_by(|a, b| {
                        a.1.pos.distance(body.pos).total_cmp(&b.1.pos.distance(body.pos))
                    })
                    .map(|(e, ..)| e)
            });
            pick.and_then(|e| bosses.get(e).ok()).map(|(e, b, a, _)| TargetInfo {
                entity: e,
                pos: b.pos,
                yaw: b.yaw,
                radius: b.radius,
                groggy: a.is(MoveRef::Boss(BossMove::Groggy)),
            })
        };
        let locked_pos = p.lock.and(target.map(|ti| ti.pos));

        // Direction de déplacement en monde, relative à la caméra.
        let stick = inp.stick();
        let stick_len = stick.length().min(1.0);
        let cam = inp.cam_yaw_rad();
        let move_dir = (stick_len > 0.15).then(|| {
            (math::right(cam) * stick.x + math::forward(cam) * stick.y).normalize_or_zero()
        });

        if p.state == PState::Dead {
            continue;
        }
        if pressed & btn::NEXT_ITEM != 0 && !health.dead() {
            let before = p.inventory.active;
            p.inventory.cycle();
            if p.inventory.active != before {
                events.push(SimEvent::ItemCycled { entity });
            }
        }
        if hitstop.0 > 0 {
            hitstop.0 -= 1;
            continue;
        }
        action.executed = true;

        let mut ctx = Ctx {
            t,
            now,
            entity,
            move_dir,
            locked_pos,
            target,
            events: &mut events,
        };

        if p.state == PState::Acting {
            let mv = action.mv.expect("Acting sans action");
            let def = t.get(mv);
            if action.tick >= def.total {
                action.stop();
                if mv == MoveRef::Player(PlayerMove::Death) {
                    p.state = PState::Dead;
                    continue;
                }
                if matches!(mv, MoveRef::Weapon(_, WeaponMove::Light(_))) {
                    p.combo_until = now + 12;
                }
                let from_guard = matches!(
                    mv,
                    MoveRef::Player(PlayerMove::GuardHit | PlayerMove::PerfectGuard)
                );
                if p.guard_held {
                    if !from_guard {
                        p.guard_start = now;
                    }
                    p.state = PState::Guard;
                } else {
                    p.state = PState::Free;
                }
            } else {
                if mv == MoveRef::Player(PlayerMove::Switch)
                    && !p.switched
                    && action.tick >= pd.switch_at
                {
                    p.switched = true;
                    p.weapon = (p.weapon + 1) % t.weapons.len() as u8;
                    p.combo = 0;
                    ctx.events.push(SimEvent::WeaponSwitched { entity });
                }
                if mv == MoveRef::Player(PlayerMove::Heal) && !p.healed && action.tick >= pd.heal_at {
                    p.healed = true;
                    health.cur = (health.cur + health.max * pd.heal_ratio).min(health.max);
                    ctx.events.push(SimEvent::Heal { entity });
                }
                let can_cancel = action.tick >= def.cancel_tick();
                let can_chain = action.tick >= def.chain_tick();
                let interrupted = (can_cancel
                    && try_defensive(&mut p, &mut body, &mut action, &mut ctx))
                    || (can_chain
                        && (try_item(&mut p, &mut body, &mut action, &mut ctx)
                            || try_offensive(&mut p, &mut body, &mut action, &mut bosses, &mut ctx)));
                if !interrupted {
                    run_frame(&mut body, &action, def, locked_pos, move_dir);
                    if def.walk > 0.0 {
                        // Marche lente autorisée (soin).
                        if let Some(d) = move_dir {
                            body.pos += d * def.walk * stick_len * DT;
                            let want = locked_pos.map(|tp| math::yaw_of(tp - body.pos)).unwrap_or(math::yaw_of(d));
                            body.yaw = math::turn_towards(body.yaw, want, pd.turn_rate.to_radians() * 0.5 * DT);
                        }
                    }
                    continue;
                }
            }
        }

        match p.state {
            PState::Acting => {
                // Une nouvelle action vient de démarrer : on exécute sa première frame.
                let def = action.def(t).expect("action");
                run_frame(&mut body, &action, def, locked_pos, move_dir);
            }
            PState::Charging => {
                p.charge += 1;
                let w = &t.weapons[p.weapon as usize];
                if p.buffer.buffered(btn::DODGE, now, pd.input_buffer) && p.can_act() {
                    p.charge = 0;
                    try_defensive(&mut p, &mut body, &mut action, &mut ctx);
                } else if !inp.held(btn::HEAVY) || p.charge >= w.charge_ticks {
                    // Relâchée avant la charge complète : lourde normale ; sinon, part toute seule.
                    let wm = if p.charge >= w.charge_ticks {
                        WeaponMove::HeavyCharged
                    } else {
                        WeaponMove::Heavy
                    };
                    p.charge = 0;
                    let mv = MoveRef::Weapon(p.weapon, wm);
                    start_move(&mut p, &mut body, &mut action, mv, &mut ctx);
                    let def = action.def(t).expect("action");
                    run_frame(&mut body, &action, def, locked_pos, move_dir);
                } else {
                    let desired = locked_pos
                        .map(|tp| math::yaw_of(tp - body.pos))
                        .or(move_dir.map(math::yaw_of));
                    if let Some(y) = desired {
                        body.yaw = math::turn_towards(body.yaw, y, pd.turn_rate.to_radians() * 0.5 * DT);
                    }
                }
            }
            PState::Free | PState::Guard => {
                if !p.guard_held && p.state == PState::Guard {
                    p.state = PState::Free;
                }
                if try_rest(&mut p, &body, &mut health, &encounter, &mut ctx) {
                    // Repos : rien d'autre ce tick.
                } else if try_defensive(&mut p, &mut body, &mut action, &mut ctx)
                    && p.state == PState::Acting
                {
                    let def = action.def(t).expect("action");
                    run_frame(&mut body, &action, def, locked_pos, move_dir);
                } else if try_item(&mut p, &mut body, &mut action, &mut ctx)
                    || try_offensive(&mut p, &mut body, &mut action, &mut bosses, &mut ctx)
                {
                    if p.state == PState::Acting {
                        let def = action.def(t).expect("action");
                        run_frame(&mut body, &action, def, locked_pos, move_dir);
                    }
                } else {
                    locomotion(&mut p, &mut body, stick_len, move_dir, locked_pos, t);
                }
            }
            PState::Dead => {}
        }
    }
}

struct Ctx<'a, 'w> {
    t: &'a Tuning,
    now: u32,
    entity: Entity,
    move_dir: Option<Vec3>,
    locked_pos: Option<Vec3>,
    target: Option<TargetInfo>,
    events: &'a mut ResMut<'w, SimEvents>,
}

/// Esquive ou garde. Retourne vrai si l'état a changé.
fn try_defensive(p: &mut Player, body: &mut Body, action: &mut Action, ctx: &mut Ctx) -> bool {
    let pd = &ctx.t.player;
    if p.buffer.buffered(btn::DODGE, ctx.now, pd.input_buffer) && p.can_act() {
        p.buffer.consume(btn::DODGE);
        p.charge = 0;
        let mv = if let Some(d) = ctx.move_dir {
            body.yaw = math::yaw_of(d);
            PlayerMove::Dodge
        } else {
            PlayerMove::Backstep
        };
        start_move(p, body, action, MoveRef::Player(mv), ctx);
        ctx.events.push(SimEvent::Dodge { entity: ctx.entity });
        return true;
    }
    if p.guard_held && p.state != PState::Guard {
        if action.mv.is_some() {
            action.stop();
        }
        p.charge = 0;
        p.state = PState::Guard;
        p.guard_start = ctx.now;
        return true;
    }
    false
}

/// Attaques, spéciale, changement d'arme. Retourne vrai si une action a démarré.
fn try_offensive(
    p: &mut Player,
    body: &mut Body,
    action: &mut Action,
    bosses: &mut Query<(Entity, &Body, &mut Action, &Health), (With<Boss>, Without<Player>)>,
    ctx: &mut Ctx,
) -> bool {
    let t = ctx.t;
    let pd = &t.player;
    let buf = pd.input_buffer;
    let now = ctx.now;
    if !p.can_act() {
        return false;
    }
    let w = p.weapon;
    let wd = &t.weapons[w as usize];

    if p.buffer.buffered(btn::LIGHT, now, buf) {
        p.buffer.consume(btn::LIGHT);
        // Coup fatal sur un boss groggy, de face et à portée.
        if let Some(ti) = ctx.target.filter(|ti| ti.groggy) {
            let to_player = body.pos - ti.pos;
            let dist = math::flat_len(to_player);
            let ang = math::wrap(math::yaw_of(to_player) - ti.yaw).abs();
            if dist <= pd.fatal_range + ti.radius && ang <= pd.fatal_arc.to_radians() {
                if let Ok((_, _, mut bact, _)) = bosses.get_mut(ti.entity) {
                    bact.start(MoveRef::Boss(BossMove::FatalReceived), 0.0);
                    bact.executed = true;
                }
                body.pos = ti.pos + math::forward(ti.yaw) * (ti.radius + body.radius + 0.35);
                body.yaw = math::yaw_of(ti.pos - body.pos);
                start_move(p, body, action, MoveRef::Weapon(w, WeaponMove::Fatal), ctx);
                ctx.events.push(SimEvent::Fatal { pos: ti.pos + Vec3::Y * 1.5 });
                return true;
            }
        }
        let idx = match action.mv {
            Some(MoveRef::Weapon(pw, WeaponMove::Light(i))) if pw == w => i + 1,
            _ if now <= p.combo_until => p.combo,
            _ => 0,
        } % wd.light.len() as u8;
        p.combo = (idx + 1) % wd.light.len() as u8;
        start_move(p, body, action, MoveRef::Weapon(w, WeaponMove::Light(idx)), ctx);
        return true;
    }
    if p.buffer.buffered(btn::HEAVY, now, buf) {
        p.buffer.consume(btn::HEAVY);
        action.stop();
        p.state = PState::Charging;
        p.charge = 0;
        p.combo = 0;
        return true;
    }
    if p.buffer.buffered(btn::SPECIAL, now, buf) && p.special >= pd.special_per_segment {
        p.buffer.consume(btn::SPECIAL);
        p.special -= pd.special_per_segment;
        start_move(p, body, action, MoveRef::Weapon(w, WeaponMove::Special), ctx);
        return true;
    }
    if p.buffer.buffered(btn::SWITCH, now, buf) {
        p.buffer.consume(btn::SWITCH);
        p.switched = false;
        start_move(p, body, action, MoveRef::Player(PlayerMove::Switch), ctx);
        return true;
    }
    false
}

/// Utilise l'objet de l'emplacement rapide sélectionné. Ne demande pas d'endurance.
fn try_item(p: &mut Player, body: &mut Body, action: &mut Action, ctx: &mut Ctx) -> bool {
    if !p.buffer.buffered(btn::ITEM, ctx.now, ctx.t.player.input_buffer) {
        return false;
    }
    p.buffer.consume(btn::ITEM);
    let Some(item) = p.inventory.active_item() else { return false };
    if !p.inventory.consume(item) {
        return false;
    }
    match item {
        Item::HealFlask => {
            p.healed = false;
            start_move(p, body, action, MoveRef::Player(PlayerMove::Heal), ctx);
        }
    }
    true
}

/// Repos au checkpoint (hors combat) : PV, endurance et objets restaurés.
fn try_rest(p: &mut Player, body: &Body, health: &mut Health, enc: &Encounter, ctx: &mut Ctx) -> bool {
    if !p.buffer.buffered(btn::INTERACT, ctx.now, ctx.t.player.input_buffer) {
        return false;
    }
    p.buffer.consume(btn::INTERACT);
    if enc.active || !near_checkpoint(&ctx.t.arena, body.pos) {
        return false;
    }
    health.cur = health.max;
    p.stamina = ctx.t.player.max_stamina;
    p.stamina_delay = 0;
    p.regain = 0.0;
    p.regain_timer = 0;
    p.inventory.refill(ctx.t);
    p.lock = None;
    p.vel = Vec3::ZERO;
    p.sprinting = false;
    ctx.events.push(SimEvent::Rested { entity: ctx.entity });
    true
}

/// Coût d'endurance d'une action : explicite, ou proportionnel aux dégâts pour les attaques.
pub fn stamina_cost(mv: MoveRef, t: &Tuning) -> f32 {
    let def = t.get(mv);
    match (def.stamina, mv) {
        (Some(c), _) => c,
        (None, MoveRef::Weapon(..)) => def.total_damage() * t.player.stamina_per_damage,
        _ => 0.0,
    }
}

/// Démarre une action : coût d'endurance, orientation initiale, événements.
fn start_move(p: &mut Player, body: &mut Body, action: &mut Action, mv: MoveRef, ctx: &mut Ctx) {
    p.spend_stamina(stamina_cost(mv, ctx.t), ctx.t);
    if matches!(mv, MoveRef::Weapon(..)) {
        // Les attaques s'orientent d'emblée vers la cible verrouillée ou la direction du stick.
        if let Some(tp) = ctx.locked_pos {
            body.yaw = math::yaw_of(tp - body.pos);
        } else if let Some(d) = ctx.move_dir {
            body.yaw = math::yaw_of(d);
        }
        let heavy = matches!(mv, MoveRef::Weapon(_, WeaponMove::Heavy | WeaponMove::HeavyCharged));
        ctx.events.push(SimEvent::Swing { entity: ctx.entity, heavy });
    }
    let dist = ctx.target.map(|ti| math::flat_len(ti.pos - body.pos)).unwrap_or(0.0);
    action.start(mv, dist);
    p.state = PState::Acting;
    p.sprinting = false;
    p.sprint_latched = false;
}

/// Démarre une action subie (réaction à un coup, garde…), en dehors de la boucle d'input.
pub fn force_move(p: &mut Player, action: &mut Action, mv: MoveRef) {
    action.start(mv, 0.0);
    action.executed = false;
    p.state = PState::Acting;
    p.charge = 0;
    p.sprinting = false;
    p.sprint_latched = false;
    p.buffer.clear();
}

/// Exécute la frame courante d'une action : suivi de cible et root motion.
pub fn run_frame(
    body: &mut Body,
    action: &Action,
    def: &super::data::MoveDef,
    target: Option<Vec3>,
    move_dir: Option<Vec3>,
) {
    if action.tick < def.track_until {
        let desired = target
            .map(|tp| math::yaw_of(tp - body.pos))
            .or(move_dir.map(math::yaw_of));
        if let Some(y) = desired {
            body.yaw = math::turn_towards(body.yaw, y, def.track_rate.to_radians() * DT);
        }
    }
    for m in &def.motion {
        if action.tick >= m.start && action.tick < m.end {
            body.pos += math::forward(body.yaw) * motion_speed(m, action.target_dist) * DT;
        }
    }
}

/// Vitesse d'un segment de root motion (vers l'avant), selon la distance à la cible figée.
pub fn motion_speed(m: &super::data::Motion, target_dist: f32) -> f32 {
    if m.to_target {
        let span = (m.end - m.start).max(1) as f32 * DT;
        ((target_dist - m.stop_dist).max(0.0) / span).min(m.speed)
    } else {
        m.speed
    }
}

fn locomotion(
    p: &mut Player,
    body: &mut Body,
    stick_len: f32,
    move_dir: Option<Vec3>,
    locked_pos: Option<Vec3>,
    t: &Tuning,
) {
    let pd = &t.player;
    let guarding = p.state == PState::Guard;
    let can_sprint = !guarding && move_dir.is_some() && stick_len > 0.5 && p.stamina > 0.0;
    p.sprint_latched &= can_sprint;
    p.sprinting = can_sprint && (p.dodge_held >= pd.sprint_hold || p.sprint_latched);
    let speed = if guarding {
        pd.guard.walk_speed
    } else if p.sprinting {
        pd.sprint_speed
    } else {
        pd.run_speed
    } * stick_len;
    if p.sprinting {
        p.spend_stamina(pd.sprint_stamina * DT, t);
    }
    let target_vel = move_dir.unwrap_or(Vec3::ZERO) * speed;
    let dv = target_vel - p.vel;
    let max_dv = pd.accel * DT;
    p.vel += if dv.length() > max_dv { dv.normalize() * max_dv } else { dv };
    body.pos += p.vel * DT;

    let rate = pd.turn_rate.to_radians() * DT;
    if let (Some(tp), false) = (locked_pos, p.sprinting) {
        body.yaw = math::turn_towards(body.yaw, math::yaw_of(tp - body.pos), rate);
    } else if let Some(d) = move_dir {
        body.yaw = math::turn_towards(body.yaw, math::yaw_of(d), rate);
    }
}

/// Régénérations et minuteries de fin de tick.
pub fn player_end_tick(tuning: Res<Tuning>, mut q: Query<(&mut Player, &Action)>) {
    let pd = &tuning.player;
    for (mut p, action) in &mut q {
        if !matches!(p.state, PState::Free | PState::Guard) {
            p.vel = Vec3::ZERO;
        }
        if p.stamina_delay > 0 {
            p.stamina_delay -= 1;
        } else if !p.sprinting && p.state != PState::Dead {
            let attacking = matches!(action.mv, Some(MoveRef::Weapon(..)));
            if !attacking {
                let rate = if p.state == PState::Guard {
                    pd.stamina_regen_guarding
                } else {
                    pd.stamina_regen
                };
                p.stamina = (p.stamina + rate * DT).min(pd.max_stamina);
            }
        }
        if p.regain_timer > 0 {
            p.regain_timer -= 1;
            if p.regain_timer == 0 {
                p.regain = 0.0;
            }
        }
        let max = p.special_max(&tuning);
        p.special = p.special.clamp(0.0, max);
    }
}
