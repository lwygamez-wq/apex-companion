//! Test de charge : 1 h de roulage sur un circuit de ~7,2 km (type Spa), 60 Hz,
//! enregistrement de ~1 Ko comme un vrai .ibt (≈ 250 variables).
//! `cargo run --release -p apex-core --features testkit --example load_test`

use apex_core::analysis::progress::{progress_zones, SessionLap};
use apex_core::analysis::session::best_clean_lap;
use apex_core::analysis::{compare_laps, LapInput};
use apex_core::ingest::ibt::{import_ibt_file, writer, VarType};
use apex_core::mock::{generate_lap_on, CornerFault, Seg, DEMO_TRACK};
use apex_core::storage::{InsertOutcome, Store};
use std::time::Instant;

fn main() {
    let dir = std::env::temp_dir().join("apex-load");
    std::fs::create_dir_all(&dir).unwrap();
    let track: Vec<Seg> = DEMO_TRACK.iter().chain(DEMO_TRACK.iter()).copied().collect();
    let hz = 60.0f32;

    // 1) Génération des tours (≈ 1 h).
    let t0 = Instant::now();
    let mut laps = vec![];
    let mut total = 0.0;
    let mut i = 0u64;
    while total < 3600.0 {
        let mut f = vec![CornerFault { apex_speed_factor: 1.0, ..Default::default() }; 14];
        f[(i % 14) as usize] = CornerFault { early_brake_m: 10.0, apex_speed_factor: 0.96, ..Default::default() };
        let (tr, lt) = generate_lap_on(&track, &f, hz, i);
        total += lt;
        laps.push(tr);
        i += 1;
    }
    let length = laps[0].max_dist() as f64 + 1.0;
    println!("{} tours générés ({:.0} s de roulage) en {:.1?}", laps.len(), total, t0.elapsed());

    // 2) Écriture d'un .ibt réaliste : 11 variables utiles + 240 de remplissage.
    let mut names: Vec<String> = (0..240).map(|k| format!("Pad{k}")).collect();
    let mut vars = vec![
        writer::Var { name: "SessionTime", ty: VarType::Double, unit: "s" },
        writer::Var { name: "Lap", ty: VarType::Int, unit: "" },
        writer::Var { name: "LapDist", ty: VarType::Float, unit: "m" },
        writer::Var { name: "LapDistPct", ty: VarType::Float, unit: "%" },
        writer::Var { name: "Speed", ty: VarType::Float, unit: "m/s" },
        writer::Var { name: "Throttle", ty: VarType::Float, unit: "%" },
        writer::Var { name: "Brake", ty: VarType::Float, unit: "%" },
        writer::Var { name: "Gear", ty: VarType::Int, unit: "" },
        writer::Var { name: "SteeringWheelAngle", ty: VarType::Float, unit: "rad" },
        writer::Var { name: "OnPitRoad", ty: VarType::Bool, unit: "" },
        writer::Var { name: "PlayerTrackSurface", ty: VarType::Int, unit: "" },
    ];
    let pad_names: Vec<&'static str> = names.drain(..).map(|s| &*Box::leak(s.into_boxed_str())).collect();
    for n in &pad_names {
        vars.push(writer::Var { name: n, ty: VarType::Float, unit: "" });
    }
    let t1 = Instant::now();
    let mut rows = Vec::new();
    let mut base = 0.0f64;
    for (n, tr) in laps.iter().enumerate() {
        for k in 0..tr.len() {
            let d = tr.dist_m[k] as f64;
            let mut row = vec![
                base + tr.time_s[k] as f64,
                n as f64 + 1.0,
                d,
                (d / length).min(0.99999),
                tr.speed_mps[k] as f64,
                tr.throttle[k] as f64,
                tr.brake[k] as f64,
                tr.gear.as_ref().unwrap()[k] as f64,
                tr.steer_rad.as_ref().unwrap()[k] as f64,
                0.0,
                3.0,
            ];
            row.extend(std::iter::repeat_n(0.5, 240));
            rows.push(row);
        }
        base += *tr.time_s.last().unwrap() as f64 + 1.0 / hz as f64;
    }
    let yaml = "---\nWeekendInfo:\n TrackDisplayName: Circuit Long\n TrackLength: 7.20 km\n TrackWetness: Dry\nSessionInfo:\n Sessions:\n - SessionNum: 0\n   SessionType: Offline Testing\nDriverInfo:\n DriverCarIdx: 0\n Drivers:\n - CarIdx: 0\n   CarScreenName: Formule Test\n...\n";
    let bytes = writer::build(&vars, &rows, yaml, 60, 1_760_000_000);
    let path = dir.join("long.ibt");
    std::fs::write(&path, &bytes).unwrap();
    drop(rows);
    drop(bytes);
    println!("fichier .ibt : {:.0} Mo, {} échantillons, écrit en {:.1?}", std::fs::metadata(&path).unwrap().len() as f64 / 1e6, (total * 60.0) as u64, t1.elapsed());

    // 3) Import + stockage + analyses.
    let t = Instant::now();
    let s = import_ibt_file(&path).unwrap();
    println!("import (lecture + découpage) : {:.2?} — {} tours", t.elapsed(), s.laps.len());
    let db = dir.join("load.db");
    let _ = std::fs::remove_file(&db);
    let mut st = Store::open(&db).unwrap();
    let t = Instant::now();
    let InsertOutcome::Inserted(sid) = st.insert_session(&s).unwrap() else { panic!() };
    println!("stockage SQLite : {:.2?} — base {:.1} Mo", t.elapsed(), std::fs::metadata(&db).unwrap().len() as f64 / 1e6);

    let rows = st.laps(sid).unwrap();
    let metas: Vec<_> = rows.iter().map(|r| r.meta.clone()).collect();
    let bi = best_clean_lap(&metas).unwrap();
    let t = Instant::now();
    let traces: Vec<_> = rows.iter().map(|r| st.trace(r.id).unwrap()).collect();
    println!("relecture des {} traces : {:.2?}", traces.len(), t.elapsed());
    let meta = st.session_meta(sid).unwrap();
    let t = Instant::now();
    let c = compare_laps(
        &LapInput { lap_id: rows[bi].id, session: &meta, lap: &rows[bi].meta, trace: &traces[bi] },
        &LapInput { lap_id: rows[1].id, session: &meta, lap: &rows[1].meta, trace: &traces[1] },
        1.0,
    )
    .unwrap();
    println!("comparaison 2 tours (pas 1 m) : {:.2?} — {} virages, {} conseils, JSON {:.1} Mo", t.elapsed(), c.corners.len(), c.insights.len(), serde_json::to_vec(&c).unwrap().len() as f64 / 1e6);
    let sl: Vec<_> = rows.iter().zip(&traces).map(|(r, t)| SessionLap { lap_id: r.id, meta: &r.meta, trace: t }).collect();
    let t = Instant::now();
    let z = progress_zones(&meta, &sl[bi], &sl, 2.0);
    println!("zones de progrès ({} tours comparés) : {:.2?} — {} zones", sl.len() - 1, t.elapsed(), z.len());
}
