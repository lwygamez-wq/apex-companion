//! Mémoire d'essais setup par (voiture, circuit) : chaque changement de réglage entre deux séances
//! consécutives, son effet mesuré, la file des prochains essais et le setup recommandé.
//!
//! Cadre (décision du 12 nov.) : pas de générateur de setup, pas de valeur hors des crans du jeu, un
//! réglage à la fois, effet mesuré. « Recommandé » = le setup de base + les seuls changements validés
//! par une mesure : jamais un optimum garanti.
//!
//! - Un changement n'est **attribuable** que si un seul réglage (ou la paire de roues d'un même essieu)
//!   a changé entre les deux séances. Sinon l'effet est noté mais « non attribuable ».
//! - Un effet est **validé** s'il dépasse 2 erreurs types de la différence et 1 °C (règle de
//!   `setup_trial`), dans le sens attendu par le réglage.
//! - La file d'essais classe les constats du dernier diagnostic par netteté (écart / dispersion) et par
//!   répétition (nombre de séances où le même constat revient). Un essai déjà fait sans effet mesuré
//!   descend en fin de file avec la mention « déjà essayé ».

use super::setup_diag::SetupDiagnosis;
use super::setup_trial::{axle_effect, candidates, side_of, Trial, TrialMetric};
use serde::Serialize;

/// Une séance de la mémoire : diagnostic (avec le setup enregistré), temps et consommation.
#[derive(Debug, Clone)]
pub struct SessionSetup {
    pub session_id: i64,
    pub started_at: i64,
    pub diag: SetupDiagnosis,
    /// Médiane des tours utilisables (s) et leur nombre.
    pub lap_median_s: Option<f64>,
    pub laps: usize,
    /// Énergie (ou carburant) par tour, médiane, et son unité (« % » ou « L »).
    pub per_lap: Option<f32>,
    pub per_lap_unit: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Change {
    pub key: String,
    pub label: String,
    pub from: String,
    pub to: String,
    pub steps: i32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MeasuredEffect {
    /// « Pneus avant, écart intérieur / extérieur » …
    pub what: String,
    pub before: f32,
    pub after: f32,
    pub effect: f32,
    pub threshold: f32,
    pub significant: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    /// Un seul réglage changé, effet mesuré dans le sens attendu.
    Validated,
    /// Un seul réglage changé, pas d'effet au-delà de la dispersion (ou dans l'autre sens).
    NotConclusive,
    /// Plusieurs réglages changés à la fois : aucun effet attribuable.
    NotAttributable,
    /// Setup identique entre les deux séances.
    NoChange,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemoryEntry {
    pub from_session: i64,
    pub to_session: i64,
    /// Début des deux séances (horodatage Unix), pour l'affichage.
    pub from_started_at: i64,
    pub to_started_at: i64,
    pub changes: Vec<Change>,
    pub effects: Vec<MeasuredEffect>,
    pub status: EntryStatus,
    pub lap_before_s: Option<f64>,
    pub lap_after_s: Option<f64>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QueueItem {
    pub trial: Trial,
    /// Écart / dispersion du constat sur la dernière séance.
    pub sharpness: f32,
    /// Séances (sur celles qui ont des mesures pneus) où le même constat revient (même essieu, même sens, ≥ 3 °C).
    pub repeats: usize,
    pub sessions_measured: usize,
    /// Déjà essayé sans effet mesuré (texte), le cas échéant.
    pub tried: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Recommended {
    /// Séance dont le setup sert de base.
    pub base_session: i64,
    /// Changements validés, avec leur preuve.
    pub changes: Vec<(Change, String)>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SetupMemory {
    pub entries: Vec<MemoryEntry>,
    pub queue: Vec<QueueItem>,
    pub skipped: Vec<String>,
    pub recommended: Option<Recommended>,
    /// Consommation mesurée par setup : (séance, résumé du setup, consommation par tour, tours, unité).
    pub consumption: Vec<(i64, String, f32, usize, &'static str)>,
    pub missing: Vec<String>,
}

fn fr1(v: f32) -> String {
    format!("{v:.1}").replace('.', ",")
}

fn changes(a: &SetupDiagnosis, b: &SetupDiagnosis) -> Vec<Change> {
    b.setup
        .iter()
        .filter_map(|rb| {
            let ra = a.setup.iter().find(|r| r.key == rb.key)?;
            (ra.step != rb.step).then(|| Change { key: rb.key.clone(), label: rb.label.clone(), from: ra.display.clone(), to: rb.display.clone(), steps: rb.step - ra.step })
        })
        .collect()
}

/// Les changements ne touchent qu'un réglage (une roue, ou les deux roues d'un même essieu).
fn single_setting(c: &[Change]) -> bool {
    let base = |k: &str| k.split("-W_").next().unwrap_or(k).to_string() + if k.contains("_FL") || k.contains("_FR") { "|AV" } else if k.contains("_RL") || k.contains("_RR") { "|AR" } else { "" };
    let mut keys: Vec<String> = c.iter().map(|x| base(&x.key)).collect();
    keys.sort();
    keys.dedup();
    keys.len() == 1
}

const METRICS: [(usize, &str, TrialMetric, &str, &str); 4] = [
    (0, "avant", TrialMetric::InnerMinusOuter, "WM_CAMBER", "écart intérieur / extérieur"),
    (1, "arrière", TrialMetric::InnerMinusOuter, "WM_CAMBER", "écart intérieur / extérieur"),
    (0, "avant", TrialMetric::MiddleMinusEdges, "WM_PRESSURE", "écart milieu / bords"),
    (1, "arrière", TrialMetric::MiddleMinusEdges, "WM_PRESSURE", "écart milieu / bords"),
];

/// Sens attendu de l'effet d'un changement de carrossage ou de pression sur la métrique de l'essieu :
/// carrossage moins négatif → intérieur moins chaud (écart intérieur − extérieur baisse) ; pression plus
/// basse → milieu moins chaud (écart milieu − bords baisse). `None` : réglage sans sens connu ici.
fn expected_sign(c: &Change, metric: TrialMetric) -> Option<f32> {
    let (x, y) = (crate::setup::number(&c.from)?, crate::setup::number(&c.to)?);
    match metric {
        TrialMetric::InnerMinusOuter if c.key.starts_with("WM_CAMBER") => Some(if y.abs() < x.abs() { -1.0 } else { 1.0 }),
        TrialMetric::MiddleMinusEdges if c.key.starts_with("WM_PRESSURE") => Some(if y < x { -1.0 } else { 1.0 }),
        _ => None,
    }
}

/// Construit la mémoire à partir des séances d'une même voiture sur un même circuit (ordre chronologique).
pub fn build(sessions: &[SessionSetup]) -> SetupMemory {
    let mut s: Vec<&SessionSetup> = sessions.iter().filter(|x| !x.diag.setup.is_empty()).collect();
    s.sort_by_key(|x| x.started_at);
    let mut missing = vec![];
    if s.len() < sessions.len() {
        missing.push(format!("{} séance(s) sans setup enregistré (avant Apex 0.36) : hors mémoire.", sessions.len() - s.len()));
    }
    let mut entries = vec![];
    for w in s.windows(2) {
        let (a, b) = (w[0], w[1]);
        let ch = changes(&a.diag, &b.diag);
        let mut effects = vec![];
        let mut validated = false;
        for (axle, name, metric, key, what) in METRICS {
            let Some(e) = axle_effect(&a.diag, &b.diag, axle, metric) else { continue };
            // Seuls les effets d'un réglage qui touche cet essieu et cette métrique sont « attendus ».
            let axle_keys = if axle == 0 { ["_FL", "_FR"] } else { ["_RL", "_RR"] };
            let touched: Vec<&Change> = ch.iter().filter(|c| c.key.starts_with(key) && axle_keys.iter().any(|k| c.key.ends_with(k))).collect();
            let expected = touched.first().and_then(|c| expected_sign(c, metric));
            if e.significant && expected.is_some_and(|sg| sg * e.effect > 0.0) {
                validated = true;
            }
            effects.push(MeasuredEffect { what: format!("Pneus {name}, {what}"), before: e.before.0, after: e.after.0, effect: e.effect, threshold: e.threshold, significant: e.significant });
        }
        let status = if ch.is_empty() {
            EntryStatus::NoChange
        } else if !single_setting(&ch) {
            EntryStatus::NotAttributable
        } else if validated {
            EntryStatus::Validated
        } else {
            EntryStatus::NotConclusive
        };
        let sig: Vec<String> = effects.iter().filter(|e| e.significant).map(|e| format!("{} {} → {} °C", e.what, fr1(e.before), fr1(e.after))).collect();
        let text = match status {
            EntryStatus::NoChange => "Même setup : référence de dispersion.".to_string(),
            EntryStatus::NotAttributable => format!("{} réglages changés à la fois : aucun effet attribuable{}.", ch.len(), if sig.is_empty() { String::new() } else { format!(" (écarts mesurés : {})", sig.join(" ; ")) }),
            EntryStatus::Validated => format!("{} : effet mesuré ({}).", ch[0].label, sig.join(" ; ")),
            EntryStatus::NotConclusive => format!("{} : pas d'effet au-delà de la dispersion, non concluant.", ch[0].label),
        };
        entries.push(MemoryEntry { from_session: a.session_id, to_session: b.session_id, from_started_at: a.started_at, to_started_at: b.started_at, changes: ch, effects, status, lap_before_s: a.lap_median_s, lap_after_s: b.lap_median_s, text });
    }

    // File d'essais d'après la dernière séance mesurée.
    let (mut queue, mut skipped) = (vec![], vec![]);
    if let Some(last) = s.last() {
        match candidates(&last.diag) {
            Ok((cands, sk)) => {
                skipped = sk;
                let measured: Vec<&&SessionSetup> = s.iter().filter(|x| !x.diag.tyres.is_empty()).collect();
                for (sharpness, t) in cands {
                    let axle = if t.axle == "avant" { 0 } else { 1 };
                    let repeats = measured.iter().filter(|x| side_of(&x.diag, axle, t.metric).is_some_and(|(v, _, _)| v.abs() >= 3.0 && v.signum() == t.measured.signum())).count();
                    let tried = entries
                        .iter()
                        .filter(|e| e.status == EntryStatus::NotConclusive && e.changes.iter().any(|c| t.keys.contains(&c.key)))
                        .map(|e| format!("déjà essayé entre les séances {} et {} : {}", e.from_session, e.to_session, e.text))
                        .next();
                    queue.push(QueueItem { trial: t, sharpness, repeats, sessions_measured: measured.len(), tried });
                }
                // Plus net et plus répété d'abord ; un essai déjà fait sans effet en dernier.
                queue.sort_by(|a, b| a.tried.is_some().cmp(&b.tried.is_some()).then((b.repeats as f32 * b.sharpness).total_cmp(&(a.repeats as f32 * a.sharpness))));
            }
            Err(why) => missing.push(why),
        }
    }

    // Setup recommandé : base = setup de la dernière séance ; changements validés gardés s'ils y sont encore.
    let recommended = s.last().map(|last| {
        let kept: Vec<(Change, String)> = entries
            .iter()
            .filter(|e| e.status == EntryStatus::Validated)
            .flat_map(|e| e.changes.iter().map(move |c| (c.clone(), format!("séances {} → {} : {}", e.from_session, e.to_session, e.text))))
            .filter(|(c, _)| last.diag.setup.iter().any(|r| r.key == c.key && r.display == c.to))
            .collect();
        let text = if kept.is_empty() {
            "Aucun changement validé par une mesure : le setup recommandé est ton setup actuel. Le prochain essai de la file dira si un réglage fait mieux.".to_string()
        } else {
            format!("Ton setup actuel, dont {} changement{} validé{} par une mesure.", kept.len(), if kept.len() > 1 { "s" } else { "" }, if kept.len() > 1 { "s" } else { "" })
        };
        Recommended { base_session: last.session_id, changes: kept, text }
    });

    let consumption = s
        .iter()
        .filter_map(|x| {
            let wing = x.diag.setup.iter().find(|r| r.key.contains("REARWING") || r.label.to_lowercase().contains("aileron")).map(|r| format!("aileron {}", r.display));
            Some((x.session_id, wing.unwrap_or_else(|| format!("{} réglages", x.diag.setup.len())), x.per_lap?, x.laps, x.per_lap_unit))
        })
        .collect();
    SetupMemory { entries, queue, skipped, recommended, consumption, missing }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::setup_diag::{SetupRow, Stat, TyreDiag};

    // Diagnostics SYNTHÉTIQUES (pas de setup réel, encore moins payant).
    fn row(key: &str, label: &str, step: i32, display: &str) -> SetupRow {
        SetupRow { key: key.into(), label: label.into(), group: "x".into(), step, min: 0, max: 20, display: display.into() }
    }
    fn st(v: f32) -> Option<Stat> {
        Some(Stat { value: v, unit: "°C", laps: 10, spread: 1.5 })
    }
    fn tyre(io: f32) -> TyreDiag {
        TyreDiag { wheel: "x", outer: None, middle: None, inner: None, inner_minus_outer: st(io), middle_minus_edges: st(0.5), carcass: None, inner_layer: None, game_scale: None, pressure_hot: None, pressure_set: None }
    }
    fn session(id: i64, rear_camber: (i32, &str), wing: (i32, &str), io_rear: f32) -> SessionSetup {
        let diag = SetupDiagnosis {
            laps_used: 10,
            tyres: vec![tyre(1.0), tyre(1.0), tyre(io_rear), tyre(io_rear)],
            setup: vec![
                row("WM_CAMBER-W_RL", "Carrossage ArG", rear_camber.0, rear_camber.1),
                row("WM_CAMBER-W_RR", "Carrossage ArD", rear_camber.0, rear_camber.1),
                row("WM_CAMBER-W_FL", "Carrossage AvG", 10, "-1.5 deg"),
                row("WM_CAMBER-W_FR", "Carrossage AvD", 10, "-1.5 deg"),
                row("VM_REARWING", "Aileron arrière", wing.0, wing.1),
            ],
            ..Default::default()
        };
        SessionSetup { session_id: id, started_at: id * 1000, diag, lap_median_s: Some(98.0), laps: 10, per_lap: Some(4.3), per_lap_unit: "%" }
    }

    #[test]
    fn one_setting_changed_with_a_clear_effect_is_validated_and_kept() {
        // Carrossage AR −1,0 → −0,8° (moins négatif) : intérieur − extérieur 7,7 → 4,0 °C (seuil ≈ 1 °C).
        let a = session(1, (10, "-1.0 deg"), (6, "P6"), 7.7);
        let b = session(2, (12, "-0.8 deg"), (6, "P6"), 4.0);
        let m = build(&[a, b]);
        assert_eq!(m.entries.len(), 1);
        let e = &m.entries[0];
        assert_eq!(e.status, EntryStatus::Validated, "{}", e.text);
        assert_eq!(e.changes.len(), 2, "les deux roues de l'essieu");
        let r = m.recommended.unwrap();
        assert_eq!(r.base_session, 2);
        assert_eq!(r.changes.len(), 2);
        assert!(r.changes[0].1.contains("effet mesuré"), "{:?}", r.changes);
    }

    #[test]
    fn several_settings_changed_are_never_attributed() {
        // Monza 18 nov. en version synthétique : carrossage AR ET aileron changés.
        let a = session(1, (10, "-1.0 deg"), (6, "P6"), 7.7);
        let b = session(2, (12, "-0.8 deg"), (4, "P4"), 4.0);
        let m = build(&[a, b]);
        assert_eq!(m.entries[0].status, EntryStatus::NotAttributable);
        assert!(m.recommended.unwrap().changes.is_empty(), "rien n'est présenté comme acquis");
    }

    #[test]
    fn a_change_without_effect_is_not_conclusive_and_its_trial_goes_last() {
        let a = session(1, (10, "-1.0 deg"), (6, "P6"), 7.7);
        let b = session(2, (12, "-0.8 deg"), (6, "P6"), 7.6);
        let m = build(&[a, b]);
        assert_eq!(m.entries[0].status, EntryStatus::NotConclusive);
        // La file propose encore le carrossage AR (constat répété sur 2 séances), marqué « déjà essayé ».
        let q = m.queue.iter().find(|q| q.trial.label == "Carrossage AR").unwrap();
        assert_eq!(q.repeats, 2);
        assert!(q.tried.as_deref().unwrap().starts_with("déjà essayé"));
        assert_eq!(m.queue.last().unwrap().trial.label, "Carrossage AR");
        assert_eq!(m.consumption.len(), 2);
    }
}
