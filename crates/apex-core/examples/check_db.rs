//! Contrôle sur une copie d'une vraie base Apex : dédoublonnage, circuits, historique, carte de risque.
//! `cargo run -p apex-core --example check_db -- copie/apex.db`
use apex_core::service::AppService;
use apex_core::storage::SessionFilter;

fn main() {
    let path = std::env::args().nth(1).expect("chemin de la copie de apex.db");
    let svc = AppService::open(std::path::Path::new(&path)).unwrap();
    let sessions = svc.list_sessions(&SessionFilter::default()).unwrap();
    println!("{} sessions après dédoublonnage", sessions.len());
    // Toutes les sorties de piste, toutes sessions, par circuit (début de sortie en m).
    for s in &sessions {
        let d = svc.session_detail(s.id).unwrap();
        let starts: Vec<String> = d.off_tracks.iter().map(|z| format!("{:.0}{}", z.at_m, z.corner.as_deref().map(|c| format!("({c})")).unwrap_or_default())).collect();
        if !starts.is_empty() {
            println!("session {} {} : sorties {:?}", s.id, s.track, starts);
        }
    }
    if let Ok(dir) = std::env::var("APEX_DUMP") {
        let cs = svc.circuits().unwrap();
        std::fs::write(format!("{dir}/circuits.json"), serde_json::to_string(&cs).unwrap()).unwrap();
        let hs: Vec<_> = cs.iter().map(|c| svc.lap_history(c.sim, &c.track, c.track_config.as_deref(), &c.car).unwrap()).collect();
        std::fs::write(format!("{dir}/history.json"), serde_json::to_string(&hs).unwrap()).unwrap();
    }
    for c in svc.circuits().unwrap() {
        println!(
            "\n{} ({}) — {} : {} sessions, {} tours ({} lancés, {} propres), meilleur propre {:?}, meilleur tout {:?}, noms {} , réf. ext. {:?}",
            c.track, c.track_config.clone().unwrap_or_default(), c.car, c.sessions, c.laps, c.timed_laps, c.clean_laps, c.best_clean_s, c.best_any_s, c.corner_names, c.external_reference
        );
        let h = svc.lap_history(c.sim, &c.track, c.track_config.as_deref(), &c.car).unwrap();
        let pbs: Vec<String> = h.iter().filter(|l| l.personal_best_at_time).map(|l| format!("{:.3}", l.lap_time_s.unwrap())).collect();
        println!("  historique : {} tours ; progression du record propre : {}", h.len(), pbs.join(" → "));
        let last = sessions.iter().find(|s| s.track == c.track && s.car == c.car).unwrap();
        let d = svc.session_detail(last.id).unwrap();
        println!("  carte de risque : {:?}", d.risk_headline);
        println!("  sorties (session la plus récente) : {:?}", d.off_tracks.iter().map(|z| format!("{:.0} m {:?} {:?}", z.at_m, z.corner, z.laps)).collect::<Vec<_>>());
        println!("  fenêtres : {:?}", d.risk.iter().map(|r| format!("{} {:.0}-{:.0}", r.name, r.entry_m, r.exit_m)).collect::<Vec<_>>());
        for r in d.risk.iter().take(6) {
            println!("    {:<16} {:?} {}/{} sorties, coût {:?}, à prendre {:?}", r.name, r.verdict, r.offs, r.passes, r.off_cost_s.map(|v| (v * 100.0).round() / 100.0), r.gain_available_s.map(|v| (v * 100.0).round() / 100.0));
        }
    }
}

#[allow(dead_code)]
fn unused() {}
