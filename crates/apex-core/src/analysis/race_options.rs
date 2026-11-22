//! Comparateur de stratégies de course : 0 à 3 arrêts, avec ou sans économie, temps passé au stand
//! estimé et sa fourchette, marge d'arrivée, usure projetée.
//!
//! Tout part de mesures : consommation et sa dispersion (tes tours de course), temps perdu dans la voie
//! des stands et temps de service par unité remise (tes arrêts enregistrés). Ce qui n'est pas mesuré
//! n'est pas inventé :
//! - le coût du lift and coast (secondes perdues par tour) : on donne le seuil de rentabilité
//!   (« gagnant s'il coûte moins de 1,4 s par tour ») au lieu d'un temps total ;
//! - le changement de pneus : aucun arrêt enregistré n'en a changé, son temps est inconnu ;
//! - les incidents (voiture de sécurité, drapeaux) ne sont pas prévus.
//!
//! Une option n'est annoncée « la plus rapide » que si son avance sur la suivante dépasse deux écarts
//! types de la différence ; sinon les options sont « équivalentes » et la plus sûre est désignée.

use super::strategy::{plan, race_laps, target, RaceLength, StrategyInput, Unit, SAVE_MAX};
use serde::Serialize;

/// Marge d'arrivée visée, en tours de consommation.
pub const ARRIVAL_MIN_LAPS: f32 = 0.3;
pub const ARRIVAL_MAX_LAPS: f32 = 0.8;

/// Temps d'un arrêt, mesuré sur tes arrêts enregistrés.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PitModel {
    /// Temps perdu par un passage au stand hors service (voie, freinage, relance), médiane (s).
    pub lane_s: f32,
    /// Dispersion de ce temps (s).
    pub lane_sd_s: f32,
    /// Temps de service par unité remise (s par % d'énergie, ou par litre).
    pub refill_s_per_unit: f32,
    /// Arrêts mesurés.
    pub stops: usize,
}

/// Arrêt mesuré : temps perdu hors service, temps à l'arrêt, énergie (%) et carburant (l) remis.
/// `pit_model` lit `added_energy` : l'appelant y met la quantité de l'unité de la course.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StopSample {
    pub lane_extra_s: f32,
    pub still_s: f32,
    pub added_energy: f32,
    pub added_fuel: f32,
}

/// Modèle d'arrêt tiré des arrêts mesurés : service = pente (moindres carrés passant par l'origine)
/// du temps à l'arrêt sur la quantité remise ; il faut au moins 2 arrêts avec un vrai plein.
pub fn pit_model(samples: &[StopSample]) -> Option<PitModel> {
    let s: Vec<&StopSample> = samples.iter().filter(|s| s.added_energy > 5.0 && s.still_s > 1.0 && s.lane_extra_s > 0.0).collect();
    if s.len() < 2 {
        return None;
    }
    let rate = s.iter().map(|x| x.added_energy * x.still_s).sum::<f32>() / s.iter().map(|x| x.added_energy * x.added_energy).sum::<f32>();
    let mut lane: Vec<f32> = s.iter().map(|x| x.lane_extra_s).collect();
    lane.sort_by(f32::total_cmp);
    let med = lane[lane.len() / 2];
    let mean = lane.iter().sum::<f32>() / lane.len() as f32;
    let sd = (lane.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / (lane.len() as f32 - 1.0).max(1.0)).sqrt();
    // Peu d'arrêts : la dispersion mesurée est sous-estimée, on garde au moins 2 s.
    Some(PitModel { lane_s: med, lane_sd_s: sd.max(2.0), refill_s_per_unit: rate, stops: s.len() })
}

/// Modèle d'arrêt pour un circuit : service mesuré sur tous les arrêts (il ne dépend pas du circuit),
/// voie des stands mesurée sur ce circuit (au moins un arrêt). Sans arrêt ici : `None`.
pub fn pit_model_for(here: &[StopSample], all: &[StopSample]) -> Option<PitModel> {
    let global = pit_model(all)?;
    let local: Vec<f32> = here.iter().filter(|s| s.added_energy > 5.0 && s.still_s > 1.0 && s.lane_extra_s > 0.0).map(|s| s.lane_extra_s).collect();
    if local.is_empty() {
        return None;
    }
    let mut l = local.clone();
    l.sort_by(f32::total_cmp);
    let lane = l[l.len() / 2];
    let sd = if l.len() >= 2 {
        let m = l.iter().sum::<f32>() / l.len() as f32;
        (l.iter().map(|x| (x - m).powi(2)).sum::<f32>() / (l.len() - 1) as f32).sqrt()
    } else {
        0.0
    };
    Some(PitModel { lane_s: lane, lane_sd_s: sd.max(2.0), refill_s_per_unit: global.refill_s_per_unit, stops: l.len() })
}

#[derive(Debug, Clone, Serialize)]
pub struct OptionsInput {
    pub strategy: StrategyInput,
    /// Dispersion de la consommation d'un tour (même unité).
    pub per_lap_sd: f32,
    pub pit: Option<PitModel>,
    /// Usure du pneu le plus usé, en % par tour (mesurée) ; `None` si inconnue.
    pub wear_per_lap: Option<f32>,
}

/// Une stratégie candidate.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Candidate {
    pub stops: u32,
    /// Économie demandée (part de ta consommation : 0,05 = 5 %) ; 0 sans économie.
    pub saving: f32,
    pub target_per_lap: f32,
    /// Tours de la course pour cette option (course au temps : moins de tours si on passe plus de temps au stand).
    pub laps: u32,
    /// Fenêtre du premier arrêt (tours).
    pub window: Option<(u32, u32)>,
    /// Niveau à régler à chaque arrêt (même règle que l'ingénieur).
    pub fills: Vec<f32>,
    /// Temps passé au stand estimé (voie + service), et son écart type (s).
    pub pit_time_s: Option<f32>,
    pub pit_time_sd_s: Option<f32>,
    /// Marge d'arrivée prévue (tours de consommation) et son écart type.
    pub arrival_laps: f32,
    pub arrival_sd_laps: f32,
    /// Économie : seuil de rentabilité (s par tour) sous lequel le lift and coast fait gagner du temps
    /// face à l'option sans économie avec un arrêt de plus.
    pub breakeven_s_per_lap: Option<f32>,
    /// Usure restante projetée du pneu le plus usé au bout du plus long relais (%), sans changement.
    pub wear_left_pct: Option<f32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Verdict {
    /// La plus rapide, avec son avance sur la suivante et l'écart type de cette avance.
    Clear { best: usize, second: usize, delta_s: f32, sd_s: f32 },
    /// Écart dans l'incertitude : la plus sûre (sans économie, plus grande marge) est désignée.
    Equivalent { safest: usize, others: Vec<usize>, delta_s: f32, sd_s: f32 },
    /// Une seule option possible sans économie.
    Only { best: usize },
    /// Pas de quoi comparer (temps d'arrêt non mesuré, contenance inconnue).
    Unknown { reason: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct Options {
    pub unit: Unit,
    pub candidates: Vec<Candidate>,
    pub verdict: Verdict,
    /// Ce qui n'est pas mesuré (dit à l'écran, jamais deviné).
    pub missing: Vec<String>,
    /// Phrase courte pour la radio.
    pub radio: String,
    /// Modèle d'arrêt utilisé (mesuré), `None` si aucun arrêt mesuré.
    pub pit_model: Option<PitModel>,
}

/// Niveaux à régler à chaque arrêt, quantité remise au total (niveau réglé moins niveau à l'entrée) et
/// fenêtre du premier arrêt (au plus tôt, au plus tard). Chaque arrêt au plus tard, niveau réglé au juste
/// nécessaire pour les relais restants équilibrés (`pit_fill`, même règle que l'ingénieur en course).
fn fills_for(laps: u32, c: f32, cap: f32, reserve: f32, start: f32, stops: u32) -> (Vec<f32>, f32, Option<(u32, u32)>) {
    let mut fills = vec![];
    let mut added = 0.0f32;
    let (mut level, mut lap, mut r) = (start, 0u32, stops);
    let mut window = None;
    let per_tank = (((cap - reserve) / c) + 1e-4).floor().max(1.0) as u32;
    while r > 0 && lap < laps {
        let left = laps - lap;
        let latest = (((level - reserve) / c) + 1e-4).floor().max(1.0) as u32;
        let b = latest.min(left.saturating_sub(1)).max(1);
        if window.is_none() {
            let earliest = laps.saturating_sub(stops * per_tank).max(1);
            window = Some((earliest.min(b), b));
        }
        lap += b;
        let arriving = level - b as f32 * c;
        let fill = super::strategy::pit_fill(laps - lap, r, c, reserve, cap);
        fills.push(fill);
        added += (fill - arriving.max(0.0)).max(0.0);
        level = fill;
        r -= 1;
    }
    (fills, added, window)
}

/// Compare les stratégies possibles.
pub fn compare(i: &OptionsInput) -> Options {
    let s = &i.strategy;
    let u = s.unit;
    let base = plan(s);
    let mut missing = vec![];
    let Some(cap) = base.capacity else {
        return Options { unit: u, candidates: vec![], verdict: Verdict::Unknown { reason: "contenance inconnue".into() }, missing, radio: String::new(), pit_model: i.pit };
    };
    let c = s.per_lap.max(0.01);
    let reserve = s.margin_laps.max(0.0) * c;
    let formation = s.formation_laps.max(0.0) * c;
    let start = cap - formation;
    if i.pit.is_none() {
        missing.push("temps d'arrêt : aucun arrêt mesuré avec un vrai plein".into());
    }
    if i.wear_per_lap.is_none() {
        missing.push("usure des pneus par tour".into());
    }
    missing.push("changement de pneus : aucun arrêt enregistré n'en a changé, son temps n'est pas mesuré".into());
    missing.push("coût du lift and coast : tes tours varient trop peu en consommation pour le mesurer".into());

    let min_stops = base.stops;
    let mut out: Vec<Candidate> = vec![];
    for k in min_stops.saturating_sub(1)..=(min_stops + 1).min(3) {
        // Course au temps : on retire du chrono le temps passé au stand de cette option.
        let pit_guess = i.pit.map(|p| k as f32 * (p.lane_s + p.refill_s_per_unit * cap * 0.6)).unwrap_or(0.0);
        let laps = match s.length {
            RaceLength::Minutes(m) => race_laps(RaceLength::Minutes((m - pit_guess / 60.0).max(1.0)), s.lap_time_s, s.top_class),
            l => race_laps(l, s.lap_time_s, s.top_class),
        };
        let Some(t) = target(start, cap, reserve, laps, k) else { continue };
        let saving = (1.0 - t.per_lap / c).max(0.0);
        if saving > SAVE_MAX + 1e-4 {
            continue;
        }
        let cc = if saving > 0.0 { t.per_lap } else { c };
        let (fills, added, window) = fills_for(laps, cc, cap, reserve, start, k);
        if fills.len() as u32 != k {
            continue;
        }
        let (pit_time_s, pit_time_sd_s) = match i.pit {
            Some(p) => (Some(k as f32 * p.lane_s + p.refill_s_per_unit * added), Some((k as f32).sqrt() * p.lane_sd_s)),
            None => (None, None),
        };
        // Marge d'arrivée : réserve gardée (en tours) ; la dispersion d'un tour s'ajoute sur le dernier relais.
        let last = fills.last().map(|f| ((f - reserve) / cc).floor().max(1.0)).unwrap_or(laps as f32);
        let arrival_laps = reserve / c;
        let arrival_sd_laps = i.per_lap_sd * last.sqrt() / c;
        let longest = if k == 0 { laps as f32 } else { ((laps as f32) / (k + 1) as f32).ceil().max(last) };
        let wear_left_pct = i.wear_per_lap.map(|w| (100.0 - w * longest).max(0.0));
        out.push(Candidate { stops: k, saving, target_per_lap: t.per_lap.min(c.max(t.per_lap)), laps, window, fills, pit_time_s, pit_time_sd_s, arrival_laps, arrival_sd_laps, breakeven_s_per_lap: None, wear_left_pct });
    }
    // Seuil de rentabilité de chaque option avec économie face à l'option sans économie la plus proche.
    let snapshot = out.clone();
    for cand in out.iter_mut().filter(|c| c.saving > 0.0) {
        if let Some(full) = snapshot.iter().filter(|o| o.saving == 0.0 && o.stops > cand.stops).min_by_key(|o| o.stops) {
            if let (Some(a), Some(b)) = (cand.pit_time_s, full.pit_time_s) {
                cand.breakeven_s_per_lap = Some(((b - a) / cand.laps.max(1) as f32).max(0.0));
            }
        }
    }
    let full: Vec<usize> = (0..out.len()).filter(|&j| out[j].saving == 0.0).collect();
    let verdict = if i.pit.is_none() {
        Verdict::Unknown { reason: "temps d'arrêt non mesuré".into() }
    } else if full.len() == 1 {
        Verdict::Only { best: full[0] }
    } else if full.is_empty() {
        Verdict::Unknown { reason: "aucune option sans économie".into() }
    } else {
        let mut f = full.clone();
        f.sort_by(|a, b| out[*a].pit_time_s.unwrap_or(f32::MAX).total_cmp(&out[*b].pit_time_s.unwrap_or(f32::MAX)));
        let (a, b) = (f[0], f[1]);
        let delta = out[b].pit_time_s.unwrap_or(0.0) - out[a].pit_time_s.unwrap_or(0.0);
        let dk = (out[b].stops as f32 - out[a].stops as f32).abs();
        let sd = (dk * i.pit.map(|p| p.lane_sd_s.powi(2)).unwrap_or(0.0)).sqrt().max(1.0);
        if delta > 2.0 * sd {
            Verdict::Clear { best: a, second: b, delta_s: delta, sd_s: sd }
        } else {
            let safest = *f.iter().max_by(|x, y| out[**x].stops.cmp(&out[**y].stops)).unwrap_or(&a);
            Verdict::Equivalent { safest, others: f.into_iter().filter(|x| *x != safest).collect(), delta_s: delta, sd_s: sd }
        }
    };
    let radio = radio_line(u, &out, &verdict);
    Options { unit: u, candidates: out, verdict, missing, radio, pit_model: i.pit }
}

fn stops_txt(k: u32) -> String {
    match k {
        0 => "sans arrêt".into(),
        1 => "un arrêt".into(),
        n => format!("{n} arrêts"),
    }
}

/// Annonce courte et prononçable.
pub fn radio_line(u: Unit, c: &[Candidate], v: &Verdict) -> String {
    let save = c.iter().find(|x| x.saving > 0.0 && x.breakeven_s_per_lap.is_some());
    let save_txt = save.map(|x| {
        format!(
            " Option {} : économise {} par tour, rentable si ça te coûte moins de {} par tour.",
            stops_txt(x.stops),
            u.per_lap(x.target_per_lap * x.saving / (1.0 - x.saving)),
            format!("{:.1} seconde", x.breakeven_s_per_lap.unwrap_or(0.0)).replace('.', ","),
        )
    });
    let window = |x: &Candidate| x.window.map(|(a, b)| if a < b { format!(", arrêt entre les tours {a} et {b}") } else { format!(", arrêt au tour {b}") }).unwrap_or_default();
    let main = match v {
        Verdict::Clear { best, delta_s, .. } => format!("Plan : {}{}. {delta_s:.0} secondes de moins au stand que l'option suivante.", stops_txt(c[*best].stops), window(&c[*best])),
        Verdict::Equivalent { safest, .. } => format!("Plan : {}{}. Les autres options se valent à quelques secondes près.", stops_txt(c[*safest].stops), window(&c[*safest])),
        Verdict::Only { best } => format!("Plan : {}{}.", stops_txt(c[*best].stops), window(&c[*best])),
        Verdict::Unknown { .. } => return String::new(),
    };
    main + &save_txt.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arrêts mesurés du corpus (voie hors service, temps à l'arrêt, énergie remise) : Imola 12 nov. ×2,
    /// Monza 18 nov. ×2, Bahreïn 12 nov.
    fn corpus_stops() -> Vec<StopSample> {
        [(35.6, 38.9, 94.5), (35.8, 26.6, 64.7), (27.9, 26.1, 63.5), (27.4, 25.6, 62.0), (28.2, 14.8, 35.2)]
            .iter()
            .map(|&(l, s, a)| StopSample { lane_extra_s: l, still_s: s, added_energy: a, added_fuel: 0.0 })
            .collect()
    }

    #[test]
    fn service_time_is_about_0_41_s_per_percent_on_the_corpus() {
        let m = pit_model(&corpus_stops()).unwrap();
        assert!((m.refill_s_per_unit - 0.410).abs() < 0.005, "{}", m.refill_s_per_unit);
        assert_eq!((m.lane_s, m.stops), (28.2, 5));
        assert!(m.lane_sd_s > 4.0 && m.lane_sd_s < 5.0, "{}", m.lane_sd_s);
        assert!(pit_model(&corpus_stops()[..1]).is_none(), "un seul arrêt ne suffit pas");
    }

    fn monza(minutes: f32, per_lap: f32) -> OptionsInput {
        OptionsInput {
            strategy: StrategyInput {
                length: RaceLength::Minutes(minutes),
                unit: Unit::Energy,
                per_lap,
                lap_time_s: 98.2,
                tank: Some(100.0),
                margin_laps: 0.5,
                formation_laps: 0.0,
                fuel_ratio: None,
                fuel_tank: None,
                top_class: true,
                pit_loss_s: Some(54.0),
            },
            per_lap_sd: 0.1,
            pit: Some(PitModel { lane_s: 27.6, lane_sd_s: 2.0, refill_s_per_unit: 0.41, stops: 2 }),
            wear_per_lap: Some(0.55),
        }
    }

    #[test]
    fn monza_60_minutes_one_stop_beats_two_and_zero_needs_too_much_saving() {
        // 60 min à 98,2 s, 4,35 %/tour : ~36 tours, 157 % d'énergie : un arrêt minimum.
        let o = compare(&monza(60.0, 4.35));
        let one = o.candidates.iter().position(|c| c.stops == 1 && c.saving == 0.0).unwrap();
        let two = o.candidates.iter().position(|c| c.stops == 2 && c.saving == 0.0).unwrap();
        assert!(o.candidates.iter().all(|c| c.stops != 0), "0 arrêt demanderait plus de 8 % d'économie");
        let (p1, p2) = (o.candidates[one].pit_time_s.unwrap(), o.candidates[two].pit_time_s.unwrap());
        // Un arrêt de plus : une voie des stands de plus (27,6 s), la même énergie remise au total, à peu près.
        assert!((p2 - p1 - 27.6).abs() < 4.0, "{p1} {p2}");
        match &o.verdict {
            Verdict::Clear { best, second, delta_s, sd_s } => {
                assert_eq!((*best, *second), (one, two));
                assert!(*delta_s > 2.0 * sd_s);
            }
            v => panic!("{v:?}"),
        }
        assert!(o.radio.starts_with("Plan : un arrêt"), "{}", o.radio);
        // 1 arrêt : arrivée avec la marge (0,5 tour), dans la plage visée.
        let c = &o.candidates[one];
        assert!((ARRIVAL_MIN_LAPS..=ARRIVAL_MAX_LAPS).contains(&c.arrival_laps), "{}", c.arrival_laps);
        assert!(c.wear_left_pct.is_some());
    }

    #[test]
    fn a_saving_option_gives_a_breakeven_instead_of_a_time() {
        // Juste au-dessus d'un plein : 0 arrêt possible avec un peu d'économie.
        let o = compare(&monza(39.0, 4.35));
        let save = o.candidates.iter().find(|c| c.stops == 0).expect("option sans arrêt avec économie");
        assert!(save.saving > 0.0 && save.saving <= SAVE_MAX);
        let be = save.breakeven_s_per_lap.unwrap();
        // Un arrêt évité ≈ 27,6 s de voie + le service : réparti sur ~23 tours, 1 à 3 s par tour.
        assert!(be > 1.0 && be < 3.5, "{be}");
        assert!(o.radio.contains("rentable si ça te coûte moins de"), "{}", o.radio);
        // Le coût du lift and coast n'est jamais inventé.
        assert!(o.missing.iter().any(|m| m.contains("lift and coast")));
    }

    #[test]
    fn without_measured_stops_nothing_is_ranked() {
        let mut i = monza(60.0, 4.35);
        i.pit = None;
        let o = compare(&i);
        assert!(matches!(o.verdict, Verdict::Unknown { .. }));
        assert!(o.candidates.iter().all(|c| c.pit_time_s.is_none()));
        assert!(o.radio.is_empty());
    }
}
