//! Rapport texte sur la session de démo : `cargo run -p apex-core --example demo_report`
//! Avec un fichier réel : `cargo run -p apex-core --example demo_report -- chemin\vers\fichier.ibt`

use apex_core::analysis::session::{best_clean_lap, consistency, theoretical_best};
use apex_core::analysis::{compare_laps, LapInput};
use apex_core::ingest::ibt::import_ibt_file;
use apex_core::mock::demo_session;

fn main() {
    let s = match std::env::args().nth(1) {
        Some(p) => import_ibt_file(p.as_ref()).expect("import .ibt"),
        None => demo_session(1, 1_760_000_000),
    };
    println!("{} — {} — {} ({:?})\n", s.meta.sim.display_name(), s.meta.track, s.meta.car, s.meta.kind);
    for l in &s.laps {
        println!(
            "  tour {:>2}  {:<10} {:>9}  {:?}{}",
            l.meta.number,
            l.meta.kind.as_str(),
            l.meta.lap_time_s.map_or("—".into(), |t| format!("{:.3} s", t)),
            l.meta.validity,
            if l.meta.flags.is_disturbed() { "  ⚠ perturbé" } else { "" }
        );
    }
    let metas: Vec<_> = s.laps.iter().map(|l| l.meta.clone()).collect();
    let Some(bi) = best_clean_lap(&metas) else {
        println!("\nAucun tour propre : pas de référence possible.");
        return;
    };
    let length = s.meta.track_length_m.unwrap_or_else(|| s.laps[bi].trace.max_dist());
    let pairs: Vec<_> = s.laps.iter().map(|l| (&l.meta, &l.trace)).collect();
    if let Some((t, n)) = theoretical_best(&pairs, length, 1.0) {
        println!("\nMeilleur tour : {:.3} s — théorique ({n} segments) : {t:.3} s", metas[bi].lap_time_s.unwrap());
    }
    if let Some(c) = consistency(&metas) {
        println!("Régularité : σ = {:.3} s sur {} tours propres ({:.2} %)", c.stddev_s, c.clean_laps, c.cv_percent);
    }
    for (i, l) in s.laps.iter().enumerate() {
        if i == bi || !l.meta.is_clean() {
            continue;
        }
        let rf = &s.laps[bi];
        let c = compare_laps(
            &LapInput { lap_id: bi as i64, session: &s.meta, lap: &rf.meta, trace: &rf.trace },
            &LapInput { lap_id: i as i64, session: &s.meta, lap: &l.meta, trace: &l.trace },
            1.0,
        )
        .expect("comparaison");
        println!("\n=== Tour {} vs meilleur tour {} : {:+.3} s ===", l.meta.number, rf.meta.number, c.delta_s.last().unwrap());
        for w in &c.warnings {
            println!("  ⚠ {w}");
        }
        for ins in &c.insights {
            println!("  [{:?}] {} (gain {:.2}–{:.2} s)", ins.priority, ins.title, ins.est_gain_s.unwrap()[0], ins.est_gain_s.unwrap()[1]);
            println!("    {}", ins.explanation);
            println!("    → {}", ins.recommendation);
        }
        if c.insights.is_empty() {
            println!("  Aucun écart significatif détecté.");
        }
    }
}
