//! Relevé de piste sur de vrais .ibt : `overlay_check <sortie.json> <fichiers.ibt…>`.
//! Importe les fichiers, calcule vibreurs / lignes / bords, et écrit les données de l'aperçu.
use apex_core::service::AppService;
use apex_core::storage::SessionFilter;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let svc = AppService::in_memory().unwrap();
    let paths: Vec<std::path::PathBuf> = a[2..].iter().map(Into::into).collect();
    for r in svc.import_paths(&paths) {
        println!("{} → {:?} {:?}", r.path, r.outcome, r.error);
    }
    let sessions = svc.list_sessions(&SessionFilter::default()).unwrap();
    let s = sessions.iter().max_by_key(|s| s.clean_lap_count).unwrap();
    let d = svc.session_detail(s.id).unwrap();
    println!("session {} {} {} propres, fiche {:?}", s.id, s.track, s.clean_lap_count, d.profile);
    let clean: Vec<_> = d.laps.iter().filter(|l| l.clean).collect();
    let mut by: Vec<_> = clean.clone();
    by.sort_by(|x, y| x.meta.lap_time_s.unwrap().total_cmp(&y.meta.lap_time_s.unwrap()));
    let (r, c) = (by[0].id, by[by.len() / 2].id);
    let cmp = svc.compare(r, c, 1.0).unwrap();
    let t0 = std::time::Instant::now();
    let ov = svc.track_overlay(s.id, Some(r)).unwrap();
    println!(
        "relevé en {:?} : {} tours, {} sessions, vibreurs {}, lignes {}, hors piste {}, bords {:?}",
        t0.elapsed(),
        ov.laps,
        ov.sessions,
        ov.kerbs.len(),
        ov.lines.len(),
        ov.off.len(),
        ov.edges.as_ref().map(|e| (e.left.len(), e.source.clone()))
    );
    println!("banque : {:?} (sites, configurations {:?})", d.bank, apex_core::trackdb::bank_size());
    if let (Some(_b), Some(p)) = (&d.bank, &d.profile) {
        let _ = p;
        let t = svc.lap_trace(r).unwrap();
        let len = d.meta.track_length_m.unwrap();
        let rs = apex_core::analysis::resample::resample(&t, len, 1.0).unwrap();
        let xy: Vec<[f32; 2]> = rs.x.clone().unwrap().into_iter().zip(rs.y.clone().unwrap()).map(|(a, b)| [a, b]).collect();
        let full = apex_core::trackdb::bank_match((p.track.lat.unwrap(), p.track.lon.unwrap()), p.track.origin.unwrap(), &xy).unwrap();
        let turns = apex_core::trackdb::bank_turns_on_lap(&full, &xy, &rs.dist, p.track.turns);
        println!("virages du tracé ({}) : {:?}", turns.len(), turns.iter().map(|(d, k)| format!("V{k}@{d:.0}")).collect::<Vec<_>>());
        println!("noms appliqués : {:?}", cmp.corners.iter().map(|k| format!("{}@{:.0}", k.corner.name, k.corner.apex_m)).collect::<Vec<_>>());
    }
    let out = serde_json::json!({ "sessions": sessions, "session": d, "compare": cmp, "settings": svc.settings(), "overlay": ov });
    std::fs::write(&a[1], serde_json::to_string(&out).unwrap()).unwrap();
}
