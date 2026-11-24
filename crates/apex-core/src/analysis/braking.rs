//! Comparaison de freinage par virage : où chaque tour commence à freiner, jusqu'où, vitesse mini à la
//! corde et remise des gaz, pour la carte (repères F / G) et la fiche virage.
//!
//! Le point de freinage vient du **canal frein mesuré** : premier point où la pédale atteint
//! `BRAKE_ON` (10 %) entre le début de la fenêtre du virage et la vitesse minimale, comme
//! `corners::corner_metrics`. Il n'est jamais déduit d'une décélération. Rien n'est affiché quand :
//! - un des deux tours n'a pas de canal frein (pédale jamais au-dessus de 5 % sur tout le tour) ;
//! - un des deux tours ne freine pas pour ce virage (pic sous 20 %) ;
//! - le point est ambigu : deux appuis distincts de plus de 5 m chacun avant la corde (touche de frein
//!   puis vrai freinage), séparés de plus de 10 m ; on ne sait pas lequel comparer.
//!
//! Écarts signés du point de vue du pilote : `brake_gap_m` > 0 = tu freines **plus tard** que la
//! référence ; `min_speed_gap_kmh` > 0 = tu passes plus vite à la corde ; `throttle_gap_m` > 0 = tu
//! remets les gaz **plus tard**. En dessous de `SAME_M` (5 m) / `SAME_KMH` (1 km/h), c'est « pareil ».

use super::corners::{CornerDef, CornerMetrics, BRAKE_ON};
use super::resample::Resampled;
use serde::Serialize;

/// Seuil d'un appui de frein « réel » pour le virage (pic), comme les phases et les conseils.
pub const BRAKE_PEAK_MIN: f32 = 0.20;
/// Sous ce pic sur tout le tour, le canal frein est considéré absent.
pub const CHANNEL_MIN: f32 = 0.05;
/// Écart de point de freinage ou de remise des gaz en dessous duquel on dit « au même endroit ».
pub const SAME_M: f32 = 5.0;
/// Écart de vitesse mini en dessous duquel on dit « même vitesse ».
pub const SAME_KMH: f32 = 1.0;
/// Longueur minimale d'un appui pour compter comme un appui distinct (m).
const PRESS_MIN_M: f32 = 5.0;
/// Deux appuis séparés de moins de 10 m sont un seul freinage modulé (pédale qui repasse un instant
/// sous 10 %), pas une touche de frein suivie d'un vrai freinage.
const PRESS_GAP_M: f32 = 10.0;

/// Zone de freinage d'un tour : début (pédale ≥ 10 %), relâchement, pic.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct BrakeZone {
    pub start_m: f32,
    pub end_m: f32,
    pub peak: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BrakeCompare {
    pub reference: Option<BrakeZone>,
    pub compared: Option<BrakeZone>,
    /// > 0 : tu freines plus tard que la référence (m). `None` si l'un des deux points manque.
    pub brake_gap_m: Option<f32>,
    /// > 0 : tu passes plus vite à la corde (km/h).
    pub min_speed_gap_kmh: f32,
    /// > 0 : tu remets les gaz plus tard (m).
    pub throttle_gap_m: Option<f32>,
    /// Pourquoi aucun repère de freinage n'est comparé (canal absent, pas de freinage, point ambigu).
    pub missing: Option<String>,
}

/// Le tour a-t-il un canal frein exploitable ?
pub fn has_brake_channel(r: &Resampled) -> bool {
    r.brake.iter().any(|&b| b > CHANNEL_MIN)
}

/// Appuis distincts (≥ 10 %, plus de 5 m) entre `from` et `to` : (début, fin, pic).
fn presses(r: &Resampled, from: usize, to: usize) -> Vec<(usize, usize, f32)> {
    let mut out: Vec<(usize, usize, f32)> = Vec::new();
    let mut i = from;
    while i <= to {
        if r.brake[i] >= BRAKE_ON {
            let s = i;
            let mut peak = 0.0f32;
            while i <= to && r.brake[i] >= BRAKE_ON {
                peak = peak.max(r.brake[i]);
                i += 1;
            }
            let gap = ((PRESS_GAP_M / r.step_m).round() as usize).max(1);
            match out.last_mut() {
                Some(last) if s - last.1 < gap => {
                    last.1 = i.min(r.len() - 1);
                    last.2 = last.2.max(peak);
                }
                _ => out.push((s, i.min(r.len() - 1), peak)),
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Zone de freinage d'un tour pour le virage, ou la raison pour laquelle il n'y en a pas.
fn zone(r: &Resampled, d: &CornerDef, m: &CornerMetrics) -> Result<BrakeZone, &'static str> {
    if !has_brake_channel(r) {
        return Err("canal frein absent");
    }
    if m.brake_peak < BRAKE_PEAK_MIN || m.brake_on_m.is_none() {
        return Err("pas de freinage");
    }
    let (s, e) = (r.idx(d.search_start_m), r.idx(m.min_speed_at_m));
    let min_len = ((PRESS_MIN_M / r.step_m).round() as usize).max(1);
    let p: Vec<_> = presses(r, s, e).into_iter().filter(|p| p.1 - p.0 >= min_len).collect();
    if p.len() > 1 {
        return Err("deux appuis de frein, point ambigu");
    }
    let start = m.brake_on_m.unwrap();
    // Relâchement : première fois sous le seuil après le début (au plus tard la sortie du virage).
    let (a, b) = (r.idx(start), r.idx(d.exit_m));
    let end = (a..=b).find(|&i| r.brake[i] < BRAKE_ON).map_or(d.exit_m, |i| r.dist[i]);
    Ok(BrakeZone { start_m: start, end_m: end, peak: m.brake_peak })
}

/// Compare le freinage des deux tours sur un virage (métriques déjà calculées par `corner_metrics`).
pub fn compare_braking(r: &Resampled, c: &Resampled, d: &CornerDef, mr: &CornerMetrics, mc: &CornerMetrics) -> BrakeCompare {
    let zr = zone(r, d, mr);
    let zc = zone(c, d, mc);
    let missing = match (&zr, &zc) {
        (Ok(_), Ok(_)) => None,
        (Err(e), Ok(_)) => Some(format!("référence : {e}")),
        (Ok(_), Err(e)) => Some(format!("ton tour : {e}")),
        (Err(a), Err(b)) if a == b => Some((*a).to_string()),
        (Err(a), Err(b)) => Some(format!("référence : {a} ; ton tour : {b}")),
    };
    let (zr, zc) = (zr.ok(), zc.ok());
    let brake_gap_m = match (zr, zc) {
        (Some(a), Some(b)) => Some(b.start_m - a.start_m),
        _ => None,
    };
    let throttle_gap_m = match (mr.throttle_on_m, mc.throttle_on_m) {
        (Some(a), Some(b)) => Some(b - a),
        _ => None,
    };
    BrakeCompare { reference: zr, compared: zc, brake_gap_m, min_speed_gap_kmh: mc.min_speed_kmh - mr.min_speed_kmh, throttle_gap_m, missing }
}

/// Phrase d'ingénieur chiffrée pour la fiche virage (vide de conseil si aucun écart mesurable).
pub fn braking_sentence(b: &BrakeCompare) -> String {
    let mut parts: Vec<String> = Vec::new();
    match b.brake_gap_m {
        Some(g) if g <= -SAME_M => parts.push(format!("tu freines {:.0} m trop tôt", -g)),
        Some(g) if g >= SAME_M => parts.push(format!("tu freines {:.0} m plus tard", g)),
        Some(_) => parts.push("même point de freinage (à 5 m près)".into()),
        None => {}
    }
    let v = b.min_speed_gap_kmh;
    if v <= -SAME_KMH {
        parts.push(format!("{:.0} km/h de moins à la corde", -v));
    } else if v >= SAME_KMH {
        parts.push(format!("{:.0} km/h de plus à la corde", v));
    }
    match b.throttle_gap_m {
        Some(g) if g >= SAME_M => parts.push(format!("gaz {:.0} m plus tard", g)),
        Some(g) if g <= -SAME_M => parts.push(format!("gaz {:.0} m plus tôt", -g)),
        _ => {}
    }
    if parts.is_empty() {
        return match &b.missing {
            Some(m) => format!("Freinage non comparé ({m})."),
            None => "Aucun écart mesurable sur ce virage.".into(),
        };
    }
    let mut s = parts.join(", ");
    s[..1].make_ascii_uppercase();
    if let Some(m) = &b.missing {
        s.push_str(&format!(" (freinage non comparé : {m})"));
    }
    s + "."
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::corners::corner_metrics;

    fn grid(n: usize) -> Resampled {
        Resampled {
            step_m: 1.0,
            dist: (0..n).map(|i| i as f32).collect(),
            time: (0..n).map(|i| i as f32 * 0.02).collect(),
            speed_kmh: vec![250.0; n],
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

    const D: CornerDef = CornerDef { id: 1, name: String::new(), search_start_m: 50.0, entry_m: 180.0, apex_m: 300.0, exit_m: 500.0 };

    /// Freine de `b0` à `b1`, vitesse mini `vmin` à 300 m, gaz à `g`.
    fn lap(b0: usize, b1: usize, vmin: f32, g: usize) -> Resampled {
        let mut r = grid(600);
        for i in b0..b1 {
            r.brake[i] = 0.9;
            r.throttle[i] = 0.0;
        }
        for i in b1..g {
            r.throttle[i] = 0.0;
        }
        for i in b0..=300 {
            r.speed_kmh[i] = 250.0 - (250.0 - vmin) * (i - b0) as f32 / (300 - b0) as f32;
        }
        for i in 301..500 {
            r.speed_kmh[i] = vmin + (i - 300) as f32 * 0.5;
        }
        r
    }

    /// Référence : freine à 200 m, 92 km/h à la corde, gaz à 330 m. Toi : freine 18 m plus tôt (182 m),
    /// 4 km/h de moins (88), gaz 12 m plus tard (342 m).
    #[test]
    fn measures_brake_point_min_speed_and_throttle_gaps() {
        let (r, c) = (lap(200, 270, 92.0, 330), lap(182, 268, 88.0, 342));
        let (mr, mc) = (corner_metrics(&r, &D), corner_metrics(&c, &D));
        let b = compare_braking(&r, &c, &D, &mr, &mc);
        assert_eq!(b.missing, None);
        assert_eq!(b.reference.unwrap().start_m, 200.0);
        assert_eq!(b.reference.unwrap().end_m, 270.0);
        assert_eq!(b.compared.unwrap().start_m, 182.0);
        assert_eq!(b.brake_gap_m, Some(-18.0));
        assert!((b.min_speed_gap_kmh + 4.0).abs() < 0.01);
        assert_eq!(b.throttle_gap_m, Some(12.0));
        assert_eq!(braking_sentence(&b), "Tu freines 18 m trop tôt, 4 km/h de moins à la corde, gaz 12 m plus tard.");
    }

    #[test]
    fn under_five_metres_is_the_same_point() {
        let (r, c) = (lap(200, 270, 92.0, 330), lap(203, 270, 92.0, 331));
        let b = compare_braking(&r, &c, &D, &corner_metrics(&r, &D), &corner_metrics(&c, &D));
        assert_eq!(b.brake_gap_m, Some(3.0));
        assert_eq!(braking_sentence(&b), "Même point de freinage (à 5 m près).");
    }

    /// Sans canal frein (pédale jamais enregistrée), aucun point n'est inventé à partir de la décélération.
    #[test]
    fn no_brake_channel_means_no_brake_point() {
        let r = lap(200, 270, 92.0, 330);
        let mut c = lap(182, 268, 88.0, 342);
        c.brake.iter_mut().for_each(|b| *b = 0.0);
        let b = compare_braking(&r, &c, &D, &corner_metrics(&r, &D), &corner_metrics(&c, &D));
        assert_eq!(b.compared, None);
        assert_eq!(b.brake_gap_m, None);
        assert_eq!(b.missing.as_deref(), Some("ton tour : canal frein absent"));
        assert!(braking_sentence(&b).contains("freinage non comparé : ton tour : canal frein absent"));
    }

    /// Une touche de frein (10 m) avant le vrai freinage : point ambigu, pas d'écart affiché.
    #[test]
    fn a_dab_before_braking_is_ambiguous() {
        let r = lap(200, 270, 92.0, 330);
        let mut c = lap(200, 270, 92.0, 330);
        for i in 150..160 {
            c.brake[i] = 0.3;
        }
        let b = compare_braking(&r, &c, &D, &corner_metrics(&r, &D), &corner_metrics(&c, &D));
        assert_eq!(b.brake_gap_m, None);
        assert_eq!(b.missing.as_deref(), Some("ton tour : deux appuis de frein, point ambigu"));
    }

    /// Freinage modulé (pédale qui repasse 4 m sous 10 %) : un seul freinage, l'écart est mesuré.
    #[test]
    fn modulated_braking_is_one_press() {
        let r = lap(200, 270, 92.0, 330);
        let mut c = lap(190, 270, 92.0, 330);
        for i in 220..224 {
            c.brake[i] = 0.05;
        }
        let b = compare_braking(&r, &c, &D, &corner_metrics(&r, &D), &corner_metrics(&c, &D));
        assert_eq!(b.missing, None);
        assert_eq!(b.brake_gap_m, Some(-10.0));
    }

    /// Virage pris sans freiner par les deux : rien à comparer au freinage, la vitesse mini reste.
    #[test]
    fn lift_corner_reports_no_braking_but_keeps_min_speed() {
        let mut r = grid(600);
        let mut c = grid(600);
        for i in 250..=300 {
            r.speed_kmh[i] = 230.0;
            c.speed_kmh[i] = 226.0;
        }
        r.brake[10] = 0.6; // canal présent ailleurs dans le tour
        c.brake[10] = 0.6;
        let b = compare_braking(&r, &c, &D, &corner_metrics(&r, &D), &corner_metrics(&c, &D));
        assert_eq!(b.missing.as_deref(), Some("pas de freinage"));
        assert_eq!(braking_sentence(&b), "4 km/h de moins à la corde (freinage non comparé : pas de freinage).");
    }
}
