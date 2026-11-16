//! Débrief de course : où est parti le temps, et quoi viser à la prochaine course.
//!
//! Chaîne suivie pour chaque ligne : donnée enregistrée → mesure → interprétation → priorité. Rien
//! n'est estimé « à vue » : une perte n'apparaît que si elle se mesure (temps au tour contre ta médiane
//! de course, arrêts, énergie restante à l'arrivée, écart au leader enregistré). Ce qui n'a pas pu être
//! mesuré est listé dans `missing` au lieu d'être deviné.
//!
//! Calcul déterministe : aucune IA ici.

use crate::analysis::offtrack::OffTrackZone;
use crate::ingest::lmu::Recording;
use crate::model::{LapKind, LapMeta};
use serde::Serialize;

/// Un arrêt au stand, mesuré dans l'enregistrement.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StopFact {
    /// Tour de rentrée.
    pub lap: i32,
    /// Temps passé dans la voie des stands (s), dont à l'arrêt.
    pub lane_s: f64,
    pub still_s: f64,
    /// Énergie virtuelle à l'entrée et à la sortie (%) ; 0 si la voiture n'en a pas.
    pub energy_in: f32,
    pub energy_out: f32,
    pub fuel_in_l: f32,
    pub fuel_out_l: f32,
}

/// Ce que seul l'enregistrement d'origine contient (LMU) : arrêts, énergie, écart au leader.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct RaceExtras {
    pub stops: Vec<StopFact>,
    /// Énergie par tour en piste, médiane (%).
    pub energy_per_lap: Option<f32>,
    /// Énergie au plus bas du dernier tour (%).
    pub energy_finish: Option<f32>,
    /// Tours bouclés après le dernier plein.
    pub laps_after_last_fill: Option<u32>,
    /// (tour bouclé, retard sur le leader en s) quand il est enregistré et dans le même tour.
    pub gaps: Vec<(i32, f64)>,
}

/// Lit arrêts, énergie et écarts dans un enregistrement LMU.
pub fn race_extras(r: &Recording) -> RaceExtras {
    let t = &r.ticks;
    if t.len() < 2 {
        return RaceExtras::default();
    }
    let mut stops = vec![];
    let mut cur: Option<(usize, f64)> = None;
    for i in 1..t.len() {
        let k = &t[i];
        if k.pit && cur.is_none() {
            cur = Some((i, 0.0));
        }
        if let Some((start, still)) = cur.as_mut() {
            if k.pit && k.speed < 0.5 {
                *still += k.et - t[i - 1].et;
            }
            if !k.pit {
                let a = &t[*start];
                stops.push(StopFact { lap: a.lap, lane_s: k.et - a.et, still_s: *still, energy_in: a.energy, energy_out: k.energy, fuel_in_l: a.fuel, fuel_out_l: k.fuel });
                cur = None;
            }
        }
    }
    // Sortie des stands au départ et simples traversées : pas des arrêts.
    let first_lap = t[0].lap;
    stops.retain(|s| s.lap > first_lap && (s.lane_s > 15.0 || s.still_s > 2.0));

    let mut starts: Vec<&crate::ingest::lmu::Tick> = vec![];
    for k in t {
        if starts.last().is_none_or(|p| p.lap != k.lap) {
            starts.push(k);
        }
    }
    let pit_laps: Vec<i32> = stops.iter().flat_map(|s| [s.lap, s.lap + 1]).collect();
    let mut per: Vec<f32> = starts.windows(2).filter(|w| !pit_laps.contains(&w[0].lap) && w[0].lap > first_lap).map(|w| w[0].energy - w[1].energy).filter(|d| *d > 0.3 && *d < 15.0).collect();
    per.sort_by(|a, b| a.total_cmp(b));
    let energy_per_lap = per.get(per.len() / 2).copied();
    let last_lap = t.last().map(|k| k.lap).unwrap_or(0);
    // Le jeu remet parfois 100 % après le drapeau : on prend le plus bas du dernier tour roulé.
    let low = t.iter().filter(|k| k.lap >= last_lap - 1 && !k.pit && k.energy > 0.0).map(|k| k.energy).fold(f32::MAX, f32::min);
    let energy_finish = (energy_per_lap.is_some() && low != f32::MAX).then_some(low);
    let laps_after_last_fill = stops.iter().rev().find(|s| s.energy_out > s.energy_in + 2.0 || s.fuel_out_l > s.fuel_in_l + 1.0).map(|s| starts.iter().filter(|k| k.lap > s.lap).count().saturating_sub(1) as u32);
    let gaps = r.laps.iter().filter(|e| e.laps_behind_leader == 0 && e.time_behind_leader > 0.0).map(|e| (e.total_laps, e.time_behind_leader)).collect();
    RaceExtras { stops, energy_per_lap, energy_finish, laps_after_last_fill, gaps }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LossKind {
    /// Tours nettement plus lents que ta médiane (erreur, sortie, trafic).
    Errors,
    /// Arrêts au stand.
    Stops,
    /// Énergie en trop à l'arrivée : temps d'arrêt qui aurait pu être évité.
    Overfill,
    /// Rythme de course face au leader (écart enregistré).
    Pace,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TimeLoss {
    pub kind: LossKind,
    pub seconds: f64,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DebriefAction {
    pub title: String,
    pub detail: String,
    /// Temps mesuré que cette priorité vise (s).
    pub seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StintPace {
    pub from_lap: i32,
    pub to_lap: i32,
    pub laps: usize,
    pub median_s: f64,
    /// Médiane de la première et de la seconde moitié des tours propres du relais (s).
    pub first_half_s: f64,
    pub second_half_s: f64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct RaceDebrief {
    /// Médiane de tes tours de course sans incident (s) : la référence de toutes les pertes.
    pub median_s: f64,
    pub clean_laps: usize,
    pub race_laps: usize,
    /// Pertes mesurées, de la plus grosse à la plus petite.
    pub losses: Vec<TimeLoss>,
    pub stints: Vec<StintPace>,
    pub stops: Vec<StopFact>,
    pub energy_per_lap: Option<f32>,
    pub energy_finish: Option<f32>,
    /// Tours d'énergie restants à l'arrivée.
    pub spare_laps: Option<f32>,
    /// Niveau qu'il aurait fallu régler au dernier plein (%), et niveau réglé.
    pub fill_needed: Option<f32>,
    pub fill_set: Option<f32>,
    /// Au plus trois priorités pour la prochaine course, chacune appuyée sur une perte mesurée.
    pub actions: Vec<DebriefAction>,
    /// Ce qui n'a pas pu être mesuré, et pourquoi.
    pub missing: Vec<String>,
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[v.len() / 2])
}

fn secs(v: f64) -> String {
    format!("{v:.1}").replace('.', ",")
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n > 1 { many } else { one })
}

/// Seuil au-delà duquel un tour compte comme raté : 2 s ou 2 % de la médiane, le plus grand des deux.
pub fn error_threshold(median_s: f64) -> f64 {
    (median_s * 0.02).max(2.0)
}

/// Débrief d'une course. `laps` : tous les tours de la séance, dans l'ordre. `extras` : absent quand
/// l'enregistrement d'origine n'est pas lisible (ou hors LMU).
pub fn race_debrief(laps: &[LapMeta], off_tracks: &[OffTrackZone], extras: Option<&RaceExtras>) -> Option<RaceDebrief> {
    let timed: Vec<&LapMeta> = laps.iter().filter(|l| l.lap_time_s.is_some() && l.kind != LapKind::Incomplete).collect();
    let first = timed.iter().map(|l| l.number).min()?;
    // Tours de course « normaux » : lancés, hors stand, hors drapeau jaune, hors premier tour (départ).
    let racing: Vec<&LapMeta> = timed.iter().copied().filter(|l| l.kind == LapKind::Flying && !l.flags.pit && !l.flags.yellow_flag && l.number > first).collect();
    let mut undisturbed: Vec<f64> = racing.iter().filter(|l| !l.flags.off_track && !l.flags.incident).filter_map(|l| l.lap_time_s).collect();
    if undisturbed.len() < 3 {
        return None;
    }
    let med = median(&mut undisturbed)?;
    let thr = error_threshold(med);
    let mut d = RaceDebrief { median_s: med, clean_laps: undisturbed.len(), race_laps: timed.len(), ..Default::default() };

    // --- Tours ratés -------------------------------------------------------------------------
    let mut bad: Vec<(i32, f64, bool)> = racing.iter().filter_map(|l| l.lap_time_s.map(|t| (l.number, t - med, l.flags.off_track))).filter(|(_, e, _)| *e > thr).collect();
    bad.sort_by(|a, b| b.1.total_cmp(&a.1));
    let err_s: f64 = bad.iter().map(|b| b.1).sum();
    let off_n = bad.iter().filter(|b| b.2).count();
    // Endroit où ces tours sont le plus souvent sortis.
    let zone = off_tracks
        .iter()
        .filter_map(|z| z.corner.as_ref().map(|c| (c, z.laps.iter().filter(|n| bad.iter().any(|b| b.0 == **n as i32)).count())))
        .filter(|(_, n)| *n >= 2)
        .max_by_key(|(_, n)| *n);
    if !bad.is_empty() {
        let worst: Vec<String> = bad.iter().take(3).map(|b| format!("tour {} (+{} s)", b.0, secs(b.1))).collect();
        let mut detail = format!("{} à plus de {} s de ta médiane ({}). Les plus coûteux : {}.", plural(bad.len(), "tour", "tours"), secs(thr), fmt_lap(med), worst.join(", "));
        if off_n > 0 {
            detail += &format!(" {off_n} avec sortie de piste");
            if let Some((c, n)) = zone {
                detail += &format!(", dont {n} à {c}");
            }
            detail += ".";
        }
        d.losses.push(TimeLoss { kind: LossKind::Errors, seconds: err_s, title: format!("Tours ratés : {} s", secs(err_s)), detail });
    }

    // --- Arrêts ------------------------------------------------------------------------------
    let mut stop_s = 0.0;
    let mut n_stops = 0;
    for w in timed.windows(2) {
        if let (LapKind::InLap, LapKind::OutLap, Some(a), Some(b)) = (w[0].kind, w[1].kind, w[0].lap_time_s, w[1].lap_time_s) {
            let loss = a + b - 2.0 * med;
            if (5.0..900.0).contains(&loss) {
                stop_s += loss;
                n_stops += 1;
            }
        }
    }
    if let Some(x) = extras {
        d.stops = x.stops.clone();
        d.energy_per_lap = x.energy_per_lap;
        d.energy_finish = x.energy_finish;
    }
    if n_stops > 0 {
        let mut detail = format!("{} : tour de rentrée et tour de sortie comparés à deux tours à ta médiane.", plural(n_stops, "arrêt", "arrêts"));
        if let Some(s) = d.stops.iter().max_by(|a, b| a.still_s.total_cmp(&b.still_s)) {
            detail += &format!(" Voie des stands {} s, dont {} s à l'arrêt.", secs(s.lane_s), secs(s.still_s));
        }
        d.losses.push(TimeLoss { kind: LossKind::Stops, seconds: stop_s, title: format!("Arrêts : {} s", secs(stop_s)), detail });
    }

    // --- Énergie en trop à l'arrivée -------------------------------------------------------------
    let mut overfill: Option<(f64, f32, f32, f32)> = None;
    if let Some(x) = extras {
        if let (Some(c), Some(end)) = (x.energy_per_lap.filter(|c| *c > 0.0), x.energy_finish) {
            let spare = end / c;
            d.spare_laps = Some(spare);
            let fill = x.stops.iter().rev().find(|s| s.energy_out > s.energy_in + 2.0);
            if let (Some(s), Some(after)) = (fill, x.laps_after_last_fill) {
                // Tours courus après le plein, plus une demi-tour de marge.
                let needed = (after as f32 + 0.5) * c;
                d.fill_needed = Some(needed);
                d.fill_set = Some(s.energy_out);
                let extra = s.energy_out - needed;
                let added = s.energy_out - s.energy_in;
                // Au-delà d'un tour d'énergie en trop : temps d'arrêt évitable, au rythme de ton propre plein.
                if extra > c && added > 1.0 && s.still_s > 1.0 {
                    let sec = (s.still_s / added as f64) * extra as f64;
                    overfill = Some((sec, extra, needed, s.energy_out));
                    d.losses.push(TimeLoss {
                        kind: LossKind::Overfill,
                        seconds: sec,
                        title: format!("Plein trop généreux : au plus {} s", secs(sec)),
                        detail: format!(
                            "Arrivée avec {} % d'énergie, soit {} tours. Réglé {:.0} %, il fallait {:.0} % ({} tours courus ensuite et une demi-tour de marge). Ton plein a pris {} s pour {:.0} % : les {:.0} % en trop valent au plus {} s, moins si l'arrêt comptait aussi un changement de pneus.",
                            secs(end as f64),
                            secs(spare as f64),
                            s.energy_out,
                            needed,
                            after,
                            secs(s.still_s),
                            added,
                            extra,
                            secs(sec)
                        ),
                    });
                }
            }
        }
        if x.energy_per_lap.is_none() {
            d.missing.push("Énergie : non enregistrée sur cette course (version d'Apex trop ancienne, ou voiture sans énergie virtuelle).".into());
        }
    } else {
        d.missing.push("Arrêts détaillés et énergie : enregistrement d'origine introuvable ou hors Le Mans Ultimate.".into());
    }

    // --- Rythme face au leader -------------------------------------------------------------------
    match extras.map(|x| &x.gaps) {
        Some(g) if g.len() >= 8 => {
            let bad_laps: Vec<i32> = bad.iter().map(|b| b.0).collect();
            let pit: Vec<i32> = timed.iter().filter(|l| l.flags.pit || l.kind != LapKind::Flying).map(|l| l.number).collect();
            // Ce que le leader reprend sur un tour normal : médiane des variations d'écart, hors arrêts et tours ratés.
            let mut inc: Vec<f64> = g.windows(2).filter(|w| w[1].0 == w[0].0 + 1 && !pit.contains(&w[1].0) && !bad_laps.contains(&w[1].0)).map(|w| w[1].1 - w[0].1).filter(|v| v.abs() < thr * 3.0).collect();
            if inc.len() >= 6 {
                let per_lap = median(&mut inc).unwrap_or(0.0);
                let total = per_lap * racing.len() as f64;
                if per_lap > 0.05 {
                    d.losses.push(TimeLoss {
                        kind: LossKind::Pace,
                        seconds: total,
                        title: format!("Rythme face au leader : {} s", secs(total)),
                        detail: format!("Sur un tour normal, le leader te reprend {} s (médiane de {} tours, hors arrêts et tours ratés), soit {} s sur {} tours de course.", secs(per_lap), inc.len(), secs(total), racing.len()),
                    });
                }
            }
        }
        Some(_) => d.missing.push("Écart au leader : non enregistré sur cette course (versions avant la 0.35, ou leader à plus d'un tour).".into()),
        None => {}
    }
    d.losses.sort_by(|a, b| b.seconds.total_cmp(&a.seconds));

    // --- Rythme par relais -----------------------------------------------------------------------
    let mut cur: Vec<&LapMeta> = vec![];
    let flush = |cur: &mut Vec<&LapMeta>, out: &mut Vec<StintPace>| {
        let mut t: Vec<f64> = cur.iter().filter(|l| !l.flags.off_track && !l.flags.incident).filter_map(|l| l.lap_time_s).filter(|t| *t - med <= thr).collect();
        if t.len() >= 6 {
            // Ordre de course conservé : première moitié du relais contre seconde moitié.
            let half = t.len() / 2;
            let (fa, fb) = (median(&mut t[..half].to_vec()).unwrap_or(med), median(&mut t[half..].to_vec()).unwrap_or(med));
            let all = median(&mut t).unwrap_or(med);
            out.push(StintPace { from_lap: cur[0].number, to_lap: cur[cur.len() - 1].number, laps: cur.len(), median_s: all, first_half_s: fa, second_half_s: fb });
        }
        cur.clear();
    };
    for l in &timed {
        if l.kind != LapKind::Flying || l.flags.pit {
            flush(&mut cur, &mut d.stints);
        } else if !l.flags.yellow_flag && l.number > first {
            cur.push(l);
        }
    }
    flush(&mut cur, &mut d.stints);

    // --- Priorités -------------------------------------------------------------------------------
    if err_s >= 5.0 {
        let mut detail = format!("{} ratés t'ont coûté {} s, soit {:.0} % d'un tour.", plural(bad.len(), "tour", "tours"), secs(err_s), err_s / med * 100.0);
        if let Some((c, n)) = zone {
            detail += &format!(" {n} sorties à {c} : c'est là qu'il faut garder de la marge.");
        } else if off_n > 0 {
            detail += &format!(" {off_n} avec sortie de piste.");
        }
        d.actions.push(DebriefAction { title: "Finir tous les tours".into(), detail, seconds: Some(err_s) });
    }
    // Une priorité seulement à partir de deux tours d'énergie en trop : en dessous, c'est la marge normale.
    if let Some((sec, extra, needed, set)) = overfill.filter(|o| d.energy_per_lap.is_some_and(|c| o.1 >= 2.0 * c)) {
        d.actions.push(DebriefAction {
            title: "Régler un plein plus juste".into(),
            detail: format!("{extra:.0} % d'énergie en trop à l'arrêt ({set:.0} % réglés pour {needed:.0} % nécessaires) : au plus {} s d'arrêt évitables.", secs(sec)),
            seconds: Some(sec),
        });
    }
    if let Some(s) = d.stints.iter().filter(|s| s.second_half_s - s.first_half_s > 0.4).max_by(|a, b| (a.second_half_s - a.first_half_s).total_cmp(&(b.second_half_s - b.first_half_s))) {
        let drop = s.second_half_s - s.first_half_s;
        d.actions.push(DebriefAction {
            title: "Tenir le rythme en fin de relais".into(),
            detail: format!("Relais des tours {} à {} : {} en première moitié, {} en seconde, soit +{} s par tour.", s.from_lap, s.to_lap, fmt_lap(s.first_half_s), fmt_lap(s.second_half_s), secs(drop)),
            seconds: Some(drop * (s.laps / 2) as f64),
        });
    }
    d.actions.sort_by(|a, b| b.seconds.unwrap_or(0.0).total_cmp(&a.seconds.unwrap_or(0.0)));
    d.actions.truncate(3);
    Some(d)
}

/// `94.331` → `1:34.3`.
fn fmt_lap(s: f64) -> String {
    let m = (s / 60.0).floor();
    format!("{}:{:04.1}", m as i64, s - m * 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LapFlags, Validity, ValiditySource};

    // Tours synthétiques (inventés pour le test, pas de la télémétrie réelle).
    fn lap(n: i32, kind: LapKind, t: f64, off: bool) -> LapMeta {
        LapMeta { number: n, kind, lap_time_s: Some(t), sector_times_s: vec![], flags: LapFlags { off_track: off, pit: kind != LapKind::Flying, ..Default::default() }, validity: Validity::Valid, validity_source: ValiditySource::Derived, fuel_start_l: None, fuel_used_l: None, energy_used_pct: None, tyre_wear_pct: None }
    }

    fn race() -> Vec<LapMeta> {
        let mut v = vec![lap(1, LapKind::Flying, 110.0, false)];
        // Relais 1 : 100,0 s puis 100,6 s en seconde moitié ; tour 6 raté (+12 s, sortie), tour 9 (+3 s).
        for n in 2..=13 {
            let base = if n <= 7 { 100.0 } else { 100.6 };
            let t = match n {
                6 => 112.0,
                9 => 103.4,
                _ => base,
            };
            v.push(lap(n, LapKind::Flying, t, n == 6));
        }
        v.push(lap(14, LapKind::InLap, 115.0, false));
        v.push(lap(15, LapKind::OutLap, 150.0, false));
        for n in 16..=27 {
            v.push(lap(n, LapKind::Flying, 100.0, false));
        }
        v
    }

    #[test]
    fn measures_where_the_time_went() {
        let zones = [OffTrackZone { at_m: 800.0, corner: Some("Tosa".into()), laps: vec![6] }];
        let extras = RaceExtras {
            stops: vec![StopFact { lap: 14, lane_s: 70.0, still_s: 36.0, energy_in: 2.0, energy_out: 62.0, fuel_in_l: 2.0, fuel_out_l: 50.0 }],
            energy_per_lap: Some(4.0),
            energy_finish: Some(14.0),
            laps_after_last_fill: Some(12),
            gaps: vec![],
        };
        let d = race_debrief(&race(), &zones, Some(&extras)).unwrap();
        // Médiane des tours sans incident ; seuil d'un tour raté : 2 s (2 % de 100 s).
        assert!((d.median_s - 100.0).abs() < 1e-9);
        assert_eq!(error_threshold(100.0), 2.0);
        let get = |k| d.losses.iter().find(|l| l.kind == k).unwrap();
        // Tours 6 (+12) et 9 (+3,4) : 15,4 s. Le premier tour (départ) ne compte pas.
        assert!((get(LossKind::Errors).seconds - 15.4).abs() < 1e-6, "{:?}", get(LossKind::Errors));
        assert!(get(LossKind::Errors).detail.contains("tour 6 (+12,0 s)"));
        // Arrêt : 115 + 150 − 2 × 100 = 65 s.
        assert!((get(LossKind::Stops).seconds - 65.0).abs() < 1e-6);
        // Plein : 12 tours ensuite + demi-tour = 50 % nécessaires, 62 % réglés : 12 % en trop, à 36 s pour 60 % = 7,2 s.
        assert_eq!((d.fill_needed, d.fill_set, d.spare_laps), (Some(50.0), Some(62.0), Some(3.5)));
        assert!((get(LossKind::Overfill).seconds - 7.2).abs() < 1e-6);
        // Pertes triées, priorités : erreurs, plein, puis fin de relais (+0,6 s par tour sur le relais 1).
        assert_eq!(d.losses[0].kind, LossKind::Stops);
        let titles: Vec<_> = d.actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["Finir tous les tours", "Régler un plein plus juste", "Tenir le rythme en fin de relais"]);
        assert_eq!(d.stints.len(), 2);
        assert!((d.stints[0].second_half_s - d.stints[0].first_half_s - 0.6).abs() < 1e-9);
        // Toute priorité cite un chiffre.
        assert!(d.actions.iter().all(|a| a.detail.chars().any(|c| c.is_ascii_digit())));
    }

    #[test]
    fn says_nothing_when_nothing_is_measurable() {
        // Course propre et régulière : aucune perte, aucune priorité inventée.
        let laps: Vec<LapMeta> = (1..=12).map(|n| lap(n, LapKind::Flying, 100.0 + (n % 2) as f64 * 0.2, false)).collect();
        let d = race_debrief(&laps, &[], None).unwrap();
        assert!(d.losses.is_empty() && d.actions.is_empty());
        assert_eq!(d.missing.len(), 1);
        // Trop peu de tours pour une médiane : pas de débrief du tout.
        assert!(race_debrief(&laps[..3], &[], None).is_none());
    }

    #[test]
    fn pace_against_the_leader_needs_recorded_gaps() {
        // Le leader reprend 0,8 s par tour ; tour 6 (raté) exclu de la mesure.
        let gaps: Vec<(i32, f64)> = (1..=13).map(|n| (n, 5.0 + 0.8 * n as f64 + if n >= 6 { 12.0 } else { 0.0 })).collect();
        let d = race_debrief(&race(), &[], Some(&RaceExtras { gaps, ..Default::default() })).unwrap();
        let p = d.losses.iter().find(|l| l.kind == LossKind::Pace).unwrap();
        // 24 tours de course (tours 2 à 13 et 16 à 27 : hors premier tour et tours de stand) × 0,8 s.
        assert!((p.seconds - 0.8 * 24.0).abs() < 1e-6, "{p:?}");
        let d = race_debrief(&race(), &[], Some(&RaceExtras::default())).unwrap();
        assert!(d.losses.iter().all(|l| l.kind != LossKind::Pace));
        assert!(d.missing.iter().any(|m| m.contains("Écart au leader")));
    }
}
