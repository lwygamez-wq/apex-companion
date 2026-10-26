//! Stratégie carburant d'une course : carburant au départ, nombre d'arrêts, fenêtre d'arrêt,
//! économie nécessaire pour éviter un arrêt. Calcul pur, à partir de ta consommation réelle.

use serde::{Deserialize, Serialize};

/// Format de la course.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RaceLength {
    Laps(u32),
    Minutes(f32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyInput {
    pub length: RaceLength,
    /// Consommation moyenne d'un tour de course (l).
    pub fuel_per_lap_l: f32,
    /// Rythme de course (s / tour), pour les courses au temps.
    pub lap_time_s: f32,
    /// Plein utilisable (l) ; `None` si inconnu.
    pub tank_l: Option<f32>,
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
    /// Carburant à mettre au départ du relais (l).
    pub fuel_l: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Strategy {
    /// Tours à couvrir (course au temps : estimation, avec le tour entamé à la fin du chrono).
    pub laps: u32,
    /// Carburant total nécessaire, marge comprise (l).
    pub total_fuel_l: f32,
    pub stops: u32,
    pub stints: Vec<Stint>,
    /// Fenêtre du premier arrêt : (au plus tôt, au plus tard), en tours.
    pub window: Option<(u32, u32)>,
    /// Économie par tour qui permettrait de faire un arrêt de moins (l / tour), si elle est raisonnable.
    pub save_per_lap_to_skip_stop_l: Option<f32>,
    /// Phrase de synthèse.
    pub summary: String,
}

fn litres(v: f32) -> String {
    format!("{:.1}", v).replace('.', ",")
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
    let per = i.fuel_per_lap_l.max(0.01);
    let laps = race_laps(i.length, i.lap_time_s);
    let reserve = (i.margin_laps.max(0.0) + i.formation_laps.max(0.0)) * per;
    let total = laps as f32 * per + reserve;
    let Some(tank) = i.tank_l.filter(|t| *t > per) else {
        return Strategy {
            laps,
            total_fuel_l: total,
            stops: 0,
            stints: vec![Stint { from_lap: 1, to_lap: laps, fuel_l: total }],
            window: None,
            save_per_lap_to_skip_stop_l: None,
            summary: format!("{laps} tours, {} litres au total (marge comprise). Contenance du réservoir inconnue.", litres(total)),
        };
    };
    if total <= tank {
        return Strategy {
            laps,
            total_fuel_l: total,
            stops: 0,
            stints: vec![Stint { from_lap: 1, to_lap: laps, fuel_l: total }],
            window: None,
            save_per_lap_to_skip_stop_l: None,
            summary: format!("Sans arrêt : mets {} litres au départ ({laps} tours, marge comprise ; réservoir {} litres).", litres(total), litres(tank)),
        };
    }
    // Tours par plein (réserve gardée en fin de course seulement, formation au premier relais).
    let first_laps = ((tank - i.formation_laps.max(0.0) * per) / per).floor().max(1.0) as u32;
    let laps_per_tank = (tank / per).floor().max(1.0) as u32;
    let mut stints = vec![];
    let mut from = 1u32;
    let mut cap = first_laps;
    while from <= laps {
        let remaining = laps - from + 1;
        // Dernier relais : on ajoute la marge, et il doit tenir dans un plein.
        let last_fuel = remaining as f32 * per + i.margin_laps.max(0.0) * per + if from == 1 { i.formation_laps.max(0.0) * per } else { 0.0 };
        if last_fuel <= tank {
            stints.push(Stint { from_lap: from, to_lap: laps, fuel_l: last_fuel });
            break;
        }
        // Le relais s'arrête au plus tard un tour avant l'arrivée (ravitaillement de complément).
        let to = from + cap.min(remaining - 1).max(1) - 1;
        let fuel = ((to - from + 1) as f32 * per + if from == 1 { i.formation_laps.max(0.0) * per } else { 0.0 }).min(tank);
        stints.push(Stint { from_lap: from, to_lap: to, fuel_l: fuel });
        from = to + 1;
        cap = laps_per_tank;
    }
    let stops = stints.len() as u32 - 1;
    // Fenêtre du premier arrêt : assez tard pour que le reste tienne en `stops` pleins,
    // au plus tard quand le réservoir de départ est vide.
    let latest = first_laps.min(laps.saturating_sub(1).max(1));
    let after_first_capacity = stops as f32 * laps_per_tank as f32 - i.margin_laps.max(0.0);
    let earliest = (laps as f32 - after_first_capacity).ceil().max(1.0) as u32;
    let window = (earliest <= latest).then_some((earliest, latest));
    // Un arrêt de moins : il faudrait couvrir la course avec `stops` pleins (départ compris).
    let fuel_available = stops as f32 * tank;
    let needed_per_lap = (fuel_available - reserve) / laps as f32;
    let save = per - needed_per_lap;
    let save_per_lap_to_skip_stop_l = (save > 0.0 && save <= per * 0.08).then_some(save);
    let first_fill = stints[0].fuel_l;
    let mut summary = format!(
        "{} arrêt{} : départ avec {} litres, arrêt entre les tours {} et {}.",
        stops,
        if stops > 1 { "s" } else { "" },
        litres(first_fill),
        window.map(|w| w.0).unwrap_or(latest),
        window.map(|w| w.1).unwrap_or(latest)
    );
    if let Some(s) = save_per_lap_to_skip_stop_l {
        summary += &format!(" Ou économise {} litre par tour pour faire un arrêt de moins.", format!("{s:.2}").replace('.', ","));
    }
    Strategy { laps, total_fuel_l: total, stops, stints, window, save_per_lap_to_skip_stop_l, summary }
}

/// Bilan carburant en course, pour le coach (« carburant ») : de quoi finir ou non.
/// `laps_left` : tours restant à couvrir après celui en cours.
pub fn finish_check(fuel_now_l: f32, per_lap_l: f32, laps_left: f32) -> String {
    if per_lap_l <= 0.0 || laps_left <= 0.0 {
        return String::new();
    }
    let need = laps_left * per_lap_l;
    let spare = fuel_now_l - need;
    if spare >= 0.0 {
        format!("Pour finir, il te faut {} litres : marge de {} litre{}.", litres(need), litres(spare), if spare >= 2.0 { "s" } else { "" })
    } else {
        let save = -spare / laps_left;
        if save <= per_lap_l * 0.08 {
            format!("Il manque {} litre pour finir : économise {} litre par tour.", litres(-spare), format!("{save:.2}").replace('.', ","))
        } else {
            format!("Il manque {} litres pour finir : arrêt nécessaire.", litres(-spare))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(length: RaceLength, tank: Option<f32>) -> StrategyInput {
        StrategyInput { length, fuel_per_lap_l: 1.6, lap_time_s: 110.0, tank_l: tank, margin_laps: 1.0, formation_laps: 0.5 }
    }

    #[test]
    fn short_race_fits_in_one_tank() {
        // 25 min à 1:50 : 13,6 → 14 tours + le tour entamé = 15 tours.
        let s = plan(&input(RaceLength::Minutes(25.0), Some(46.0)));
        assert_eq!(s.laps, 15);
        assert_eq!(s.stops, 0);
        assert!((s.total_fuel_l - (15.0 + 1.5) * 1.6).abs() < 1e-3);
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
        assert!(s.stints.iter().all(|st| st.fuel_l <= 46.0 + 1e-3));
        assert!(s.save_per_lap_to_skip_stop_l.is_none(), "économie irréaliste");
    }

    #[test]
    fn small_overshoot_suggests_fuel_saving() {
        // 28 tours : 44,8 + 2,4 = 47,2 l > 46 → un arrêt, ou ~0,04 l/tour d'économie.
        let s = plan(&input(RaceLength::Laps(28), Some(46.0)));
        assert_eq!(s.stops, 1);
        let save = s.save_per_lap_to_skip_stop_l.unwrap();
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
        assert!(finish_check(20.0, 1.6, 10.0).starts_with("Pour finir, il te faut 16,0 litres : marge de 4,0 litres"));
        assert!(finish_check(15.5, 1.6, 10.0).contains("économise 0,05 litre par tour"));
        assert!(finish_check(10.0, 1.6, 10.0).contains("arrêt nécessaire"));
    }
}
