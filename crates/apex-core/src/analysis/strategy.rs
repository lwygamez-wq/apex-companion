//! Stratégie carburant d'une course : carburant (ou énergie virtuelle) au départ, nombre d'arrêts,
//! fenêtre d'arrêt, niveau à reprendre à chaque arrêt, cible de consommation par tour, économie
//! nécessaire pour éviter un arrêt, ratio carburant (Le Mans Ultimate).
//! Calcul pur, à partir de ta consommation réelle. Même calcul en litres (iRacing, LMP2) et en %
//! d'énergie virtuelle (Le Mans Ultimate : Hypercar et LMGT3, réservoir d'énergie de 100 %).
//!
//! Le même moteur (`target`) sert au calculateur et à l'ingénieur de course en direct : les chiffres
//! annoncés à la radio sont ceux du calculateur, recalculés à chaque tour avec le niveau réel.

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
    pub fn per_lap(self, v: f32) -> String {
        let n = format!("{:.2}", v).replace('.', ",");
        match self {
            Unit::Litres => format!("{n} litre"),
            Unit::Energy => format!("{n} %"),
        }
    }
}

/// Économie maximale proposée pour faire un arrêt de moins (part de la consommation). Au-delà, le
/// lift and coast coûte plus de temps qu'il n'en fait gagner, ou devient irréaliste à tenir.
pub const SAVE_MAX: f32 = 0.08;

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
    /// Marge de sécurité, en tours de consommation, gardée à chaque arrivée au stand et à l'arrivée.
    pub margin_laps: f32,
    /// Tour de formation / lancé avant le départ (compté comme une fraction de tour).
    pub formation_laps: f32,
    /// Énergie virtuelle : ratio carburant (litres par 1 % d'énergie, réglé au stand) et réservoir de
    /// carburant (l). Ensemble, ils disent si le carburant limite avant l'énergie.
    #[serde(default)]
    pub fuel_ratio: Option<f32>,
    #[serde(default)]
    pub fuel_tank: Option<f32>,
    /// Ta voiture est dans la catégorie la plus rapide (Hypercar en LMU) : le drapeau tombe quand le
    /// leader de ta catégorie passe la ligne, pas de tour en plus (voir `race_laps`).
    #[serde(default)]
    pub top_class: bool,
    /// Temps perdu par arrêt (voie des stands + plein), mesuré sur tes courses : une course au temps
    /// compte moins de tours quand on s'arrête.
    #[serde(default)]
    pub pit_loss_s: Option<f32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Stint {
    /// Premier et dernier tour du relais (1 = premier tour de course).
    pub from_lap: u32,
    pub to_lap: u32,
    /// Niveau avec lequel partir pour ce relais (l, ou % d'énergie).
    pub amount: f32,
    /// Énergie virtuelle : carburant que le ratio met avec ce niveau (l).
    pub fuel_l: Option<f32>,
    /// Durée estimée du relais (s).
    pub duration_s: f32,
}

/// Plan « attaque » (le moins d'arrêts sans économiser) ou « économie » (un arrêt de moins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    #[default]
    Attack,
    Save,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlanOption {
    pub kind: PlanKind,
    pub stops: u32,
    /// Consommation par tour à ne pas dépasser pour tenir ce plan (cible de l'ingénieur).
    pub target_per_lap: f32,
    /// Économie demandée par rapport à ta consommation (part : 0,05 = 5 %), plan économie seulement.
    pub saving: Option<f32>,
    pub stints: Vec<Stint>,
    /// Fenêtre du premier arrêt : (au plus tôt, au plus tard), en tours.
    pub window: Option<(u32, u32)>,
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
    /// Tours que tient un plein (ou 100 % d'énergie), marge gardée.
    pub laps_per_tank: Option<u32>,
    /// Plans possibles : attaque (premier) et, si l'économie reste raisonnable, économie.
    pub options: Vec<PlanOption>,
    /// Ratio carburant utilisé (l par 1 % d'énergie) et réservoir de carburant trop petit pour 100 %.
    pub fuel_ratio: Option<f32>,
    pub fuel_limited: bool,
    /// Énergie (ou carburant) utilisable après un arrêt : 100 %, ou moins si le carburant limite.
    pub capacity: Option<f32>,
    /// Phrase de synthèse.
    pub summary: String,
}

/// Tours à parcourir. Course au temps : quand le chrono tombe, le leader termine son tour (l'arrondi
/// au-dessus). Hors de la catégorie la plus rapide, un tour de plus : le drapeau tombe quand le leader
/// de la catégorie la plus rapide passe la ligne, une GT3 peut boucler un tour de plus. En Hypercar
/// (`top_class`), ce tour en trop faisait mettre un tour d'énergie de trop (Imola, 12 nov.).
pub fn race_laps(length: RaceLength, lap_time_s: f32, top_class: bool) -> u32 {
    match length {
        RaceLength::Laps(n) => n.max(1),
        RaceLength::Minutes(m) => {
            let lt = lap_time_s.max(1.0);
            ((m * 60.0) / lt).ceil().max(1.0) as u32 + u32::from(!top_class)
        }
    }
}

/// Cible de consommation pour finir avec un nombre d'arrêts donné.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Target {
    /// Consommation par tour à ne pas dépasser.
    pub per_lap: f32,
    /// Tours à faire avant de rentrer au stand (au plus tard, à la cible) ; `None` : pas d'arrêt.
    /// 0 : déjà sur la réserve, arrêt à ce tour.
    pub box_after: Option<u32>,
}

/// Plus grande consommation par tour qui permet de couvrir `laps` tours avec `stops` arrêts.
/// `level` : niveau actuel ; `cap` : niveau après un arrêt (plein utilisable) ; `reserve` :
/// quantité gardée en arrivant au stand et à l'arrivée. Les relais suivants sont équilibrés.
pub fn target(level: f32, cap: f32, reserve: f32, laps: u32, stops: u32) -> Option<Target> {
    if laps == 0 {
        return None;
    }
    let a = level - reserve;
    if stops == 0 || laps == 1 {
        return (a > 0.0).then(|| Target { per_lap: a / laps as f32, box_after: None });
    }
    let b = cap - reserve;
    if b <= 0.0 {
        return None;
    }
    if a <= 0.0 {
        let ns = (laps as f32 / stops as f32).ceil();
        return Some(Target { per_lap: b / ns, box_after: Some(0) });
    }
    let mut best = 0.0f32;
    for n1 in 1..laps {
        let ns = ((laps - n1) as f32 / stops as f32).ceil().max(1.0);
        best = best.max((a / n1 as f32).min(b / ns));
    }
    // Arrêt au plus tard : le reste tient toujours (relais suivants plus courts).
    let box_after = ((a / best) + 1e-4).floor() as u32;
    Some(Target { per_lap: best, box_after: Some(box_after.clamp(1, laps - 1)) })
}

/// Arrêts nécessaires au minimum quand on peut économiser jusqu'à `save_max` (part de `per_lap`).
pub fn stops_needed(level: f32, cap: f32, reserve: f32, laps: u32, per_lap: f32, save_max: f32) -> Option<u32> {
    let floor = per_lap * (1.0 - save_max) - 1e-5;
    (0..=60u32).find(|&r| target(level, cap, reserve, laps, r).is_some_and(|t| t.per_lap >= floor))
}

/// Relais à la consommation `c` : chaque arrêt au plus tard, dernier relais rempli au juste nécessaire.
#[allow(clippy::too_many_arguments)]
fn build_stints(laps: u32, c: f32, start: f32, cap: f32, reserve: f32, formation: f32, lt: f32, ratio: Option<f32>) -> Vec<Stint> {
    let mut v: Vec<Stint> = vec![];
    let mut from = 1u32;
    let mut level = start;
    let st = |from: u32, to: u32, amount: f32| Stint { from_lap: from, to_lap: to, amount, fuel_l: ratio.map(|r| amount * r), duration_s: (to - from + 1) as f32 * lt };
    while from <= laps && v.len() < 400 {
        let remaining = laps - from + 1;
        let extra = if from == 1 { formation } else { 0.0 };
        let need = remaining as f32 * c + reserve;
        if need <= level + 1e-3 {
            v.push(st(from, laps, need + extra));
            break;
        }
        let n = ((((level - reserve) / c) + 1e-4).floor().max(1.0) as u32).min(remaining.saturating_sub(1).max(1));
        let to = from + n - 1;
        v.push(st(from, to, (n as f32 * c + reserve).min(level) + extra));
        from = to + 1;
        level = cap;
    }
    v
}

/// Niveau à régler à un arrêt : de quoi faire le relais suivant, les relais restants étant
/// équilibrés, à la consommation `c` + 2 %, plus la marge. Même règle que l'ingénieur en course.
pub fn pit_fill(laps_after: u32, stops_left: u32, c: f32, reserve: f32, cap: f32) -> f32 {
    // La marge (`reserve`) suffit : l'ingénieur recalcule à chaque tour et demande du lift si besoin.
    // Les 2 % ajoutés en plus de la marge faisaient finir avec un demi-tour d'énergie de trop.
    let ns = (laps_after as f32 / stops_left.max(1) as f32).ceil();
    (ns * c + reserve).min(cap)
}

/// Relais tels que l'ingénieur les annoncera si tu roules à `c` par tour avec `stops` arrêts :
/// à chaque tour, cible et arrêt recalculés ; arrêt figé deux tours avant ; niveau réglé au juste
/// nécessaire (`pit_fill`). Départ plein (le remplissage du départ ne coûte pas de temps).
#[allow(clippy::too_many_arguments)]
fn simulate(laps: u32, c: f32, cap: f32, reserve: f32, formation: f32, stops: u32, lt: f32, ratio: Option<f32>) -> Vec<Stint> {
    let st = |from: u32, to: u32, amount: f32| Stint { from_lap: from, to_lap: to, amount, fuel_l: ratio.map(|r| amount * r), duration_s: (to - from + 1) as f32 * lt };
    if stops == 0 {
        return vec![st(1, laps, (laps as f32 * c + reserve).min(cap - formation) + formation)];
    }
    let mut v = vec![];
    let (mut level, mut amount, mut from, mut lap, mut r) = (cap - formation, cap, 1u32, 0u32, stops);
    let mut committed: Option<u32> = None;
    while lap < laps && v.len() < 400 {
        let left = laps - lap;
        let box_now = r > 0
            && match target(level, cap, reserve, left, r).and_then(|t| t.box_after) {
                Some(b) => {
                    let b = committed.map(|cb| cb - lap).unwrap_or(b.max(1));
                    if committed.is_none() && b <= 2 {
                        committed = Some(lap + b);
                    }
                    b <= 1
                }
                None => false,
            }
            // Sécurité : jamais sous la moitié de la marge.
            || (r > 0 && level - c < reserve * 0.5);
        lap += 1;
        level -= c;
        if box_now && lap < laps {
            v.push(st(from, lap, amount));
            amount = pit_fill(laps - lap, r, c, reserve, cap);
            level = amount;
            from = lap + 1;
            r -= 1;
            committed = None;
        }
    }
    v.push(st(from, laps, amount));
    v
}

/// Ratio carburant mesuré (l par 1 % d'énergie) sur des tours où l'on a les deux consommations :
/// (médiane mesurée, conseillé). Conseillé : le plus haut mesuré ou la médiane + 2 %, arrondi au
/// centième supérieur, pour que le carburant ne lâche jamais avant l'énergie.
pub fn fuel_ratio(pairs: &[(f32, f32)]) -> Option<(f32, f32)> {
    let mut r: Vec<f32> = pairs.iter().filter(|(f, e)| *f > 0.05 && *e > 0.1).map(|(f, e)| f / e).collect();
    if r.is_empty() {
        return None;
    }
    r.sort_by(|a, b| a.total_cmp(b));
    let median = r[r.len() / 2];
    let high = (median * 1.02).max(*r.last().unwrap());
    Some((median, (high * 100.0 - 1e-3).ceil() / 100.0))
}

pub fn plan(i: &StrategyInput) -> Strategy {
    // Course au temps : chaque arrêt mange du chrono, donc des tours. On planifie sans, puis on retire
    // le temps des arrêts prévus (deux passes : un arrêt de moins peut en découler).
    if let (RaceLength::Minutes(m), Some(pl)) = (i.length, i.pit_loss_s.filter(|p| *p > 0.0)) {
        let at = |stops: u32| StrategyInput { length: RaceLength::Minutes((m - stops as f32 * pl / 60.0).max(1.0)), pit_loss_s: None, ..i.clone() };
        let first = plan(&StrategyInput { pit_loss_s: None, ..i.clone() });
        if first.stops == 0 {
            return first;
        }
        let second = plan(&at(first.stops));
        return if second.stops == first.stops { second } else { plan(&at(second.stops)) };
    }
    let u = i.unit;
    let per = i.per_lap.max(0.01);
    let lt = i.lap_time_s.max(1.0);
    let laps = race_laps(i.length, i.lap_time_s, i.top_class);
    let formation = i.formation_laps.max(0.0) * per;
    let reserve = i.margin_laps.max(0.0) * per;
    let total = laps as f32 * per + reserve + formation;
    let ratio = if u == Unit::Energy { i.fuel_ratio.filter(|r| *r > 0.0) } else { None };
    // Énergie : un réservoir de carburant trop petit pour le ratio limite le plein d'énergie.
    let fuel_cap = ratio.zip(i.fuel_tank.filter(|t| *t > 0.0)).map(|(r, t)| t / r);
    let mut cap = i.tank.filter(|t| *t > 0.0);
    let fuel_limited = matches!((cap, fuel_cap), (Some(c), Some(f)) if f < c);
    if fuel_limited {
        cap = fuel_cap;
    }
    let stint = |from: u32, to: u32, amount: f32| Stint { from_lap: from, to_lap: to, amount, fuel_l: ratio.map(|r| amount * r), duration_s: (to - from + 1) as f32 * lt };
    let base = |stops: u32, stints: Vec<Stint>, options: Vec<PlanOption>, summary: String, cap: Option<f32>| Strategy {
        laps,
        unit: u,
        total,
        stops,
        stints,
        window: None,
        save_per_lap: None,
        laps_per_tank: cap.map(|c| (((c - reserve) / per) + 1e-4).floor().max(1.0) as u32),
        options,
        fuel_ratio: ratio,
        fuel_limited,
        capacity: cap,
        summary,
    };
    let Some(cap) = cap.filter(|t| *t > per + reserve) else {
        return base(0, vec![stint(1, laps, total)], vec![], format!("{laps} tours, {} au total (marge comprise). Contenance inconnue.", u.amount(total)), None);
    };
    let start = cap - formation;
    let flat = build_stints(laps, per, start, cap, reserve, formation, lt, ratio);
    let stops = flat.len() as u32 - 1;
    let window_for = |c: f32, stops: u32| -> Option<(u32, u32)> {
        if stops == 0 {
            return None;
        }
        let latest = ((((start - reserve) / c) + 1e-4).floor().max(1.0) as u32).min(laps.saturating_sub(1).max(1));
        let per_tank = (((cap - reserve) / c) + 1e-4).floor().max(1.0) as u32;
        let earliest = laps.saturating_sub(stops * per_tank).max(1);
        (earliest <= latest).then_some((earliest, latest))
    };
    let attack_t = target(start, cap, reserve, laps, stops).map(|t| t.per_lap).unwrap_or(per).max(per);
    // Relais affichés : ceux que l'ingénieur annoncera à ta consommation (même calcul qu'en course).
    let sim = simulate(laps, per, cap, reserve, formation, stops, lt, ratio);
    let attack_stints = if sim.len() as u32 == stops + 1 { sim } else { flat.clone() };
    let mut options = vec![PlanOption { kind: PlanKind::Attack, stops, target_per_lap: attack_t, saving: None, stints: attack_stints, window: window_for(per, stops) }];
    // Un arrêt de moins : la cible qui le permet, si l'économie reste raisonnable.
    if stops > 0 {
        if let Some(t) = target(start, cap, reserve, laps, stops - 1).filter(|t| t.per_lap >= per * (1.0 - SAVE_MAX) && t.per_lap < per) {
            let mut st = simulate(laps, t.per_lap, cap, reserve, formation, stops - 1, lt, ratio);
            if st.len() as u32 != stops {
                st = build_stints(laps, t.per_lap, start, cap, reserve, formation, lt, ratio);
            }
            if st.len() as u32 == stops {
                options.push(PlanOption { kind: PlanKind::Save, stops: stops - 1, target_per_lap: t.per_lap, saving: Some(1.0 - t.per_lap / per), stints: st, window: window_for(t.per_lap, stops - 1) });
            }
        }
    }
    let save_per_lap = options.get(1).map(|o| per - o.target_per_lap);
    let window = options[0].window;
    let summary = if stops == 0 {
        match u {
            Unit::Litres => format!("Sans arrêt : mets {} au départ ({laps} tours, marge comprise ; réservoir {}).", u.amount(flat[0].amount), u.amount(cap)),
            Unit::Energy => format!("Sans arrêt : {laps} tours demandent {} sur 100 (marge comprise).", u.amount(flat[0].amount)),
        }
    } else {
        let (w0, w1) = window.unwrap_or((1, 1));
        let mut s = match u {
            Unit::Litres => format!("{stops} arrêt{} : départ avec {}, arrêt entre les tours {w0} et {w1}.", if stops > 1 { "s" } else { "" }, u.amount(options[0].stints[0].amount)),
            Unit::Energy => format!(
                "{stops} arrêt{} : {} d'énergie tiennent {} tours ; premier arrêt entre les tours {w0} et {w1}.",
                if stops > 1 { "s" } else { "" },
                if fuel_limited { format!("{:.0} %", cap) } else { "100 %".into() },
                (((cap - reserve) / per) + 1e-4).floor() as u32
            ),
        };
        if let Some(sv) = save_per_lap {
            s += &format!(" Ou économise {} par tour pour faire un arrêt de moins.", u.per_lap(sv));
        }
        s
    };
    let shown = options[0].stints.clone();
    let mut out = base(stops, shown, options, summary, Some(cap));
    out.window = window;
    out.save_per_lap = save_per_lap;
    out
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
        if save <= per_lap * SAVE_MAX {
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
        StrategyInput { length, unit: Unit::Litres, per_lap: 1.6, lap_time_s: 110.0, tank, margin_laps: 1.0, formation_laps: 0.5, fuel_ratio: None, fuel_tank: None, top_class: false, pit_loss_s: None }
    }

    #[test]
    fn short_race_fits_in_one_tank() {
        // 25 min à 1:50 : 13,6 → 14 tours + le tour entamé = 15 tours.
        let s = plan(&input(RaceLength::Minutes(25.0), Some(46.0)));
        assert_eq!(s.laps, 15);
        assert_eq!(s.stops, 0);
        assert!((s.total - (15.0 + 1.5) * 1.6).abs() < 1e-3);
        assert!(s.summary.contains("Sans arrêt : mets 26,4 litres"), "{}", s.summary);
        // Sans arrêt, il reste de la marge : cible au-dessus de la consommation (on peut attaquer).
        assert!(s.options[0].target_per_lap > 1.6);
    }

    #[test]
    fn long_race_needs_a_stop_with_window() {
        // 40 tours à 1,6 l : 64 l + marge, réservoir 46 l → 1 arrêt.
        let s = plan(&input(RaceLength::Laps(40), Some(46.0)));
        assert_eq!(s.stops, 1);
        assert_eq!(s.stints.len(), 2);
        assert_eq!(s.stints[0].from_lap, 1);
        assert_eq!(s.stints.last().unwrap().to_lap, 40);
        // Départ plein (46 l, 0,8 l de formation), 1 tour de marge gardé en arrivant au stand :
        // 27 tours au plus ; le reste (≥ 40 − 27) doit tenir dans un plein.
        let (a, b) = s.window.unwrap();
        assert_eq!(b, 27);
        assert_eq!(a, 13);
        assert!(s.stints.iter().all(|st| st.amount <= 46.0 + 1e-3));
        assert!(s.save_per_lap.is_none(), "économie irréaliste");
        assert_eq!(s.options.len(), 1);
    }

    #[test]
    fn small_overshoot_suggests_fuel_saving() {
        // 28 tours : 44,8 + 2,4 = 47,2 l > 46 → un arrêt, ou ~0,04 l/tour d'économie.
        let s = plan(&input(RaceLength::Laps(28), Some(46.0)));
        assert_eq!(s.stops, 1);
        let save = s.save_per_lap.unwrap();
        assert!((save - (47.2 - 46.0) / 28.0).abs() < 1e-3, "{save}");
        assert!(s.summary.contains("économise"));
        let eco = &s.options[1];
        assert_eq!((eco.kind, eco.stops), (PlanKind::Save, 0));
        assert!((eco.saving.unwrap() - save / 1.6).abs() < 1e-4);
        assert!(eco.stints[0].amount <= 46.0 + 1e-3);
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

    fn spa(ratio: Option<f32>, fuel_tank: Option<f32>) -> Strategy {
        // 6 h à 3:30 (Spa en Hypercar), 4,8 % d'énergie par tour.
        plan(&StrategyInput { length: RaceLength::Minutes(360.0), unit: Unit::Energy, per_lap: 4.8, lap_time_s: 210.0, tank: Some(100.0), margin_laps: 0.5, formation_laps: 0.5, fuel_ratio: ratio, fuel_tank, top_class: false, pit_loss_s: None })
    }

    #[test]
    fn energy_stints_for_a_six_hour_hypercar_race() {
        let s = spa(None, None);
        assert_eq!(s.laps, 104);
        assert_eq!(s.laps_per_tank, Some(20));
        assert_eq!(s.stops, 5);
        assert!(s.stints.iter().all(|st| st.amount <= 100.0 + 1e-3));
        assert_eq!(s.stints.len(), 6);
        assert_eq!(s.stints.last().unwrap().to_lap, 104);
        // Relais enchaînés, chacun tenable avec son niveau de départ, à 4,8 % par tour.
        for w in s.stints.windows(2) {
            assert_eq!(w[1].from_lap, w[0].to_lap + 1);
        }
        for (i, st) in s.stints.iter().enumerate() {
            let n = (st.to_lap - st.from_lap + 1) as f32;
            let start = if i == 0 { st.amount - 0.5 * 4.8 } else { st.amount };
            assert!(start - n * 4.8 >= 0.5 * 4.8 * 0.5 - 1e-3, "{st:?}");
            assert!((st.duration_s - n * 210.0).abs() < 1e-2);
        }
        assert!(s.summary.contains("100 % d'énergie tiennent 20 tours"), "{}", s.summary);
        // Plan économie : 4 arrêts si l'on tient la cible ; chaque relais respecte la cible et la marge.
        let eco = s.options.iter().find(|o| o.kind == PlanKind::Save).expect("plan économie");
        assert_eq!(eco.stops, 4);
        assert!(eco.saving.unwrap() < SAVE_MAX);
        let t = eco.target_per_lap;
        for st in &eco.stints {
            let n = (st.to_lap - st.from_lap + 1) as f32;
            assert!(st.amount + 1e-3 >= n * t + 0.5 * 4.8, "{st:?}");
            assert!(st.amount <= 100.0 + 1e-3);
        }
    }

    #[test]
    fn fuel_ratio_gives_litres_and_can_limit_the_stint() {
        // 0,86 l par % : 100 % d'énergie = 86 l, le réservoir de 90 l suffit.
        let s = spa(Some(0.86), Some(90.0));
        assert!(!s.fuel_limited);
        assert!((s.stints[1].fuel_l.unwrap() - s.stints[1].amount * 0.86).abs() < 1e-3);
        // Réservoir de 75 l : 75 / 0,86 = 87 % d'énergie au plus, le carburant limite les relais.
        let s = spa(Some(0.86), Some(75.0));
        assert!(s.fuel_limited);
        assert!(s.stints.iter().all(|st| st.fuel_l.unwrap() <= 75.0 + 1e-3));
        assert!(s.stops > 5);
        assert!(s.summary.contains("87 % d'énergie tiennent"), "{}", s.summary);
    }

    #[test]
    fn suggested_ratio_never_runs_dry_before_energy() {
        // Tours à 0,84 l/% sauf un à 0,87 : le conseillé couvre le plus gourmand.
        let pairs = [(4.2, 5.0), (4.2, 5.0), (4.35, 5.0), (4.2, 5.0)];
        let (m, s) = fuel_ratio(&pairs).unwrap();
        assert!((m - 0.84).abs() < 1e-4);
        assert!((s - 0.87).abs() < 1e-4, "{s}");
        // Tours réguliers : médiane + 2 %, arrondie au centième supérieur.
        let (_, s) = fuel_ratio(&[(4.2, 5.0); 3]).unwrap();
        assert!((s - 0.86).abs() < 1e-4, "{s}");
        assert!(fuel_ratio(&[]).is_none());
    }

    #[test]
    fn target_balances_the_remaining_stints() {
        // 40 tours, 1 arrêt, réservoir 46, réserve 1,6 : relais de 20/20 → cible 2,2.
        let t = target(46.0, 46.0, 1.6, 40, 1).unwrap();
        assert!((t.per_lap - 44.4 / 20.0).abs() < 1e-3, "{t:?}");
        assert_eq!(t.box_after, Some(20));
        // Sans arrêt : ce qui reste, divisé par les tours.
        let t = target(30.0, 46.0, 1.0, 10, 0).unwrap();
        assert!((t.per_lap - 2.9).abs() < 1e-4);
        assert_eq!(t.box_after, None);
        // Déjà sur la réserve : arrêt à ce tour.
        assert_eq!(target(1.0, 46.0, 1.6, 30, 1).unwrap().box_after, Some(0));
        // Arrêts minimum avec 0 % ou 5 % d'économie.
        assert_eq!(stops_needed(46.0, 46.0, 1.6, 28, 1.6, 0.0), Some(1));
        assert_eq!(stops_needed(46.0, 46.0, 1.6, 28, 1.6, 0.05), Some(0));
    }

    /// Course LMU d'Imola du 12 nov. (9X8, 90 min, 3,18 %/tour, 1:34,3, arrêt de 72,5 s) : 57 tours
    /// courus, un arrêt au tour 30. L'ancien calcul comptait 59 tours (tour multiclasse en trop, arrêt
    /// non décompté) et faisait mettre 96 % à l'arrêt : 14,4 % d'énergie restaient à l'arrivée.
    #[test]
    fn imola_hypercar_race_counts_the_laps_actually_run() {
        assert_eq!(race_laps(RaceLength::Minutes(90.0), 94.3, false), 59);
        assert_eq!(race_laps(RaceLength::Minutes(90.0), 94.3, true), 58);
        let i = StrategyInput { length: RaceLength::Minutes(90.0), unit: Unit::Energy, per_lap: 3.18, lap_time_s: 94.3, tank: Some(100.0), margin_laps: 0.5, formation_laps: 0.5, fuel_ratio: Some(0.83), fuel_tank: Some(110.0), top_class: true, pit_loss_s: Some(72.5) };
        let s = plan(&i);
        assert_eq!((s.laps, s.stops), (57, 1));
        // Niveau du 2e relais : les tours qui restent après l'arrêt, plus la demi-tour de marge.
        let second = &s.stints[1];
        let laps_after = (second.to_lap - second.from_lap + 1) as f32;
        assert!((second.amount - (laps_after * 3.18 + 0.5 * 3.18)).abs() < 0.01, "{second:?}");
        assert!(second.amount < 90.0, "{second:?}");
        // Pas de temps d'arrêt mesuré : la course au temps garde son compte (sans tour en trop).
        assert_eq!(plan(&StrategyInput { pit_loss_s: None, ..i.clone() }).laps, 58);
        // GT3 dans la même course : le tour en plus reste (le leader Hypercar peut finir avant).
        assert_eq!(plan(&StrategyInput { top_class: false, ..i }).laps, 58);
    }

    #[test]
    fn pit_fill_keeps_only_the_margin() {
        // 26 tours après l'arrêt, 3,18 %/tour, marge d'une demi-tour : 84,3 % (l'ancien calcul ajoutait 2 %).
        assert!((pit_fill(26, 1, 3.18, 1.59, 100.0) - 84.27).abs() < 0.01);
    }
}
