//! Génère des réponses IPC réelles (session démo) en JSON, pour tester l'UI dans
//! un navigateur sans Tauri : `cargo run -p apex-core --example dump_fixtures -- <dossier>`

use apex_core::service::AppService;
use apex_core::storage::SessionFilter;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "fixtures".into()));
    std::fs::create_dir_all(&out).unwrap();
    let svc = AppService::in_memory().unwrap();
    svc.load_demo().unwrap();
    svc.load_demo().unwrap();
    let sessions = svc.list_sessions(&SessionFilter::default()).unwrap();
    let sid = sessions[0].id;
    let d = svc.session_detail(sid).unwrap();
    let best = d.best_lap_id.unwrap();
    let other = d.laps.iter().rfind(|l| l.clean && l.id != best).unwrap().id;
    let w = |name: &str, v: serde_json::Value| std::fs::write(out.join(format!("{name}.json")), serde_json::to_string(&v).unwrap()).unwrap();
    w("list_sessions", serde_json::to_value(&sessions).unwrap());
    w("get_session", serde_json::to_value(&d).unwrap());
    w("compare_laps", serde_json::to_value(svc.compare(best, other, 1.0).unwrap()).unwrap());
    // Objectif fixé depuis le premier conseil, pour montrer la carte « Objectif en cours ».
    if let Some(g) = svc.compare(best, other, 1.0).unwrap().insights.iter().find_map(|i| i.goal.clone()) {
        svc.create_goal(sid, &g).unwrap();
    }
    w("get_dashboard", serde_json::to_value(svc.dashboard().unwrap()).unwrap());
    w("list_connectors", serde_json::to_value(svc.connectors_info()).unwrap());
    w("get_settings", serde_json::to_value(svc.settings()).unwrap());
    println!("fixtures écrites dans {}", out.display());
}
