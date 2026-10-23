//! Virages détectés (apex, % du tour, sens local, vitesse mini) sur le meilleur tour de chaque circuit
//! d'une copie de base — sert à construire/contrôler les tables de noms.
//! `cargo run -p apex-core --example dump_corners -- copie/apex.db [filtre]`
use apex_core::analysis::corners::{detect_corners, CornerConfig};
use apex_core::analysis::resample::resample;
use apex_core::storage::{SessionFilter, Store};

fn main() {
    let path = std::env::args().nth(1).expect("copie de apex.db");
    let filt = std::env::args().nth(2).unwrap_or_default().to_lowercase();
    let st = Store::open(std::path::Path::new(&path)).unwrap();
    let mut seen = std::collections::HashSet::new();
    for s in st.list_sessions(&SessionFilter::default()).unwrap() {
        if !s.track.to_lowercase().contains(&filt) || !seen.insert(s.track.clone()) {
            continue;
        }
        let meta = st.session_meta(s.id).unwrap();
        let Some(best) = st
            .laps(s.id)
            .unwrap()
            .into_iter()
            .filter(|l| l.clean && l.meta.lap_time_s.is_some())
            .min_by(|a, b| {
                a.meta
                    .lap_time_s
                    .unwrap()
                    .total_cmp(&b.meta.lap_time_s.unwrap())
            })
        else {
            continue;
        };
        let t = st.trace(best.id).unwrap();
        let len = meta.track_length_m.unwrap_or_else(|| t.max_dist());
        let r = resample(&t, len, 1.0).unwrap();
        let (Some(x), Some(y)) = (r.x.as_ref(), r.y.as_ref()) else {
            continue;
        };
        println!(
            "== {} {:?} len {:.0} tour {:?}",
            s.track, meta.track_config, len, best.meta.lap_time_s
        );
        let n = x.len();
        let head = |i: usize| {
            let (a, b) = (i.saturating_sub(5), (i + 5).min(n - 1));
            (y[b] - y[a]).atan2(x[b] - x[a])
        };
        for c in detect_corners(&r, &CornerConfig::default()) {
            let at = |m: f32| ((m / r.step_m).round().max(0.0) as usize).min(n - 1);
            let mut d = head(at(c.apex_m + 25.0)) - head(at(c.apex_m - 25.0));
            d = (d + std::f32::consts::PI).rem_euclid(2.0 * std::f32::consts::PI)
                - std::f32::consts::PI;
            println!(
                "{:>3} apex {:>5.0} m  {:.3}  {}  {:>4.0}° local  vmin {:>3.0}",
                c.id,
                c.apex_m,
                c.apex_m / len,
                if d > 0.0 { "G" } else { "D" },
                d.to_degrees().abs(),
                r.speed_kmh[at(c.apex_m)]
            );
        }
        if let Ok(dir) = std::env::var("APEX_XY") {
            let csv: String = (0..n)
                .step_by(5)
                .map(|i| format!("{},{}\n", x[i], y[i]))
                .collect();
            std::fs::write(
                format!(
                    "{dir}/{}.csv",
                    s.track.split_whitespace().next().unwrap().to_lowercase()
                ),
                csv,
            )
            .unwrap();
        }
    }
}
