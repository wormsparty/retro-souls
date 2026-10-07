//! Exporte les timings des actions (tirés des RON) en JSON pour les scripts Blender,
//! afin que les animations soient calées sur les frame data.
//!
//! `cargo run --bin export_timings > tools/blender/timings.json`

use serde_json::{Map, Value, json};
use souls::sim::data::{MoveDef, Tuning};

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
    let out = json!({"player": player, "boss": boss, "switch_at": p.switch_at, "heal_at": p.heal_at});
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
