//! Bibliothèque de références : pour chaque (simulateur, circuit, configuration, voiture, série, conditions),
//! le meilleur tour propre et complet du pilote, élu automatiquement à chaque import.
//!
//! Un tour n'est élu que s'il est fiable : tour lancé chronométré, valide, sans sortie, incident, drapeau,
//! passage au stand ni coupure, télémétrie couvrant tout le tour (pas de trou de distance, fin de trace
//! alignée sur le chrono) et pas anormalement plus rapide que les autres tours propres de sa séance.
//! La référence en place n'est remplacée que par un tour plus rapide et fiable : un tour incomplet
//! ne chasse jamais une bonne référence.

use crate::model::{LapKind, LapMeta, LapTrace, Validity};
use serde::{Deserialize, Serialize};

/// Couverture minimale de la longueur du circuit par la trace.
pub const MIN_COVERAGE: f32 = 0.97;
/// Plus grand trou de distance toléré entre deux échantillons (m).
pub const MAX_GAP_M: f32 = 60.0;
/// Un tour plus rapide que 93 % de la médiane des tours propres de sa séance est écarté (coupe, chrono faux).
pub const MIN_RATIO_TO_MEDIAN: f64 = 0.93;

/// Conditions de piste, séparées dans la bibliothèque (un tour sous la pluie ne sert pas de référence sur le sec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Conditions {
    Sec,
    Humide,
    /// Le simulateur ne dit pas si la piste est mouillée.
    Inconnu,
}

impl Conditions {
    pub fn from_wet(wet: Option<bool>) -> Self {
        match wet {
            Some(true) => Conditions::Humide,
            Some(false) => Conditions::Sec,
            None => Conditions::Inconnu,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Conditions::Sec => "sec",
            Conditions::Humide => "humide",
            Conditions::Inconnu => "inconnu",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "sec" => Conditions::Sec,
            "humide" => Conditions::Humide,
            _ => Conditions::Inconnu,
        }
    }
}

/// Raison pour laquelle un tour ne peut pas devenir référence.
#[derive(Debug, Clone, PartialEq)]
pub enum Rejection {
    NotFlying,
    NoTime,
    Invalid,
    Disturbed,
    Incomplete { coverage: f32 },
    Gap { gap_m: f32 },
    TimeMismatch { trace_s: f32, lap_s: f64 },
    TooFast { ratio: f64 },
}

/// Vérifie qu'un tour peut servir de référence. `session_median_s` : médiane des tours propres de la séance
/// (au moins 3 tours, sinon `None`).
pub fn check(meta: &LapMeta, trace: &LapTrace, track_length_m: Option<f32>, session_median_s: Option<f64>) -> Result<(), Rejection> {
    if meta.kind != LapKind::Flying {
        return Err(Rejection::NotFlying);
    }
    let Some(t) = meta.lap_time_s.filter(|t| *t > 0.0) else { return Err(Rejection::NoTime) };
    if meta.validity == Validity::Invalid {
        return Err(Rejection::Invalid);
    }
    if meta.flags.is_disturbed() {
        return Err(Rejection::Disturbed);
    }
    let (Some(d0), Some(d1)) = (trace.dist_m.first(), trace.dist_m.last()) else { return Err(Rejection::Incomplete { coverage: 0.0 }) };
    let len = track_length_m.filter(|l| *l > 100.0).unwrap_or(*d1);
    let coverage = (d1 - d0.max(0.0)) / len;
    if !coverage.is_finite() || coverage < MIN_COVERAGE {
        return Err(Rejection::Incomplete { coverage });
    }
    let gap = trace.dist_m.windows(2).map(|w| w[1] - w[0]).fold(0.0f32, f32::max);
    if gap > MAX_GAP_M {
        return Err(Rejection::Gap { gap_m: gap });
    }
    let end = trace.time_s.last().copied().unwrap_or(0.0);
    if (end as f64 - t).abs() > (0.02 * t).max(1.0) {
        return Err(Rejection::TimeMismatch { trace_s: end, lap_s: t });
    }
    if let Some(m) = session_median_s.filter(|m| *m > 0.0) {
        let ratio = t / m;
        if ratio < MIN_RATIO_TO_MEDIAN {
            return Err(Rejection::TooFast { ratio });
        }
    }
    Ok(())
}

/// Médiane des temps des tours propres (au moins 3), pour écarter un chrono aberrant.
pub fn clean_median(laps: &[LapMeta]) -> Option<f64> {
    let mut v: Vec<f64> = laps.iter().filter(|l| l.is_clean()).filter_map(|l| l.lap_time_s).collect();
    if v.len() < 3 {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[v.len() / 2])
}

/// Décision d'élection : un candidat fiable remplace la référence en place seulement s'il est plus rapide.
/// `current` : temps de la référence en place (`None` si aucune ou si sa séance a disparu).
pub fn replaces(current: Option<f64>, candidate: f64) -> bool {
    current.is_none_or(|c| candidate < c - 1e-6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LapFlags, ValiditySource};

    fn meta(t: f64) -> LapMeta {
        LapMeta {
            number: 3,
            kind: LapKind::Flying,
            lap_time_s: Some(t),
            sector_times_s: vec![],
            flags: LapFlags::default(),
            validity: Validity::Valid,
            validity_source: ValiditySource::SimReported,
            fuel_start_l: None,
            fuel_used_l: None,
            energy_used_pct: None,
            tyre_wear_pct: None,
        }
    }

    /// Trace de 5 000 m en `t` secondes, un point tous les 10 m.
    fn trace(len: f32, t: f32) -> LapTrace {
        let n = (len / 10.0) as usize + 1;
        let dist: Vec<f32> = (0..n).map(|i| i as f32 * 10.0).collect();
        let time: Vec<f32> = dist.iter().map(|d| d / len * t).collect();
        LapTrace { time_s: time, dist_m: dist, speed_mps: vec![50.0; n], throttle: vec![1.0; n], brake: vec![0.0; n], ..Default::default() }
    }

    #[test]
    fn a_clean_complete_lap_is_accepted() {
        assert_eq!(check(&meta(100.0), &trace(5000.0, 100.0), Some(5000.0), Some(102.0)), Ok(()));
    }

    #[test]
    fn invalid_disturbed_or_partial_laps_are_rejected() {
        let tr = trace(5000.0, 100.0);
        let mut m = meta(100.0);
        m.validity = Validity::Invalid;
        assert_eq!(check(&m, &tr, Some(5000.0), None), Err(Rejection::Invalid));
        let mut m = meta(100.0);
        m.flags.off_track = true;
        assert_eq!(check(&m, &tr, Some(5000.0), None), Err(Rejection::Disturbed));
        let mut m = meta(100.0);
        m.kind = LapKind::OutLap;
        assert_eq!(check(&m, &tr, Some(5000.0), None), Err(Rejection::NotFlying));
        // Trace arrêtée à 4 500 m sur 5 000 : 90 % du tour.
        let short = trace(4500.0, 90.0);
        assert!(matches!(check(&meta(100.0), &short, Some(5000.0), None), Err(Rejection::Incomplete { coverage }) if (coverage - 0.9).abs() < 0.01));
        // Trou de 200 m (perte de télémétrie).
        let mut holed = trace(5000.0, 100.0);
        let cut: Vec<usize> = (100..120).collect();
        for k in [&mut holed.dist_m, &mut holed.time_s, &mut holed.speed_mps, &mut holed.throttle, &mut holed.brake] {
            let kept: Vec<f32> = k.iter().enumerate().filter(|(i, _)| !cut.contains(i)).map(|(_, v)| *v).collect();
            *k = kept;
        }
        assert!(matches!(check(&meta(100.0), &holed, Some(5000.0), None), Err(Rejection::Gap { gap_m }) if (gap_m - 210.0).abs() < 1.0));
        // Chrono de 100 s, trace de 104 s : plus de 2 s d'écart.
        assert!(matches!(check(&meta(100.0), &trace(5000.0, 104.0), Some(5000.0), None), Err(Rejection::TimeMismatch { .. })));
        // 92 s quand la séance tourne en 100 s : 0,92 de la médiane.
        assert!(matches!(check(&meta(92.0), &trace(5000.0, 92.0), Some(5000.0), Some(100.0)), Err(Rejection::TooFast { ratio }) if (ratio - 0.92).abs() < 1e-9));
    }

    #[test]
    fn only_a_faster_lap_replaces_the_reference() {
        assert!(replaces(None, 101.0));
        assert!(replaces(Some(101.0), 100.9));
        assert!(!replaces(Some(101.0), 101.0));
        assert!(!replaces(Some(101.0), 101.5));
    }

    #[test]
    fn conditions_round_trip() {
        for c in [Conditions::Sec, Conditions::Humide, Conditions::Inconnu] {
            assert_eq!(Conditions::parse(c.as_str()), c);
        }
        assert_eq!(Conditions::from_wet(Some(true)), Conditions::Humide);
        assert_eq!(Conditions::from_wet(None), Conditions::Inconnu);
    }
}
