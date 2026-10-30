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
    /// Écart de trajectoire à l'apex (m) : positif = plus à l'intérieur que la référence.
    pub apex_offset_m: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LapSummary {
    pub lap_id: i64,
    pub number: i32,
    pub lap_time_s: Option<f64>,
    pub clean: bool,
    pub coverage: f32,
    /// Pilote du tour quand il ne s'agit pas d'un tour du pilote (référence externe).
    pub driver: Option<String>,
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
    /// Écart latéral de la trajectoire analysée par rapport à la référence, à distance égale (m).
    /// Positif = à gauche de la référence (sens de la marche). `None` sans position GPS.
    pub line_offset_m: Option<Vec<f32>>,
    /// Portions hors piste (m depuis la ligne) du tour de référence et du tour analysé.
    pub ref_off_track_m: Vec<(f32, f32)>,
    pub cmp_off_track_m: Vec<(f32, f32)>,
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
    // Deux tours de la même session : mêmes conditions, inutile d'alerter sur une météo inconnue.
    if a.source_fingerprint == b.source_fingerprint {
        return w;
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

/// Direction de la trajectoire de référence au point `i` (vecteur unitaire), sur ±5 m.
fn tangent(x: &[f32], y: &[f32], i: usize, k: usize) -> Option<(f32, f32)> {
    let a = i.saturating_sub(k);
    let b = (i + k).min(x.len() - 1);
    let (dx, dy) = (x[b] - x[a], y[b] - y[a]);
    let n = (dx * dx + dy * dy).sqrt();
    (n > 1e-3).then(|| (dx / n, dy / n))
}

/// Écart latéral signé (gauche positive) entre deux trajectoires rééchantillonnées sur la même grille.
///
/// La distance au tour vient de la position projetée sur la ligne médiane du circuit (LapDistPct),
/// donc à distance égale les deux voitures sont dans la même section de piste : l'écart mesuré
/// perpendiculairement à la trajectoire de référence est un écart de trajectoire.
pub fn line_offset(r: &Resampled, c: &Resampled) -> Option<Vec<f32>> {
    let (rx, ry, cx, cy) = (r.x.as_ref()?, r.y.as_ref()?, c.x.as_ref()?, c.y.as_ref()?);
    let n = rx.len().min(cx.len());
    if n < 3 {
        return None;
    }
    let k = (5.0 / r.step_m).ceil().max(1.0) as usize;
    let mut last = (1.0, 0.0);
    Some(
        (0..n)
            .map(|i| {
                let (tx, ty) = tangent(rx, ry, i, k).unwrap_or(last);
                last = (tx, ty);
                // Normale gauche = tangente tournée de +90°.
                (cx[i] - rx[i]) * -ty + (cy[i] - ry[i]) * tx
            })
            .collect(),
    )
}

/// +1 virage à gauche, -1 à droite (variation de cap de la référence entre l'entrée et la sortie).
pub(crate) fn turn_sign(r: &Resampled, d: &CornerDef) -> Option<f32> {
    let (x, y) = (r.x.as_ref()?, r.y.as_ref()?);
    let k = (5.0 / r.step_m).ceil().max(1.0) as usize;
    let (ax, ay) = tangent(x, y, r.idx(d.entry_m), k)?;
    let (bx, by) = tangent(x, y, r.idx(d.apex_m), k)?;
    let z = ax * by - ay * bx;
    (z.abs() > 0.05).then_some(z.signum())
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

    let offset = line_offset(&r, &c);
    let mut defs = detect_corners(&r, &CornerConfig::default());
    apply_corner_names(&mut defs, names);
    let th = Thresholds::default();
    let mut corners = Vec::with_capacity(defs.len());
    let mut insights = Vec::new();
    for d in defs {
        let mut mr = corner_metrics(&r, &d);
        let mut mc = corner_metrics(&c, &d);
        // Vitesses de sortie comparées au même endroit (le plus tôt des deux points de mesure).
        let at = mr.exit_at_m.min(mc.exit_at_m);
        mr.exit_speed_kmh = r.speed_kmh[r.idx(at)];
        mc.exit_speed_kmh = c.speed_kmh[c.idx(at)];
        mr.exit_at_m = at;
        mc.exit_at_m = at;
        if !hard_mismatch {
            if let Some(i) = corner_insight(&d, &mr, &mc, &q, &th) {
                insights.push(i);
            }
        }
        let apex_offset_m = match (&offset, turn_sign(&r, &d)) {
            (Some(o), Some(s)) => o.get(r.idx(d.apex_m)).map(|v| v * s),
            _ => None,
        };
        corners.push(CornerComparison { time_delta_s: mc.time_s - mr.time_s, apex_offset_m, corner: d, reference: mr, compared: mc });
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
        driver: None,
    };
    Ok(Comparison {
        step_m,
        length_m: length,
        reference: summary(rf, &r),
        compared: summary(cmp, &c),
        ref_trace: r,
        cmp_trace: c,
        delta_s: delta,
        line_offset_m: offset,
        ref_off_track_m: super::offtrack::off_track_spans(rf.trace),
        cmp_off_track_m: super::offtrack::off_track_spans(cmp.trace),
        corners,
        insights,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cercle parcouru dans le sens trigonométrique (virage à gauche permanent), rayon `rad`,
    /// échantillonné par angle pour que deux rayons différents restent dans la même section.
    fn circle(rad: f32) -> Resampled {
        let n = 600;
        let ang = |i: usize| i as f32 / n as f32 * std::f32::consts::PI;
        Resampled {
            step_m: 1.0,
            dist: (0..n).map(|i| i as f32).collect(),
            time: (0..n).map(|i| i as f32 * 0.02).collect(),
            speed_kmh: vec![150.0; n],
            throttle: vec![1.0; n],
            brake: vec![0.0; n],
            gear: None,
            steer_deg: None,
            x: Some((0..n).map(|i| rad * ang(i).cos()).collect()),
            y: Some((0..n).map(|i| rad * ang(i).sin()).collect()),
            coverage: 1.0,
            source_density: 1.0,
        }
    }

    #[test]
    fn lateral_offset_sign_and_magnitude() {
        let r = circle(100.0);
        // Rayon plus petit dans un virage à gauche = plus à gauche (intérieur).
        let o = line_offset(&r, &circle(97.5)).unwrap();
        assert!(o[10..590].iter().all(|v| (v - 2.5).abs() < 0.05), "{:?}", o[300]);
        let o = line_offset(&r, &circle(101.0)).unwrap();
        assert!((o[300] + 1.0).abs() < 0.05);
        let d = CornerDef { id: 1, name: "V1".into(), search_start_m: 0.0, entry_m: 100.0, apex_m: 300.0, exit_m: 500.0 };
        assert_eq!(turn_sign(&r, &d), Some(1.0));
    }

    #[test]
    fn no_offset_without_positions() {
        let mut r = circle(100.0);
        r.x = None;
        assert!(line_offset(&r, &circle(99.0)).is_none());
    }
}
