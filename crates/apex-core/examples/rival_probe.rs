//! Sonde de la voiture de référence de la catégorie (enregistrements Apex 0.45 et plus) : le jeu
//! publie-t-il la position et la vitesse des autres voitures assez souvent (≥ 5 Hz) pour comparer un tour ?
//! `cargo run --release -p apex-core --example rival_probe -- course.apexlmu`
fn main() {
    for p in std::env::args().skip(1) {
        let r = apex_core::ingest::lmu::decode(&std::fs::read(&p).expect("lecture")).expect("décodage");
        let v = &r.rival;
        println!("== {} · {} relevés", p.rsplit('/').next().unwrap_or(&p), v.len());
        if v.len() < 2 {
            println!("Pas de voiture de référence enregistrée (fichier d'avant 0.45, ou seul dans la catégorie).");
            continue;
        }
        let span = v.last().unwrap().et - v[0].et;
        let tel: Vec<f64> = v.iter().filter(|x| x.has_telem).map(|x| x.telem_et).collect();
        let mut uniq = tel.clone();
        uniq.dedup();
        let mut lapd: Vec<f32> = v.iter().map(|x| x.lap_dist).collect();
        lapd.dedup();
        let moving = v.iter().filter(|x| x.has_telem && x.speed > 1.0).count();
        let pedals = v.iter().filter(|x| x.throttle > 0.0 || x.brake > 0.0).count();
        let mut gaps: Vec<f64> = uniq.windows(2).map(|w| w[1] - w[0]).filter(|g| *g > 0.0).collect();
        gaps.sort_by(f64::total_cmp);
        let p = |q: f64| gaps.get(((gaps.len().max(1) - 1) as f64 * q) as usize).copied().unwrap_or(f64::NAN);
        println!("durée {:.0} s · télémétrie publiée sur {:.0} % des relevés", span, tel.len() as f64 / v.len() as f64 * 100.0);
        println!("télémétrie : {:.1} mises à jour/s (écart médian {:.3} s, 95e centile {:.3} s) · classement : {:.1} mises à jour/s", uniq.len() as f64 / span, p(0.5), p(0.95), lapd.len() as f64 / span);
        println!("vitesse non nulle : {:.0} % · pédales non nulles : {:.0} %", moving as f64 / v.len() as f64 * 100.0, pedals as f64 / v.len() as f64 * 100.0);
        let ok = uniq.len() as f64 / span >= 5.0 && moving * 2 > v.len();
        println!("VERDICT {}", if ok { "OK : comparaison virage par virage possible (≥ 5 Hz)" } else { "INSUFFISANT : moins de 5 mises à jour par seconde ou vitesse absente" });
    }
}
