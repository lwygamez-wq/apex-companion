//! Sorties de piste : où elles ont lieu, et sur quels tours.
//!
//! Source : canal `track_surface` (iRacing PlayerTrackSurface, 0 = hors piste) conservé dans
//! `LapTrace::extra`. Sans ce canal (autres simulateurs, sessions anciennes), rien n'est déduit.

use crate::model::LapTrace;
use serde::Serialize;

pub const TRACK_SURFACE: &str = "track_surface";
const OFF_TRACK: f32 = 0.0;

/// Portions hors piste d'un tour : (début, fin) en mètres depuis la ligne.
pub fn off_track_spans(t: &LapTrace) -> Vec<(f32, f32)> {
    let Some(s) = t.extra.get(TRACK_SURFACE) else { return vec![] };
    let mut out: Vec<(f32, f32)> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &v) in s.iter().enumerate().take(t.dist_m.len()) {
        let off = v == OFF_TRACK;
        match (off, start) {
            (true, None) => start = Some(i),
            (false, Some(a)) => {
                out.push((t.dist_m[a], t.dist_m[i - 1]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(a) = start {
        out.push((t.dist_m[a], *t.dist_m.last().unwrap()));
    }
    // Fusionne les micro-coupures (< 15 m) d'une même sortie.
    let mut merged: Vec<(f32, f32)> = Vec::new();
    for sp in out {
        match merged.last_mut() {
            Some(last) if sp.0 - last.1 < 15.0 => last.1 = sp.1,
            _ => merged.push(sp),
        }
    }
    merged
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OffTrackZone {
    /// Début typique de la sortie (m depuis la ligne).
    pub at_m: f32,
    /// Virage concerné (nom affiché), si la sortie est dans ou juste après un virage.
    pub corner: Option<String>,
    /// Numéros des tours concernés.
    pub laps: Vec<u32>,
}

/// Regroupe les sorties de tous les tours par endroit (à 80 m près), les plus fréquentes d'abord.
/// `corners` : (nom, début, fin) des virages de la référence.
pub fn off_track_zones(laps: &[(u32, Vec<(f32, f32)>)], corners: &[(String, f32, f32)]) -> Vec<OffTrackZone> {
    let mut pts: Vec<(f32, u32)> = laps.iter().flat_map(|(n, sp)| sp.iter().map(move |s| (s.0, *n))).collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut zones: Vec<(Vec<f32>, Vec<u32>)> = Vec::new();
    for (d, n) in pts {
        match zones.last_mut() {
            Some((ds, ns)) if d - ds[ds.len() - 1] <= 80.0 => {
                ds.push(d);
                if !ns.contains(&n) {
                    ns.push(n);
                }
            }
            _ => zones.push((vec![d], vec![n])),
        }
    }
    let mut out: Vec<OffTrackZone> = zones
        .into_iter()
        .map(|(ds, mut ns)| {
            ns.sort_unstable();
            let at = ds[ds.len() / 2];
            let corner = corners
                .iter()
                .filter(|(_, a, b)| at >= a - 60.0 && at <= b + 120.0)
                .min_by(|x, y| ((x.1 + x.2) / 2.0 - at).abs().total_cmp(&((y.1 + y.2) / 2.0 - at).abs()))
                .map(|c| c.0.clone());
            OffTrackZone { at_m: at, corner, laps: ns }
        })
        .collect();
    out.sort_by(|a, b| b.laps.len().cmp(&a.laps.len()).then(a.at_m.total_cmp(&b.at_m)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(surface: &[f32]) -> LapTrace {
        let n = surface.len();
        LapTrace {
            time_s: (0..n).map(|i| i as f32).collect(),
            dist_m: (0..n).map(|i| i as f32 * 10.0).collect(),
            speed_mps: vec![10.0; n],
            throttle: vec![1.0; n],
            brake: vec![0.0; n],
            extra: [(TRACK_SURFACE.to_string(), surface.to_vec())].into_iter().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn spans_and_zones() {
        // Sortie de 20 à 40 m, coupure d'un échantillon (fusionnée), puis 90 m.
        let t = trace(&[3., 3., 0., 0., 0., 3., 3., 3., 3., 0., 3.]);
        assert_eq!(off_track_spans(&t), vec![(20.0, 40.0), (90.0, 90.0)]);
        assert!(off_track_spans(&LapTrace::default()).is_empty());
        let laps = vec![(1, vec![(1000.0, 1010.0)]), (2, vec![(1030.0, 1040.0), (3000.0, 3005.0)]), (3, vec![(990.0, 995.0)])];
        let corners = vec![("Virage 19".to_string(), 900.0, 1000.0), ("Virage 1".to_string(), 2900.0, 2990.0)];
        let z = off_track_zones(&laps, &corners);
        assert_eq!(z.len(), 2);
        assert_eq!(z[0].laps, vec![1, 2, 3]);
        assert_eq!(z[0].corner.as_deref(), Some("Virage 19"));
        assert_eq!(z[1].corner.as_deref(), Some("Virage 1"));
    }
}
