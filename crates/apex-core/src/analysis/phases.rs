//! Temps perdu par phase de virage : freinage, entrée, milieu, sortie.
//!
//! Les bornes sont des distances communes aux deux tours (fixées sur la référence), donc chaque phase est un écart de chrono exact entre deux
//! points du circuit, pas une estimation : `delta(fin) - delta(début)`.
//!
//! - Freinage : de l'entrée de la fenêtre du virage (30 m avant le levé de la référence) au
//!   relâchement du frein par la référence.
//! - Entrée : du relâchement à la vitesse minimale de la référence.
//! - Milieu : de la vitesse minimale à la remise des gaz de la référence.
//! - Sortie : de la remise des gaz à la fin de la fenêtre (ligne droite suivante comprise).
//!
//! La somme des phases est l'écart du virage (`CornerComparison::time_delta_s`).

use super::corners::{CornerDef, CornerMetrics, BRAKE_ON};
use super::resample::Resampled;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CornerPhase {
    Freinage,
    Entree,
    Milieu,
    Sortie,
}

impl CornerPhase {
    pub const ALL: [CornerPhase; 4] = [CornerPhase::Freinage, CornerPhase::Entree, CornerPhase::Milieu, CornerPhase::Sortie];

    /// Libellé court (« freinage », « entrée »…).
    pub fn label(self) -> &'static str {
        match self {
            CornerPhase::Freinage => "freinage",
            CornerPhase::Entree => "entrée",
            CornerPhase::Milieu => "milieu",
            CornerPhase::Sortie => "sortie",
        }
    }

    /// Libellé prononçable (« au freinage », « à l'entrée »…).
    pub fn spoken(self) -> &'static str {
        match self {
            CornerPhase::Freinage => "au freinage",
            CornerPhase::Entree => "à l'entrée",
            CornerPhase::Milieu => "au milieu du virage",
            CornerPhase::Sortie => "en sortie",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PhaseLoss {
    pub phase: CornerPhase,
    pub range_m: [f32; 2],
    /// Positif = le tour analysé perd du temps dans cette phase.
    pub delta_s: f32,
}

/// Fin du freinage d'un tour : premier point après le début du freinage où la pédale repasse sous le
/// seuil, au plus tard à `limit`.
fn brake_release(r: &Resampled, from_m: f32, limit_m: f32) -> f32 {
    let (a, b) = (r.idx(from_m), r.idx(limit_m));
    (a..=b).find(|&i| r.brake[i] < BRAKE_ON).map_or(limit_m, |i| r.dist[i])
}

/// Découpe le virage `d` en phases et mesure le temps perdu dans chacune.
///
/// `floor_m` : fin de la fenêtre du virage précédent (une phase ne remonte pas avant).
/// `delta` : `cmp.time(d) - ref.time(d)` sur la grille de `r`.
pub fn corner_phases(r: &Resampled, d: &CornerDef, mr: &CornerMetrics, delta: &[f32], floor_m: f32) -> Vec<PhaseLoss> {
    let braked = mr.brake_peak >= 0.2 && mr.brake_on_m.is_some_and(|b| b <= d.apex_m);
    // Même fenêtre que l'écart du virage (`time_delta_s`) : la somme des phases lui est égale.
    let start = d.entry_m.max(floor_m).min(d.apex_m);
    // Relâchement : celui de la référence ; sans freinage de la référence, pas de phase de freinage.
    let release = if braked { brake_release(r, mr.brake_on_m.unwrap().max(start), d.apex_m) } else { start };
    let release = release.clamp(start, d.apex_m);
    let throttle = mr.throttle_on_m.unwrap_or(d.exit_m).clamp(d.apex_m, d.exit_m);
    let bounds = [start, release, d.apex_m, throttle, d.exit_m];
    let at = |m: f32| delta.get(r.idx(m)).copied().unwrap_or(0.0);
    CornerPhase::ALL
        .iter()
        .enumerate()
        .filter(|(k, _)| bounds[k + 1] - bounds[*k] >= r.step_m)
        .map(|(k, &phase)| PhaseLoss { phase, range_m: [bounds[k], bounds[k + 1]], delta_s: at(bounds[k + 1]) - at(bounds[k]) })
        .collect()
}

/// Phase où le tour perd le plus, si la perte dépasse `min_s`.
pub fn worst_phase(phases: &[PhaseLoss], min_s: f32) -> Option<&PhaseLoss> {
    phases.iter().filter(|p| p.delta_s >= min_s).max_by(|a, b| a.delta_s.total_cmp(&b.delta_s))
}

/// Médiane par phase sur plusieurs tours (même virage, mêmes bornes de référence).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PhaseStat {
    pub phase: CornerPhase,
    /// Perte médiane par tour (s) ; négatif = tu y gagnes en général.
    pub median_s: f32,
    /// Tours où la perte dépasse 0,03 s.
    pub laps_losing: usize,
    pub laps: usize,
}

pub fn phase_stats(per_lap: &[Vec<PhaseLoss>]) -> Vec<PhaseStat> {
    CornerPhase::ALL
        .iter()
        .filter_map(|&phase| {
            let mut v: Vec<f32> = per_lap.iter().filter_map(|l| l.iter().find(|p| p.phase == phase).map(|p| p.delta_s)).collect();
            if v.is_empty() {
                return None;
            }
            v.sort_by(f32::total_cmp);
            let n = v.len();
            let median_s = if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 };
            Some(PhaseStat { phase, median_s, laps_losing: v.iter().filter(|x| **x > 0.03).count(), laps: n })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize) -> Resampled {
        Resampled {
            step_m: 1.0,
            dist: (0..n).map(|i| i as f32).collect(),
            time: (0..n).map(|i| i as f32 * 0.02).collect(),
            speed_kmh: vec![150.0; n],
            throttle: vec![1.0; n],
            brake: vec![0.0; n],
            gear: None,
            steer_deg: None,
            x: None,
            y: None,
            coverage: 1.0,
            source_density: 1.0,
        }
    }

    fn metrics(brake_on: Option<f32>, throttle_on: Option<f32>) -> CornerMetrics {
        CornerMetrics {
            corner_id: 1,
            time_s: 0.0,
            brake_on_m: brake_on,
            brake_peak: if brake_on.is_some() { 0.9 } else { 0.0 },
            min_speed_kmh: 80.0,
            min_speed_at_m: 300.0,
            throttle_on_m: throttle_on,
            exit_speed_kmh: 150.0,
            exit_at_m: 500.0,
            throttle_ramp_s: None,
            steer_reversals: None,
        }
    }

    const D: CornerDef = CornerDef { id: 1, name: String::new(), search_start_m: 50.0, entry_m: 180.0, apex_m: 300.0, exit_m: 500.0 };

    /// Référence : freine de 200 à 260 m, corde à 300, gaz à 340 (fenêtre dès 180 m). Le tour analysé
    /// freine plus tôt (perd 0,15 s entre 180 et 260), porte moins de vitesse (0,05 s jusqu'à la corde),
    /// remet les gaz tard (0,10 s) et sort pareil.
    #[test]
    fn splits_loss_by_phase_with_exact_bounds() {
        let mut r = grid(600);
        for i in 200..260 {
            r.brake[i] = 0.8;
        }
        let mut delta = vec![0.0f32; 600];
        for (i, v) in delta.iter_mut().enumerate() {
            *v = match i {
                0..=180 => 0.0,
                181..=260 => (i - 180) as f32 / 80.0 * 0.15,
                261..=300 => 0.15 + (i - 260) as f32 / 40.0 * 0.05,
                301..=340 => 0.20 + (i - 300) as f32 / 40.0 * 0.10,
                _ => 0.30,
            };
        }
        let p = corner_phases(&r, &D, &metrics(Some(200.0), Some(340.0)), &delta, 0.0);
        let got: Vec<(CornerPhase, [f32; 2], f32)> = p.iter().map(|x| (x.phase, x.range_m, (x.delta_s * 1000.0).round() / 1000.0)).collect();
        assert_eq!(
            got,
            vec![
                (CornerPhase::Freinage, [180.0, 260.0], 0.15),
                (CornerPhase::Entree, [260.0, 300.0], 0.05),
                (CornerPhase::Milieu, [300.0, 340.0], 0.10),
                (CornerPhase::Sortie, [340.0, 500.0], 0.0),
            ]
        );
        assert_eq!(worst_phase(&p, 0.03).unwrap().phase, CornerPhase::Freinage);
        // La somme des phases = l'écart sur toute la fenêtre.
        assert!((p.iter().map(|x| x.delta_s).sum::<f32>() - 0.30).abs() < 1e-4);
    }

    /// Virage pris sans frein par la référence : pas de phase de freinage ; la fenêtre ne remonte pas dans le virage précédent.
    #[test]
    fn lift_corner_has_no_braking_phase_and_respects_floor() {
        let r = grid(600);
        let delta: Vec<f32> = (0..600).map(|i| if i > 250 { 0.08 } else { 0.0 }).collect();
        let p = corner_phases(&r, &D, &metrics(None, Some(320.0)), &delta, 200.0);
        assert_eq!(p[0].phase, CornerPhase::Entree);
        assert_eq!(p[0].range_m, [200.0, 300.0]);
        assert!((p[0].delta_s - 0.08).abs() < 1e-6);
        assert!(worst_phase(&p, 0.1).is_none());
    }

    #[test]
    fn stats_take_the_median_per_phase() {
        let lap = |f: f32, s: f32| vec![PhaseLoss { phase: CornerPhase::Freinage, range_m: [0.0, 1.0], delta_s: f }, PhaseLoss { phase: CornerPhase::Sortie, range_m: [1.0, 2.0], delta_s: s }];
        let st = phase_stats(&[lap(0.20, -0.01), lap(0.10, 0.0), lap(0.50, 0.04)]);
        assert_eq!(st.len(), 2);
        assert_eq!(st[0], PhaseStat { phase: CornerPhase::Freinage, median_s: 0.20, laps_losing: 3, laps: 3 });
        assert_eq!(st[1].median_s, 0.0);
        assert_eq!(st[1].laps_losing, 1);
    }
}
