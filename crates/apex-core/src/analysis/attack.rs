//! Plan d'attaque de la prochaine séance : 3 virages au plus, chacun avec un repère chiffré tiré de
//! **ton meilleur passage propre** de la séance (analyse de régularité), jamais d'une valeur inventée.
//!
//! Ordre : virages où le meilleur passage gagne le plus sur ta médiane (≥ 0,05 s, déjà filtré par la
//! régularité). Repère retenu, dans l'ordre :
//! 1. point de freinage (canal frein mesuré) si le meilleur passage freine à au moins 5 m de ton habitude ;
//! 2. sinon vitesse mini à la corde si l'écart atteint 2 km/h ;
//! 3. sinon remise des gaz si l'écart atteint 8 m.
//!
//! Sans aucun de ces écarts, le virage n'a pas de repère et n'entre pas dans le plan.
//!
//! Repère de piste : les données ne donnent aucun panneau (100 / 50 m) ; la distance est mesurée depuis
//! la ligne de départ et donnée aussi par rapport à ton habitude (« 14 m plus tard que d'habitude »).

use super::regularity::{CornerRegularity, RegularityReport};
use serde::Serialize;

pub const MAX_ITEMS: usize = 3;
const BRAKE_MIN_M: f32 = 5.0;
const SPEED_MIN_KMH: f32 = 2.0;
const THROTTLE_MIN_M: f32 = 8.0;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttackKind {
    Brake,
    MinSpeed,
    Throttle,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AttackItem {
    pub corner: String,
    pub apex_m: f32,
    pub kind: AttackKind,
    /// Repère cible : distance depuis la ligne (m) pour le freinage et les gaz, km/h pour la vitesse mini.
    pub target: f32,
    /// Écart de la cible avec ton habitude (> 0 : plus tard / plus vite).
    pub vs_usual: f32,
    pub best_lap: i32,
    /// Temps du meilleur passage gagné sur ta médiane dans ce virage (s).
    pub gain_s: f64,
    /// Consigne courte et chiffrée (Briefing, carte).
    pub text: String,
}

fn fr(v: f64) -> String {
    format!("{v:.2}").replace('.', ",")
}

fn item(c: &CornerRegularity) -> Option<AttackItem> {
    let base = |kind, target: f32, vs: f32, text: String| AttackItem { corner: c.name.clone(), apex_m: c.apex_m, kind, target, vs_usual: vs, best_lap: c.best_lap, gain_s: c.gain_s, text };
    let gain = fr(c.gain_s);
    if let (Some(b), Some(u)) = (c.best_brake_m, c.usual_brake_m) {
        let d = b - u;
        if d.abs() >= BRAKE_MIN_M {
            let way = if d > 0.0 { "plus tard" } else { "plus tôt" };
            return Some(base(AttackKind::Brake, b, d, format!("{} : freine à {:.0} m de la ligne, {:.0} m {way} que d'habitude (ton meilleur passage, tour {}) : {gain} s à gagner.", c.name, b, d.abs(), c.best_lap)));
        }
    }
    let v = c.best_min_kmh - c.usual_min_kmh;
    if v >= SPEED_MIN_KMH {
        return Some(base(AttackKind::MinSpeed, c.best_min_kmh, v, format!("{} : garde {:.0} km/h à la corde, {:.0} de plus que d'habitude (ton meilleur passage, tour {}) : {gain} s à gagner.", c.name, c.best_min_kmh, v, c.best_lap)));
    }
    if let (Some(t), Some(u)) = (c.best_throttle_m, c.usual_throttle_m) {
        let d = t - u;
        if d <= -THROTTLE_MIN_M {
            return Some(base(AttackKind::Throttle, t, d, format!("{} : gaz à {:.0} m de la ligne, {:.0} m plus tôt que d'habitude (ton meilleur passage, tour {}) : {gain} s à gagner.", c.name, t, -d, c.best_lap)));
        }
    }
    None
}

/// Plan d'attaque : 3 virages au plus, triés par gain, chacun avec un repère mesuré.
pub fn attack_plan(r: &RegularityReport) -> Vec<AttackItem> {
    let mut corners: Vec<&CornerRegularity> = r.corners.iter().collect();
    corners.sort_by(|a, b| b.gain_s.total_cmp(&a.gain_s));
    corners.into_iter().filter_map(item).take(MAX_ITEMS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corner(name: &str, gain: f64, best_brake: Option<f32>, usual_brake: Option<f32>, best_min: f32, usual_min: f32) -> CornerRegularity {
        CornerRegularity {
            name: name.into(),
            apex_m: 100.0,
            passes: 10,
            best_s: 5.0,
            median_s: 5.0 + gain,
            gain_s: gain,
            best_lap: 12,
            brake_spread_m: None,
            differences: vec![],
            advice: String::new(),
            best_brake_m: best_brake,
            usual_brake_m: usual_brake,
            best_min_kmh: best_min,
            usual_min_kmh: usual_min,
            best_throttle_m: Some(400.0),
            usual_throttle_m: Some(412.0),
        }
    }

    #[test]
    fn three_corners_at_most_with_targets_from_the_best_pass() {
        let r = RegularityReport {
            laps: 10,
            total_gain_s: 1.0,
            headline: String::new(),
            corners: vec![
                corner("Roggia", 0.21, Some(1862.0), Some(1848.0), 80.0, 80.0),
                corner("Ascari", 0.30, Some(3500.0), Some(3502.0), 120.0, 116.0),
                corner("Parabolica", 0.12, Some(5100.0), Some(5100.0), 150.0, 150.0),
                corner("Lesmo 1", 0.40, None, None, 170.0, 170.0),
                corner("Rettifilo", 0.08, Some(700.0), Some(690.0), 70.0, 70.0),
            ],
        };
        let p = attack_plan(&r);
        assert_eq!(p.len(), 3);
        // Lesmo 1 : gaz 12 m plus tôt (pas de frein, même vitesse) ; Ascari : vitesse mini ; Roggia : freinage.
        assert_eq!(p[0].corner, "Lesmo 1");
        assert_eq!(p[0].kind, AttackKind::Throttle);
        assert_eq!(p[0].text, "Lesmo 1 : gaz à 400 m de la ligne, 12 m plus tôt que d'habitude (ton meilleur passage, tour 12) : 0,40 s à gagner.");
        assert_eq!(p[1].kind, AttackKind::MinSpeed);
        assert_eq!(p[1].text, "Ascari : garde 120 km/h à la corde, 4 de plus que d'habitude (ton meilleur passage, tour 12) : 0,30 s à gagner.");
        assert_eq!(p[2].corner, "Roggia");
        assert_eq!((p[2].kind, p[2].target, p[2].vs_usual), (AttackKind::Brake, 1862.0, 14.0));
        assert_eq!(p[2].text, "Roggia : freine à 1862 m de la ligne, 14 m plus tard que d'habitude (ton meilleur passage, tour 12) : 0,21 s à gagner.");
        assert!(p.iter().all(|i| i.text.chars().any(|c| c.is_ascii_digit())));
    }

    #[test]
    fn no_measured_target_no_item() {
        let mut c = corner("Parabolica", 0.2, Some(5100.0), Some(5102.0), 150.0, 149.0);
        c.best_throttle_m = Some(5300.0);
        c.usual_throttle_m = Some(5304.0);
        let r = RegularityReport { laps: 10, total_gain_s: 0.2, headline: String::new(), corners: vec![c] };
        assert!(attack_plan(&r).is_empty());
    }
}
