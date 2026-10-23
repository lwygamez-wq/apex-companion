//! Données réelles pour l'aperçu de l'écran d'analyse : `dump_analysis <apex.db> <session> [réf] > out.json`.
use apex_core::service::AppService;
use apex_core::storage::SessionFilter;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let svc = AppService::open(std::path::Path::new(&a[1])).unwrap();
    let sid: i64 = a[2].parse().unwrap();
    let d = svc.session_detail(sid).unwrap();
    let r = a.get(3).and_then(|x| x.parse().ok()).or(d.analysis_reference_lap_id).unwrap();
    let c = d.best_lap_id.filter(|b| *b != r).or(d.laps.iter().find(|l| l.id != r && l.meta.lap_time_s.is_some()).map(|l| l.id)).unwrap();
    let cmp = svc.compare(r, c, 1.0).unwrap();
    let out = serde_json::json!({
        "sessions": svc.list_sessions(&SessionFilter::default()).unwrap(),
        "session": d,
        "compare": cmp,
        "settings": svc.settings(),
    });
    println!("{}", serde_json::to_string(&out).unwrap());
}
