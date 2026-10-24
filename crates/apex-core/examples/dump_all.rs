//! Données réelles de toutes les pages pour l'aperçu d'interface : `dump_all <apex.db> > out.json`.
//! Les jetons et clés des réglages sont masqués.
use apex_core::service::AppService;
use apex_core::storage::SessionFilter;
use serde_json::json;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let svc = AppService::open(std::path::Path::new(&a[1])).unwrap();
    let sessions = svc.list_sessions(&SessionFilter::default()).unwrap();
    let ext = svc.list_sessions(&SessionFilter { external: Some(true), ..Default::default() }).unwrap();
    let mut details = serde_json::Map::new();
    for s in sessions.iter().chain(ext.iter()) {
        if let Ok(d) = svc.session_detail(s.id) {
            details.insert(s.id.to_string(), serde_json::to_value(d).unwrap());
        }
    }
    let first = sessions.iter().find(|s| s.clean_lap_count >= 3).unwrap_or(&sessions[0]);
    let d = svc.session_detail(first.id).unwrap();
    let r = d.analysis_reference_lap_id.unwrap();
    let c = d.laps.iter().rev().find(|l| l.clean && l.id != r).or(d.laps.iter().rev().find(|l| l.meta.lap_time_s.is_some() && l.id != r)).unwrap().id;
    let cmp = svc.compare(r, c, 1.0).unwrap();
    let overlay = svc.track_overlay(first.id, Some(r)).ok();
    let circuits = svc.circuits().unwrap();
    let mut hist = serde_json::Map::new();
    for ci in &circuits {
        let h = svc.lap_history(ci.sim, &ci.track, ci.track_config.as_deref(), &ci.car, ci.setup).unwrap();
        hist.insert(format!("{}|{}|{:?}", ci.track, ci.car, ci.setup), serde_json::to_value(h).unwrap());
    }
    let mut st = serde_json::to_value(svc.settings()).unwrap();
    for k in ["garage61_token", "azure_tts_key"] {
        if st.get(k).is_some_and(|v| !v.is_null()) {
            st[k] = json!("••••");
        }
    }
    let voices: Vec<_> = svc.tts_voices().into_iter().map(|(id, label, installed)| json!({"id": id, "label": label, "installed": installed})).collect();
    let out = json!({
        "sessions": sessions, "external": ext, "details": details, "compare": cmp, "overlay": overlay,
        "circuits": circuits, "history": hist, "dashboard": svc.dashboard().ok(), "settings": st,
        "plan": svc.training_plan(first.id).ok(), "voices": voices, "connectors": svc.connectors_info(),
        "autolog": svc.iracing_autolog().ok(),
    });
    println!("{}", serde_json::to_string(&out).unwrap());
}
