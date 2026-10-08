//! Exporte les timings des actions (tirés des RON) en JSON pour les scripts Blender,
//! afin que les animations soient calées sur les frame data, ainsi que l'arène et le niveau
//! (le décor est construit à partir des mêmes données que les collisions).
//!
//! `cargo run --bin export_timings > tools/blender/timings.json`

use serde_json::{Map, Value, json};
use giants_flame::sim::data::{MoveDef, Tuning};

fn mv(d: &MoveDef) -> Value {
    json!({
        "total": d.total,
        "hits": d.hits.iter().map(|h| [h.start, h.end]).collect::<Vec<_>>(),
        "motion": d.motion.iter().map(|m| json!({"start": m.start, "end": m.end, "speed": m.speed})).collect::<Vec<_>>(),
        "iframes": d.iframes,
        "counter": d.counter,
    })
}

fn main() {
    let t = Tuning::builtin();
    let mut player = Map::new();
    let p = &t.player;
    for d in [&p.dodge, &p.backstep, &p.guard_hit, &p.perfect_guard, &p.guard_break, &p.hit_light, &p.hit_heavy, &p.switch, &p.death, &p.heal] {
        player.insert(d.anim.clone(), mv(d));
    }
    for w in &t.weapons {
        let mut all: Vec<&MoveDef> = w.light.iter().collect();
        all.extend([&w.heavy, &w.heavy_charged, &w.special, &w.fatal]);
        all.extend(w.special_counter.iter());
        for d in all {
            player.insert(d.anim.clone(), mv(d));
        }
        player.insert(w.charge_anim.clone(), json!({"total": w.charge_ticks, "hits": [], "motion": []}));
    }
    let mut boss = Map::new();
    let b = &t.boss;
    for d in [&b.groggy, &b.fatal_received, &b.roar, &b.death] {
        boss.insert(d.anim.clone(), mv(d));
    }
    for a in &b.attacks {
        boss.insert(a.mv.anim.clone(), mv(&a.mv));
    }
    // Ennemis : par modèle, le premier type qui l'utilise donne les timings des animations
    // (les autres s'y recalent à l'exécution).
    let mut enemies = Map::new();
    for k in &t.enemies {
        let Value::Object(m) = enemies.entry(k.model.clone()).or_insert_with(|| json!({})) else { continue };
        for d in [&k.alert, &k.hit, &k.death].into_iter().chain(k.attacks.iter().map(|a| &a.mv)) {
            m.entry(d.anim.clone()).or_insert_with(|| mv(d));
        }
    }
    let out = json!({
        "player": player,
        "boss": boss,
        "enemies": enemies,
        "switch_at": p.switch_at,
        "heal_at": p.heal_at,
        "arena": serde_json::to_value(&t.arena).unwrap(),
        "level": serde_json::to_value(&t.level).unwrap(),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
