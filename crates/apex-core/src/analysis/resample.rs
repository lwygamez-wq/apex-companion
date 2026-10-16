//! Rééchantillonnage d'un tour sur une grille de distance fixe.
//!
//! C'est le socle de toute comparaison : deux tours ne sont comparables que
//! point par point **à distance égale**, pas à temps égal.

use crate::model::LapTrace;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ResampleError {
    #[error("trace trop courte ({0} échantillons exploitables)")]
    TooShort(usize),
    #[error("pas de grille invalide : {0} m")]
    BadStep(f32),
    #[error("trace invalide : {0}")]
    Invalid(String),
}

/// Tour rééchantillonné : `dist[k] = k * step_m`.
#[derive(Debug, Clone, Serialize)]
pub struct Resampled {
    pub step_m: f32,
    pub dist: Vec<f32>,
    pub time: Vec<f32>,
    pub speed_kmh: Vec<f32>,
    pub throttle: Vec<f32>,
    pub brake: Vec<f32>,
    pub gear: Option<Vec<f32>>,
    pub steer_deg: Option<Vec<f32>>,
    pub x: Option<Vec<f32>>,
    pub y: Option<Vec<f32>>,
    /// Part de la grille couverte par des échantillons réels (écart source ≤ `MAX_GAP_M`).
    pub coverage: f32,
    /// Densité moyenne de la source (échantillons par mètre).
    pub source_density: f32,
}

/// Au-delà de cet écart entre deux échantillons source, la zone est considérée non couverte.
pub const MAX_GAP_M: f32 = 15.0;

impl Resampled {
    pub fn len(&self) -> usize {
        self.dist.len()
    }
    pub fn is_empty(&self) -> bool {
        self.dist.is_empty()
    }
    /// Index de grille le plus proche d'une distance (borné).
    pub fn idx(&self, d: f32) -> usize {
        ((d / self.step_m).round().max(0.0) as usize).min(self.len().saturating_sub(1))
    }
}

/// Rééchantillonne `trace` sur [0, length_m) au pas `step_m`.
pub fn resample(trace: &LapTrace, length_m: f32, step_m: f32) -> Result<Resampled, ResampleError> {
    if !(step_m > 0.0 && step_m.is_finite()) {
        return Err(ResampleError::BadStep(step_m));
    }
    trace.validate().map_err(ResampleError::Invalid)?;

    // Indices à distance strictement croissante (on écarte la gigue et les retours arrière).
    let mut keep: Vec<usize> = Vec::with_capacity(trace.len());
    let mut last = f32::NEG_INFINITY;
    for (i, &d) in trace.dist_m.iter().enumerate() {
        if d > last {
            keep.push(i);
            last = d;
        }
    }
    if keep.len() < 10 {
        return Err(ResampleError::TooShort(keep.len()));
    }
    let src_d: Vec<f32> = keep.iter().map(|&i| trace.dist_m[i]).collect();
    let pick = |ch: &[f32]| -> Vec<f32> { keep.iter().map(|&i| ch[i]).collect() };

    let n = ((length_m / step_m).floor() as usize).max(2);
    let grid: Vec<f32> = (0..n).map(|k| k as f32 * step_m).collect();

    // Pour chaque point de grille : (index gauche, fraction, couvert ?)
    let mut locs: Vec<(usize, f32, bool)> = Vec::with_capacity(n);
    let mut j = 0usize;
    for &g in &grid {
        while j + 2 < src_d.len() && src_d[j + 1] < g {
            j += 1;
        }
        let (d0, d1) = (src_d[j], src_d[j + 1]);
        let frac = (g - d0) / (d1 - d0);
        let inside = g >= src_d[0] - MAX_GAP_M && g <= *src_d.last().unwrap() + MAX_GAP_M;
        let covered = inside && (d1 - d0) <= MAX_GAP_M;
        locs.push((j, frac, covered));
    }

    // Interpolation linéaire bornée (pas d'extrapolation, sauf pour le temps).
    let interp = |ch: &[f32]| -> Vec<f32> {
        locs.iter()
            .map(|&(j, f, _)| {
                let f = f.clamp(0.0, 1.0);
                ch[j] + (ch[j + 1] - ch[j]) * f
            })
            .collect()
    };
    // Le temps est extrapolé linéairement aux bords (quelques mètres autour de la ligne).
    let src_t = pick(&trace.time_s);
    let time: Vec<f32> = locs
        .iter()
        .map(|&(j, f, _)| src_t[j] + (src_t[j + 1] - src_t[j]) * f)
        .collect();

    let speed: Vec<f32> = interp(&pick(&trace.speed_mps)).into_iter().map(|v| v * 3.6).collect();
    let covered = locs.iter().filter(|l| l.2).count();

    Ok(Resampled {
        step_m,
        time,
        speed_kmh: speed,
        throttle: interp(&pick(&trace.throttle)),
        brake: interp(&pick(&trace.brake)),
        gear: trace.gear.as_ref().map(|g| {
            // Le rapport ne s'interpole pas : plus proche voisin.
            let s = pick(g);
            locs.iter().map(|&(j, f, _)| if f < 0.5 { s[j] } else { s[j + 1] }).collect()
        }),
        steer_deg: trace
            .steer_rad
            .as_ref()
            .map(|s| interp(&pick(s)).into_iter().map(f32::to_degrees).collect()),
        x: trace.pos_x.as_ref().map(|c| interp(&pick(c))),
        y: trace.pos_y.as_ref().map(|c| interp(&pick(c))),
        coverage: covered as f32 / n as f32,
        source_density: src_d.len() as f32 / (src_d.last().unwrap() - src_d[0]).max(1.0),
        dist: grid,
    })
}

/// Moyenne glissante centrée (fenêtre en points).
pub fn smooth(v: &[f32], window: usize) -> Vec<f32> {
    if window <= 1 || v.is_empty() {
        return v.to_vec();
    }
    let h = window / 2;
    let mut prefix = vec![0f64; v.len() + 1];
    for (i, &x) in v.iter().enumerate() {
        prefix[i + 1] = prefix[i] + x as f64;
    }
    (0..v.len())
        .map(|i| {
            let a = i.saturating_sub(h);
            let b = (i + h + 1).min(v.len());
            ((prefix[b] - prefix[a]) / (b - a) as f64) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant_speed_trace(v: f32, hz: f32, length: f32) -> LapTrace {
        let dt = 1.0 / hz;
        let n = (length / (v * dt)) as usize + 1;
        let time: Vec<f32> = (0..n).map(|i| i as f32 * dt).collect();
        LapTrace {
            dist_m: time.iter().map(|t| t * v).collect(),
            speed_mps: vec![v; n],
            throttle: vec![1.0; n],
            brake: vec![0.0; n],
            time_s: time,
            ..Default::default()
        }
    }

    #[test]
    fn time_at_distance_is_exact_for_constant_speed() {
        let t = constant_speed_trace(50.0, 60.0, 2000.0);
        let r = resample(&t, 2000.0, 1.0).unwrap();
        assert_eq!(r.len(), 2000);
        for k in [0usize, 100, 999, 1999] {
            assert!((r.time[k] - r.dist[k] / 50.0).abs() < 1e-3, "k={k}");
        }
        assert!((r.speed_kmh[500] - 180.0).abs() < 1e-3);
        assert!(r.coverage > 0.99);
    }

    #[test]
    fn non_monotonic_samples_are_ignored() {
        let mut t = constant_speed_trace(40.0, 60.0, 1000.0);
        t.dist_m[50] = t.dist_m[48]; // gigue arrière
        let r = resample(&t, 1000.0, 2.0).unwrap();
        assert!(r.time.windows(2).all(|w| w[1] >= w[0]));
    }

    #[test]
    fn gaps_reduce_coverage() {
        let mut t = constant_speed_trace(50.0, 10.0, 2000.0);
        // Trou de télémétrie : on supprime 300 m d'échantillons.
        let keep: Vec<usize> = (0..t.len()).filter(|&i| !(t.dist_m[i] > 800.0 && t.dist_m[i] < 1100.0)).collect();
        let f = |c: &Vec<f32>| keep.iter().map(|&i| c[i]).collect::<Vec<_>>();
        t = LapTrace { time_s: f(&t.time_s), dist_m: f(&t.dist_m), speed_mps: f(&t.speed_mps), throttle: f(&t.throttle), brake: f(&t.brake), ..Default::default() };
        let r = resample(&t, 2000.0, 1.0).unwrap();
        assert!(r.coverage < 0.87 && r.coverage > 0.8, "coverage={}", r.coverage);
    }

    #[test]
    fn smooth_keeps_constant() {
        assert_eq!(smooth(&[3.0; 10], 5), vec![3.0; 10]);
    }
}
