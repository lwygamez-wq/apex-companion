//! Rejoue l'analyse sur un export de session (.apex.json.gz) avec le code actuel.
//! `cargo run -p apex-core --example analyze_export -- export.apex.json.gz`

use apex_core::analysis::session::best_clean_lap;
use apex_core::analysis::{compare::compare_laps_named, LapInput};
use apex_core::model::{LapMeta, LapTrace, SessionMeta};
use apex_core::tracks::builtin_corner_names;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("chemin de l'export");
    let gz = flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap());
    let doc: serde_json::Value = serde_json::from_reader(gz).unwrap();
    let meta: SessionMeta = serde_json::from_value(doc["session"].clone()).unwrap();
    let laps: Vec<(i64, LapMeta, LapTrace)> = doc["laps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["id"].as_i64().unwrap(), serde_json::from_value(l.clone()).unwrap(), serde_json::from_value(l["trace"].clone()).unwrap()))
        .collect();
    let names = builtin_corner_names(&meta);
    let metas: Vec<LapMeta> = laps.iter().map(|l| l.1.clone()).collect();
    let bi = best_clean_lap(&metas).expect("aucun tour propre");
    let (rid, rm, rt) = &laps[bi];
    println!("{} / {} — référence : tour {} ({:.3} s), {} noms de virages intégrés\n", meta.track, meta.car, rm.number, rm.lap_time_s.unwrap(), names.len());
    let mut by_title: BTreeMap<String, usize> = BTreeMap::new();
    let mut printed = 0;
    for (id, m, t) in &laps {
        if id == rid || m.lap_time_s.is_none() {
            continue;
        }
        let c = compare_laps_named(
            &LapInput { lap_id: *rid, session: &meta, lap: rm, trace: rt },
            &LapInput { lap_id: *id, session: &meta, lap: m, trace: t },
            1.0,
            &names,
        )
        .unwrap();
        // APEX_JSON=fichier.json : écrit la comparaison du premier tour (pour l'aperçu de l'interface).
        if let (0, Ok(out)) = (printed, std::env::var("APEX_JSON")) {
            std::fs::write(out, serde_json::to_string(&c).unwrap()).unwrap();
        }
        if printed == 0 {
            for k in &c.corners {
                println!("  {:<16} apex {:>5.0} m  vmin {:>4.0} km/h  sortie {:>4.0} km/h  freinage {}", k.corner.name, k.corner.apex_m, k.reference.min_speed_kmh, k.reference.exit_speed_kmh, k.reference.brake_on_m.map_or("aucun (levé)".into(), |b| format!("{b:.0} m")));
            }
            println!();
        }
        {
            let offs: Vec<String> = c.corners.iter().map(|k| k.apex_offset_m.map_or("—".into(), |v| format!("{v:+.1}"))).collect();
            println!("  tour {:>2} : trajectoire à l'apex (m, + = intérieur) {}", m.number, offs.join("  "));
        }
        for i in &c.insights {
            *by_title.entry(i.title.clone()).or_default() += 1;
            if printed < 4 && m.number == 12 {
                println!("Tour {} — [{:?}/{:?}] {}\n  {}\n  → {}\n", m.number, i.priority, i.confidence, i.title, i.explanation, i.recommendation);
            }
        }
        printed += 1;
    }
    let mut v: Vec<_> = by_title.into_iter().collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    println!("Conseils les plus fréquents sur {printed} tours comparés :");
    for (t, n) in v.iter().take(10) {
        println!("  {n:>2} × {t}");
    }
}
