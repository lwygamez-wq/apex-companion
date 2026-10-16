//! Zones de progrès d'une session : chaque tour propre est comparé au meilleur,
//! et les pertes sont regroupées par virage. Un défaut répété = priorité réelle.

use super::compare::{compare_laps_named, CornerName, LapInput};
use super::insights::Insight;
use crate::model::{LapMeta, LapTrace, SessionMeta};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct ProgressZone {
    pub segment: String,
    pub range_m: [f32; 2],
    /// Nombre de tours propres où une perte significative est mesurée.
    pub occurrences: usize,
    pub laps_compared: usize,
    pub avg_loss_s: f32,
    pub max_loss_s: f32,
    /// Conseil le plus fréquent sur ce virage (titre).
    pub typical_issue: String,
    /// Conseil représentatif (celui de plus forte perte).
    pub example: Insight,
}

pub struct SessionLap<'a> {
    pub lap_id: i64,
    pub meta: &'a LapMeta,
    pub trace: &'a LapTrace,
}

pub fn progress_zones(session: &SessionMeta, reference: &SessionLap, laps: &[SessionLap], step_m: f32) -> Vec<ProgressZone> {
    progress_zones_named(session, reference, laps, step_m, &[])
}

pub fn progress_zones_named(session: &SessionMeta, reference: &SessionLap, laps: &[SessionLap], step_m: f32, names: &[CornerName]) -> Vec<ProgressZone> {
    let rf = LapInput { lap_id: reference.lap_id, session, lap: reference.meta, trace: reference.trace };
    let mut by_seg: BTreeMap<String, Vec<Insight>> = BTreeMap::new();
    let mut compared = 0usize;
    for l in laps.iter().filter(|l| l.lap_id != reference.lap_id && l.meta.is_clean()) {
        let Ok(c) = compare_laps_named(&rf, &LapInput { lap_id: l.lap_id, session, lap: l.meta, trace: l.trace }, step_m, names) else { continue };
        compared += 1;
        for i in c.insights {
            if let Some(s) = i.segment.clone() {
                by_seg.entry(s).or_default().push(i);
            }
        }
    }
    let mut zones: Vec<ProgressZone> = by_seg
        .into_iter()
        .map(|(segment, list)| {
            let losses: Vec<f32> = list.iter().map(|i| i.est_gain_s.map_or(0.0, |g| g[1])).collect();
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for i in &list {
                *counts.entry(i.title.as_str()).or_default() += 1;
            }
            let typical = counts.iter().max_by_key(|(_, n)| **n).map(|(t, _)| t.to_string()).unwrap_or_default();
            let example = list
                .iter()
                .max_by(|a, b| a.est_gain_s.map_or(0.0, |g| g[1]).total_cmp(&b.est_gain_s.map_or(0.0, |g| g[1])))
                .cloned()
                .unwrap();
            ProgressZone {
                range_m: example.segment_range_m.unwrap_or([0.0, 0.0]),
                segment,
                occurrences: list.len(),
                laps_compared: compared,
                avg_loss_s: losses.iter().sum::<f32>() / losses.len() as f32,
                max_loss_s: losses.iter().copied().fold(0.0, f32::max),
                typical_issue: typical,
                example,
            }
        })
        .collect();
    // Tri : récurrence × perte moyenne (ce qui coûte le plus sur la durée).
    zones.sort_by(|a, b| (b.occurrences as f32 * b.avg_loss_s).total_cmp(&(a.occurrences as f32 * a.avg_loss_s)));
    zones
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::session::best_clean_lap;
    use crate::mock::demo_session;

    #[test]
    fn repeated_v1_early_braking_ranks_first() {
        let s = demo_session(2, 0);
        let metas: Vec<_> = s.laps.iter().map(|l| l.meta.clone()).collect();
        let bi = best_clean_lap(&metas).unwrap();
        let laps: Vec<SessionLap> = s.laps.iter().enumerate().map(|(i, l)| SessionLap { lap_id: i as i64, meta: &l.meta, trace: &l.trace }).collect();
        let z = progress_zones(&s.meta, &laps[bi], &laps, 2.0);
        assert!(!z.is_empty());
        assert_eq!(z[0].segment, "V1", "{:?}", z.iter().map(|z| (&z.segment, z.occurrences)).collect::<Vec<_>>());
        assert_eq!(z[0].occurrences, 2);
        assert!(z[0].typical_issue.contains("précoce"));
    }
}
