//! Stratégie carburant d'une course : carburant (ou énergie virtuelle) au départ, nombre d'arrêts,
//! fenêtre d'arrêt, niveau à reprendre à chaque arrêt, économie nécessaire pour éviter un arrêt.
//! Calcul pur, à partir de ta consommation réelle. Même calcul en litres (iRacing, LMP2) et en %
//! d'énergie virtuelle (Le Mans Ultimate : Hypercar et LMGT3, réservoir d'énergie de 100 %).

use serde::{Deserialize, Serialize};

/// Format de la course.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RaceLength {
    Laps(u32),
    Minutes(f32),
}

/// Ce qui limite le relais : le carburant (litres) ou l'énergie virtuelle (% du réservoir d'énergie).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    #[default]
    Litres,
    Energy,
}

impl Unit {
    /// « 26,4 litres » / « 87,4 % d'énergie ».
    pub fn amount(self, v: f32) -> String {
        let n = format!("{:.1}", v).replace('.', ",");
        match self {
            Unit::Litres => format!("{n} litre{}", if v >= 2.0 { "s" } else { "" }),
            Unit::Energy => format!("{n} % d'énergie"),
        }
    }
    /// Consommation par tour : « 0,05 litre » / « 0,2 % ».
    fn per_lap(self, v: f32) -> String {
        let n = format!("{:.2}", v).replace('.', ",");
        match self {
            Unit::Litres => format!("{n} litre"),
            Unit::Energy => format!("{n} %"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyInput {
    pub length: RaceLength,
    #[serde(default)]
    pub unit: Unit,
    /// Consommation moyenne d'un tour de course (l, ou % d'énergie).
    pub per_lap: f32,
    /// Rythme de course (s / tour), pour les courses au temps et la durée des relais.
    pub lap_time_s: f32,
    /// Plein utilisable (l, ou 100 % d'énergie) ; `None` si inconnu.
    pub tank: Option<f32>,
    /// Marge de sécurité, en tours de consommation.
    pub margin_laps: f32,
    /// Tour de formation / lancé avant le départ (compté comme une fraction de tour).
    pub formation_laps: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Stint {
    /// Premier et dernier tour du relais (1 = premier tour de course).
    pub from_lap: u32,
    pub to_lap: u32,
    /// Niveau avec lequel partir pour ce relais (l, ou % d'énergie).
    pub amount: f32,
    /// Durée estimée du relais (s).
    pub duration_s: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Strategy {
    /// Tours à couvrir (course au temps : estimation, avec le tour entamé à la fin du chrono).
    pub laps: u32,
    pub unit: Unit,
    /// Total nécessaire, marge comprise (l, ou % d'énergie).
    pub total: f32,
    pub stops: u32,
    pub stints: Vec<Stint>,
    /// Fenêtre du premier arrêt : (au plus tôt, au plus tard), en tours.
    pub window: Option<(u32, u32)>,
    /// Économie par tour qui permettrait de faire un arrêt de moins, si elle est raisonnable.
    pub save_per_lap: Option<f32>,
    /// Tours que tient un plein (ou 100 % d'énergie).
    pub laps_per_tank: Option<u32>,
    /// Phrase de synthèse.
    pub summary: String,
}

/// Tours à parcourir. Course au temps : quand le chrono tombe, le leader termine son tour ;
/// on compte donc un tour de plus que ce que le temps permet.
pub fn race_laps(length: RaceLength, lap_time_s: f32) -> u32 {
    match length {
        RaceLength::Laps(n) => n.max(1),
        RaceLength::Minutes(m) => {
            let lt = lap_time_s.max(1.0);
            ((m * 60.0) / lt).ceil() as u32 + 1
        }
    }
}

pub fn plan(i: &StrategyInput) -> Strategy {
    let u = i.unit;
    let per = i.per_lap.max(0.01);
    let lt = i.lap_time_s.max(1.0);
    let laps = race_laps(i.length, i.lap_time_s);
    let formation = i.formation_laps.max(0.0);
    let margin = i.margin_laps.max(0.0);
    let reserve = (margin + formation) * per;
    let total = laps as f32 * per + reserve;
    let stint = |from: u32, to: u32, amount: f32| Stint { from_lap: from, to_lap: to, amount, duration_s: (to - from + 1) as f32 * lt };
    let Some(tank) = i.tank.filter(|t| *t > per) else {
        return Strategy {
            laps,
            unit: u,
            total,
            stops: 0,
            stints: vec![stint(1, laps, total)],
            window: None,
            save_per_lap: None,
            laps_per_tank: None,
            summary: format!("{laps} tours, {} au total (marge comprise). Contenance inconnue.", u.amount(total)),
        };
    };
    let laps_per_tank = (tank / per).floor().max(1.0) as u32;
    if total <= tank {
        return Strategy {
            laps,
            unit: u,
            total,
            stops: 0,
            stints: vec![stint(1, laps, total)],
            window: None,
            save_per_lap: None,
            laps_per_tank: Some(laps_per_tank),
            summary: match u {
                Unit::Litres => format!("Sans arrêt : mets {} au départ ({laps} tours, marge comprise ; réservoir {}).", u.amount(total), u.amount(tank)),
                Unit::Energy => format!("Sans arrêt : {laps} tours demandent {} sur 100 (marge comprise).", u.amount(total)),
            },
        };
    }
    // Tours par plein (réserve gardée en fin de course seulement, formation au premier relais).
    let first_laps = ((tank - formation * per) / per).floor().max(1.0) as u32;
    let mut stints = vec![];
    let mut from = 1u32;
    let mut cap = first_laps;
    while from <= laps {
        let remaining = laps - from + 1;
        // Dernier relais : on ajoute la marge, et il doit tenir dans un plein.
        let last = remaining as f32 * per + margin * per + if from == 1 { formation * per } else { 0.0 };
        if last <= tank {
            stints.push(stint(from, laps, last));
            break;
        }
        // Le relais s'arrête au plus tard un tour avant l'arrivée (ravitaillement de complément).
        let to = from + cap.min(remaining - 1).max(1) - 1;
        let amount = ((to - from + 1) as f32 * per + if from == 1 { formation * per } else { 0.0 }).min(tank);
        stints.push(stint(from, to, amount));
        from = to + 1;
        cap = laps_per_tank;
    }
    let stops = stints.len() as u32 - 1;
    // Fenêtre du premier arrêt : assez tard pour que le reste tienne en `stops` pleins,
    // au plus tard quand le réservoir de départ est vide.
    let latest = first_laps.min(laps.saturating_sub(1).max(1));
    let after_first_capacity = stops as f32 * laps_per_tank as f32 - margin;
    let earliest = (laps as f32 - after_first_capacity).ceil().max(1.0) as u32;
    let window = (earliest <= latest).then_some((earliest, latest));
    // Un arrêt de moins : il faudrait couvrir la course avec `stops` pleins (départ compris).
    let needed_per_lap = (stops as f32 * tank - reserve) / laps as f32;
    let save = per - needed_per_lap;
    let save_per_lap = (save > 0.0 && save <= per * 0.08).then_some(save);
    let (w0, w1) = window.unwrap_or((latest, latest));
    let mut summary = match u {
        Unit::Litres => format!("{stops} arrêt{} : départ avec {}, arrêt entre les tours {w0} et {w1}.", if stops > 1 { "s" } else { "" }, u.amount(stints[0].amount)),
        Unit::Energy => format!(
            "{stops} arrêt{} : 100 % d'énergie tiennent {laps_per_tank} tours ; premier arrêt entre les tours {w0} et {w1}.",
            if stops > 1 { "s" } else { "" }
        ),
    };
    if let Some(s) = save_per_lap {
        summary += &format!(" Ou économise {} par tour pour faire un arrêt de moins.", u.per_lap(s));
    }
    Strategy { laps, unit: u, total, stops, stints, window, save_per_lap, laps_per_tank: Some(laps_per_tank), summary }
}

/// Bilan en course, pour le coach (« carburant ») : de quoi finir ou non.
/// `laps_left` : tours restant à couvrir après celui en cours.
pub fn finish_check(unit: Unit, now: f32, per_lap: f32, laps_left: f32) -> String {
    if per_lap <= 0.0 || laps_left <= 0.0 {
        return String::new();
    }
    let need = laps_left * per_lap;
    let spare = now - need;
    if spare >= 0.0 {
        format!("Pour finir, il te faut {} : marge de {}.", unit.amount(need), unit.amount(spare))
    } else {
        let save = -spare / laps_left;
        if save <= per_lap * 0.08 {
            format!("Il manque {} pour finir : économise {} par tour.", unit.amount(-spare), unit.per_lap(save))
        } else {
            format!("Il manque {} pour finir : arrêt nécessaire.", unit.amount(-spare))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(length: RaceLength, tank: Option<f32>) -> StrategyInput {
        StrategyInput { length, unit: Unit::Litres, per_lap: 1.6, lap_time_s: 110.0, tank, margin_laps: 1.0, formation_laps: 0.5 }
    }

    #[test]
    fn short_race_fits_in_one_tank() {
        // 25 min à 1:50 : 13,6 → 14 tours + le tour entamé = 15 tours.
        let s = plan(&input(RaceLength::Minutes(25.0), Some(46.0)));
        assert_eq!(s.laps, 15);
        assert_eq!(s.stops, 0);
        assert!((s.total - (15.0 + 1.5) * 1.6).abs() < 1e-3);
        assert!(s.summary.contains("Sans arrêt : mets 26,4 litres"), "{}", s.summary);
    }

    #[test]
    fn long_race_needs_a_stop_with_window() {
        // 40 tours à 1,6 l : 64 l + marge, réservoir 46 l → 1 arrêt.
        let s = plan(&input(RaceLength::Laps(40), Some(46.0)));
        assert_eq!(s.stops, 1);
        assert_eq!(s.stints.len(), 2);
        assert_eq!(s.stints[0].from_lap, 1);
        assert_eq!(s.stints.last().unwrap().to_lap, 40);
        // Départ plein (46 l) : 28 tours max (0,5 tour de formation) ; le reste (≥ 40 − 27) doit tenir dans 46 l.
        let (a, b) = s.window.unwrap();
        assert_eq!(b, 28);
        assert_eq!(a, 13);
        assert!(s.stints.iter().all(|st| st.amount <= 46.0 + 1e-3));
        assert!(s.save_per_lap.is_none(), "économie irréaliste");
    }

    #[test]
    fn small_overshoot_suggests_fuel_saving() {
        // 28 tours : 44,8 + 2,4 = 47,2 l > 46 → un arrêt, ou ~0,04 l/tour d'économie.
        let s = plan(&input(RaceLength::Laps(28), Some(46.0)));
        assert_eq!(s.stops, 1);
        let save = s.save_per_lap.unwrap();
        assert!((save - (47.2 - 46.0) / 28.0).abs() < 1e-3, "{save}");
        assert!(s.summary.contains("économise"));
    }

    #[test]
    fn unknown_tank_still_gives_total() {
        let s = plan(&input(RaceLength::Laps(10), None));
        assert_eq!(s.stops, 0);
        assert!(s.summary.contains("inconnue"));
    }

    #[test]
    fn finish_check_messages() {
        assert!(finish_check(Unit::Litres, 20.0, 1.6, 10.0).starts_with("Pour finir, il te faut 16,0 litres : marge de 4,0 litres"));
        assert!(finish_check(Unit::Litres, 15.5, 1.6, 10.0).contains("économise 0,05 litre par tour"));
        assert!(finish_check(Unit::Litres, 10.0, 1.6, 10.0).contains("arrêt nécessaire"));
        assert!(finish_check(Unit::Energy, 40.0, 4.5, 8.0).starts_with("Pour finir, il te faut 36,0 % d'énergie : marge de 4,0 % d'énergie"));
    }

    #[test]
    fn energy_stints_for_a_six_hour_hypercar_race() {
        // 6 h à 3:30 (Spa en Hypercar), 4,8 % d'énergie par tour : 100 % = 20 tours.
        let s = plan(&StrategyInput { length: RaceLength::Minutes(360.0), unit: Unit::Energy, per_lap: 4.8, lap_time_s: 210.0, tank: Some(100.0), margin_laps: 0.5, formation_laps: 0.5 });
        assert_eq!(s.laps, 104);
        assert_eq!(s.laps_per_tank, Some(20));
        assert_eq!(s.stops, 5);
        assert!(s.stints.iter().all(|st| st.amount <= 100.0 + 1e-3));
        assert_eq!(s.stints.last().unwrap().to_lap, 104);
        // Relais plein : 20 tours × 3:30 = 70 min.
        assert!((s.stints[1].duration_s - 20.0 * 210.0).abs() < 1e-3);
        assert!(s.summary.contains("100 % d'énergie tiennent 20 tours"), "{}", s.summary);
    }
}
