//! Où part le temps en fin de relais : chaque tour du relais est comparé au meilleur tour du relais,
//! virage par virage et phase par phase ; la médiane de la seconde moitié moins celle de la première
//! dit où le rythme se perd (ou se gagne) au fil du relais. L'usure des pneus mesurée par le jeu
//! (LMU) est donnée à côté, avant contre arrière, sans conclure à la place des chiffres.

use super::compare::{compare_laps_named, CornerName, LapInput};
use super::phases::{CornerPhase, PhaseLoss};
use crate::model::{LapMeta, LapTrace, SessionMeta};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PhaseDrift {
    pub corner: String,
    pub phase: CornerPhase,
    /// Médiane de la seconde moitié moins médiane de la première (s) ; positif = plus lent en fin de relais.
    pub drift_s: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StintDrift {
    /// Tours comparés dans chaque moitié (hors tour de référence).
    pub laps_first: usize,
    pub laps_second: usize,
    /// Somme des dérives par phase, tous virages confondus (s), dans l'ordre freinage → sortie.
    pub by_phase: Vec<(CornerPhase, f32)>,
    /// Dérives les plus fortes (≥ 0,03 s), au plus trois.
    pub top: Vec<PhaseDrift>,
    /// Somme de toutes les dérives des virages (s).
    pub total_s: f32,
    /// Usure moyenne par tour (points de %), avant et arrière (LMU).
    pub wear_front_pct: Option<f32>,
    pub wear_rear_pct: Option<f32>,
    /// Phrase prête à afficher (`drift_sentence`), remplie par le débrief.
    pub summary: Option<String>,
}

fn median(v: &mut [f32]) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

/// Agrège les phases mesurées tour par tour. `per_lap` : dans l'ordre du relais, pour chaque tour
/// (sauf la référence) la liste (virage, phases) ; `split` : nombre de tours de la première moitié.
pub fn drift_from_phases(per_lap: &[Vec<(String, Vec<PhaseLoss>)>], split: usize) -> Option<StintDrift> {
    let (a, b) = per_lap.split_at(split.min(per_lap.len()));
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let collect = |half: &[Vec<(String, Vec<PhaseLoss>)>]| {
        let mut m: BTreeMap<(String, CornerPhase), Vec<f32>> = BTreeMap::new();
        for lap in half {
            for (c, ph) in lap {
                for p in ph {
                    m.entry((c.clone(), p.phase)).or_default().push(p.delta_s);
                }
            }
        }
        m
    };
    let (mut fa, mut fb) = (collect(a), collect(b));
    // Ordre des virages sur le tour : celui du premier tour comparé.
    let order: Vec<String> = per_lap[0].iter().map(|(c, _)| c.clone()).collect();
    let mut drifts = vec![];
    for c in &order {
        for ph in CornerPhase::ALL {
            let key = (c.clone(), ph);
            let (Some(x), Some(y)) = (fa.get_mut(&key).and_then(|v| median(v)), fb.get_mut(&key).and_then(|v| median(v))) else { continue };
            drifts.push(PhaseDrift { corner: c.clone(), phase: ph, drift_s: y - x });
        }
    }
    let by_phase = CornerPhase::ALL.iter().map(|&p| (p, drifts.iter().filter(|d| d.phase == p).map(|d| d.drift_s).sum())).collect();
    let total_s = drifts.iter().map(|d| d.drift_s).sum();
    let mut top: Vec<PhaseDrift> = drifts.into_iter().filter(|d| d.drift_s >= 0.03).collect();
    top.sort_by(|x, y| y.drift_s.total_cmp(&x.drift_s));
    top.truncate(3);
    Some(StintDrift { laps_first: a.len(), laps_second: b.len(), by_phase, top, total_s, wear_front_pct: None, wear_rear_pct: None, summary: None })
}

/// Usure moyenne par tour, avant et arrière (points de %), sur les tours qui la donnent.
pub fn wear_per_lap(laps: &[&LapMeta]) -> (Option<f32>, Option<f32>) {
    let w: Vec<[f32; 4]> = laps.iter().filter_map(|l| l.tyre_wear_pct).filter(|w| w.iter().all(|v| v.is_finite() && *v >= 0.0)).collect();
    if w.is_empty() {
        return (None, None);
    }
    let n = w.len() as f32;
    (Some(w.iter().map(|x| (x[0] + x[1]) / 2.0).sum::<f32>() / n), Some(w.iter().map(|x| (x[2] + x[3]) / 2.0).sum::<f32>() / n))
}

/// Dérive d'un relais. `laps` : tours propres du relais dans l'ordre (au moins 6 avec télémétrie).
pub fn stint_drift(session: &SessionMeta, laps: &[(&LapMeta, &LapTrace)], names: &[CornerName]) -> Option<StintDrift> {
    if laps.len() < 6 {
        return None;
    }
    let (ri, (rm, rt)) = laps.iter().enumerate().filter(|(_, (m, _))| m.lap_time_s.is_some()).min_by(|a, b| a.1 .0.lap_time_s.unwrap().total_cmp(&b.1 .0.lap_time_s.unwrap()))?;
    let rf = LapInput { lap_id: rm.number as i64, session, lap: rm, trace: rt };
    let half = laps.len() / 2;
    let mut per_lap = vec![];
    let mut split = 0;
    for (i, (m, t)) in laps.iter().enumerate() {
        if i == ri {
            continue;
        }
        let Ok(c) = compare_laps_named(&rf, &LapInput { lap_id: m.number as i64, session, lap: m, trace: t }, 5.0, names) else { continue };
        if c.cmp_trace.coverage < 0.97 {
            continue;
        }
        if i < half {
            split += 1;
        }
        per_lap.push(c.corners.into_iter().map(|k| (k.corner.name, k.phases)).collect::<Vec<_>>());
    }
    let mut d = drift_from_phases(&per_lap, split)?;
    let metas: Vec<&LapMeta> = laps.iter().map(|(m, _)| *m).collect();
    (d.wear_front_pct, d.wear_rear_pct) = wear_per_lap(&metas);
    Some(d)
}

/// Phrase du débrief : où se perd le temps entre les deux moitiés du relais, et l'usure mesurée.
pub fn drift_sentence(d: &StintDrift) -> Option<String> {
    let fr = |v: f32| format!("{v:.2}").replace('.', ",");
    let mut s = String::new();
    if !d.top.is_empty() {
        let parts: Vec<String> = d.top.iter().map(|t| format!("{} {} +{} s", t.corner, t.phase.spoken(), fr(t.drift_s))).collect();
        s += &format!("Ce qui ralentit le plus d'une moitié à l'autre : {}.", parts.join(", "));
        let pos: f32 = d.by_phase.iter().map(|(_, v)| v.max(0.0)).sum();
        if let Some((p, v)) = d.by_phase.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1)).filter(|(_, v)| *v >= 0.05 && pos > 0.0 && *v / pos >= 0.5) {
            s += &format!(" Sur tout le tour, {} s {} ({} % de la perte).", fr(v), p.spoken(), (v / pos * 100.0).round() as i32);
        }
    }
    if let (Some(f), Some(r)) = (d.wear_front_pct, d.wear_rear_pct) {
        if f > 0.0 || r > 0.0 {
            s += &format!("{}Usure par tour : avant {} %, arrière {} %.", if s.is_empty() { "" } else { " " }, fr(f), fr(r));
        }
    }
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lap(v: &[(&str, f32, f32)]) -> Vec<(String, Vec<PhaseLoss>)> {
        v.iter()
            .map(|(c, f, s)| {
                (c.to_string(), vec![PhaseLoss { phase: CornerPhase::Freinage, range_m: [0.0, 1.0], delta_s: *f }, PhaseLoss { phase: CornerPhase::Sortie, range_m: [1.0, 2.0], delta_s: *s }])
            })
            .collect()
    }

    /// Relais : la sortie de Rivazza se dégrade de 0,20 s, Tosa de 0,06 s en sortie ; le freinage ne bouge pas.
    #[test]
    fn locates_the_end_of_stint_loss() {
        let laps = vec![
            lap(&[("Tosa", 0.05, 0.02), ("Rivazza", 0.02, 0.05)]),
            lap(&[("Tosa", 0.03, 0.04), ("Rivazza", 0.04, 0.03)]),
            lap(&[("Tosa", 0.04, 0.03), ("Rivazza", 0.03, 0.04)]),
            lap(&[("Tosa", 0.04, 0.09), ("Rivazza", 0.03, 0.24)]),
            lap(&[("Tosa", 0.05, 0.10), ("Rivazza", 0.03, 0.25)]),
            lap(&[("Tosa", 0.03, 0.09), ("Rivazza", 0.03, 0.22)]),
        ];
        let d = drift_from_phases(&laps, 3).unwrap();
        assert_eq!((d.laps_first, d.laps_second), (3, 3));
        let top: Vec<(&str, CornerPhase, i32)> = d.top.iter().map(|t| (t.corner.as_str(), t.phase, (t.drift_s * 100.0).round() as i32)).collect();
        assert_eq!(top, vec![("Rivazza", CornerPhase::Sortie, 20), ("Tosa", CornerPhase::Sortie, 6)]);
        let sortie = d.by_phase.iter().find(|(p, _)| *p == CornerPhase::Sortie).unwrap().1;
        assert!((sortie - 0.26).abs() < 1e-4, "{sortie}");
        let mut d = d;
        d.wear_front_pct = Some(1.1);
        d.wear_rear_pct = Some(1.8);
        assert_eq!(
            drift_sentence(&d).unwrap(),
            "Ce qui ralentit le plus d'une moitié à l'autre : Rivazza en sortie +0,20 s, Tosa en sortie +0,06 s. Sur tout le tour, 0,26 s en sortie (100 % de la perte). Usure par tour : avant 1,10 %, arrière 1,80 %."
        );
    }

    #[test]
    fn needs_two_laps_per_half_and_reads_wear() {
        let l = lap(&[("V1", 0.0, 0.0)]);
        assert!(drift_from_phases(&[l.clone(), l.clone(), l.clone()], 1).is_none());
        let mut a = LapMeta { number: 1, kind: crate::model::LapKind::Flying, lap_time_s: Some(100.0), sector_times_s: vec![], flags: Default::default(), validity: crate::model::Validity::Valid, validity_source: crate::model::ValiditySource::SimReported, fuel_start_l: None, fuel_used_l: None, energy_used_pct: None, tyre_wear_pct: Some([1.0, 1.2, 1.8, 2.0]) };
        let b = LapMeta { tyre_wear_pct: Some([1.2, 1.0, 2.0, 1.8]), ..a.clone() };
        let (f, r) = wear_per_lap(&[&a, &b]);
        assert!((f.unwrap() - 1.1).abs() < 1e-6 && (r.unwrap() - 1.9).abs() < 1e-6);
        a.tyre_wear_pct = None;
        assert_eq!(wear_per_lap(&[&a]), (None, None));
    }
}
