//! Régularité par virage sur les sessions d'une copie de vraie base.
//! `cargo run -p apex-core --example regularity_check -- copie/apex.db`
use apex_core::service::AppService;
use apex_core::storage::SessionFilter;

fn main() {
    let path = std::env::args().nth(1).expect("chemin de la copie de apex.db");
    let svc = AppService::open(std::path::Path::new(&path)).unwrap();
    for s in svc.list_sessions(&SessionFilter::default()).unwrap().iter().take(8) {
        let d = svc.session_detail(s.id).unwrap();
        let Some(r) = d.regularity else { println!("{} {} : pas assez de tours", s.id, s.track); continue };
        println!("\n{} {} {:?} — {} tours — {}", s.id, s.track, s.setup, r.laps, r.headline);
        for c in r.corners.iter().take(5) {
            println!("  {:<22} gain {:.2}s (best {:.2} méd {:.2}, tour {}) frein ±{:?} → {}", c.name, c.gain_s, c.best_s, c.median_s, c.best_lap, c.brake_spread_m.map(|v| v.round()), c.advice);
        }
    }
}
