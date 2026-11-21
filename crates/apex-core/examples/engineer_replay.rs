//! Rejoue l'ingénieur de course sur des enregistrements LMU et compare ses consignes à la course réelle.
//! `cargo run --release -p apex-core --example engineer_replay -- course.apexlmu [autres…]`
use apex_core::engineer_replay::replay;
use apex_core::race_engineer::EngineerPlan;

fn main() {
    for p in std::env::args().skip(1) {
        let r = apex_core::ingest::lmu::decode(&std::fs::read(&p).expect("lecture")).expect("décodage");
        // Temps perdu par arrêt mesuré sur cette course : tours de rentrée + sortie − 2 tours médians.
        let s = apex_core::ingest::lmu::import_recording(&r).unwrap();
        let mut normal: Vec<f64> = s.laps.iter().filter(|l| l.meta.is_clean()).filter_map(|l| l.meta.lap_time_s).collect();
        normal.sort_by(|a, b| a.total_cmp(b));
        let med = normal.get(normal.len() / 2).copied().unwrap_or(0.0);
        let own: Vec<f32> = s.laps.windows(2).filter_map(|w| match (w[0].meta.kind, w[1].meta.kind, w[0].meta.lap_time_s, w[1].meta.lap_time_s) {
            (apex_core::model::LapKind::InLap, apex_core::model::LapKind::OutLap, Some(a), Some(b)) => Some((a + b - 2.0 * med) as f32),
            _ => None,
        }).collect();
        // Le rejeu n'a pas la base de l'app : par défaut, le temps d'arrêt mesuré sur cette course tient lieu
        // d'historique du circuit (`PIT_LOSS=…` pour en imposer un, `PIT_LOSS=0` pour rejouer sans).
        let loss = match std::env::var("PIT_LOSS").ok().and_then(|v| v.parse::<f32>().ok()) {
            Some(v) if v > 0.0 => Some(v),
            Some(_) => None,
            None => own.first().copied(),
        };
        println!("temps d'arrêt mesuré sur la course : {own:?} ; rejoué avec {loss:?} (l'app en direct lit l'historique du circuit)");
        let rep = replay(&r, EngineerPlan::default(), loss);
        println!("\n=== {} · {} · {} tours · arrivée {:.1}", p.rsplit('/').next().unwrap_or(&p), r.header.car, rep.laps, rep.finish_level);
        for m in &rep.missing {
            println!("  manque : {m}");
        }
        for c in &rep.line_calls {
            println!("  T{:>2} restants annoncés {:>2} réels {:>2} ({:+}) exact est {:.2} vrai {:.2} conso {:.2} plein {:?} après arrêt {:?}", c.lap, c.laps_left, c.laps_left_true, c.laps_left as i32 - c.laps_left_true as i32, c.exact_est.unwrap_or(f32::NAN), c.exact_true, c.per_lap, c.fill.map(|f| (f * 10.0).round() / 10.0), c.laps_after_stop);
        }
        for (l, s) in &rep.said {
            println!("  [T{l}] {s}");
        }
        for (l, m) in &rep.after_last_stop {
            println!("  ÉCART après le dernier arrêt, T{l} : {m}");
        }
        for s in &rep.stops {
            println!("  ARRÊT T{} : réglé {:.1}, annoncé {:?} (entrée {}), fallait {:.1} | distance est {:?} vraie {:.2} | conso est {:?} vraie {:.2} | utilisé {:.1} | reste si annoncé {:?} tours, réel {:.2} tours | erreurs : distance {:?} conso {:?} arrondi {:?}",
                s.lap, s.set, s.announced, s.announced_at_entry, s.needed, s.laps_est, s.laps_true, s.per_lap_est, s.per_lap_true, s.used_after, s.spare_laps_if_announced, s.spare_laps_actual, s.err_distance, s.err_consumption, s.err_rounding);
        }
    }
}
