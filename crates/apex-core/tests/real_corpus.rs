//! Non-régression sur de vraies séances (corpus privé `apex-corpus`, préparé par l'exemple
//! `corpus_prepare` : pilotes anonymisés, fichiers compressés).
//!
//! `APEX_CORPUS_DIR=<dossier> cargo test -p apex-core --test real_corpus -- --nocapture`
//! Sans la variable, le test est ignoré.
//!
//! Pour chaque fichier, tout le parcours de l'app est rejoué : import, analyse de séance, comparaison
//! de tours et conseils, contexte du débrief IA, stratégie carburant, et (iRacing) coach radio rejoué
//! comme en direct. Le résultat est figé dans `tests/corpus/<fichier>.json` (chiffres seulement,
//! aucun nom) et comparé à chaque passage, avec une tolérance numérique (Linux et Windows ne
//! calculent pas les flottants au bit près).
//!
//! Un changement voulu : `APEX_UPDATE_SNAPSHOTS=1` réécrit les instantanés ; l'écart apparaît dans la PR.
//!
//! Règle produit vérifiée au passage : tout conseil s'appuie sur un chiffre mesuré.

use apex_core::live_coach::{CoachMessageKind, LiveCoach};
use apex_core::model::LapKind;
use apex_core::service::{AppService, StrategyOverrides};
use apex_core::storage::InsertOutcome;
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};

fn has_digit(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_digit())
}

/// Arrondit les flottants pour des instantanés lisibles et stables.
fn round(v: Value) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => {
            let x = n.as_f64().unwrap();
            json!((x * 1000.0).round() / 1000.0)
        }
        Value::Array(a) => Value::Array(a.into_iter().map(round).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, round(v))).collect()),
        v => v,
    }
}

/// Écarts entre deux instantanés : chemins JSON, tolérance relative de 0,1 % (ou 0,002 en absolu).
fn diff(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            if (x - y).abs() > 0.002_f64.max(x.abs().max(y.abs()) * 1e-3) {
                out.push(format!("{path} : {x} → {y}"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                diff(&format!("{path}.{k}"), x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), out);
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.push(format!("{path} : {} éléments → {}", x.len(), y.len()));
            }
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                diff(&format!("{path}[{i}]"), p, q, out);
            }
        }
        _ if a != b => out.push(format!("{path} : {a} → {b}")),
        _ => {}
    }
}

fn unpack(src: &Path, dir: &Path) -> PathBuf {
    let name = src.file_name().unwrap().to_string_lossy().trim_end_matches(".gz").to_string();
    let dest = dir.join(name);
    let mut b = vec![];
    flate2::read::GzDecoder::new(std::fs::File::open(src).unwrap()).read_to_end(&mut b).unwrap();
    std::fs::write(&dest, b).unwrap();
    dest
}

/// Tout ce que l'app calcule pour une séance, en chiffres. `rule` collecte les conseils sans chiffre.
fn snapshot(svc: &AppService, id: i64, file: &Path, rule: &mut Vec<String>) -> Value {
    let d = svc.session_detail(id).unwrap();
    let m = &d.meta;
    let laps: Vec<Value> = d
        .laps
        .iter()
        .map(|l| json!({ "n": l.meta.number, "kind": l.meta.kind, "time": l.meta.lap_time_s, "validity": l.meta.validity, "clean": l.clean, "sectors": l.meta.sector_times_s, "fuel": l.meta.fuel_used_l }))
        .collect();
    let lap_id = |n: i32| d.laps.iter().find(|l| l.meta.number == n).map(|l| l.id);
    let num = |id: Option<i64>| id.and_then(|i| d.laps.iter().find(|l| l.id == i)).map(|l| l.meta.number);

    // Comparaison par défaut de l'écran séance : référence d'analyse contre le tour lancé complet le
    // plus lent parmi les autres (celui où il y a le plus à apprendre), et contre le 2e meilleur.
    let reference = d.analysis_reference_lap_id;
    let mut flying: Vec<_> = d.laps.iter().filter(|l| l.meta.kind == LapKind::Flying && l.meta.lap_time_s.is_some() && Some(l.id) != reference).collect();
    flying.sort_by(|a, b| a.meta.lap_time_s.partial_cmp(&b.meta.lap_time_s).unwrap());
    let mut comparisons = vec![];
    for cmp in [flying.first(), flying.last()].into_iter().flatten() {
        let Some(r) = reference else { break };
        let c = svc.compare(r, cmp.id, 5.0).unwrap();
        for i in &c.insights {
            let numbers = has_digit(&i.recommendation) || has_digit(&i.explanation) || i.evidence.iter().any(|e| has_digit(&e.delta));
            if !numbers {
                rule.push(format!("{} tour {} « {} » : aucun chiffre", file.display(), cmp.meta.number, i.title));
            }
        }
        comparisons.push(json!({
            "reference": num(Some(r)),
            "compared": cmp.meta.number,
            "delta_end_s": c.delta_s.last(),
            "warnings": c.warnings,
            "corners": c.corners.iter().map(|k| json!({ "apex_m": k.corner.apex_m, "dt": k.time_delta_s, "min_ref": k.reference.min_speed_kmh, "min_cmp": k.compared.min_speed_kmh })).collect::<Vec<_>>(),
            "insights": c.insights.iter().map(|i| json!({ "id": i.id, "title": i.title, "priority": i.priority, "gain": i.est_gain_s, "segment": i.segment, "recommendation": i.recommendation, "evidence": i.evidence.iter().map(|e| [&e.label, &e.reference, &e.compared, &e.delta]).collect::<Vec<_>>(), "confidence": i.confidence })).collect::<Vec<_>>(),
        }));
    }

    let strategy = svc
        .race_strategy(m.sim, &m.track, m.track_config.as_deref(), &m.car, m.setup, None, 0.5, 0.5, &StrategyOverrides::default())
        .ok()
        .map(|v| json!({ "unit": v.unit, "per_lap": v.per_lap, "lap_time": v.lap_time_s, "laps": v.strategy.laps, "stops": v.strategy.stops, "stints": v.strategy.stints.iter().map(|s| (s.from_lap, s.to_lap, s.amount)).collect::<Vec<_>>() }));

    // Coach radio rejoué comme en direct (iRacing : le .ibt contient les lignes de télémétrie).
    let mut radio = vec![];
    if file.extension().is_some_and(|e| e == "ibt") {
        let (yaml, rows) = apex_core::ingest::ibt::ibt_live_rows(&std::fs::read(file).unwrap()).unwrap();
        let meta = LiveCoach::new(&yaml, None, vec![], None).0.session().clone();
        let (mut coach, hello) = LiveCoach::new(&yaml, svc.live_reference(&meta).unwrap(), vec![], None);
        radio.push(json!([0, "hello", hello.text]));
        for r in rows {
            if let Some(msg) = coach.push(r) {
                if msg.kind == CoachMessageKind::Beep {
                    continue;
                }
                if msg.kind == CoachMessageKind::Focus && msg.focus.is_some() && !has_digit(&msg.text) {
                    rule.push(format!("{} radio tour {} : « {} » sans chiffre", file.display(), msg.lap, msg.text));
                }
                radio.push(json!([msg.lap, msg.kind, msg.text]));
            }
        }
    }

    round(json!({
        "session": { "sim": m.sim, "track": m.track, "config": m.track_config, "car": m.car, "kind": m.kind, "setup": m.setup, "length_m": m.track_length_m },
        "laps": laps,
        "best": num(d.best_lap_id),
        "fastest": num(d.fastest_lap_id),
        "analysis_reference": num(reference),
        "theoretical_best_s": d.theoretical_best_s,
        "consistency": d.consistency,
        "off_tracks": d.off_tracks,
        "risk_headline": d.risk_headline,
        "risk": d.risk,
        "regularity": d.regularity,
        // Pas d'iRating dans l'instantané (dépôt public).
        "race": d.race.as_ref().map(|r| json!({ "field": r.field_size, "start": r.start_position, "finish": r.finish_position, "incidents": r.incident_total, "pace": r.pace, "headline": r.headline, "insights": r.insights })),
        "comparisons": comparisons,
        "ai_context": svc.ai_context(id).ok(),
        "strategy": strategy,
        "radio": radio,
        "lap_2": lap_id(2).is_some(),
    }))
}

#[test]
fn real_sessions_do_not_regress() {
    let Some(dir) = std::env::var_os("APEX_CORPUS_DIR") else {
        eprintln!("APEX_CORPUS_DIR non défini : test ignoré");
        return;
    };
    let update = std::env::var_os("APEX_UPDATE_SNAPSHOTS").is_some();
    let snaps = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("corpus");
    std::fs::create_dir_all(&snaps).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "gz")).collect();
    files.sort();
    assert!(!files.is_empty(), "corpus vide");

    // Une base pour tout le corpus, comme dans l'app : les séances d'un même circuit se nourrissent
    // (référence perso, carte de risque) ; l'ordre d'import est fixe (noms triés).
    let svc = AppService::open(&tmp.path().join("apex.db")).unwrap();
    let mut imported = vec![];
    for f in &files {
        let raw = unpack(f, tmp.path());
        let r = svc.import_paths(std::slice::from_ref(&raw)).remove(0);
        let id = match r.outcome {
            Some(InsertOutcome::Inserted(id)) | Some(InsertOutcome::Duplicate(id)) => id,
            None => panic!("{} : import en échec : {:?}", f.display(), r.error),
        };
        imported.push((f.clone(), raw, id));
    }

    let (mut failures, mut rule) = (vec![], vec![]);
    for (f, raw, id) in &imported {
        let name = f.file_name().unwrap().to_string_lossy().trim_end_matches(".gz").to_string();
        let got = snapshot(&svc, *id, raw, &mut rule);
        let path = snaps.join(format!("{name}.json"));
        // En CI, un fichier du corpus sans instantané est une erreur : il ne serait jamais vérifié.
        if !path.exists() && !update && std::env::var_os("CI").is_some() {
            println!("{name} : instantané manquant (APEX_UPDATE_SNAPSHOTS=1 en local, puis le joindre à la PR)");
            failures.push(name);
            continue;
        }
        if update || !path.exists() {
            std::fs::write(&path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
            println!("{name} : instantané écrit");
            continue;
        }
        let want: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let mut out = vec![];
        diff("", &want, &got, &mut out);
        if out.is_empty() {
            println!("{name} : identique");
        } else {
            println!("{name} : {} écart(s)", out.len());
            for l in out.iter().take(40) {
                println!("  {l}");
            }
            failures.push(name);
        }
    }
    for r in &rule {
        println!("RÈGLE {r}");
    }
    assert!(failures.is_empty(), "écarts sur données réelles : {failures:?} (voir la sortie ; changement voulu : APEX_UPDATE_SNAPSHOTS=1)");
    assert!(rule.is_empty(), "{} conseil(s) sans chiffre mesuré", rule.len());
}
