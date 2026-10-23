//! Contrôle fixed / open sur une copie d'une vraie base : migration, références par type de série.
//! `cargo run -p apex-core --example setup_check -- copie/apex.db [ref_fixed.csv]`
use apex_core::model::SetupType;
use apex_core::service::AppService;
use apex_core::storage::SessionFilter;

fn main() {
    let path = std::env::args().nth(1).expect("chemin de la copie de apex.db");
    let svc = AppService::open(std::path::Path::new(&path)).unwrap();
    let ext = svc.list_sessions(&SessionFilter { external: Some(true), ..Default::default() }).unwrap();
    for e in &ext {
        println!("réf externe {} {} {} {:?} {:?}", e.id, e.external_driver.as_deref().unwrap_or(""), e.track, e.best_lap_s, e.setup);
    }
    let own = svc.list_sessions(&SessionFilter::default()).unwrap();
    let mut by: std::collections::BTreeMap<String, usize> = Default::default();
    for s in &own {
        *by.entry(format!("{} / {} / {:?}", s.track, s.car, s.setup)).or_default() += 1;
    }
    for (k, n) in by {
        println!("{n:3} sessions  {k}");
    }
    let Some(csv) = std::env::args().nth(2) else { return };
    let miami: Vec<_> = own.iter().filter(|s| s.track.contains("Miami")).collect();
    let last = miami[0];
    // Simulation : le pilote précise Nathan = open, et ses sessions Miami sont des sessions fixed.
    for e in ext.iter().filter(|e| e.track.contains("Miami")) {
        let st = if e.external_driver.as_deref() == Some("Nathan Kindt") { SetupType::Open } else { SetupType::Fixed };
        svc.set_session_setup(e.id, Some(st)).unwrap();
    }
    let before = svc.session_detail(last.id).unwrap().reference_label;
    println!("avant (session {} sans type) : {:?}", last.id, before);
    let anthony = ext.iter().find(|e| e.external_driver.as_deref() == Some("Anthony Bate")).map(|e| svc.session_detail(e.id).unwrap().laps[0].id);
    let anthony = anthony.unwrap_or_else(|| {
        let p = std::path::Path::new(&csv);
        let fname = p.file_name().unwrap().to_string_lossy().into_owned();
        let name = apex_core::ingest::garage61::parse_file_name(&fname).unwrap();
        svc.import_garage61_text(name, &std::fs::read_to_string(p).unwrap(), fname, Some(SetupType::Fixed)).unwrap().lap_id
    });
    for s in miami.iter().take(3) {
        svc.set_session_setup(s.id, Some(SetupType::Fixed)).unwrap();
        let d = svc.session_detail(s.id).unwrap();
        println!("session fixed {} : {:?}", s.id, d.reference_label);
        let Some(mine) = d.laps.iter().find(|l| l.clean && l.meta.lap_time_s.is_some()).or(d.laps.iter().find(|l| l.meta.lap_time_s.is_some())) else { continue };
        let c = svc.compare(d.analysis_reference_lap_id.unwrap(), mine.id, 1.0).unwrap();
        println!("  tour {} vs réf : alertes {:?}", mine.meta.number, c.warnings.first());
        let top = c.corners.iter().take(3).map(|k| format!("{} {:+.2}", k.corner.name, k.time_delta_s)).collect::<Vec<_>>();
        println!("  virages : {top:?}");
    }
    // Freinages : ton meilleur tour Miami contre chaque référence (open puis fixed).
    let best = miami.iter().filter(|s| s.best_lap_s.is_some()).min_by(|a, b| a.best_lap_s.unwrap().total_cmp(&b.best_lap_s.unwrap())).unwrap();
    let d = svc.session_detail(best.id).unwrap();
    let mine = d.best_lap_id.unwrap();
    println!("ton meilleur tour Miami : session {} {:?}", best.id, best.best_lap_s);
    let nathan = ext.iter().find(|e| e.external_driver.as_deref() == Some("Nathan Kindt")).map(|e| svc.session_detail(e.id).unwrap().laps[0].id);
    for (label, rid) in [("open (Nathan)", nathan), ("fixed (Anthony)", Some(anthony))] {
        let Some(rid) = rid else { continue };
        let c = svc.compare(rid, mine, 1.0).unwrap();
        println!("réf {label}:");
        for k in c.corners.iter().filter(|k| k.reference.brake_peak > 0.5) {
            println!(
                "  {:<14} freinage réf {:>6} / toi {:>6}   v.mini réf {:>5.0} / toi {:>5.0} km/h   {:+.2} s",
                k.corner.name,
                k.reference.brake_on_m.map_or("-".into(), |b| format!("{b:.0}")),
                k.compared.brake_on_m.map_or("-".into(), |b| format!("{b:.0}")),
                k.reference.min_speed_kmh,
                k.compared.min_speed_kmh,
                k.time_delta_s
            );
        }
    }
    let meta = svc.session_detail(miami[0].id).unwrap().meta;
    println!("coach direct (fixed) : {:?}", svc.live_reference(&meta).unwrap().map(|r| r.label));
    let mut open = meta.clone();
    open.setup = Some(SetupType::Open);
    println!("coach direct (open)  : {:?}", svc.live_reference(&open).unwrap().map(|r| r.label));
    for c in svc.circuits().unwrap().iter().filter(|c| c.track.contains("Miami")) {
        println!("historique Miami {:?} : {} sessions, record {:?}, réf ext {:?}", c.setup, c.sessions, c.best_clean_s, c.external_reference);
    }
}
