//! Stratégie carburant sur une copie d'une vraie base, pour chaque circuit.
//! `cargo run -p apex-core --example strategy_check -- copie/apex.db`
use apex_core::analysis::strategy::RaceLength;
use apex_core::service::AppService;

fn main() {
    let path = std::env::args().nth(1).expect("chemin de la copie de apex.db");
    let svc = AppService::open(std::path::Path::new(&path)).unwrap();
    for c in svc.circuits().unwrap() {
        for len in [None, Some(RaceLength::Minutes(60.0))] {
            match svc.race_strategy(c.sim, &c.track, c.track_config.as_deref(), &c.car, c.setup, len, 1.0, 0.5) {
                Ok(v) => println!(
                    "{} / {} / {:?} [{:?}] : {:.2} L/t (max {:.2}), {:.1}s, réservoir {:?}, {} tours lus\n   → {}",
                    c.track, c.car, c.setup, v.default_length, v.fuel_per_lap_l, v.fuel_per_lap_max_l, v.lap_time_s, v.tank_l, v.laps_used, v.strategy.summary
                ),
                Err(e) => println!("{} / {} : {e}", c.track, c.car),
            }
        }
    }
}
