#![allow(clippy::type_complexity, clippy::needless_range_loop)]
//! Relevé de la piste à partir de tes tours : vibreurs, lignes blanches, herbe / gravier
//! (matière sous la voiture, `PlayerTrackSurfaceMaterial` d'iRacing) et bords de piste estimés
//! (enveloppe de toutes tes trajectoires). Plus tu roules, plus le relevé est complet.

use crate::model::LapTrace;
use serde::Serialize;
use std::collections::HashMap;

/// Famille de matière iRacing (irsdk_TrkSurf).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    Asphalt,
    Kerb,
    Paint,
    Off,
    Other,
}

pub fn material(code: f32) -> Material {
    match code.round() as i32 {
        1..=6 => Material::Asphalt, // asphalte 1-4, béton 1-2
        9 | 10 => Material::Paint,
        11..=14 => Material::Kerb,
        15..=27 => Material::Off, // herbe, terre, sable, gravier, grasscrete, astroturf
        _ => Material::Other,
    }
}

/// Bords de piste : deux polylignes, et d'où elles viennent.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TrackEdges {
    pub left: Vec<[f32; 2]>,
    pub right: Vec<[f32; 2]>,
    /// « laps » (estimés d'après tes tours) ou « tum » (largeur mesurée, base TUM).
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct TrackOverlay {
    /// Points (m, repère des trajectoires) où ta voiture roulait sur un vibreur.
    pub kerbs: Vec<[f32; 2]>,
    /// Sur la peinture (lignes blanches de bord de piste, ligne de stand…).
    pub lines: Vec<[f32; 2]>,
    /// Hors de la piste (herbe, gravier, sable, astroturf).
    pub off: Vec<[f32; 2]>,
    pub edges: Option<TrackEdges>,
    pub laps: usize,
    pub sessions: usize,
}

/// Une trace utilisable pour le relevé : positions + matière (et emplacement : piste / stands).
pub struct SurfaceLap<'a> {
    pub trace: &'a LapTrace,
}

const CELL: f32 = 1.0;

/// Points de chaque famille, dédoublonnés sur une grille de 1 m.
pub fn surface_points(laps: &[SurfaceLap]) -> (Vec<[f32; 2]>, Vec<[f32; 2]>, Vec<[f32; 2]>) {
    let mut seen: HashMap<(i32, i32, u8), ()> = HashMap::new();
    let (mut kerbs, mut lines, mut off) = (vec![], vec![], vec![]);
    for l in laps {
        let t = l.trace;
        let (Some(xs), Some(ys), Some(mat)) = (&t.pos_x, &t.pos_y, t.extra.get("surface_material")) else { continue };
        let loc = t.extra.get("track_surface");
        for i in 0..xs.len().min(mat.len()) {
            // Stands (irsdk_TrkLoc 1-2) : la ligne blanche de sortie des stands n'est pas un bord de piste.
            if loc.is_some_and(|v| matches!(v.get(i).map(|x| x.round() as i32), Some(1) | Some(2))) {
                continue;
            }
            let m = material(mat[i]);
            let k = match m {
                Material::Kerb => 0u8,
                Material::Paint => 1,
                Material::Off => 2,
                _ => continue,
            };
            let (x, y) = (xs[i], ys[i]);
            if !(x.is_finite() && y.is_finite()) {
                continue;
            }
            let key = ((x / CELL).floor() as i32, (y / CELL).floor() as i32, k);
            if seen.insert(key, ()).is_none() {
                let p = [((x * 10.0).round() / 10.0), ((y * 10.0).round() / 10.0)];
                match k {
                    0 => kerbs.push(p),
                    1 => lines.push(p),
                    _ => off.push(p),
                }
            }
        }
    }
    (kerbs, lines, off)
}

/// Demi-largeur d'une monoplace (m) : la position mesurée est celle du centre de la voiture.
const HALF_CAR: f32 = 0.9;
/// Pas des tranches le long de la trajectoire de référence (m).
const BIN_M: f32 = 4.0;

/**
 * Bords de piste estimés : pour chaque tranche de la trajectoire de référence (`spine`, points
 * espacés régulièrement), écart latéral le plus à gauche et le plus à droite où une voiture est passée
 * en restant sur la piste (asphalte, vibreur, peinture), plus une demi-largeur de voiture.
 * Une tranche vue par moins de `min_laps` tours reste sans bord (pas d'invention).
 */
pub fn estimate_edges(spine: &[[f32; 2]], laps: &[SurfaceLap], min_laps: usize) -> Option<TrackEdges> {
    combine_edges(spine, laps, min_laps, None)
}

/**
 * Bords mesurés (base de largeurs) corrigés par tes tours : là où une voiture est passée plus loin que
 * le bord mesuré (petite erreur de recalage, vibreur), le bord est repoussé pour la contenir ; là où
 * tu n'es jamais allé près du bord, la largeur mesurée est gardée.
 */
pub fn combine_edges(spine: &[[f32; 2]], laps: &[SurfaceLap], min_laps: usize, measured: Option<&TrackEdges>) -> Option<TrackEdges> {
    let n = spine.len();
    if n < 20 || laps.is_empty() {
        return None;
    }
    // Normale (vers la gauche) en chaque point.
    let normal: Vec<[f32; 2]> = (0..n)
        .map(|i| {
            let a = spine[i.saturating_sub(2)];
            let b = spine[(i + 2).min(n - 1)];
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = (dx * dx + dy * dy).sqrt().max(1e-6);
            [-dy / l, dx / l]
        })
        .collect();
    let step = {
        let mut s = 0.0;
        for i in 1..n {
            s += ((spine[i][0] - spine[i - 1][0]).powi(2) + (spine[i][1] - spine[i - 1][1]).powi(2)).sqrt();
        }
        (s / (n - 1) as f32).max(0.1)
    };
    let per_bin = ((BIN_M / step).round() as usize).max(1);
    let bins = n.div_ceil(per_bin);
    let mut lo = vec![f32::INFINITY; bins];
    let mut hi = vec![f32::NEG_INFINITY; bins];
    let mut seen_by: Vec<u16> = vec![0; bins];
    for l in laps {
        let t = l.trace;
        let (Some(xs), Some(ys)) = (&t.pos_x, &t.pos_y) else { continue };
        let mat = t.extra.get("surface_material");
        let loc = t.extra.get("track_surface");
        let total = t.dist_m.last().copied().unwrap_or(0.0).max(1.0);
        let mut touched = vec![false; bins];
        // Un point tous les ~2 m suffit.
        let every = (t.len() / (total / 2.0).max(1.0) as usize).max(1);
        let mut i = 0;
        while i < xs.len() {
            // Stands exclus ; un vibreur compte comme piste même au-delà de la limite (iRacing le classe
            // alors « hors piste ») : c'est la surface roulable que la carte doit montrer.
            let pit = loc.and_then(|v| v.get(i)).is_some_and(|lc| matches!(lc.round() as i32, 1 | 2 | -1));
            let on = !pit
                && match mat.and_then(|m| m.get(i)) {
                    Some(m) => matches!(material(*m), Material::Asphalt | Material::Kerb | Material::Paint),
                    None => loc.and_then(|v| v.get(i)).is_none_or(|lc| lc.round() as i32 == 3),
                };
            if on {
                // Index de référence attendu d'après la distance parcourue, puis recherche locale.
                let guess = ((t.dist_m[i] / total) * (n - 1) as f32).round() as isize;
                let (mut best, mut bd) = (0usize, f32::INFINITY);
                for j in (guess - 60).max(0)..=(guess + 60).min(n as isize - 1) {
                    let j = j as usize;
                    let d = (xs[i] - spine[j][0]).powi(2) + (ys[i] - spine[j][1]).powi(2);
                    if d < bd {
                        bd = d;
                        best = j;
                    }
                }
                // Point trop loin de la trajectoire (raccourci, erreur GPS) : ignoré.
                if bd < 20.0 * 20.0 {
                    let off = (xs[i] - spine[best][0]) * normal[best][0] + (ys[i] - spine[best][1]) * normal[best][1];
                    let b = best / per_bin;
                    lo[b] = lo[b].min(off);
                    hi[b] = hi[b].max(off);
                    touched[b] = true;
                }
            }
            i += every;
        }
        for (b, t) in touched.iter().enumerate() {
            if *t {
                seen_by[b] += 1;
            }
        }
    }
    // Lissage : médiane glissante sur 5 tranches, pour effacer les à-coups GPS.
    let med = |v: &[f32], b: usize| -> Option<f32> {
        let mut w: Vec<f32> = (b.saturating_sub(2)..(b + 3).min(bins)).map(|k| v[k]).filter(|x| x.is_finite()).collect();
        if w.is_empty() {
            return None;
        }
        w.sort_by(|a, b| a.total_cmp(b));
        Some(w[w.len() / 2])
    };
    // Écart latéral (signé) du point de bord mesuré le plus proche, s'il est à moins de 25 m.
    let side = |pts: &[[f32; 2]], p: [f32; 2], nv: [f32; 2]| -> Option<f32> {
        let q = pts.iter().min_by(|a, b| ((a[0] - p[0]).powi(2) + (a[1] - p[1]).powi(2)).total_cmp(&((b[0] - p[0]).powi(2) + (b[1] - p[1]).powi(2))))?;
        let d2 = (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2);
        (d2 < 25.0 * 25.0).then(|| (q[0] - p[0]) * nv[0] + (q[1] - p[1]) * nv[1])
    };
    let mut left = vec![];
    let mut right = vec![];
    let mut rows = vec![];
    for b in 0..bins {
        let j = (b * per_bin + per_bin / 2).min(n - 1);
        let (p, nv) = (spine[j], normal[j]);
        let seen = (seen_by[b] as usize) >= min_laps;
        let lap = if seen { med(&hi, b).zip(med(&lo, b)).map(|(h, l)| (h + HALF_CAR, l - HALF_CAR)) } else { None };
        let meas = measured.and_then(|m| side(&m.left, p, nv).zip(side(&m.right, p, nv)));
        let (h, l) = match (meas, lap) {
            (Some((mh, ml)), Some((h, l))) => (mh.max(h), ml.min(l)),
            (Some(m), None) => m,
            (None, Some(x)) => x,
            (None, None) => continue,
        };
        // Au moins ±2,5 m autour de la trajectoire : une piste n'est jamais plus étroite qu'une voiture.
        rows.push((b, p, nv, h.max(2.5), l.min(-2.5)));
    }
    // Lissage final (moyenne sur 5 tranches voisines contiguës) : bords sans crans.
    for k in 0..rows.len() {
        let (b0, p, nv, _, _) = rows[k];
        let near: Vec<&(usize, [f32; 2], [f32; 2], f32, f32)> = rows[k.saturating_sub(2)..(k + 3).min(rows.len())].iter().filter(|r| r.0.abs_diff(b0) <= 2).collect();
        let h = near.iter().map(|r| r.3).sum::<f32>() / near.len() as f32;
        let l = near.iter().map(|r| r.4).sum::<f32>() / near.len() as f32;
        left.push([p[0] + nv[0] * h, p[1] + nv[1] * h]);
        right.push([p[0] + nv[0] * l, p[1] + nv[1] * l]);
    }
    if left.len() < 10 {
        return None;
    }
    Some(TrackEdges { left, right, source: if measured.is_some() { "tum".into() } else { "laps".into() } })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Ligne droite de 400 m ; trois tours décalés latéralement de −3, 0 et +4 m, dont un sur un vibreur.
    fn lap(offset: f32, mat: f32) -> LapTrace {
        let n = 400;
        let xs: Vec<f32> = (0..n).map(|i| i as f32).collect();
        LapTrace {
            time_s: xs.iter().map(|x| x / 50.0).collect(),
            dist_m: xs.clone(),
            speed_mps: vec![50.0; n],
            throttle: vec![1.0; n],
            brake: vec![0.0; n],
            gear: None,
            rpm: None,
            steer_rad: None,
            pos_x: Some(xs.clone()),
            pos_y: Some(vec![offset; n]),
            fuel_l: None,
            yaw_rate: None,
            extra: BTreeMap::from([("surface_material".to_string(), vec![mat; n]), ("track_surface".to_string(), vec![3.0; n])]),
        }
    }

    #[test]
    fn surfaces_and_edges_from_laps() {
        let a = lap(-3.0, 1.0);
        let b = lap(0.0, 1.0);
        let c = lap(4.0, 11.0);
        let laps = [SurfaceLap { trace: &a }, SurfaceLap { trace: &b }, SurfaceLap { trace: &c }];
        let (kerbs, lines, off) = surface_points(&laps);
        assert!(kerbs.len() >= 390 && kerbs.iter().all(|p| p[1] == 4.0));
        assert!(lines.is_empty() && off.is_empty());
        let spine: Vec<[f32; 2]> = (0..400).map(|i| [i as f32, 0.0]).collect();
        let e = estimate_edges(&spine, &laps, 2).unwrap();
        // Bord gauche (y > 0) = tour sur le vibreur + demi-voiture ; bord droit = −3 − 0,9.
        assert!(e.left.iter().all(|p| (p[1] - 4.9).abs() < 0.01), "{:?}", &e.left[..3]);
        assert!(e.right.iter().all(|p| (p[1] + 3.9).abs() < 0.01));
        // Exiger 4 tours : aucune tranche ne qualifie.
        assert!(estimate_edges(&spine, &laps, 4).is_none());
        // Herbe : exclue de l'enveloppe, relevée à part.
        let g = lap(9.0, 16.0);
        let laps2 = [SurfaceLap { trace: &a }, SurfaceLap { trace: &b }, SurfaceLap { trace: &g }];
        assert!(!surface_points(&laps2).2.is_empty());
        let e2 = estimate_edges(&spine, &laps2, 2).unwrap();
        assert!(e2.left.iter().all(|p| p[1] < 3.0));
    }
}
