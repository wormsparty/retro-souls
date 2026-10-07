//! Tests d'intégration de la simulation (sans rendu).

use bevy::prelude::*;
use souls::sim::boss::Boss;
use souls::sim::data::{BossMove, MoveRef, PlayerMove, Tuning};
use souls::sim::fighter::{Action, Body, Health};
use souls::sim::input::{PlayerInput, PlayerInputs, btn};
use souls::sim::player::{PState, Player};
use souls::sim::{SimDebug, SimEvent, SimEvents, SimPlugin, SimSchedule, math, state_hash};

fn new_app() -> App {
    let mut app = App::new();
    app.add_plugins(SimPlugin);
    app.world_mut().resource_mut::<SimDebug>().boss_passive = true;
    step(&mut app, PlayerInput::default()); // spawn
    app
}

fn step(app: &mut App, input: PlayerInput) {
    app.world_mut().resource_mut::<PlayerInputs>().0[0] = input;
    app.world_mut().run_schedule(SimSchedule);
}

fn steps(app: &mut App, n: u32, input: PlayerInput) {
    for _ in 0..n {
        step(app, input);
    }
}

fn player(app: &mut App) -> Entity {
    app.world_mut().query_filtered::<Entity, With<Player>>().single(app.world()).unwrap()
}

fn boss(app: &mut App) -> Entity {
    app.world_mut().query_filtered::<Entity, With<Boss>>().single(app.world()).unwrap()
}

fn hp(app: &mut App, e: Entity) -> f32 {
    app.world().get::<Health>(e).unwrap().cur
}

fn tuning(app: &App) -> Tuning {
    app.world().resource::<Tuning>().clone()
}

/// Place le joueur face au boss à `dist` mètres et lance l'attaque `name` du boss.
/// Retourne le nombre de ticks avant le premier tick actif de l'attaque.
fn boss_attack(app: &mut App, name: &str, dist: f32) -> u32 {
    let t = tuning(app);
    let (p, b) = (player(app), boss(app));
    let idx = t.boss.attacks.iter().position(|a| a.name == name).expect(name);
    {
        let mut bb = app.world_mut().get_mut::<Body>(b).unwrap();
        bb.pos = Vec3::ZERO;
        bb.yaw = 0.0;
    }
    {
        let mut pb = app.world_mut().get_mut::<Body>(p).unwrap();
        pb.pos = Vec3::new(0.0, 0.0, dist);
        pb.yaw = std::f32::consts::PI; // face au boss
    }
    app.world_mut().get_mut::<Action>(b).unwrap().start(MoveRef::BossAttack(idx as u16), dist);
    app.world_mut().resource_mut::<SimEvents>().0.clear();
    t.boss.attacks[idx].mv.hits[0].start
}

fn events(app: &mut App) -> Vec<SimEvent> {
    std::mem::take(&mut app.world_mut().resource_mut::<SimEvents>().0)
}

const GUARD: PlayerInput = PlayerInput { buttons: btn::GUARD, move_x: 0, move_y: 0, cam_yaw: 0 };
const IDLE: PlayerInput = PlayerInput { buttons: 0, move_x: 0, move_y: 0, cam_yaw: 0 };

/// Appuie sur garde `lead` ticks avant l'impact et retourne (PV perdus, garde parfaite ?).
fn guard_with_lead(lead: u32) -> (f32, bool) {
    let mut app = new_app();
    let hit_start = boss_attack(&mut app, "ecrasement", 2.6);
    let p = player(&mut app);
    let before = hp(&mut app, p);
    // Le premier tick actif est exécuté au (hit_start + 1)-ème tick de sim.
    let press_at = hit_start + 1 - lead;
    steps(&mut app, press_at - 1, IDLE);
    steps(&mut app, lead + 20, GUARD);
    let ev = events(&mut app);
    let perfect = ev.iter().any(|e| matches!(e, SimEvent::PerfectGuard { .. }));
    (before - hp(&mut app, p), perfect)
}

#[test]
fn perfect_guard_window_is_tick_exact() {
    let t = Tuning::builtin();
    let w = t.player.guard.perfect_window;
    for lead in 1..=w + 1 {
        let (lost, perfect) = guard_with_lead(lead);
        // Appui `lead` ticks avant le tick d'impact.
        if lead <= w {
            assert!(perfect, "lead {lead}: devrait être parfaite");
            assert_eq!(lost, 0.0);
        } else {
            assert!(!perfect, "lead {lead}: ne devrait pas être parfaite");
        }
    }
    // Garde tenue longtemps : garde normale, dégâts réduits convertis en regain.
    let (lost, perfect) = guard_with_lead(40);
    let full = t.boss.attacks.iter().find(|a| a.name == "ecrasement").unwrap().mv.hits[0].damage;
    assert!(!perfect);
    assert!((lost - full * t.player.guard.damage_ratio).abs() < 1e-3, "lost {lost}");
}

#[test]
fn guard_spam_shrinks_window() {
    let mut app = new_app();
    let t = tuning(&app);
    let hit_start = boss_attack(&mut app, "ecrasement", 2.6);
    let p = player(&mut app);
    // Spam : appuis répétés jusqu'à l'impact, dernier appui `perfect_window` ticks avant.
    let last = hit_start + 1 - t.player.guard.perfect_window - 1;
    let mut tick = 0;
    while tick + 4 < last {
        step(&mut app, GUARD);
        step(&mut app, IDLE);
        tick += 2;
    }
    steps(&mut app, last - tick, IDLE);
    steps(&mut app, 30, GUARD);
    let pl = app.world().get::<Player>(p).unwrap();
    assert!(pl.guard_spam > 0);
    assert!(!events(&mut app).iter().any(|e| matches!(e, SimEvent::PerfectGuard { .. })));
}

#[test]
fn fury_breaks_normal_guard_but_not_perfect() {
    let t = Tuning::builtin();
    let fury = t.boss.attacks.iter().find(|a| a.name == "furie_estoc").unwrap();
    let dmg = fury.mv.hits[0].damage;

    // Garde tenue : dégâts complets.
    let mut app = new_app();
    let hit_start = boss_attack(&mut app, "furie_estoc", 4.0);
    let p = player(&mut app);
    steps(&mut app, hit_start + 10, GUARD);
    assert_eq!(hp(&mut app, p), t.player.max_hp - dmg);

    // Garde parfaite : aucun dégât.
    let mut app = new_app();
    let hit_start = boss_attack(&mut app, "furie_estoc", 4.0);
    let p = player(&mut app);
    steps(&mut app, hit_start - 2, IDLE);
    steps(&mut app, 20, GUARD);
    assert_eq!(hp(&mut app, p), t.player.max_hp);
}

#[test]
fn dodge_iframes_avoid_damage() {
    let mut app = new_app();
    let t = tuning(&app);
    let hit_start = boss_attack(&mut app, "ecrasement", 2.6);
    let p = player(&mut app);
    // Esquive vers l'arrière (backstep) lancée juste avant l'impact.
    steps(&mut app, hit_start - 3, IDLE);
    step(&mut app, PlayerInput { buttons: btn::DODGE, ..IDLE });
    steps(&mut app, 40, IDLE);
    assert_eq!(hp(&mut app, p), t.player.max_hp);
}

#[test]
fn regain_heals_when_hitting_back() {
    let mut app = new_app();
    let hit_start = boss_attack(&mut app, "ecrasement", 2.6);
    let p = player(&mut app);
    steps(&mut app, hit_start + 20, GUARD);
    let after_guard = hp(&mut app, p);
    let regain = app.world().get::<Player>(p).unwrap().regain;
    assert!(regain > 0.0);
    // Contre-attaque : le joueur est à portée, face au boss.
    let b = boss(&mut app);
    app.world_mut().get_mut::<Action>(b).unwrap().stop();
    for _ in 0..4 {
        step(&mut app, PlayerInput { buttons: btn::LIGHT, ..IDLE });
        steps(&mut app, 25, IDLE);
    }
    assert!(hp(&mut app, p) > after_guard, "le regain doit rendre des PV");
}

#[test]
fn stagger_leads_to_groggy_and_fatal() {
    let mut app = new_app();
    let t = tuning(&app);
    let (p, b) = (player(&mut app), boss(&mut app));
    {
        let mut pb = app.world_mut().get_mut::<Body>(p).unwrap();
        pb.pos = Vec3::new(0.0, 0.0, 6.0 - 2.0);
    }
    app.world_mut().get_mut::<Body>(b).unwrap().pos = Vec3::new(0.0, 0.0, 6.0);
    app.world_mut().get_mut::<Body>(b).unwrap().yaw = std::f32::consts::PI;
    app.world_mut().get_mut::<Body>(p).unwrap().yaw = 0.0;
    {
        let mut boss = app.world_mut().get_mut::<Boss>(b).unwrap();
        boss.stagger = t.boss.stagger_max - 1.0;
    }
    // Une attaque légère suffit à remplir la jauge.
    step(&mut app, PlayerInput { buttons: btn::LIGHT, ..IDLE });
    steps(&mut app, 30, IDLE);
    assert!(app.world().get::<Action>(b).unwrap().is(MoveRef::Boss(BossMove::Groggy)));
    let boss_hp = hp(&mut app, b);
    step(&mut app, PlayerInput { buttons: btn::LIGHT, ..IDLE });
    assert!(app.world().get::<Action>(b).unwrap().is(MoveRef::Boss(BossMove::FatalReceived)));
    steps(&mut app, 120, IDLE);
    assert!(hp(&mut app, b) < boss_hp - 300.0);
}

#[test]
fn weapon_switch_and_lock_on() {
    let mut app = new_app();
    let p = player(&mut app);
    step(&mut app, PlayerInput { buttons: btn::SWITCH, ..IDLE });
    steps(&mut app, 30, IDLE);
    assert_eq!(app.world().get::<Player>(p).unwrap().weapon, 1);
    step(&mut app, PlayerInput { buttons: btn::LOCK, ..IDLE });
    assert!(app.world().get::<Player>(p).unwrap().lock.is_some());
}

#[test]
fn hit_reaction_and_death() {
    let mut app = new_app();
    let t = tuning(&app);
    let p = player(&mut app);
    app.world_mut().get_mut::<Health>(p).unwrap().cur = 10.0;
    let hit_start = boss_attack(&mut app, "ecrasement", 2.6);
    steps(&mut app, hit_start + 30 + t.player.death.total, IDLE);
    assert_eq!(app.world().get::<Player>(p).unwrap().state, PState::Dead);
    assert!(events(&mut app).iter().any(|e| matches!(e, SimEvent::PlayerDied)) || hp(&mut app, p) == 0.0);
    let _ = PlayerMove::Death;
}

/// Séquence d'inputs pseudo-aléatoire mais reproductible.
fn scripted_input(i: u32) -> PlayerInput {
    let mut x = i.wrapping_mul(2654435761) ^ 0x5bd1e995;
    x ^= x >> 15;
    let buttons = match (i / 7) % 9 {
        0 => btn::LIGHT,
        1 => btn::HEAVY,
        2 => btn::GUARD,
        3 => btn::DODGE,
        4 => 0,
        5 => btn::LIGHT,
        6 => btn::GUARD,
        7 => if i % 300 < 7 { btn::SPECIAL } else { 0 },
        _ => if i % 500 < 7 { btn::SWITCH } else { 0 },
    };
    PlayerInput {
        buttons: if i == 5 { btn::LOCK } else { buttons },
        move_x: ((x & 0xff) as i32 - 128).clamp(-127, 127) as i8,
        move_y: (((x >> 8) & 0xff) as i32 - 128).clamp(-127, 127) as i8,
        cam_yaw: PlayerInput::quantize_yaw(i as f32 * 0.002),
    }
}

fn run_scripted(n: u32) -> (u64, f32, f32) {
    let mut app = App::new();
    app.add_plugins(SimPlugin);
    let mut hashes = 0u64;
    for i in 0..n {
        step(&mut app, scripted_input(i));
        hashes = hashes.rotate_left(5) ^ state_hash(app.world_mut());
    }
    let (p, b) = (player(&mut app), boss(&mut app));
    (hashes, hp(&mut app, p), hp(&mut app, b))
}

#[test]
fn simulation_is_deterministic() {
    let a = run_scripted(4000);
    let b = run_scripted(4000);
    assert_eq!(a.0, b.0, "la simulation doit être déterministe");
    let t = Tuning::builtin();
    // Il s'est vraiment passé quelque chose.
    assert!(a.1 < t.player.max_hp || a.2 < t.boss.max_hp, "{a:?}");
    let _ = math::wrap(0.0);
}

#[test]
fn stamina_goes_negative_and_blocks_actions() {
    let mut app = new_app();
    let t = tuning(&app);
    let p = player(&mut app);
    // Un coup avec 1,5 d'endurance : il part, et l'endurance passe en négatif.
    app.world_mut().get_mut::<Player>(p).unwrap().stamina = 1.5;
    step(&mut app, PlayerInput { buttons: btn::LIGHT, ..IDLE });
    let st = app.world().get::<Player>(p).unwrap().stamina;
    assert!(st < 0.0, "endurance {st}");
    assert!(st >= t.player.stamina_floor);
    steps(&mut app, 30, IDLE);
    // Sous 1 point : ni attaque ni esquive.
    app.world_mut().get_mut::<Player>(p).unwrap().stamina = 0.5;
    app.world_mut().get_mut::<Player>(p).unwrap().stamina_delay = 100;
    step(&mut app, PlayerInput { buttons: btn::LIGHT, ..IDLE });
    assert_ne!(app.world().get::<Player>(p).unwrap().state, PState::Acting);
    step(&mut app, PlayerInput { buttons: btn::DODGE, ..IDLE });
    assert_ne!(app.world().get::<Player>(p).unwrap().state, PState::Acting);
    assert!(events(&mut app).iter().any(|e| matches!(e, SimEvent::NoStamina { .. })));
}

#[test]
fn stamina_cost_is_proportional_to_damage_for_all_weapons() {
    use souls::sim::data::WeaponMove;
    use souls::sim::player::stamina_cost;
    let t = Tuning::builtin();
    for (w, wd) in t.weapons.iter().enumerate() {
        for (i, l) in wd.light.iter().enumerate() {
            let c = stamina_cost(MoveRef::Weapon(w as u8, WeaponMove::Light(i as u8)), &t);
            let ratio = c / l.total_damage();
            assert!((ratio - t.player.stamina_per_damage).abs() < 1e-5, "{}: {ratio}", l.anim);
        }
        assert_eq!(stamina_cost(MoveRef::Weapon(w as u8, WeaponMove::Fatal), &t), 0.0);
    }
}

#[test]
fn heal_restores_health_and_is_lost_if_interrupted() {
    let t = Tuning::builtin();
    // Soin normal.
    let mut app = new_app();
    let p = player(&mut app);
    app.world_mut().get_mut::<Health>(p).unwrap().cur = 100.0;
    step(&mut app, PlayerInput { buttons: btn::HEAL, ..IDLE });
    steps(&mut app, t.player.heal.total + 2, IDLE);
    let expected = 100.0 + t.player.max_hp * t.player.heal_ratio;
    assert!((hp(&mut app, p) - expected).abs() < 1e-3);
    assert_eq!(app.world().get::<Player>(p).unwrap().heals, t.player.heal_charges - 1);

    // Touché avant l'application : charge perdue, pas de soin.
    let mut app = new_app();
    let hit_start = boss_attack(&mut app, "ecrasement", 2.6);
    let p = player(&mut app);
    steps(&mut app, hit_start + 1 - 5, IDLE);
    step(&mut app, PlayerInput { buttons: btn::HEAL, ..IDLE });
    steps(&mut app, 60, IDLE);
    let pl = app.world().get::<Player>(p).unwrap();
    assert_eq!(pl.heals, t.player.heal_charges - 1);
    assert!(hp(&mut app, p) < t.player.max_hp);
    // Plus de charges : rien ne se passe.
    app.world_mut().get_mut::<Player>(p).unwrap().heals = 0;
    steps(&mut app, 120, IDLE);
    step(&mut app, PlayerInput { buttons: btn::HEAL, ..IDLE });
    assert_ne!(app.world().get::<Player>(p).unwrap().state, PState::Acting);
}

#[test]
fn charged_heavy_releases_automatically_at_full_charge() {
    use souls::sim::data::WeaponMove;
    let t = Tuning::builtin();
    for w in 0..t.weapons.len() as u8 {
        let mut app = new_app();
        let p = player(&mut app);
        app.world_mut().get_mut::<Player>(p).unwrap().weapon = w;
        let full = t.weapons[w as usize].charge_ticks;
        // Bouton lourd maintenu indéfiniment.
        steps(&mut app, full + 2, PlayerInput { buttons: btn::HEAVY, ..IDLE });
        let a = app.world().get::<Action>(p).unwrap();
        assert!(a.is(MoveRef::Weapon(w, WeaponMove::HeavyCharged)), "arme {w}: {:?}", a.mv);
        // Relâché tôt : lourde normale.
        let mut app = new_app();
        let p = player(&mut app);
        app.world_mut().get_mut::<Player>(p).unwrap().weapon = w;
        steps(&mut app, 5, PlayerInput { buttons: btn::HEAVY, ..IDLE });
        step(&mut app, IDLE);
        assert!(app.world().get::<Action>(p).unwrap().is(MoveRef::Weapon(w, WeaponMove::Heavy)));
    }
}
