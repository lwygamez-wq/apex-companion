//! Bilan d'une course LMU : arrêts, niveaux réglés, énergie à l'arrivée, et annonces de l'ingénieur
//! relues dans le journal d'Apex, avec ce qu'il aurait fallu.
//!
//! `cargo run -p apex-core --example race_review -- course.apexlmu ["Apex Companion.log"]`
//!
//! Dernière ligne : `VERDICT OK` ou `VERDICT ÉCART : …` (utilisée par l'analyse programmée).

use apex_core::ingest::lmu::{decode, Tick};
use chrono::{NaiveDateTime, TimeZone, Utc};

/// Un arrêt : de l'entrée à la sortie de la voie des stands.
struct Stop {
    lap: i32,
    /// Instant de l'arrêt depuis le début de l'enregistrement (s).
    at: f64,
    lane_s: f64,
    still_s: f64,
    e_in: f32,
    e_out: f32,
    f_in: f32,
    f_out: f32,
}

fn stops(t: &[Tick]) -> Vec<Stop> {
    let mut out = vec![];
    let mut cur: Option<(usize, f64)> = None;
    for i in 1..t.len() {
        let k = &t[i];
        if k.pit && cur.is_none() {
            cur = Some((i, 0.0));
        }
        if let Some((start, still)) = cur.as_mut() {
            if k.pit && k.speed < 0.5 {
                *still += k.et - t[i - 1].et;
            }
            if !k.pit {
                let a = &t[*start];
                // Un passage dans la voie sans plein ni arrêt (drive-through) compte aussi.
                out.push(Stop { lap: a.lap, at: a.et - t[0].et, lane_s: k.et - a.et, still_s: *still, e_in: a.energy, e_out: k.energy, f_in: a.fuel, f_out: k.fuel });
                cur = None;
            }
        }
    }
    // Passage dans la voie au départ (sortie des stands) : pas un arrêt.
    out.retain(|s| s.lane_s > 15.0 || s.still_s > 2.0);
    out
}

/// Annonces de l'ingénieur pendant la course (journal en UTC : `[2026-11-12][15:20:28]… coach : …`).
fn calls(log: &str, from_unix: i64, to_unix: i64) -> Vec<(i64, String)> {
    let keys = ["Box", "Règle l'énergie", "Changement de plan", "Dernier relais", "Relais ", "marge", "lift", "économis", "Plan "];
    log.lines()
        .filter_map(|l| {
            let ts = l.get(1..11)?.to_string() + " " + l.get(13..21)?;
            let at = Utc.from_utc_datetime(&NaiveDateTime::parse_from_str(&ts, "%Y-%m-%d %H:%M:%S").ok()?).timestamp();
            let text = l.split_once("coach : ")?.1;
            ((from_unix..=to_unix).contains(&at) && keys.iter().any(|k| text.contains(k))).then(|| (at, text.to_string()))
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage : race_review <course.apexlmu> [journal]");
        std::process::exit(2);
    };
    let r = decode(&std::fs::read(path).expect("fichier illisible")).expect("enregistrement LMU illisible");
    let h = &r.header;
    let t = &r.ticks;
    if t.len() < 2 {
        println!("VERDICT OK (séance vide)");
        return;
    }
    println!("{} · {} ({}) · séance {} · départ {}", h.track, h.car, h.car_class.as_deref().unwrap_or("?"), h.session, h.start_position.map(|p| format!("P{p}")).unwrap_or_default());

    // Débuts de tour : énergie, carburant, place.
    let mut starts: Vec<&Tick> = vec![];
    for k in t {
        if starts.last().is_none_or(|p| p.lap != k.lap) {
            starts.push(k);
        }
    }
    let laps_run = starts.len().saturating_sub(1);
    let last = t.last().unwrap();
    // Arrivée : le dernier passage de ligne ; énergie au plus bas du dernier tour (le jeu remet parfois
    // 100 % juste après le drapeau).
    let end_place = starts.last().unwrap().place;
    let end_energy = t.iter().filter(|k| k.lap >= last.lap - 1 && !k.pit).map(|k| k.energy).fold(f32::MAX, f32::min);
    let end = Tick { place: end_place, energy: if end_energy == f32::MAX { 0.0 } else { end_energy }, ..**starts.last().unwrap() };
    let st = stops(t);
    let pit_laps: Vec<i32> = st.iter().flat_map(|s| [s.lap, s.lap + 1]).collect();
    // Consommation par tour en piste (tours sans stand).
    let mut per: Vec<f32> = starts.windows(2).filter(|w| !pit_laps.contains(&w[0].lap) && w[0].lap > 0).map(|w| w[0].energy - w[1].energy).filter(|d| *d > 0.3 && *d < 15.0).collect();
    per.sort_by(|a, b| a.total_cmp(b));
    let c = per.get(per.len() / 2).copied().unwrap_or(0.0);
    let energy_car = t.iter().any(|k| k.energy > 0.0);
    println!("{laps_run} tours · place finale P{} · conso {:.2} {} par tour", end.place, if energy_car { c } else { 0.0 }, if energy_car { "%" } else { "" });

    for s in &st {
        println!("Arrêt fin du tour {} : voie {:.1} s, à l'arrêt {:.1} s, énergie {:.1} → {:.1} %, carburant {:.1} → {:.1} L", s.lap, s.lane_s, s.still_s, s.e_in, s.e_out, s.f_in, s.f_out);
    }
    let spare = if energy_car && c > 0.0 { end.energy / c } else { 0.0 };
    if energy_car {
        println!("Arrivée : {:.1} % d'énergie, soit {:.1} tours à {:.2} %", end.energy, spare, c);
    }

    // Setup et pneus (fichiers Apex 0.36 et plus).
    if let Some(s) = &r.setup {
        let get = |k: &str| s.values.iter().find(|v| v.key == k).map(|v| format!("{} (cran {}/{})", v.display, v.value, v.max)).unwrap_or_else(|| "?".into());
        println!("Setup : {} réglages · aileron AR {} · carrossage AvG {} · pression AvG {} · idéal pneus {:?}", s.values.len(), get("VM_REAR_WING"), get("WM_CAMBER-W_FL"), get("WM_PRESSURE-W_FL"), s.optimal_temp_c);
    }
    if !r.wheels.is_empty() {
        let on: Vec<_> = r.wheels.iter().filter(|w| w.temp_c[0][1] > 30.0).collect();
        let names = ["AvG", "AvD", "ArG", "ArD"];
        for (k, n) in names.iter().enumerate() {
            let m = |z: usize| on.iter().map(|w| w.temp_c[k][z]).sum::<f32>() / on.len().max(1) as f32;
            let p = on.iter().map(|w| w.pressure_kpa[k]).sum::<f32>() / on.len().max(1) as f32;
            let b = on.iter().map(|w| w.brake_c[k]).fold(0.0f32, f32::max);
            let h = t.iter().map(|x| x.ride_h_m[k]).filter(|v| *v > 0.0).fold(f32::MAX, f32::min);
            println!("  {n} : pneus {:.0}/{:.0}/{:.0} °C (gauche/milieu/droite), {:.0} kPa, freins max {:.0} °C, hauteur mini {:.1} mm", m(0), m(1), m(2), p, b, if h == f32::MAX { 0.0 } else { h * 1000.0 });
        }
    }

    let mut issues: Vec<String> = vec![];
    // Dernier relais : ce qu'il aurait fallu régler (tours réellement courus + une demi-tour de marge).
    if let Some(s) = st.iter().rev().find(|s| s.e_out > s.e_in + 2.0) {
        let after = starts.iter().filter(|k| k.lap > s.lap).count().saturating_sub(1) as f32;
        let ideal = after * c + 0.5 * c;
        println!("Dernier arrêt : réglé {:.0} %, il fallait {:.0} % ({after:.0} tours × {c:.2} + une demi-tour)", s.e_out, ideal);
    }
    let refuel = st.iter().rev().find(|s| s.e_out > s.e_in + 2.0);
    let ideal = refuel.map(|s| (starts.iter().filter(|k| k.lap > s.lap).count().saturating_sub(1) as f32 + 0.5) * c);

    if let Some(lp) = args.get(1) {
        let log = std::fs::read(lp).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let from = h.started_at_unix;
        let to = from + (last.et - t[0].et) as i64 + 120;
        let cs = calls(&log, from, to);
        println!("Annonces de l'ingénieur ({}) :", cs.len());
        for (at, text) in &cs {
            let m = (at - from) / 60;
            println!("  +{m:>3} min  {text}");
        }
        // Niveau annoncé pour le vrai dernier plein : dernière consigne avant l'arrêt.
        let fill_of = |x: &str| x.split("Règle l'énergie à ").nth(1).and_then(|r| r.split(' ').next()).and_then(|v| v.trim_end_matches('%').trim().replace(',', ".").parse::<f32>().ok());
        if let (Some(s), Some(ideal)) = (refuel, ideal) {
            let at = from + s.at as i64 + 90;
            if let Some((called, fill)) = cs.iter().rev().filter(|(t, _)| *t <= at).find_map(|(t, x)| fill_of(x).map(|f| (*t, f))) {
                // Arrêt fait bien après l'annonce (pilote resté en piste) : le niveau annoncé valait pour un
                // arrêt au moment de l'annonce. On le compare aux tours courus après le tour de rentrée d'alors.
                let late = at - 90 - called;
                let ideal = if late > 150 {
                    let lt = (last.et - t[0].et) / laps_run.max(1) as f64;
                    let after = starts.iter().filter(|k| (k.et - t[0].et) > (called - from) as f64 + lt).count().saturating_sub(1) as f32;
                    println!("Arrêt fait {:.0} min après l'annonce : niveau comparé à un arrêt au moment de l'annonce ({after:.0} tours ensuite)", late as f64 / 60.0);
                    (after + 0.5) * c
                } else {
                    ideal
                };
                println!("Ingénieur : {fill:.0} % demandés à cet arrêt, il fallait {ideal:.0} % ; réglé {:.0} %", s.e_out);
                if (fill - ideal).abs() > 5.0 {
                    issues.push(format!("plein annoncé {fill:.0} % au lieu de {ideal:.0} %"));
                }
                // Consigne suivie et trop d'énergie à l'arrivée.
                if (s.e_out - fill).abs() <= 2.0 && spare > 2.0 {
                    issues.push(format!("{spare:.1} tours d'énergie restants à l'arrivée en suivant l'ingénieur"));
                }
            }
        }
        // Arrêt annoncé en plus alors qu'il n'y en a pas eu besoin.
        let extra = cs.iter().filter(|(_, x)| x.contains("Changement de plan") && x.contains("au lieu de 0")).count();
        if extra > 0 && end.energy > 0.0 && st.iter().filter(|s| s.e_out > s.e_in + 2.0).count() <= 1 {
            issues.push("arrêt en plus annoncé à tort en fin de course".into());
        }
    }

    if issues.is_empty() {
        println!("VERDICT OK");
    } else {
        println!("VERDICT ÉCART : {}", issues.join(" ; "));
    }
}
