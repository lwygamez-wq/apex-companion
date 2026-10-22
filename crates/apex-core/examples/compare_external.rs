//! Compare les tours d'une session exportée à un tour de référence Garage 61 (CSV).
//! `cargo run -p apex-core --example compare_external -- session.apex.json.gz "Garage 61 - ... .csv"`
use apex_core::model::{ImportedLap, ImportedSession, LapMeta, LapTrace, SessionMeta};
use apex_core::service::AppService;
use apex_core::storage::InsertOutcome;

fn main() {
    let mut a = std::env::args().skip(1);
    let (export, csv) = (a.next().expect("export"), a.next().expect("csv"));
    let doc: serde_json::Value = serde_json::from_reader(flate2::read::GzDecoder::new(std::fs::File::open(export).unwrap())).unwrap();
    let meta: SessionMeta = serde_json::from_value(doc["session"].clone()).unwrap();
    let laps: Vec<ImportedLap> = doc["laps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| ImportedLap { meta: serde_json::from_value::<LapMeta>(l.clone()).unwrap(), trace: serde_json::from_value::<LapTrace>(l["trace"].clone()).unwrap() })
        .collect();
    let svc = AppService::in_memory().unwrap();
    let InsertOutcome::Inserted(sid) = svc.insert_imported(&ImportedSession { meta, laps, sample_rate_hz: Some(60.0) }).unwrap() else { panic!() };
    let r = svc.import_garage61(std::path::Path::new(&csv)).unwrap();
    println!("{:?}\n", r);
    let d = svc.session_detail(sid).unwrap();
    println!("référence d'analyse : {:?} — {:?}\n", d.analysis_reference_lap_id, d.reference_label);
    for l in d.laps.iter().filter(|l| l.meta.kind == apex_core::model::LapKind::Flying && l.meta.lap_time_s.is_some()) {
        let c = svc.compare(r.lap_id, l.id, 1.0).unwrap();
        println!("Tour {} ({:.3} s) : {:+.3} s", l.meta.number, l.meta.lap_time_s.unwrap(), c.delta_s.last().unwrap());
        if l.meta.number == 3 {
            if let Ok(out) = std::env::var("APEX_JSON") {
                std::fs::write(out, serde_json::to_string(&c).unwrap()).unwrap();
            }
            for w in &c.warnings {
                println!("  ⚠ {w}");
            }
            for k in &c.corners {
                println!("  {:<14} {:+.2} s  frein {:>5} / {:>5}  vmin {:>3.0} / {:>3.0}  traj {:?}", k.corner.name, k.time_delta_s,
                    k.reference.brake_on_m.map_or("—".into(), |b| format!("{b:.0}")), k.compared.brake_on_m.map_or("—".into(), |b| format!("{b:.0}")),
                    k.reference.min_speed_kmh, k.compared.min_speed_kmh, k.apex_offset_m.map(|v| (v * 10.0).round() / 10.0));
            }
            for i in c.insights.iter().take(4) {
                println!("\n  [{:?}/{:?}] {}\n  {}\n  → {}", i.priority, i.confidence, i.title, i.explanation, i.recommendation);
            }
            println!();
        }
    }
    println!("mobile : {:?}", svc.list_sessions(&Default::default()).unwrap().len());
}
