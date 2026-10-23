//! Coach de course sur de vrais .ibt : `cargo run -p apex-core --example race_report -- a.ibt b.ibt`
use apex_core::service::AppService;
use apex_core::storage::InsertOutcome;

fn main() {
    let svc = AppService::in_memory().unwrap();
    for path in std::env::args().skip(1) {
        let s = apex_core::ingest::ibt::import_ibt_file(std::path::Path::new(&path)).unwrap();
        let InsertOutcome::Inserted(id) = svc.insert_imported(&s).unwrap() else { continue };
        let d = svc.session_detail(id).unwrap();
        let Some(r) = d.race else { println!("{} : pas une course", s.meta.track); continue };
        if let Ok(dir) = std::env::var("APEX_JSON") {
            std::fs::write(format!("{dir}/race_{id}.json"), serde_json::to_string(&serde_json::json!({ "race": &r, "laps": &d.laps })).unwrap()).unwrap();
        }
        println!("\n== {} — {}", s.meta.track, r.headline);
        println!("positions {:?}", r.positions);
        println!("rythme {:?}", r.pace);
        println!("iRating {:?} / plateau {:?} (rang {:?}) ; qualif {:?} pole {:?} ; meilleur tour course = qualif P{:?}", r.my_irating, r.field_irating, r.irating_rank, r.my_qualify_s, r.pole_s, r.race_lap_as_qualify);
        for i in &r.incidents {
            println!("  T{:<2} {:>5.0} m {:<18} {:<28} perte {:?}", i.lap, i.dist_m, i.corner.clone().unwrap_or_default(), i.label, i.lap_loss_s.map(|x| (x * 100.0).round() / 100.0));
        }
        for i in &r.insights {
            println!("  [{:?}] {}\n      {}", i.kind, i.title, i.detail);
        }
    }
}
