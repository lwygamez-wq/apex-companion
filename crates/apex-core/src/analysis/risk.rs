//! Carte de risque : pour chaque virage, le gain disponible **contre** la probabilité d'en sortir.
//!
//! Les outils de télémétrie classiques ne regardent que le tour parfait. Pour un pilote qui sort de
//! la piste, le vrai rythme est celui de la moyenne, sorties comprises : un virage où l'on gagne 0,3 s
//! une fois sur trois et où l'on sort les deux autres fois coûte du temps, des incidents et des tours
//! invalides. Ce module mesure tout cela à partir de plusieurs sessions sur le même circuit et la
//! même voiture, sans rien inventer : chaque chiffre est une moyenne ou un comptage sur tes passages.

use super::offtrack::off_track_spans;
use super::resample::{resample, Resampled};
use crate::model::LapTrace;
use serde::Serialize;

/// Nombre minimal de passages pour émettre un verdict.
pub const MIN_PASSES: usize = 4;
/// Marge autour du virage pour rattacher une sortie de piste (m).
const BEFORE_M: f32 = 30.0;
const AFTER_M: f32 = 60.0;
/// Rattachement d'une sortie en pleine ligne droite au virage précédent au plus à cette distance (m).
const ORPHAN_M: f32 = 150.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskVerdict {
    /// Trop de sorties : sécuriser avant de chercher du temps.
    Secure,
    /// Les passages hors piste sont plus rapides que les propres : coupe de piste (tours invalides).
    Cuts,
    /// Peu de sorties et du temps à prendre par rapport à tes propres meilleurs passages.
    Push,
    /// Rien d'anormal.
    Fine,
    /// Pas assez de passages.
    Insufficient,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CornerRisk {
    pub name: String,
    pub apex_m: f32,
    pub entry_m: f32,
    pub exit_m: f32,
    /// Passages évalués (tours lancés complets).
    pub passes: usize,
    /// Passages avec une sortie de piste dans ce virage.
    pub offs: usize,
    pub off_rate: f32,
    /// Temps moyen de traversée du virage sur les passages propres (s).
    pub mean_clean_s: Option<f32>,
    /// Meilleur passage propre (s).
    pub best_clean_s: Option<f32>,
    /// Temps moyen quand tu sors (s).
    pub mean_off_s: Option<f32>,
    /// Ce que coûte une sortie, mesuré : moyenne des sorties − moyenne des passages propres (s).
    /// Négatif = couper la piste fait gagner du temps (tour invalide).
    pub off_cost_s: Option<f32>,
    /// Temps disponible en répétant ton propre meilleur passage propre (s).
    pub gain_available_s: Option<f32>,
    /// Coût attendu par tour des sorties : taux × coût (s), 0 si les sorties font gagner du temps.
    pub expected_cost_s: f32,
    pub verdict: RiskVerdict,
    pub sessions: usize,
    /// Phrase prête à afficher.
    pub message: String,
}

/// Un tour lancé complet à évaluer.
pub struct RiskLap<'a> {
    pub session_id: i64,
    pub trace: &'a LapTrace,
}

/// Fenêtres de virage : (nom, entrée, apex, sortie) en mètres, issues du tour de référence.
pub type CornerWindow = (String, f32, f32, f32);

fn corner_time(r: &Resampled, a: f32, b: f32) -> Option<f32> {
    let (i, j) = (r.idx(a), r.idx(b));
    (j > i).then(|| r.time[j] - r.time[i])
}

/// Virage auquel rattacher une sortie commençant à `d` (m) : celui dont la fenêtre contient `d`,
/// sinon le virage précédent le plus proche à moins de `ORPHAN_M`.
fn owner(corners: &[CornerWindow], d: f32) -> Option<usize> {
    corners
        .iter()
        .position(|(_, e, _, x)| d >= e - BEFORE_M && d <= x + AFTER_M)
        .or_else(|| {
            corners
                .iter()
                .enumerate()
                .filter(|(_, (_, _, _, x))| d > *x && d - *x <= ORPHAN_M)
                .min_by(|a, b| (d - a.1 .3).total_cmp(&(d - b.1 .3)))
                .map(|(i, _)| i)
        })
}

/// Nombre de tours évalués et de tours avec au moins une sortie de piste (n'importe où).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct RiskTotals {
    pub laps: usize,
    pub off_laps: usize,
}

pub fn lap_totals(laps: &[RiskLap]) -> RiskTotals {
    RiskTotals { laps: laps.len(), off_laps: laps.iter().filter(|l| !off_track_spans(l.trace).is_empty()).count() }
}

pub fn corner_risk(corners: &[CornerWindow], laps: &[RiskLap], length_m: f32) -> Vec<CornerRisk> {
    let n = corners.len();
    let mut clean: Vec<Vec<f32>> = vec![vec![]; n];
    let mut off: Vec<Vec<f32>> = vec![vec![]; n];
    let mut sessions: Vec<std::collections::BTreeSet<i64>> = vec![Default::default(); n];
    for lap in laps {
        let Ok(r) = resample(lap.trace, length_m, 1.0) else { continue };
        if r.coverage < 0.95 {
            continue;
        }
        let mut off_here = vec![false; n];
        for (a, _) in off_track_spans(lap.trace) {
            if let Some(i) = owner(corners, a) {
                off_here[i] = true;
            }
        }
        for (i, (_, e, _, x)) in corners.iter().enumerate() {
            let Some(t) = corner_time(&r, *e, *x) else { continue };
            if off_here[i] { off[i].push(t) } else { clean[i].push(t) }
            sessions[i].insert(lap.session_id);
        }
    }
    let mean = |v: &[f32]| (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32);
    let mut out: Vec<CornerRisk> = corners
        .iter()
        .enumerate()
        .map(|(i, (name, e, apex, x))| {
            let passes = clean[i].len() + off[i].len();
            let offs = off[i].len();
            let off_rate = if passes > 0 { offs as f32 / passes as f32 } else { 0.0 };
            let mean_clean = mean(&clean[i]);
            let best_clean = clean[i].iter().copied().reduce(f32::min);
            let mean_off = mean(&off[i]);
            let off_cost = match (mean_off, mean_clean) {
                (Some(o), Some(c)) => Some(o - c),
                _ => None,
            };
            let gain = match (mean_clean, best_clean) {
                (Some(m), Some(b)) => Some((m - b).max(0.0)),
                _ => None,
            };
            let expected_cost = off_rate * off_cost.unwrap_or(0.0).max(0.0);
            let (verdict, message) = if passes < MIN_PASSES {
                (RiskVerdict::Insufficient, format!("{passes} passage(s) : pas assez pour juger (il en faut {MIN_PASSES})."))
            } else if offs >= 3 && off_rate >= 0.10 && off_cost.is_some_and(|c| c < -0.10) {
                (RiskVerdict::Cuts, format!("Tu coupes la piste {offs} fois sur {passes} : ça fait gagner {:.2} s, mais le tour est invalide et compte un incident. Reste dans les limites, même plus lentement.", -off_cost.unwrap()))
            } else if offs >= 2 && (off_rate >= 0.25 || (off_rate >= 0.10 && expected_cost >= 0.20)) {
                (RiskVerdict::Secure, format!(
                    "Tu sors {offs} fois sur {passes} ({:.0} %). Une sortie coûte {} ici{}. Sécurise ce virage avant d'y chercher du temps : garde ta vitesse d'entrée propre, quitte à perdre 0,2 s.",
                    off_rate * 100.0,
                    off_cost.map_or("un tour invalide".to_string(), |c| format!("{c:.1} s")),
                    gain.filter(|g| *g > 0.05).map_or(String::new(), |g| format!(", pour {g:.2} s de gain possible au mieux")),
                ))
            } else if offs == 0 && gain.is_some_and(|g| g >= 0.25) {
                (RiskVerdict::Push, format!("Aucune sortie sur {passes} passages, et {:.2} s entre ton passage moyen et ton meilleur : c'est ici que tu peux attaquer sans risque mesuré.", gain.unwrap()))
            } else if offs == 1 && gain.is_some_and(|g| g >= 0.25) {
                (RiskVerdict::Push, format!("1 sortie sur {passes} et {:.2} s à prendre en répétant ton meilleur passage : à travailler, avec une marge.", gain.unwrap()))
            } else {
                (RiskVerdict::Fine, format!("{offs} sortie(s) sur {passes}, {} entre moyenne et meilleur passage : rien de prioritaire ici.", gain.map_or("—".into(), |g| format!("{g:.2} s"))))
            };
            CornerRisk {
                name: name.clone(),
                apex_m: *apex,
                entry_m: *e,
                exit_m: *x,
                passes,
                offs,
                off_rate,
                mean_clean_s: mean_clean,
                best_clean_s: best_clean,
                mean_off_s: mean_off,
                off_cost_s: off_cost,
                gain_available_s: gain,
                expected_cost_s: expected_cost,
                verdict,
                sessions: sessions[i].len(),
                message,
            }
        })
        .collect();
    // Priorité : sécuriser > coupes > coût attendu + gain disponible.
    let rank = |v: RiskVerdict| match v {
        RiskVerdict::Secure => 0,
        RiskVerdict::Cuts => 1,
        RiskVerdict::Push => 2,
        RiskVerdict::Fine => 3,
        RiskVerdict::Insufficient => 4,
    };
    out.sort_by(|a, b| {
        rank(a.verdict).cmp(&rank(b.verdict)).then(
            (b.expected_cost_s + b.gain_available_s.unwrap_or(0.0)).total_cmp(&(a.expected_cost_s + a.gain_available_s.unwrap_or(0.0))),
        )
    });
    out
}

/// Phrase de synthèse : ce qui coûte le plus, sorties comprises.
/// `totals` : sorties sur l'ensemble des tours, pour repérer des sorties dispersées dans tout le tour.
pub fn risk_headline(risks: &[CornerRisk], totals: RiskTotals) -> Option<String> {
    let rate = if totals.laps > 0 { totals.off_laps as f32 / totals.laps as f32 } else { 0.0 };
    let secure: Vec<&CornerRisk> = risks.iter().filter(|r| r.verdict == RiskVerdict::Secure).collect();
    if let Some(first) = secure.first() {
        let total: f32 = secure.iter().map(|r| r.expected_cost_s).sum();
        return Some(format!(
            "Priorité : rester en piste. {} virage(s) à sécuriser, {} en tête ({} sorties sur {}). Tes sorties te coûtent en moyenne {:.2} s par tour, avant même de parler de rythme.",
            secure.len(),
            first.name,
            first.offs,
            first.passes,
            total
        ));
    }
    if let Some(c) = risks.iter().find(|r| r.verdict == RiskVerdict::Cuts) {
        return Some(format!("Attention : tu coupes la piste à {} ({} fois sur {}). Ces tours ne comptent pas.", c.name, c.offs, c.passes));
    }
    // Sorties fréquentes mais réparties sur de nombreux virages : pas un virage à corriger, une marge globale.
    if totals.laps >= MIN_PASSES && rate >= 0.15 {
        let spots = risks.iter().filter(|r| r.offs > 0).count();
        return Some(format!(
            "Priorité : rester en piste. Tu sors sur {} tours sur {} ({:.0} %), à {} endroits différents : ce n'est pas un virage en particulier, tu roules trop près de la limite partout. Baisse ton rythme de quelques dixièmes jusqu'à enchaîner 5 tours propres.",
            totals.off_laps,
            totals.laps,
            rate * 100.0,
            spots
        ));
    }
    risks.iter().find(|r| r.verdict == RiskVerdict::Push).map(|p| {
        format!("Rester en piste, c'est acquis. Le temps est à prendre d'abord à {} : {:.2} s entre ton passage moyen et ton meilleur, sans sortie.", p.name, p.gain_available_s.unwrap_or(0.0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::offtrack::TRACK_SURFACE;

    /// Tour synthétique de 3000 m : trois virages ; `slow_a` ralentit en V1, `off_at` marque une sortie.
    fn lap(slow: [f32; 3], off_at: Option<usize>) -> LapTrace {
        let n = 3000;
        let windows = [(400usize, 600usize), (1400, 1600), (2400, 2600)];
        let mut t = Vec::with_capacity(n);
        let mut surf = vec![3.0f32; n];
        let mut clock = 0.0f32;
        for i in 0..n {
            let mut v = 60.0f32;
            for (k, (a, b)) in windows.iter().enumerate() {
                if i >= *a && i < *b {
                    v = 30.0 - slow[k];
                }
            }
            clock += 1.0 / v;
            t.push(clock);
        }
        if let Some(k) = off_at {
            let (a, b) = windows[k];
            for s in surf.iter_mut().take(b + 20).skip(a + 150) {
                *s = 0.0;
            }
        }
        LapTrace {
            time_s: t,
            dist_m: (0..n).map(|i| i as f32).collect(),
            speed_mps: vec![50.0; n],
            throttle: vec![1.0; n],
            brake: vec![0.0; n],
            extra: [(TRACK_SURFACE.to_string(), surf)].into_iter().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn verdicts_from_measured_passes() {
        let corners: Vec<CornerWindow> = vec![("V1".into(), 400.0, 500.0, 600.0), ("V2".into(), 1400.0, 1500.0, 1600.0), ("V3".into(), 2400.0, 2500.0, 2600.0)];
        let mut traces = vec![];
        // 8 tours : V1 sortie 3 fois (plus lent de ~4 s), V2 jamais de sortie mais passage moyen loin du meilleur,
        // V3 coupé 3 fois (plus rapide).
        for i in 0..8 {
            let off = if i < 3 { Some(0) } else if i >= 5 { Some(2) } else { None };
            let v1 = if i < 3 { 10.0 } else { 0.0 };
            let v2 = if i % 2 == 0 { 8.0 } else { 0.0 };
            let v3 = if i >= 5 { -5.0 } else { 0.0 };
            traces.push(lap([v1, v2, v3], off));
        }
        let laps: Vec<RiskLap> = traces.iter().enumerate().map(|(i, t)| RiskLap { session_id: (i / 4) as i64, trace: t }).collect();
        let r = corner_risk(&corners, &laps, 3000.0);
        let by = |n: &str| r.iter().find(|c| c.name == n).unwrap();
        assert_eq!(by("V1").verdict, RiskVerdict::Secure);
        assert_eq!((by("V1").offs, by("V1").passes), (3, 8));
        assert!(by("V1").off_cost_s.unwrap() > 3.0, "{:?}", by("V1").off_cost_s);
        assert_eq!(by("V2").verdict, RiskVerdict::Push);
        assert!(by("V2").gain_available_s.unwrap() > 0.5);
        assert_eq!(by("V3").verdict, RiskVerdict::Cuts);
        assert_eq!(r[0].name, "V1", "sécuriser passe avant tout");
        assert_eq!(by("V1").sessions, 2);
        let totals = lap_totals(&laps);
        assert_eq!(totals, RiskTotals { laps: 8, off_laps: 6 });
        assert!(risk_headline(&r, totals).unwrap().starts_with("Priorité : rester en piste"));
        // Sorties dispersées (aucun virage à sécuriser) mais fréquentes : message global, pas « c'est acquis ».
        let push_only: Vec<CornerRisk> = r.iter().filter(|c| c.verdict == RiskVerdict::Push).cloned().collect();
        let h = risk_headline(&push_only, RiskTotals { laps: 54, off_laps: 18 }).unwrap();
        assert!(h.contains("18 tours sur 54"), "{h}");
        assert!(risk_headline(&push_only, RiskTotals { laps: 54, off_laps: 2 }).unwrap().starts_with("Rester en piste, c'est acquis"));
        // Calibrage sur données réelles (Interlagos, octobre 2026) : 9 sorties sur 39 (23 %) coûtant 3,3 s
        // = 0,76 s par tour en moyenne → à sécuriser ; 3 coupes sur 54 pour 0,07 s = bruit, pas une coupe.
        let real = |offs: usize, passes: usize, cost: f32| {
            let mut v: Vec<LapTrace> = (0..passes).map(|i| if i < offs { lap([cost * 3.0, 0.0, 0.0], Some(0)) } else { lap([0.0; 3], None) }).collect();
            let laps: Vec<RiskLap> = v.iter_mut().map(|t| RiskLap { session_id: 1, trace: t }).collect();
            corner_risk(&corners[..1], &laps, 3000.0)[0].verdict
        };
        assert_eq!(real(9, 39, 3.3), RiskVerdict::Secure);
        assert_eq!(real(3, 54, -0.07), RiskVerdict::Fine);
        // Trop peu de passages : pas de verdict.
        let few = corner_risk(&corners, &laps[..3], 3000.0);
        assert!(few.iter().all(|c| c.verdict == RiskVerdict::Insufficient));
    }
}
