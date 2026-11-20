//! Calibration du diagnostic setup (étape 2) sur des enregistrements LMU réels.
//!
//! `cargo run -p apex-core --example setup_calib -- a.apexlmu [b.apexlmu …]`
//!
//! Pour chaque fichier : voiture, circuit, setup enregistré, puis les grandeurs brutes nécessaires
//! pour trancher la nature des températures (surface ou carcasse) et le seuil de talonnage :
//! températures des 3 zones et carcasse en appui / en ligne droite, montée sur les premiers tours,
//! pressions, freins, hauteur de caisse mini par roue et ce qu'il y a sous la voiture à ce moment.

use apex_core::ingest::lmu::{decode, Recording, Tick, WheelSample};

const W: [&str; 4] = ["AvG", "AvD", "ArG", "ArD"];

fn pct(v: &mut [f32], p: f32) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f32 * p).round() as usize]
}

/// Tick le plus proche (par temps) d'un échantillon roues.
fn tick_at(r: &Recording, et: f64) -> Option<&Tick> {
    let i = r.ticks.partition_point(|t| t.et < et);
    r.ticks.get(i.min(r.ticks.len().saturating_sub(1)))
}

/// Accélération latérale approchée (m/s²) : vitesse × vitesse de lacet.
fn a_lat(t: &Tick) -> f32 {
    (t.speed * t.yaw).abs()
}

fn main() {
    for p in std::env::args().skip(1) {
        let bytes = std::fs::read(&p).expect("lecture");
        let r = decode(&bytes).expect("décodage");
        let h = &r.header;
        println!("\n=== {} | {} | {} ({}) | séance {} | air {:?} °C piste {:?} °C", p.rsplit('/').next().unwrap_or(&p), h.track, h.car, h.car_class.clone().unwrap_or_default(), h.session, h.air_temp_c, h.track_temp_c);
        println!("ticks {} ({} Hz), échantillons roues {}, tours {}", r.ticks.len(), h.hz, r.wheels.len(), r.laps.len());
        match &r.setup {
            Some(s) => {
                println!("setup : {} réglages, idéal {:?}", s.values.len(), s.optimal_temp_c);
                for v in s.values.iter().filter(|v| ["CAMBER", "PRESS", "RIDE", "BRAKE", "SPRING", "WING", "ARB", "TOE"].iter().any(|k| v.key.to_uppercase().contains(k))) {
                    println!("  {} = {} (« {} », crans {}–{})", v.key, v.value, v.display, v.min, v.max);
                }
            }
            None => println!("setup : absent"),
        }
        if r.wheels.is_empty() {
            println!("pas de pneus/freins (fichier d'avant 0.36)");
            continue;
        }
        // Échantillons en piste, hors stand, en mouvement.
        let on: Vec<(&WheelSample, &Tick)> = r.wheels.iter().filter_map(|w| tick_at(&r, w.et).map(|t| (w, t))).filter(|(_, t)| !t.pit && t.speed > 20.0).collect();
        let corner: Vec<_> = on.iter().filter(|(_, t)| a_lat(t) > 12.0).collect();
        let straight: Vec<_> = on.iter().filter(|(_, t)| a_lat(t) < 2.0 && t.throttle > 0.95).collect();
        println!("en piste {} | en appui (>12 m/s²) {} | ligne droite pleine charge {}", on.len(), corner.len(), straight.len());
        for (k, name) in W.iter().enumerate() {
            let z = |set: &[&(&WheelSample, &Tick)], i: usize| pct(&mut set.iter().map(|(w, _)| w.temp_c[k][i]).collect::<Vec<_>>(), 0.5);
            let c = |set: &[&(&WheelSample, &Tick)]| pct(&mut set.iter().map(|(w, _)| w.carcass_c[k]).collect::<Vec<_>>(), 0.5);
            let cr: Vec<&(&WheelSample, &Tick)> = corner.to_vec();
            let st: Vec<&(&WheelSample, &Tick)> = straight.to_vec();
            let all: Vec<&(&WheelSample, &Tick)> = on.iter().collect();
            let p = pct(&mut on.iter().map(|(w, _)| w.pressure_kpa[k]).collect::<Vec<_>>(), 0.5);
            let b50 = pct(&mut on.iter().map(|(w, _)| w.brake_c[k]).collect::<Vec<_>>(), 0.5);
            let b95 = pct(&mut on.iter().map(|(w, _)| w.brake_c[k]).collect::<Vec<_>>(), 0.95);
            let bmax = on.iter().map(|(w, _)| w.brake_c[k]).fold(0.0f32, f32::max);
            println!(
                "{} surface G/M/D appui {:.1}/{:.1}/{:.1} droite {:.1}/{:.1}/{:.1} | carcasse appui {:.1} droite {:.1} (p5 {:.1} p95 {:.1}) | pression {:.1} kPa | frein méd {:.0} p95 {:.0} max {:.0} °C",
                name, z(&cr, 0), z(&cr, 1), z(&cr, 2), z(&st, 0), z(&st, 1), z(&st, 2), c(&cr), c(&st),
                pct(&mut all.iter().map(|(w, _)| w.carcass_c[k]).collect::<Vec<_>>(), 0.05), pct(&mut all.iter().map(|(w, _)| w.carcass_c[k]).collect::<Vec<_>>(), 0.95), p, b50, b95, bmax
            );
        }
        // Montée en température : médiane carcasse et surface milieu par tour (premiers tours).
        let laps: Vec<i32> = {
            let mut v: Vec<i32> = on.iter().map(|(_, t)| t.lap).collect();
            v.dedup();
            v.sort();
            v.dedup();
            v
        };
        let mut line = String::from("montée (tour : carcasse AvG / surface milieu AvG) :");
        for l in laps.iter().take(6) {
            let c = pct(&mut on.iter().filter(|(_, t)| t.lap == *l).map(|(w, _)| w.carcass_c[0]).collect::<Vec<_>>(), 0.5);
            let s = pct(&mut on.iter().filter(|(_, t)| t.lap == *l).map(|(w, _)| w.temp_c[0][1]).collect::<Vec<_>>(), 0.5);
            line += &format!(" {l}: {c:.0}/{s:.0}");
        }
        println!("{line}");
        // Hauteur de caisse : mini par roue, et matière sous la voiture.
        let hs: Vec<&Tick> = r.ticks.iter().filter(|t| !t.pit && t.speed > 20.0 && t.ride_h_m != [0.0; 4]).collect();
        if hs.is_empty() {
            println!("hauteur : non enregistrée");
            continue;
        }
        for (k, name) in W.iter().enumerate() {
            let mut v: Vec<f32> = hs.iter().map(|t| t.ride_h_m[k] * 1000.0).collect();
            let mut asph: Vec<f32> = hs.iter().filter(|t| t.material == 1).map(|t| t.ride_h_m[k] * 1000.0).collect();
            let mut kerb: Vec<f32> = hs.iter().filter(|t| t.material == 11).map(|t| t.ride_h_m[k] * 1000.0).collect();
            let min = hs.iter().min_by(|a, b| a.ride_h_m[k].total_cmp(&b.ride_h_m[k])).unwrap();
            let n_lt = |thr: f32, set: &[&Tick]| set.iter().filter(|t| t.ride_h_m[k] * 1000.0 < thr).count();
            let asph_t: Vec<&Tick> = hs.iter().copied().filter(|t| t.material == 1).collect();
            let kerb_t: Vec<&Tick> = hs.iter().copied().filter(|t| t.material == 11).collect();
            println!(
                "{} hauteur mm : méd {:.1} p1 {:.1} min {:.1} (matière {}, {:.0} km/h, frein {:.2}) | bitume p1 {:.1} ({} ticks < 5 mm sur {}) | vibreur p1 {:.1} ({} ticks < 5 mm sur {})",
                name, pct(&mut v, 0.5), pct(&mut v, 0.01), min.ride_h_m[k] * 1000.0, min.material, min.speed * 3.6, min.brake,
                pct(&mut asph, 0.01), n_lt(5.0, &asph_t), asph_t.len(), pct(&mut kerb, 0.01), n_lt(5.0, &kerb_t), kerb_t.len()
            );
        }
    }
}
