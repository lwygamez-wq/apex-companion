//! Objectifs de session : un seul point travaillé à la fois, vérifié par les chiffres
//! sur la session suivante.

use super::corners::{corner_metrics, detect_corners, CornerConfig, CornerMetrics};
use super::resample::resample;
use crate::analysis::session::consistency;
use crate::model::{LapMeta, LapTrace};
use serde::{Deserialize, Serialize};

/// Tolérances : on ne juge pas au mètre près (bruit de mesure, 60 Hz ≈ 1,4 m à 300 km/h).
const TOL_M: f32 = 3.0;
const TOL_KMH: f32 = 1.0;
/// Part des tours propres qui doivent atteindre la cible.
pub const SUCCESS_RATIO: f32 = 0.6;
pub const MIN_LAPS: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GoalMetric {
    /// Commencer à freiner à `target_m` ou plus loin.
    BrakePoint { apex_m: f32, target_m: f32 },
    /// Vitesse minimale au moins `target_kmh`.
    MinSpeed { apex_m: f32, target_kmh: f32 },
    /// Vitesse de sortie au moins `target_kmh`.
    ExitSpeed { apex_m: f32, target_kmh: f32 },
    /// Remettre les gaz à `target_m` ou plus tôt.
    ThrottleOn { apex_m: f32, target_m: f32 },
    /// Écart-type des tours propres au plus `target_sd_s`.
    Consistency { target_sd_s: f32 },
    /// Au moins `target` tours propres dans la session.
    CleanLaps { target: u32 },
    /// Passer ce virage sans sortie de piste sur au moins `target_rate` des tours lancés.
    CornerClean { apex_m: f32, target_rate: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalSuggestion {
    pub title: String,
    pub metric: GoalMetric,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoalResult {
    pub laps_evaluated: usize,
    pub laps_ok: usize,
    pub achieved: bool,
    pub summary: String,
}

fn corner_check(m: &CornerMetrics, metric: &GoalMetric) -> Option<(bool, f32)> {
    match *metric {
        GoalMetric::BrakePoint { target_m, .. } => m.brake_on_m.map(|b| (b >= target_m - TOL_M, b)),
        GoalMetric::MinSpeed { target_kmh, .. } => Some((m.min_speed_kmh >= target_kmh - TOL_KMH, m.min_speed_kmh)),
        GoalMetric::ExitSpeed { target_kmh, .. } => Some((m.exit_speed_kmh >= target_kmh - TOL_KMH, m.exit_speed_kmh)),
        GoalMetric::ThrottleOn { target_m, .. } => m.throttle_on_m.map(|t| (t <= target_m + TOL_M, t)),
        _ => None,
    }
}

/// Évalue un objectif sur une session (tours propres uniquement).
pub fn evaluate(metric: &GoalMetric, laps: &[(&LapMeta, &LapTrace)], length_m: f32) -> GoalResult {
    let metas: Vec<LapMeta> = laps.iter().map(|(m, _)| (*m).clone()).collect();
    let clean: Vec<&(&LapMeta, &LapTrace)> = laps.iter().filter(|(m, _)| m.is_clean()).collect();
    match *metric {
        GoalMetric::CleanLaps { target } => {
            let n = clean.len();
            GoalResult {
                laps_evaluated: laps.iter().filter(|(m, _)| m.lap_time_s.is_some()).count(),
                laps_ok: n,
                achieved: n as u32 >= target,
                summary: format!("{n} tour(s) propre(s) sur {target} visés."),
            }
        }
        GoalMetric::Consistency { target_sd_s } => match consistency(&metas) {
            Some(c) => GoalResult {
                laps_evaluated: c.clean_laps,
                laps_ok: c.clean_laps,
                achieved: c.clean_laps >= MIN_LAPS && c.stddev_s <= target_sd_s as f64,
                summary: format!("Écart-type {:.2} s sur {} tours propres (objectif ≤ {:.2} s).", c.stddev_s, c.clean_laps, target_sd_s),
            },
            None => GoalResult { laps_evaluated: 0, laps_ok: 0, achieved: false, summary: "Pas assez de tours propres pour mesurer la régularité.".into() },
        },
        GoalMetric::CornerClean { apex_m, target_rate } => {
            // Tous les tours lancés complets comptent (y compris ceux avec sortie : c'est ce qu'on mesure).
            let flying: Vec<&(&LapMeta, &LapTrace)> = laps.iter().filter(|(m, _)| m.kind == crate::model::LapKind::Flying && m.lap_time_s.is_some()).collect();
            let n = flying.len();
            let ok = flying
                .iter()
                .filter(|(_, t)| !super::offtrack::off_track_spans(t).iter().any(|(a, b)| *b >= apex_m - 150.0 && *a <= apex_m + 200.0))
                .count();
            GoalResult {
                laps_evaluated: n,
                laps_ok: ok,
                achieved: n >= MIN_LAPS && ok as f32 >= n as f32 * target_rate,
                summary: format!("{ok}/{n} passages sans sortie (objectif {:.0} %).", target_rate * 100.0),
            }
        }
        GoalMetric::BrakePoint { apex_m, .. } | GoalMetric::MinSpeed { apex_m, .. } | GoalMetric::ExitSpeed { apex_m, .. } | GoalMetric::ThrottleOn { apex_m, .. } => {
            let rs: Vec<_> = clean.iter().filter_map(|(_, t)| resample(t, length_m, 1.0).ok()).filter(|r| r.coverage > 0.95).collect();
            // Le virage est retrouvé par la position de son apex (numérotation indépendante de la session).
            let Some(def) = rs.first().and_then(|r0| {
                detect_corners(r0, &CornerConfig::default())
                    .into_iter()
                    .filter(|d| (d.apex_m - apex_m).abs() <= 80.0)
                    .min_by(|a, b| (a.apex_m - apex_m).abs().total_cmp(&(b.apex_m - apex_m).abs()))
            }) else {
                return GoalResult { laps_evaluated: 0, laps_ok: 0, achieved: false, summary: "Virage introuvable dans cette session (pas de tour propre complet ?).".into() };
            };
            let checks: Vec<(bool, f32)> = rs.iter().filter_map(|r| corner_check(&corner_metrics(r, &def), metric)).collect();
            let ok = checks.iter().filter(|c| c.0).count();
            let n = checks.len();
            let avg = if n > 0 { checks.iter().map(|c| c.1).sum::<f32>() / n as f32 } else { 0.0 };
            let unit = match metric {
                GoalMetric::MinSpeed { .. } | GoalMetric::ExitSpeed { .. } => "km/h",
                _ => "m",
            };
            GoalResult {
                laps_evaluated: n,
                laps_ok: ok,
                achieved: n >= MIN_LAPS && ok as f32 >= n as f32 * SUCCESS_RATIO,
                summary: format!("Réussi sur {ok}/{n} tours propres (moyenne {avg:.0} {unit})."),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{demo_session, track_length_m};

    #[test]
    fn brake_point_goal_on_demo() {
        // Démo : tours 1 et 5 freinent tôt en V1 ; tours 2 et 3 freinent au bon endroit.
        let s = demo_session(3, 0);
        let laps: Vec<_> = s.laps.iter().map(|l| (&l.meta, &l.trace)).collect();
        let ref_lap = &s.laps[2];
        let r = resample(&ref_lap.trace, track_length_m(), 1.0).unwrap();
        let v1 = detect_corners(&r, &CornerConfig::default())[0].clone();
        let target = corner_metrics(&r, &v1).brake_on_m.unwrap();
        let res = evaluate(&GoalMetric::BrakePoint { apex_m: v1.apex_m, target_m: target }, &laps, track_length_m());
        assert_eq!(res.laps_evaluated, 4);
        assert_eq!(res.laps_ok, 2, "{}", res.summary);
        assert!(!res.achieved, "2/4 < 60 %");
    }

    #[test]
    fn session_level_goals() {
        let s = demo_session(3, 0);
        let laps: Vec<_> = s.laps.iter().map(|l| (&l.meta, &l.trace)).collect();
        assert!(evaluate(&GoalMetric::CleanLaps { target: 4 }, &laps, 0.0).achieved);
        assert!(!evaluate(&GoalMetric::CleanLaps { target: 5 }, &laps, 0.0).achieved);
        assert!(evaluate(&GoalMetric::Consistency { target_sd_s: 0.5 }, &laps, 0.0).achieved);
        assert!(!evaluate(&GoalMetric::Consistency { target_sd_s: 0.1 }, &laps, 0.0).achieved);
    }
}
