//! Bout en bout : session démo → stockage → comparaison → conseils.
//! Les défauts sont injectés par le générateur ; l'analyse doit les retrouver.

use apex_core::analysis::insights::{Confidence, Priority};
use apex_core::analysis::session::theoretical_best;
use apex_core::analysis::{compare_laps, LapInput};
use apex_core::mock::{demo_session, generate_lap, track_length_m, CornerFault};
use apex_core::model::*;
use apex_core::storage::{InsertOutcome, SessionFilter, Store};

fn meta(n: i32) -> LapMeta {
    LapMeta {
        number: n,
        kind: LapKind::Flying,
        lap_time_s: Some(90.0),
        sector_times_s: vec![],
        flags: LapFlags::default(),
        validity: Validity::Valid,
        validity_source: ValiditySource::SimReported,
        fuel_start_l: None,
        fuel_used_l: None,
    }
}

fn clean() -> Vec<CornerFault> {
    vec![CornerFault { apex_speed_factor: 1.0, ..Default::default() }; 7]
}

#[test]
fn delta_at_finish_matches_lap_time_difference() {
    let s = demo_session(3, 0);
    let (rf, cmp) = (&s.laps[2], &s.laps[1]);
    let c = compare_laps(
        &LapInput { lap_id: 2, session: &s.meta, lap: &rf.meta, trace: &rf.trace },
        &LapInput { lap_id: 1, session: &s.meta, lap: &cmp.meta, trace: &cmp.trace },
        1.0,
    )
    .unwrap();
    let expected = (cmp.meta.lap_time_s.unwrap() - rf.meta.lap_time_s.unwrap()) as f32;
    let last = *c.delta_s.last().unwrap();
    assert!((last - expected).abs() < 0.05, "delta fin={last} attendu={expected}");
    assert!(c.corners.len() >= 6, "virages détectés : {}", c.corners.len());
}

#[test]
fn detects_injected_early_braking_on_v1() {
    let s = demo_session(3, 0);
    let mut faults = clean();
    faults[0] = CornerFault { early_brake_m: 14.0, apex_speed_factor: 0.95, ..Default::default() };
    let (rt, _) = generate_lap(&clean(), 60.0, 10);
    let (ct, _) = generate_lap(&faults, 60.0, 11);
    let (mr, mc) = (meta(1), meta(2));
    let c = compare_laps(
        &LapInput { lap_id: 1, session: &s.meta, lap: &mr, trace: &rt },
        &LapInput { lap_id: 2, session: &s.meta, lap: &mc, trace: &ct },
        1.0,
    )
    .unwrap();
    let v1 = &c.corners[0];
    let brake_diff = v1.reference.brake_on_m.unwrap() - v1.compared.brake_on_m.unwrap();
    assert!((brake_diff - 14.0).abs() <= 4.0, "écart freinage mesuré {brake_diff} m (injecté 14 m)");
    let top = &c.insights[0];
    assert_eq!(top.segment.as_deref(), Some("V1"), "{:#?}", c.insights.iter().map(|i| &i.title).collect::<Vec<_>>());
    assert!(top.title.contains("précoce"), "{}", top.title);
    assert!(top.priority <= Priority::Important);
    assert_eq!(top.confidence, Confidence::High);
    // Aucun faux positif significatif ailleurs.
    assert!(c.insights.iter().filter(|i| i.segment.as_deref() != Some("V1")).all(|i| i.est_gain_s.unwrap()[1] < 0.1));
}

#[test]
fn detects_low_apex_speed_and_late_throttle() {
    let s = demo_session(3, 0);
    let mut faults = clean();
    faults[3] = CornerFault { apex_speed_factor: 0.9, ..Default::default() };
    faults[4] = CornerFault { apex_speed_factor: 1.0, late_throttle_m: 40.0, ..Default::default() };
    let (rt, _) = generate_lap(&clean(), 60.0, 20);
    let (ct, _) = generate_lap(&faults, 60.0, 21);
    let (mr, mc) = (meta(1), meta(2));
    let c = compare_laps(
        &LapInput { lap_id: 1, session: &s.meta, lap: &mr, trace: &rt },
        &LapInput { lap_id: 2, session: &s.meta, lap: &mc, trace: &ct },
        1.0,
    )
    .unwrap();
    let titles: Vec<_> = c.insights.iter().map(|i| i.title.clone()).collect();
    assert!(titles.iter().any(|t| t.starts_with("V4") && t.contains("vitesse minimale")), "{titles:#?}");
    assert!(titles.iter().any(|t| t.starts_with("V5") && (t.contains("remise des gaz") || t.contains("sortie"))), "{titles:#?}");
}

#[test]
fn disturbed_reference_triggers_warning_and_lower_confidence() {
    let s = demo_session(3, 0);
    let off = &s.laps[4]; // tour hors-piste
    let good = &s.laps[1];
    let c = compare_laps(
        &LapInput { lap_id: 4, session: &s.meta, lap: &off.meta, trace: &off.trace },
        &LapInput { lap_id: 1, session: &s.meta, lap: &good.meta, trace: &good.trace },
        1.0,
    )
    .unwrap();
    assert!(c.warnings.iter().any(|w| w.contains("référence")));
    assert!(c.insights.iter().all(|i| i.confidence < Confidence::High));
}

#[test]
fn different_track_blocks_insights() {
    let s = demo_session(3, 0);
    let mut other = s.meta.clone();
    other.track = "Autre circuit".into();
    let (a, b) = (&s.laps[1], &s.laps[2]);
    let c = compare_laps(
        &LapInput { lap_id: 1, session: &s.meta, lap: &a.meta, trace: &a.trace },
        &LapInput { lap_id: 2, session: &other, lap: &b.meta, trace: &b.trace },
        1.0,
    )
    .unwrap();
    assert!(c.insights.is_empty());
    assert!(c.warnings.iter().any(|w| w.contains("Circuit")));
}

#[test]
fn theoretical_best_is_not_slower_than_best_lap() {
    let s = demo_session(5, 0);
    let pairs: Vec<_> = s.laps.iter().map(|l| (&l.meta, &l.trace)).collect();
    let (theo, segs) = theoretical_best(&pairs, track_length_m(), 1.0).unwrap();
    let best = s.laps.iter().filter(|l| l.meta.is_clean()).filter_map(|l| l.meta.lap_time_s).fold(f64::MAX, f64::min);
    assert!(segs >= 5);
    assert!(theo <= best + 0.01 && theo > best - 1.5, "théorique={theo} meilleur={best}");
}

#[test]
fn store_then_compare_from_db() {
    let dir = tempfile::tempdir().unwrap();
    let mut st = Store::open(&dir.path().join("apex.db")).unwrap();
    let InsertOutcome::Inserted(sid) = st.insert_session(&demo_session(9, 1_760_000_000)).unwrap() else { panic!() };
    let laps = st.laps(sid).unwrap();
    let meta = st.session_meta(sid).unwrap();
    let (ta, tb) = (st.trace(laps[2].id).unwrap(), st.trace(laps[1].id).unwrap());
    let c = compare_laps(
        &LapInput { lap_id: laps[2].id, session: &meta, lap: &laps[2].meta, trace: &ta },
        &LapInput { lap_id: laps[1].id, session: &meta, lap: &laps[1].meta, trace: &tb },
        2.0,
    )
    .unwrap();
    assert!(!c.insights.is_empty());
    // Sérialisable pour l'UI.
    let json = serde_json::to_string(&c).unwrap();
    assert!(json.len() > 1000);
    drop(st);
    // Réouverture : les données survivent.
    let st = Store::open(&dir.path().join("apex.db")).unwrap();
    assert_eq!(st.list_sessions(&SessionFilter::default()).unwrap().len(), 1);
}
