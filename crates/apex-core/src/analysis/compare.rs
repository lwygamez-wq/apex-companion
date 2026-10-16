//! Comparaison de deux tours : delta par distance, virages, conseils, avertissements.

use super::corners::{corner_metrics, detect_corners, CornerConfig, CornerDef, CornerMetrics};
use super::insights::{corner_insight, DataQuality, Insight, Thresholds};
use super::resample::{resample, ResampleError, Resampled};
use crate::model::{LapMeta, LapTrace, SessionMeta};
use serde::Serialize;

pub struct LapInput<'a> {
    pub lap_id: i64,
    pub session: &'a SessionMeta,
    pub lap: &'a LapMeta,
    pub trace: &'a LapTrace,
}

#[derive(Debug, Clone, Serialize)]
pub struct CornerComparison {
    pub corner: CornerDef,
    pub reference: CornerMetrics,
    pub compared: CornerMetrics,
    pub time_delta_s: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LapSummary {
    pub lap_id: i64,
    pub number: i32,
    pub lap_time_s: Option<f64>,
    pub clean: bool,
    pub coverage: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Comparison {
    pub step_m: f32,
    pub length_m: f32,
    pub reference: LapSummary,
    pub compared: LapSummary,
    pub ref_trace: Resampled,
    pub cmp_trace: Resampled,
    /// `cmp.time(d) - ref.time(d)` : positif = le tour analysé perd du temps.
    pub delta_s: Vec<f32>,
    pub corners: Vec<CornerComparison>,
    pub insights: Vec<Insight>,
    /// Avertissements de comparabilité, affichés en tête d'écran.
    pub warnings: Vec<String>,
}

/// Détecte les incompatibilités de conditions entre deux tours.
pub fn comparability_warnings(a: &SessionMeta, b: &SessionMeta) -> Vec<String> {
    let mut w = Vec::new();
    if a.sim != b.sim {
        w.push(format!("Simulateurs différents ({} / {}) : physique non comparable.", a.sim.display_name(), b.sim.display_name()));
    }
    if a.track != b.track || a.track_config != b.track_config {
        w.push("Circuit ou configuration différents : comparaison invalide.".into());
    }
    if a.car != b.car {
        w.push(format!("Voitures différentes ({} / {}).", a.car, b.car));
    }
    match (a.weather.wet, b.weather.wet) {
        (Some(x), Some(y)) if x != y => w.push("Piste sèche contre piste humide.".into()),
        (None, _) | (_, None) => w.push("Météo inconnue pour au moins un tour : comparabilité non vérifiable.".into()),
        _ => {}
    }
    if let (Some(x), Some(y)) = (a.weather.track_temp_c, b.weather.track_temp_c) {
        if (x - y).abs() > 8.0 {
            w.push(format!("Écart de température piste de {:.0} °C.", (x - y).abs()));
        }
    }
    w
}

/// Nom donné à un virage par le pilote ou par une table de circuit : (distance de l'apex en m, nom).
pub type CornerName = (f32, String);

/// Applique les noms connus aux virages détectés (apex le plus proche, à moins de 60 m).
pub fn apply_corner_names(defs: &mut [CornerDef], names: &[CornerName]) {
    for d in defs.iter_mut() {
        if let Some((_, n)) = names
            .iter()
            .filter(|(a, _)| (a - d.apex_m).abs() <= 60.0)
            .min_by(|x, y| (x.0 - d.apex_m).abs().total_cmp(&(y.0 - d.apex_m).abs()))
        {
            d.name = n.clone();
        }
    }
}

pub fn compare_laps(rf: &LapInput, cmp: &LapInput, step_m: f32) -> Result<Comparison, ResampleError> {
    compare_laps_named(rf, cmp, step_m, &[])
}

pub fn compare_laps_named(rf: &LapInput, cmp: &LapInput, step_m: f32, names: &[CornerName]) -> Result<Comparison, ResampleError> {
    let length = rf
        .session
        .track_length_m
        .filter(|l| *l > 100.0)
        .unwrap_or_else(|| rf.trace.max_dist())
        .min(rf.trace.max_dist().max(cmp.trace.max_dist()) + 20.0);

    let r = resample(rf.trace, length, step_m)?;
    let c = resample(cmp.trace, length, step_m)?;
    let delta: Vec<f32> = r.time.iter().zip(&c.time).map(|(a, b)| b - a).collect();

    let mut warnings = comparability_warnings(rf.session, cmp.session);
    let hard_mismatch = rf.session.track != cmp.session.track
        || rf.session.track_config != cmp.session.track_config
        || rf.session.sim != cmp.session.sim;
    if !rf.lap.is_clean() {
        warnings.push(format!("Le tour de référence n°{} n'est pas un tour propre : références faussées possibles.", rf.lap.number));
    }
    if !cmp.lap.is_clean() {
        warnings.push(format!("Le tour analysé n°{} est perturbé ou invalide.", cmp.lap.number));
    }

    let q = DataQuality {
        both_clean: rf.lap.is_clean() && cmp.lap.is_clean(),
        min_coverage: r.coverage.min(c.coverage),
        min_density_per_m: r.source_density.min(c.source_density),
        has_steering: r.steer_deg.is_some() && c.steer_deg.is_some(),
        conditions_comparable: warnings.iter().all(|w| w.starts_with("Météo inconnue") || w.starts_with("Le tour")),
        repeat_count: 0,
        notes: if r.steer_deg.is_none() || c.steer_deg.is_none() {
            vec!["canal volant absent : corrections de volant non analysées".into()]
        } else {
            vec![]
        },
    };

    let mut defs = detect_corners(&r, &CornerConfig::default());
    apply_corner_names(&mut defs, names);
    let th = Thresholds::default();
    let mut corners = Vec::with_capacity(defs.len());
    let mut insights = Vec::new();
    for d in defs {
        let mr = corner_metrics(&r, &d);
        let mc = corner_metrics(&c, &d);
        if !hard_mismatch {
            if let Some(i) = corner_insight(&d, &mr, &mc, &q, &th) {
                insights.push(i);
            }
        }
        corners.push(CornerComparison { time_delta_s: mc.time_s - mr.time_s, corner: d, reference: mr, compared: mc });
    }
    insights.sort_by(|a, b| {
        a.priority.cmp(&b.priority).then(
            b.est_gain_s.map_or(0.0, |g| g[1]).total_cmp(&a.est_gain_s.map_or(0.0, |g| g[1])),
        )
    });
    if hard_mismatch {
        insights.clear();
    }

    let summary = |i: &LapInput, rs: &Resampled| LapSummary {
        lap_id: i.lap_id,
        number: i.lap.number,
        lap_time_s: i.lap.lap_time_s,
        clean: i.lap.is_clean(),
        coverage: rs.coverage,
    };
    Ok(Comparison {
        step_m,
        length_m: length,
        reference: summary(rf, &r),
        compared: summary(cmp, &c),
        ref_trace: r,
        cmp_trace: c,
        delta_s: delta,
        corners,
        insights,
        warnings,
    })
}
