//! Programme d'entraînement sur une copie d'une vraie base : `cargo run -p apex-core --example plan_check -- copie/apex.db`
use apex_core::service::AppService;
use apex_core::storage::SessionFilter;

fn main() {
    let path = std::env::args().nth(1).expect("copie de apex.db");
    let svc = AppService::open(std::path::Path::new(&path)).unwrap();
    let mut seen = std::collections::HashSet::new();
    for s in svc.list_sessions(&SessionFilter::default()).unwrap() {
        if !seen.insert((s.track.clone(), s.car.clone())) {
            continue;
        }
        let p = svc.new_training_plan(s.id).unwrap();
        println!("\n== {} / {}", p.track, p.car);
        for o in &p.objectives {
            println!("  • {}\n      {}", o.objective.title, o.objective.why);
        }
        println!("  radio : {}", p.spoken);
    }
}
