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
        // Phases : bornes ordonnées, dans la fenêtre du virage, sans chevauchement, somme = écart du virage.
        let mut last_end = 0.0f32;
        for k in &c.corners {
            for p in &k.phases {
                if p.range_m[0] < last_end - 0.01 || p.range_m[1] <= p.range_m[0] || p.range_m[1] > k.corner.exit_m + 0.01 {
                    rule.push(format!("{} tour {} {} : phase {:?} mal bornée {:?}", file.display(), cmp.meta.number, k.corner.name, p.phase, p.range_m));
                }
                last_end = p.range_m[1];
            }
            let sum: f32 = k.phases.iter().map(|p| p.delta_s).sum();
            if !k.phases.is_empty() && (sum - k.time_delta_s).abs() > 0.005 {
                rule.push(format!("{} tour {} {} : phases {sum:.3} s ≠ virage {:.3} s", file.display(), cmp.meta.number, k.corner.name, k.time_delta_s));
            }
        }
        comparisons.push(json!({
            "reference": num(Some(r)),
            "compared": cmp.meta.number,
            "delta_end_s": c.delta_s.last(),
            "warnings": c.warnings,
            "corners": c.corners.iter().map(|k| json!({ "apex_m": k.corner.apex_m, "dt": k.time_delta_s, "min_ref": k.reference.min_speed_kmh, "min_cmp": k.compared.min_speed_kmh, "phases": k.phases.iter().map(|p| json!([p.phase, p.range_m, p.delta_s])).collect::<Vec<_>>() })).collect::<Vec<_>>(),
            "insights": c.insights.iter().map(|i| json!({ "id": i.id, "title": i.title, "priority": i.priority, "gain": i.est_gain_s, "segment": i.segment, "recommendation": i.recommendation, "evidence": i.evidence.iter().map(|e| [&e.label, &e.reference, &e.compared, &e.delta]).collect::<Vec<_>>(), "confidence": i.confidence })).collect::<Vec<_>>(),
        }));
    }

    for a in d.debrief.iter().flat_map(|x| &x.actions) {
        if !has_digit(&a.detail) {
            rule.push(format!("{} débrief « {} » : aucun chiffre", file.display(), a.title));
        }
    }

    // Diagnostic setup (LMU) : chiffres seulement ; tout constat cite un chiffre mesuré.
    let setup_diag = (m.sim == apex_core::model::SimId::Lmu).then(|| svc.setup_diagnosis(id).unwrap()).map(|g| {
        for f in g.tyre_findings.iter().chain(&g.height_findings) {
            if !has_digit(&f.text) || f.laps == 0 {
                rule.push(format!("{} diagnostic « {} » : aucun chiffre mesuré", file.display(), f.text));
            }
        }
        json!({
            "laps": g.laps_used,
            "tyres": g.tyres.iter().map(|t| json!([t.wheel, t.outer.as_ref().map(|x| x.value), t.middle.as_ref().map(|x| x.value), t.inner.as_ref().map(|x| x.value), t.carcass.as_ref().map(|x| x.value), t.pressure_hot.as_ref().map(|x| x.value)])).collect::<Vec<_>>(),
            "tyre_findings": g.tyre_findings.iter().map(|f| &f.text).collect::<Vec<_>>(),
            "brakes": g.brakes.iter().map(|b| json!([b.wheel, b.median.as_ref().map(|x| x.value), b.peak.as_ref().map(|x| x.value)])).collect::<Vec<_>>(),
            "heights": g.heights.iter().map(|h| json!([h.wheel, h.median.as_ref().map(|x| x.value), h.low.as_ref().map(|x| x.value), h.spikes_under_5mm, h.low_zone])).collect::<Vec<_>>(),
            "height_findings": g.height_findings.iter().map(|f| &f.text).collect::<Vec<_>>(),
            "balance": g.balance.iter().map(|b| json!([b.corner, b.phase, b.trend, b.ratio.value, b.ratio.laps])).collect::<Vec<_>>(),
            "locks": g.locks.len(),
            "setup_rows": g.setup.len(),
            "missing": g.missing,
        })
    });

    // Ingénieur rejoué comme en direct (courses LMU avec fin de chrono, 0.38+) : à chaque arrêt, ce qu'il
    // annonce à l'entrée des stands, et ce qui serait resté à l'arrivée en le suivant. Cible : 0,3 à 0,8 tour.
    let engineer = (m.sim == apex_core::model::SimId::Lmu && m.kind == apex_core::model::SessionKind::Race)
        .then(|| apex_core::ingest::lmu::decode(&std::fs::read(file).unwrap()).unwrap())
        .filter(|r| r.laps.iter().any(|e| e.time_remain_s.is_some()))
        .map(|r| {
            let hint = svc.engineer_hint(m);
            let rep = apex_core::engineer_replay::replay(&r, hint.plan, hint.pit_loss_s);
            for (l, m) in &rep.after_last_stop {
                rule.push(format!("{} T{l} : consigne après le dernier arrêt « {m} »", file.display()));
            }
            for s in &rep.stops {
                match s.spare_laps_if_announced {
                    Some(x) if (0.3..=0.8).contains(&x) => {}
                    x => rule.push(format!("{} arrêt T{} : arrivée {x:?} tour(s) en suivant l'ingénieur ({:?} % annoncés, {:.1} nécessaires), hors 0,3–0,8", file.display(), s.lap, s.announced, s.needed)),
                }
            }
            json!({
                "pit_loss": hint.pit_loss_s,
                "stops": rep.stops.iter().map(|s| json!({ "lap": s.lap, "set": s.set, "announced": s.announced, "needed": s.needed, "laps_est": s.laps_est, "laps_true": s.laps_true, "per_est": s.per_lap_est, "per_true": s.per_lap_true, "spare_if_announced": s.spare_laps_if_announced, "spare_actual": s.spare_laps_actual })).collect::<Vec<_>>(),
                "line_exact_errors": rep.line_calls.iter().filter_map(|c| c.exact_est.map(|e| e - c.exact_true)).collect::<Vec<_>>(),
                "said": rep.said,
                "after_last_stop": rep.after_last_stop,
            })
        });

    // Banc de stratégie (courses LMU) : options comparées sur la distance réellement courue, contre
    // ce qui a été fait (arrêts, temps au stand mesuré, énergie restante à l'arrivée).
    let options = (m.sim == apex_core::model::SimId::Lmu && m.kind == apex_core::model::SessionKind::Race)
        .then(|| apex_core::ingest::lmu::decode(&std::fs::read(file).unwrap()).unwrap())
        .and_then(|r| {
            let ex = apex_core::analysis::debrief::race_extras(&r);
            let laps = d.laps.iter().filter(|l| l.meta.lap_time_s.is_some()).count() as u32;
            let v = svc.race_strategy(m.sim, &m.track, m.track_config.as_deref(), &m.car, m.setup, Some(apex_core::analysis::strategy::RaceLength::Laps(laps)), 0.5, 0.0, &StrategyOverrides::default()).ok()?;
            let o = v.options?;
            let pick = |i: &usize| o.candidates.get(*i);
            let rec = match &o.verdict {
                apex_core::analysis::race_options::Verdict::Clear { best, .. } | apex_core::analysis::race_options::Verdict::Only { best } => pick(best),
                apex_core::analysis::race_options::Verdict::Equivalent { safest, .. } => pick(safest),
                apex_core::analysis::race_options::Verdict::Unknown { .. } => None,
            };
            let done_stops = ex.stops.iter().filter(|s| s.energy_out > s.energy_in + 2.0 || s.fuel_out_l > s.fuel_in_l + 1.0).count();
            let done_still: f64 = ex.stops.iter().map(|s| s.still_s).sum();
            let done_spare = ex.energy_finish.zip(ex.energy_per_lap).map(|(e, c)| e / c);
            if let Some(c) = rec {
                if c.pit_time_s.is_some() && c.saving > 0.0 {
                    rule.push(format!("{} : stratégie recommandée avec économie (coût non mesuré)", file.display()));
                }
            }
            println!(
                "BANC stratégie {} : fait {} arrêt(s), {:.1} s à l'arrêt, arrivée {} ; recommandé {:?}",
                file.file_name().unwrap().to_string_lossy(),
                done_stops,
                done_still,
                done_spare.map(|x| format!("{x:.2} tour")).unwrap_or("?".into()),
                rec.map(|c| (c.stops, c.pit_time_s, c.pit_time_sd_s, c.fills.clone()))
            );
            // Gain estimé en suivant la recommandation : énergie remise en trop (arrivée au-delà de la marge
            // visée de 0,5 tour) fois le temps de service mesuré par unité. Jamais présenté comme garanti.
            let rate = o.candidates.iter().find_map(|c| c.pit_time_s.map(|_| ())).and(Some(v.per_lap));
            let overfill_laps = done_spare.map(|x| (x - 0.5).max(0.0));
            let gain_s = match (overfill_laps, rate, o.pit_model) {
                (Some(x), Some(c), Some(p)) => Some(x * c * p.refill_s_per_unit),
                _ => None,
            };
            println!("BANC gain estimé {} : {:?} s (plein en trop {:?} tour)", file.file_name().unwrap().to_string_lossy(), gain_s, overfill_laps);
            Some(json!({
                "gain_s": gain_s,
                "verdict": o.verdict,
                "candidates": o.candidates.iter().map(|c| json!({ "stops": c.stops, "saving": c.saving, "laps": c.laps, "window": c.window, "fills": c.fills, "pit_s": c.pit_time_s, "pit_sd": c.pit_time_sd_s, "breakeven": c.breakeven_s_per_lap, "wear_left": c.wear_left_pct })).collect::<Vec<_>>(),
                "radio": o.radio,
                "done": { "stops": done_stops, "still_s": done_still, "spare_laps": done_spare },
            }))
        });

    // Mémoire d'essais setup (LMU) : changements entre séances, effets, file d'essais, setup recommandé.
    // Règle : un changement recommandé doit venir d'un essai à un seul réglage avec effet mesuré.
    let setup_memory = (m.sim == apex_core::model::SimId::Lmu).then(|| svc.setup_memory(id).unwrap()).map(|mm| {
        if let Some(r) = &mm.recommended {
            for (c, proof) in &r.changes {
                if !proof.contains("effet mesuré") {
                    rule.push(format!("{} : {} recommandé sans effet mesuré", file.display(), c.label));
                }
            }
        }
        for e in &mm.entries {
            println!("BANC setup {} : {} → {} {:?} ({} changement(s)) {}", file.file_name().unwrap().to_string_lossy(), e.from_session, e.to_session, e.status, e.changes.len(), e.text);
        }
        json!({
            "entries": mm.entries.iter().map(|e| json!({ "status": e.status, "changes": e.changes.len(), "effects": e.effects.iter().map(|x| json!([x.what, x.before, x.after, x.threshold, x.significant])).collect::<Vec<_>>() })).collect::<Vec<_>>(),
            "queue": mm.queue.iter().map(|q| json!([q.trial.label, q.trial.direction, q.trial.steps, q.repeats, q.sessions_measured, q.tried.is_some()])).collect::<Vec<_>>(),
            "skipped": mm.skipped,
            "recommended": mm.recommended.as_ref().map(|r| r.changes.len()),
            "consumption": mm.consumption.iter().map(|c| json!([c.1, c.2, c.3, c.4])).collect::<Vec<_>>(),
            "missing": mm.missing,
        })
    });

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
        // Référence d'une autre séance : son temps (bibliothèque ou choix manuel).
        "analysis_reference_time": reference.filter(|r| num(Some(*r)).is_none()).and_then(|r| svc.lap_row(r).ok()).and_then(|l| l.meta.lap_time_s),
        // Bibliothèque de références : combinaisons dont cette séance porte la référence élue.
        "library": svc.references().unwrap().iter().filter(|e| e.reference.session_id == id).map(|e| json!({ "lap": e.reference.lap_number, "time": e.reference.lap_time_s, "conditions": e.reference.key.conditions, "setup": e.reference.key.setup })).collect::<Vec<_>>(),
        "theoretical_best_s": d.theoretical_best_s,
        "consistency": d.consistency,
        "off_tracks": d.off_tracks,
        "risk_headline": d.risk_headline,
        "risk": d.risk,
        "regularity": d.regularity,
        // Pas d'iRating dans l'instantané (dépôt public).
        "race": d.race.as_ref().map(|r| json!({ "field": r.field_size, "start": r.start_position, "finish": r.finish_position, "incidents": r.incident_total, "pace": r.pace, "headline": r.headline, "insights": r.insights })),
        // Débrief de course : pertes mesurées, priorités (chiffres et textes).
        "debrief": d.debrief,
        "comparisons": comparisons,
        "ai_context": svc.ai_context(id).ok(),
        "strategy": strategy,
        "options": options,
        "setup_memory": setup_memory,
        "setup_diag": setup_diag,
        "engineer_replay": engineer,
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

    // Bibliothèque de références : une relecture complète ne change rien, et aucune référence élue
    // ne vient d'un tour invalide, perturbé ou incomplet.
    let before: Vec<i64> = svc.references().unwrap().iter().map(|e| e.reference.id).collect();
    let again = svc.refresh_references(None).unwrap();
    let after: Vec<i64> = svc.references().unwrap().iter().map(|e| e.reference.id).collect();
    if again.elected != 0 || again.retired != 0 || before != after {
        rule.push(format!("bibliothèque de références instable à la relecture : {again:?}"));
    }
    for e in svc.references().unwrap() {
        let r = &e.reference;
        let Some(lap) = e.lap_id.map(|id| svc.lap_row(id).unwrap()) else {
            rule.push(format!("référence {} / {} : tour introuvable", r.key.track, r.key.car));
            continue;
        };
        if !lap.clean || lap.meta.validity == apex_core::model::Validity::Invalid {
            rule.push(format!("référence {} / {} tirée d'un tour non propre (tour {})", r.key.track, r.key.car, r.lap_number));
        }
    }
    println!("bibliothèque : {} référence(s)", after.len());
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
