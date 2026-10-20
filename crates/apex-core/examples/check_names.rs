//! Vérifie l'alignement des noms (géométrie publique) sur une vraie session exportée,
//! en le comparant au relevé exact quand il existe.
//! `cargo run -p apex-core --example check_names -- export.apex.json.gz`
use apex_core::model::{LapKind, LapMeta, LapTrace, SessionMeta};
use apex_core::tracks::{aligned_corner_names, builtin_corner_names};

fn main() {
    let path = std::env::args().nth(1).expect("chemin de l'export");
    let doc: serde_json::Value = serde_json::from_reader(flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())).unwrap();
    let mut meta: SessionMeta = serde_json::from_value(doc["session"].clone()).unwrap();
    if let Ok(sim) = std::env::var("APEX_SIM") {
        meta.sim = apex_core::model::SimId::parse(&sim).unwrap();
    }
    let exact = builtin_corner_names(&meta);
    for l in doc["laps"].as_array().unwrap() {
        let m: LapMeta = serde_json::from_value(l.clone()).unwrap();
        if m.kind != LapKind::Flying || m.lap_time_s.is_none() {
            continue;
        }
        let t: LapTrace = serde_json::from_value(l["trace"].clone()).unwrap();
        let al = aligned_corner_names(&meta, &t);
        let mut ok = 0;
        let mut bad = vec![];
        for (d, n) in &al {
            match exact.iter().filter(|(e, _)| (e - d).abs() <= 60.0).min_by(|a, b| (a.0 - d).abs().total_cmp(&(b.0 - d).abs())) {
                Some((_, en)) if en == n => ok += 1,
                Some((_, en)) => bad.push(format!("{d:.0} m : {n} (relevé : {en})")),
                None => bad.push(format!("{d:.0} m : {n} (pas dans le relevé)")),
            }
        }
        println!("tour {:>2} : {} noms alignés, {ok} identiques au relevé, {} différents {:?}", m.number, al.len(), bad.len(), bad);
    }
}
