//! Statistiques de session : meilleur tour, tour théorique, régularité.

use super::corners::{detect_corners, CornerConfig};
use super::resample::{resample, Resampled};
use crate::model::{LapMeta, LapTrace};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Consistency {
    pub clean_laps: usize,
    pub mean_s: f64,
    pub stddev_s: f64,
    /// Écart-type rapporté à la moyenne (%).
    pub cv_percent: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionStats {
    pub best_lap_index: Option<usize>,
    pub best_lap_s: Option<f64>,
    /// Somme des meilleurs segments (segments = découpage entre virages détectés).
    pub theoretical_best_s: Option<f64>,
    pub theoretical_segments: usize,
    pub consistency: Option<Consistency>,
}

/// Meilleur tour propre (index dans `laps`).
pub fn best_clean_lap(laps: &[LapMeta]) -> Option<usize> {
    laps.iter()
        .enumerate()
        .filter(|(_, l)| l.is_clean())
        .min_by(|a, b| a.1.lap_time_s.unwrap().total_cmp(&b.1.lap_time_s.unwrap()))
        .map(|(i, _)| i)
}

/// Régularité sur les tours propres à moins de 107 % du meilleur (écarte les tours ratés).
pub fn consistency(laps: &[LapMeta]) -> Option<Consistency> {
    let best = laps[best_clean_lap(laps)?].lap_time_s?;
    let times: Vec<f64> = laps
        .iter()
        .filter(|l| l.is_clean())
        .filter_map(|l| l.lap_time_s)
        .filter(|t| *t <= best * 1.07)
        .collect();
    if times.len() < 2 {
        return None;
    }
    let n = times.len() as f64;
    let mean = times.iter().sum::<f64>() / n;
    let var = times.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / (n - 1.0);
    Some(Consistency { clean_laps: times.len(), mean_s: mean, stddev_s: var.sqrt(), cv_percent: var.sqrt() / mean * 100.0 })
}

/// Tour théorique = somme des meilleurs temps par segment sur les tours propres.
/// Segments bornés par les milieux entre apex consécutifs (découpage stable d'un tour à l'autre).
pub fn theoretical_best(laps: &[(&LapMeta, &LapTrace)], length_m: f32, step_m: f32) -> Option<(f64, usize)> {
    let clean: Vec<Resampled> = laps
        .iter()
        .filter(|(m, _)| m.is_clean())
        .filter_map(|(_, t)| resample(t, length_m, step_m).ok())
        .filter(|r| r.coverage > 0.97)
        .collect();
    if clean.len() < 2 {
        return None;
    }
    let reference = &clean[0];
    let corners = detect_corners(reference, &CornerConfig::default());
    let mut bounds = vec![0usize];
    for w in corners.windows(2) {
        bounds.push(reference.idx((w[0].apex_m + w[1].apex_m) / 2.0));
    }
    bounds.push(reference.len() - 1);
    bounds.dedup();
    let mut total = 0f64;
    for b in bounds.windows(2) {
        let best = clean.iter().map(|r| (r.time[b[1]] - r.time[b[0]]) as f64).fold(f64::MAX, f64::min);
        total += best;
    }
    // Complément ligne→fin de grille (dernier mètre) : on reprend l'écart du meilleur tour.
    let tail = laps
        .iter()
        .filter(|(m, _)| m.is_clean())
        .filter_map(|(m, t)| resample(t, length_m, step_m).ok().map(|r| m.lap_time_s.unwrap() - *r.time.last().unwrap() as f64))
        .fold(f64::MAX, f64::min);
    Some((total + tail.max(0.0), bounds.len() - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn lap(n: i32, t: Option<f64>, kind: LapKind, off: bool) -> LapMeta {
        LapMeta {
            number: n,
            kind,
            lap_time_s: t,
            sector_times_s: vec![],
            flags: LapFlags { off_track: off, ..Default::default() },
            validity: if off { Validity::Invalid } else { Validity::Valid },
            validity_source: ValiditySource::Derived,
            fuel_start_l: None,
            fuel_used_l: None,
        }
    }

    #[test]
    fn best_lap_ignores_invalid_and_out_laps() {
        let laps = vec![
            lap(1, Some(80.0), LapKind::OutLap, false),
            lap(2, Some(90.0), LapKind::Flying, false),
            lap(3, Some(88.0), LapKind::Flying, true),
            lap(4, Some(89.5), LapKind::Flying, false),
        ];
        assert_eq!(best_clean_lap(&laps), Some(3));
        let c = consistency(&laps).unwrap();
        assert_eq!(c.clean_laps, 2);
        assert!((c.mean_s - 89.75).abs() < 1e-9);
    }
}
