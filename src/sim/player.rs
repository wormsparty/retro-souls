//! Machine à états du joueur.

use bevy::prelude::*;

use super::boss::Boss;
use super::data::{BossMove, MoveRef, PlayerMove, Tuning, WeaponMove};
use super::encounter::{Dropped, Encounter, near_checkpoint, near_dropped, near_pickup};
use super::fighter::{Action, Body, Foe, Health, Hitstop};
use super::input::{InputBuffer, PlayerInputs, btn};
use super::items::{self, Inventory, Item};
use super::{DT, SimEvent, SimEvents, SimTick, math};

/// Gravité pendant une chute (m/s²).
pub const GRAVITY: f32 = 22.0;
/// Une chute dans le vide est mortelle au bout de ce temps.
pub const FALL_DEATH_TICKS: u32 = 45;
/// Un adversaire plus loin que ça (ou trop haut / trop bas) n'est pas une cible automatique.
const AUTO_TARGET_RANGE: f32 = 8.0;

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
    /// Passé par-dessus bord : plus aucun contrôle, la mort au bout de la chute.
    Falling,
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
    /// Braises (monnaie), gagnées en battant des ennemis.
    pub embers: u32,
    /// Ticks passés à l'état `Dead` (réapparition au bout de `RESPAWN_TICKS`).
    pub dead_ticks: u32,
    /// Objet en cours d'utilisation (action `Heal`, commune à tous les consommables).
    pub using: Option<Item>,
    /// Effets des consommables : régénération (mousse) et arme enflammée (résine), en ticks.
    pub regen_ticks: u32,
    pub resin_ticks: u32,
    /// Dernier checkpoint où l'on s'est reposé, checkpoints découverts (bits).
    pub checkpoint: u8,
    pub found: u32,
    /// Objets ramassés, ennemis uniques vaincus (bits, voir `Progress`).
    pub picked: u64,
    pub slain: u64,
    /// Chute en cours (le corps continue de tomber après la mort, jusqu'à la réapparition).
    pub falling: bool,
    pub fall_vy: f32,
    pub fall_ticks: u32,
    /// Hauteur du sol quitté (la caméra ne descend pas plus bas que ça).
    pub fall_from: f32,
    /// Dernière position au sol avant la chute (les braises y restent).
    pub fall_at: Vec3,
    /// Braises laissées à la dernière mort, à récupérer.
    pub dropped: Option<Dropped>,
    /// En l'air (saut) : vitesse verticale, hauteur du sol quitté, ticks écoulés.
    pub airborne: bool,
    pub air_vy: f32,
    pub air_from: f32,
    pub air_ticks: u32,
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
            using: None,
            regen_ticks: 0,
            resin_ticks: 0,
            checkpoint: 0,
            found: 1,
            picked: 0,
            slain: 0,
            falling: false,
            fall_vy: 0.0,
            fall_ticks: 0,
            fall_from: 0.0,
            fall_at: Vec3::ZERO,
            dropped: None,
            airborne: false,
            air_vy: 0.0,
            air_from: 0.0,
            air_ticks: 0,
        }
    }

    /// Multiplicateur de dégâts de l'arme (résine ardente).
    pub fn damage_mult(&self) -> f32 {
        if self.resin_ticks > 0 { items::RESIN_DAMAGE } else { 1.0 }
    }

    /// Multiplicateur des dégâts subis (talisman).
    pub fn defense_mult(&self) -> f32 {
        if self.inventory.wears(Item::IronBrooch) { items::BROOCH_DAMAGE } else { 1.0 }
    }

    /// Commence une chute (le corps est déjà au-dessus du vide ; `from` : dernière position au sol).
    pub fn start_fall(&mut self, action: &mut Action, momentum: Vec3, from: Vec3) {
        self.falling = true;
        self.airborne = false;
        self.state = PState::Falling;
        self.fall_vy = 0.0;
        self.fall_ticks = 0;
        self.fall_from = from.y;
        self.fall_at = from;
        let m = Vec3::new(momentum.x, 0.0, momentum.z);
        let len = math::flat_len(m);
        self.vel = if len > 7.0 { m / len * 7.0 } else { m };
        self.lock = None;
        self.charge = 0;
        self.sprinting = false;
        self.sprint_latched = false;
        action.stop();
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

/// Adversaires (boss et ennemis) vus par le joueur.
type Foes<'w, 's> = Query<'w, 's, (Entity, &'static Body, &'static mut Action, &'static Health, Has<Boss>), (With<Foe>, Without<Player>)>;

/// Adversaire vivant à portée (et à peu près à la même hauteur) le plus proche.
fn nearest_foe(foes: &Foes, pos: Vec3, range: f32) -> Option<Entity> {
    foes.iter()
        .filter(|(_, b, _, h, _)| !h.dead() && b.pos.distance(pos) <= range && (b.pos.y - pos.y).abs() < 4.0)
        .min_by(|a, b| a.1.pos.distance(pos).total_cmp(&b.1.pos.distance(pos)))
        .map(|(e, ..)| e)
}

#[allow(clippy::type_complexity)]
pub fn player_act(
    tuning: Res<Tuning>,
    inputs: Res<PlayerInputs>,
    tick: Res<SimTick>,
    mut encounter: ResMut<Encounter>,
    mut events: ResMut<SimEvents>,
    mut players: Query<
        (Entity, &mut Player, &mut Body, &mut Action, &mut Hitstop, &mut Health),
        Without<Foe>,
    >,
    mut foes: Foes,
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

        // Verrouillage (perdu à la mort du joueur ou de la cible, ou si elle est trop loin).
        let lockable = |e: Entity| {
            foes.get(e).is_ok_and(|(_, b, _, h, _)| !h.dead() && b.pos.distance(body.pos) <= pd.lock_range * 1.3)
        };
        if health.dead() || p.lock.is_some_and(|e| !lockable(e)) {
            p.lock = None;
        }
        if pressed & btn::LOCK != 0 && !health.dead() && !p.falling {
            p.lock = if p.lock.is_some() { None } else { nearest_foe(&foes, body.pos, pd.lock_range) };
        }

        let target = p
            .lock
            .or_else(|| nearest_foe(&foes, body.pos, AUTO_TARGET_RANGE))
            .and_then(|e| foes.get(e).ok())
            .map(|(e, b, a, _, is_boss)| TargetInfo {
                entity: e,
                pos: b.pos,
                yaw: b.yaw,
                radius: b.radius,
                groggy: is_boss && a.is(MoveRef::Boss(BossMove::Groggy)),
            });
        let locked_pos = p.lock.and(target.map(|ti| ti.pos));

        // Direction de déplacement en monde, relative à la caméra.
        let stick = inp.stick();
        let stick_len = stick.length().min(1.0);
        let cam = inp.cam_yaw_rad();
        let move_dir = (stick_len > 0.15).then(|| {
            (math::right(cam) * stick.x + math::forward(cam) * stick.y).normalize_or_zero()
        });

        // Chute : plus de contrôle ; le corps continue de tomber, même après la mort.
        if p.falling {
            p.fall_vy -= GRAVITY * DT;
            let v = p.vel + Vec3::Y * p.fall_vy;
            if body.pos.y > p.fall_from - 80.0 {
                body.pos += v * DT;
            }
            p.fall_ticks += 1;
            if p.state == PState::Falling && p.fall_ticks >= FALL_DEATH_TICKS {
                health.cur = 0.0;
                p.state = PState::Dead;
                events.push(SimEvent::PlayerDied);
            }
            continue;
        }
        // Saut : la gravité s'applique quel que soit l'état (touché, voire tué, en plein saut).
        // L'atterrissage (ou la chute dans le vide) est décidé avec les collisions.
        if p.airborne {
            p.air_vy -= GRAVITY * DT;
            body.pos.y += p.air_vy * DT;
            p.air_ticks += 1;
        }
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
                    use_item_effect(&mut p, &mut health, entity, &mut ctx);
                }
                let can_cancel = action.tick >= def.cancel_tick();
                let can_chain = action.tick >= def.chain_tick();
                let interrupted = (can_cancel
                    && try_defensive(&mut p, &mut body, &mut action, &mut ctx))
                    || (can_chain
                        && (try_item(&mut p, &mut body, &mut action, &mut ctx)
                            || try_offensive(&mut p, &mut body, &mut action, &mut foes, &mut ctx)));
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
                if p.airborne {
                    // En l'air : on ne fait que corriger un peu sa trajectoire.
                    air_control(&mut p, &mut body, stick_len, move_dir, t);
                } else if try_interact(&mut p, &body, &mut health, &mut encounter, &mut ctx) {
                    // Repos ou objet ramassé : rien d'autre ce tick.
                } else if try_jump(&mut p, &mut body, &mut ctx) {
                    air_control(&mut p, &mut body, stick_len, move_dir, t);
                } else if try_defensive(&mut p, &mut body, &mut action, &mut ctx)
                    && p.state == PState::Acting
                {
                    let def = action.def(t).expect("action");
                    run_frame(&mut body, &action, def, locked_pos, move_dir);
                } else if try_item(&mut p, &mut body, &mut action, &mut ctx)
                    || try_offensive(&mut p, &mut body, &mut action, &mut foes, &mut ctx)
                {
                    if p.state == PState::Acting {
                        let def = action.def(t).expect("action");
                        run_frame(&mut body, &action, def, locked_pos, move_dir);
                    }
                } else {
                    locomotion(&mut p, &mut body, stick_len, move_dir, locked_pos, t);
                }
            }
            PState::Dead | PState::Falling => {}
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
fn try_offensive(p: &mut Player, body: &mut Body, action: &mut Action, foes: &mut Foes, ctx: &mut Ctx) -> bool {
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
                if let Ok((_, _, mut bact, _, _)) = foes.get_mut(ti.entity) {
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
    // Tous les consommables passent par la même action (on porte l'objet à la bouche, on
    // écrase la braise…) ; l'effet s'applique à `heal_at`, perdu si on est touché avant.
    p.healed = false;
    p.using = Some(item);
    start_move(p, body, action, MoveRef::Player(PlayerMove::Heal), ctx);
    true
}

/// Effet du consommable en cours d'utilisation.
fn use_item_effect(p: &mut Player, health: &mut Health, entity: Entity, ctx: &mut Ctx) {
    let Some(item) = p.using.take() else { return };
    match item {
        Item::HealFlask => {
            health.cur = (health.cur + health.max * ctx.t.player.heal_ratio).min(health.max);
            ctx.events.push(SimEvent::Heal { entity });
            return;
        }
        Item::FadedEmber => p.embers = p.embers.saturating_add(items::FADED_EMBERS),
        Item::LivelyEmber => p.embers = p.embers.saturating_add(items::LIVELY_EMBERS),
        Item::GoldenMoss => p.regen_ticks = items::MOSS_TICKS,
        Item::EmberResin => p.resin_ticks = items::RESIN_TICKS,
        Item::FlaskShard | Item::IronBrooch | Item::CarouselFeather => {}
    }
    ctx.events.push(SimEvent::ItemUsed { entity, item });
}

/// Interagir : récupérer ses braises ou ramasser l'objet à portée, sinon se reposer au checkpoint.
/// Rien à portée : le bouton est laissé au saut.
fn try_interact(p: &mut Player, body: &Body, health: &mut Health, enc: &mut Encounter, ctx: &mut Ctx) -> bool {
    if !p.buffer.buffered(btn::INTERACT, ctx.now, ctx.t.player.input_buffer) {
        return false;
    }
    if let Some(d) = p.dropped.filter(|d| near_dropped(d, body.pos)) {
        p.buffer.consume(btn::INTERACT);
        p.dropped = None;
        p.embers = p.embers.saturating_add(d.embers);
        ctx.events.push(SimEvent::EmbersRecovered { entity: ctx.entity, pos: d.pos(), embers: d.embers });
        return true;
    }
    if let Some(i) = near_pickup(ctx.t, p.picked, body.pos) {
        p.buffer.consume(btn::INTERACT);
        p.picked |= 1u64 << i;
        for (item, n) in &ctx.t.level.pickups[i as usize].items {
            p.inventory.add(*item, *n);
        }
        ctx.events.push(SimEvent::PickedUp { entity: ctx.entity, pickup: i });
        return true;
    }
    // Pas de repos pendant un combat, ni avec des ennemis aux trousses.
    let Some(cp) = near_checkpoint(ctx.t, body.pos).filter(|_| !enc.active && !enc.hunted) else {
        return false;
    };
    p.buffer.consume(btn::INTERACT);
    if p.found & (1u32 << cp) == 0 {
        p.found |= 1u32 << cp;
        ctx.events.push(SimEvent::Kindled { checkpoint: cp });
    }
    p.checkpoint = cp;
    enc.respawn_enemies = true;
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

/// Saut (le bouton d'interaction, quand il n'y a rien à portée). Il faut de l'endurance.
fn try_jump(p: &mut Player, body: &mut Body, ctx: &mut Ctx) -> bool {
    let jd = &ctx.t.player.jump;
    if !p.buffer.buffered(btn::INTERACT, ctx.now, ctx.t.player.input_buffer) || !p.can_act() {
        return false;
    }
    p.buffer.consume(btn::INTERACT);
    p.spend_stamina(jd.stamina, ctx.t);
    p.state = PState::Free;
    p.airborne = true;
    p.air_vy = jd.speed;
    p.air_from = body.pos.y;
    p.air_ticks = 0;
    p.charge = 0;
    // On s'élance dans la direction du stick, à la vitesse acquise.
    if let Some(d) = ctx.move_dir {
        body.yaw = math::yaw_of(d);
    }
    ctx.events.push(SimEvent::Jumped { entity: ctx.entity });
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
    let mut cost = stamina_cost(mv, ctx.t);
    if matches!(mv, MoveRef::Player(PlayerMove::Dodge | PlayerMove::Backstep)) && p.inventory.wears(Item::CarouselFeather) {
        cost *= items::FEATHER_DODGE;
    }
    p.spend_stamina(cost, ctx.t);
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
    p.using = None;
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

/// Déplacement en l'air : l'élan est conservé (stick relâché compris) ; le stick ne fait que
/// l'infléchir lentement, sans le ralentir s'il pousse dans le même sens.
fn air_control(p: &mut Player, body: &mut Body, stick_len: f32, move_dir: Option<Vec3>, t: &Tuning) {
    let pd = &t.player;
    if let Some(d) = move_dir {
        let want = d * math::flat_len(p.vel).max(pd.run_speed * stick_len);
        let dv = want - p.vel;
        let max_dv = pd.jump.air_control * DT;
        p.vel += if dv.length() > max_dv { dv.normalize() * max_dv } else { dv };
        body.yaw = math::turn_towards(body.yaw, math::yaw_of(d), pd.turn_rate.to_radians() * 0.4 * DT);
    }
    p.sprinting = false;
    body.pos += p.vel * DT;
}

/// Régénérations et minuteries de fin de tick.
pub fn player_end_tick(tuning: Res<Tuning>, mut q: Query<(&mut Player, &Action, &mut Health)>) {
    let pd = &tuning.player;
    for (mut p, action, mut health) in &mut q {
        if p.resin_ticks > 0 {
            p.resin_ticks -= 1;
        }
        if p.regen_ticks > 0 {
            p.regen_ticks -= 1;
            if !health.dead() && !p.falling {
                health.cur = (health.cur + items::MOSS_HP_PER_SEC * DT).min(health.max);
            }
        }
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
